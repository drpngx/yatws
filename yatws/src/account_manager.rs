// yatws/src/account_manager.rs

use crate::account::{AccountInfo, AccountState, AccountValue, Position, AccountObserver, Execution, ExecutionFilter};
use crate::base::IBKRError;
use crate::contract::Contract;
use crate::handler::AccountHandler;
use crate::conn::MessageBroker;
use crate::protocol_decoder::ClientErrorCode; // Added import
use parking_lot::{RwLock, Mutex, Condvar};
use crate::protocol_encoder::Encoder;

use chrono::{Utc, TimeZone};
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::sync::{Arc};
use std::sync::atomic::{AtomicUsize, Ordering, AtomicBool};
use std::time::Duration;


// State for tracking refresh operations
#[derive(Debug, Default)]
struct RefreshState {
  summary_request_id: Option<i32>,
  waiting_for_summary_end: bool,
  waiting_for_position_end: bool,
  // No specific request ID for positions, TWS manages it internally
  // Add flags for other data if needed (e.g., PnL)
}

// State for tracking execution requests
#[derive(Debug, Default)]
struct ExecutionRequestState {
  waiting: bool,
  executions: HashMap<String, Execution>, // Store by ExecID for merging
  end_received: bool, // Track if execDetailsEnd arrived
}

pub struct AccountManager {
  message_broker: Arc<MessageBroker>,
  account_state: RwLock<AccountState>,
  // Store executions temporarily if needed, or rely on on-demand requests
  // executions: RwLock<Vec<Arc<Execution>>>,
  observers: RwLock<HashMap<usize, Box<dyn AccountObserver + Send + Sync>>>,
  next_observer_id: AtomicUsize,
  refresh_state: Mutex<RefreshState>, // Mutex + Condvar for blocking refresh
  refresh_cond: Condvar,
  executions_state: Mutex<HashMap<i32, ExecutionRequestState>>, // State for execution requests (ReqID -> State)
  executions_cond: Condvar, // Condvar for execution requests
  is_subscribed: AtomicBool, // Track background update subscription
}

impl AccountManager {
  pub fn new(message_broker: Arc<MessageBroker>, initial_account_id: Option<String>) -> Arc<Self> {
    let initial_state = AccountState {
      account_id: initial_account_id.unwrap_or_default(),
      ..Default::default()
    };
    Arc::new(AccountManager {
      message_broker,
      account_state: RwLock::new(initial_state),
      observers: RwLock::new(HashMap::new()),
      next_observer_id: AtomicUsize::new(1),
      refresh_state: Mutex::new(RefreshState::default()),
      refresh_cond: Condvar::new(),
      executions_state: Mutex::new(HashMap::new()),
      executions_cond: Condvar::new(),
      is_subscribed: AtomicBool::new(false),
    })
  }

  // --- Helper to get specific account value ---
  pub fn get_account_value(&self, key: &str) -> Result<Option<AccountValue>, IBKRError> {
    // read() returns the guard directly
    let state = self.account_state.read();
    // Optional: Check for poison if needed: if state.is_poisoned() { ... }
    Ok(state.values.get(key).cloned())
  }

  fn get_parsed_value<T: std::str::FromStr>(&self, key: &str) -> Result<T, IBKRError> {
    // Lock inside this function
    let state = self.account_state.read();
    match state.values.get(key) {
      Some(av) => av.value.parse::<T>().map_err(|_| {
        IBKRError::ParseError(format!("Failed to parse value '{}' for key '{}'", av.value, key))
      }),
      None => Err(IBKRError::InternalError(format!("Account value key '{}' not found", key))),
    }
  }

  // --- Public API Methods ---

  pub fn get_account_info(&self) -> Result<AccountInfo, IBKRError> {
    // read() returns the guard directly
    let state = self.account_state.read();

    if state.last_updated.is_none() || state.values.is_empty() {
      return Err(IBKRError::InternalError("Account state not yet populated. Call refresh() or wait for updates.".to_string()));
    }

    // Helper closures remain the same conceptually, but access `state` guard directly
    let parse_or_err = |key: &str| -> Result<f64, IBKRError> {
      state.values.get(key)
        .ok_or_else(|| IBKRError::InternalError(format!("Key '{}' not found in account values", key)))?
        .value.parse::<f64>()
        .map_err(|e| IBKRError::ParseError(format!("Failed to parse value for key '{}': {}", key, e)))
    };
    let parse_or_zero = |key: &str| -> f64 {
      state.values.get(key)
        .and_then(|v| v.value.parse::<f64>().ok())
        .unwrap_or_else(|| { warn!("Failed to parse '{}', using 0.0", key); 0.0 })
    };
    // let parse_int_or_err = |key: &str| -> Result<i32, IBKRError> {
    //   state.values.get(key)
    //     .ok_or_else(|| IBKRError::InternalError(format!("Key '{}' not found in account values", key)))?
    //     .value.parse::<i32>()
    //     .map_err(|e| IBKRError::ParseError(format!("Failed to parse value for key '{}': {}", key, e)))
    // };
    let parse_int_or_minus_one = |key: &str| -> i32 {
      state.values.get(key)
        .and_then(|v| v.value.parse::<i32>().ok())
        .unwrap_or(-1)
    };
    let get_string_or_default = |key: &str| -> String {
      state.values.get(key).map_or(String::new(), |v| v.value.clone())
    };

    Ok(AccountInfo {
      account_id: state.account_id.clone(),
      account_type: get_string_or_default("AccountType"),
      base_currency: get_string_or_default("Currency"),
      equity: parse_or_err("NetLiquidation").or_else(|_| parse_or_err("EquityWithLoanValue")).unwrap_or(0.0),
      buying_power: parse_or_err("BuyingPower").unwrap_or(0.0),
      cash_balance: parse_or_err("TotalCashValue").unwrap_or(0.0),
      day_trades_remaining: parse_int_or_minus_one("DayTradesRemaining"),
      leverage: parse_or_zero("Leverage-S"),
      maintenance_margin: parse_or_err("MaintMarginReq").or_else(|_| parse_or_err("FullMaintMarginReq")).unwrap_or(0.0),
      initial_margin: parse_or_err("InitMarginReq").or_else(|_| parse_or_err("FullInitMarginReq")).unwrap_or(0.0),
      excess_liquidity: parse_or_err("ExcessLiquidity").or_else(|_| parse_or_err("FullExcessLiquidity")).unwrap_or(0.0),
      updated_at: state.last_updated.unwrap_or_else(Utc::now),
    })
  }

  pub fn get_buying_power(&self) -> Result<f64, IBKRError> {
    self.get_parsed_value("BuyingPower")
  }

  pub fn get_cash_balance(&self) -> Result<f64, IBKRError> {
    self.get_parsed_value("TotalCashValue")
  }

  pub fn get_equity(&self) -> Result<f64, IBKRError> {
    self.get_parsed_value("NetLiquidation")
      .or_else(|_| self.get_parsed_value("EquityWithLoanValue"))
  }

  pub fn list_open_positions(&self) -> Result<Vec<Position>, IBKRError> {
    // read() returns guard directly
    let state = self.account_state.read();
    Ok(state.portfolio.values().filter(|p| p.quantity != 0.0).cloned().collect())
  }

  pub fn get_daily_pnl(&self) -> Result<f64, IBKRError> {
    self.get_parsed_value("DailyPnL")
  }

  pub fn get_unrealized_pnl(&self) -> Result<f64, IBKRError> {
    self.get_parsed_value("UnrealizedPnL")
  }

  pub fn get_realized_pnl(&self) -> Result<f64, IBKRError> {
    self.get_parsed_value("RealizedPnL")
  }

  /// Requests and returns execution details merged with commission data for the current trading day.
  /// It uses a default filter (all executions for the current connection).
  /// See `get_executions_filtered` for using a custom filter.
  pub fn get_day_executions(&self) -> Result<Vec<Execution>, IBKRError> {
    let filter = ExecutionFilter::default(); // Use default filter
    self.get_executions_filtered(&filter)
  }

  /// Requests and returns execution details merged with commission data, based on the provided filter.
  ///
  /// **Note:** This function attempts to merge commission details received from `commissionReport` messages.
  /// Due to the asynchronous nature of TWS messages, commission reports might
  /// arrive *after* this function returns. A short grace period is included after
  /// receiving the end signal for executions, but it's not foolproof. Observers might see
  /// execution updates later when commission data is merged.
  /// Note: This is a blocking call.
  pub fn get_executions_filtered(&self, filter: &ExecutionFilter) -> Result<Vec<Execution>, IBKRError> {
    info!("Requesting executions with filter: {:?}", filter);
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let request_msg = encoder.encode_request_executions(req_id, &filter)?;

    // Prepare state for waiting
    {
      let mut exec_state = self.executions_state.lock();
      if exec_state.contains_key(&req_id) {
        // Should not happen if req_id generation is correct
        return Err(IBKRError::DuplicateRequestId(req_id));
      } // Initialize with empty map
      exec_state.insert(req_id, ExecutionRequestState { waiting: true, executions: HashMap::new(), end_received: false });
    }

    self.message_broker.send_message(&request_msg)?;

    // Wait for execDetailsEnd
    let wait_timeout = Duration::from_secs(20); // Adjust timeout as needed
    let commission_grace_period = Duration::from_secs(2); // Time to wait for commissions after end signal
    let start_time = std::time::Instant::now();
    let final_results: Vec<_>;

    { // Scope for locking and waiting
      let mut exec_state = self.executions_state.lock();
      // Wait until execDetailsEnd is received OR timeout
      while exec_state.get(&req_id).map_or(false, |s| s.waiting && !s.end_received) && start_time.elapsed() < wait_timeout {
        let remaining_timeout = wait_timeout.checked_sub(start_time.elapsed()).unwrap_or(Duration::from_millis(1));
        let timeout_result = self.executions_cond.wait_for(&mut exec_state, remaining_timeout);
        if timeout_result.timed_out() {
          // Check again if the state changed right before timeout
          if exec_state.get(&req_id).map_or(true, |s| s.waiting && !s.end_received) { // Still waiting after timeout
            warn!("Execution request {} timed out waiting for ExecDetailsEnd.", req_id);
            exec_state.remove(&req_id); // Clean up state
            return Err(IBKRError::Timeout(format!("Execution request {} timed out waiting for end signal", req_id)));
          }
        }
      } // End primary wait loop (execDetailsEnd received or timed out)

      // Check if execDetailsEnd was actually received
      if exec_state.get(&req_id).map_or(true, |s| !s.end_received) {
        // This means the main loop timed out before end_received was set
        warn!("Execution request {} loop ended without receiving ExecDetailsEnd.", req_id);
        exec_state.remove(&req_id); // Clean up state
        return Err(IBKRError::Timeout(format!("Execution request {} timed out before end signal", req_id)));
      }

      // *** Start Grace Period Wait for Commissions ***
      info!("ExecDetailsEnd received for {}, waiting grace period ({}ms) for commissions...", req_id, commission_grace_period.as_millis());
      // We don't expect a specific signal here, just wait passively.
      // The Condvar wait will re-acquire the lock if signaled, but we ignore signals here.
      let _grace_result = self.executions_cond.wait_for(&mut exec_state, commission_grace_period);
      info!("Grace period finished for {}.", req_id);


      // Retrieve results after grace period
      if let Some(state) = exec_state.remove(&req_id) {
        // Convert the HashMap values into a Vec
        final_results = state.executions.into_values().collect();
        info!("Execution request {} completed successfully with {} executions (commissions merged where available).", req_id, final_results.len());
        // Log missing commissions
        let missing_commissions = final_results.iter().filter(|e| e.commission.is_none()).count();
        if missing_commissions > 0 {
          warn!("Execution request {}: {} executions are missing commission details after grace period.", req_id, missing_commissions);
        }
      } else {
        // Should not happen if logic is correct, as we checked for end_received above
        error!("Execution request {} state unexpectedly missing after wait.", req_id);
        return Err(IBKRError::InternalError(format!("State missing for execution req {}", req_id)));
      }
    } // Lock released
    Ok(final_results)
  }


  pub fn add_observer<T: AccountObserver + Send + Sync + 'static>(&self, observer: T) -> usize {
    let observer_id = self.next_observer_id.fetch_add(1, Ordering::SeqCst);
    // write() returns guard directly
    let mut observers = self.observers.write();
    // Optional: Check if observers.is_poisoned() if necessary
    observers.insert(observer_id, Box::new(observer));
    debug!("Added account observer with ID: {}", observer_id);
    observer_id
  }

  pub fn remove_observer(&self, observer_id: usize) -> bool {
    // write() returns guard directly
    let mut observers = self.observers.write();
    let removed = observers.remove(&observer_id).is_some();
    if removed {
      debug!("Removed account observer with ID: {}", observer_id);
    } else {
      warn!("Attempted to remove non-existent observer ID: {}", observer_id);
    }
    removed
  }

  pub fn refresh(&self) -> Result<(), IBKRError> {
    info!("Starting full account refresh (summary and positions)...");
    let req_id = self.message_broker.next_request_id();

    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    // Include PnL tags directly in the summary request
    // Request a comprehensive set of tags, including PnL
    // --- SEND EMPTY TAGS ---
    // let tags = "AccountType,NetLiquidation,TotalCashValue,SettledCash,AccruedCash,BuyingPower,EquityWithLoanValue,PreviousEquityWithLoanValue,GrossPositionValue,ReqTEquity,ReqTMargin,SMA,InitMarginReq,MaintMarginReq,AvailableFunds,ExcessLiquidity,Cushion,FullInitMarginReq,FullMaintMarginReq,FullAvailableFunds,FullExcessLiquidity,LookAheadNextChange,LookAheadInitMarginReq,LookAheadMaintMarginReq,LookAheadAvailableFunds,LookAheadExcessLiquidity,HighestSeverity,DayTradesRemaining,Leverage-S,Currency,DailyPnL,UnrealizedPnL,RealizedPnL".to_string();
    // let tags = "AccountType,NetLiquidation,TotalCashValue,BuyingPower,Currency".to_string(); // <-- Simplified tags commented out
    let tags = ""; // <-- SENDING EMPTY TAGS FOR TESTING
    info!("Requesting account summary with EMPTY tags");
    // --- END SEND EMPTY TAGS ---
    let summary_msg = encoder.encode_request_account_summary(req_id, "All", tags)?; // Use empty tags
    // --- ADD RequestPositions ---
    // let position_msg = encoder.encode_request_positions()?; // <-- COMMENTED OUT FOR TESTING

    { // Scope for r_state lock starts
      let mut r_state = self.refresh_state.lock();
      // Check if any refresh operation is already running
      if r_state.waiting_for_summary_end || r_state.waiting_for_position_end {
        return Err(IBKRError::AlreadyRunning("Refresh already in progress".to_string()));
      }
      r_state.summary_request_id = Some(req_id); // <-- RESTORED FOR TESTING
      // r_state.summary_request_id = None; // Ensure None
      r_state.waiting_for_summary_end = true; // <-- RESTORED FOR TESTING
      // r_state.waiting_for_summary_end = false; // Ensure false
      // r_state.waiting_for_position_end = true; // <-- COMMENTED OUT FOR TESTING
      r_state.waiting_for_position_end = false; // Ensure false
    } // Lock released briefly

    self.message_broker.send_message(&summary_msg)?; // <-- RESTORED FOR TESTING
    // --- REMOVE DELAY ---
    // let delay_ms = 200;
    // debug!("Waiting {}ms before sending position request...", delay_ms);
    // std::thread::sleep(Duration::from_millis(delay_ms));
    // --- END REMOVE DELAY ---
    // --- ADD send for position_msg ---
    // self.message_broker.send_message(&position_msg)?; // <-- COMMENTED OUT FOR TESTING

    let wait_timeout = Duration::from_secs(15);
    let start_time = std::time::Instant::now();
    let mut cancel_req_id = None;

    { // Scope for waiting lock starts
      // Re-lock state for waiting
      let mut r_state = self.refresh_state.lock(); // Lock acquired

      // Wait until summary is complete, or timeout (Position wait commented out)
      while r_state.waiting_for_summary_end // Check (Position check removed)
        && start_time.elapsed() < wait_timeout { // Check
          let remaining_timeout = wait_timeout.checked_sub(start_time.elapsed()).unwrap_or(Duration::from_millis(1));
          let timeout_result = self.refresh_cond.wait_for(&mut r_state, remaining_timeout); // Wait

          if timeout_result.timed_out() { // Check timeout
            // Check which operations timed out
            if r_state.waiting_for_summary_end { // Check
              warn!("Account refresh timed out waiting for SummaryEnd.");
              cancel_req_id = r_state.summary_request_id; // Access
            }
            // Position timeout check removed
            // if r_state.waiting_for_position_end { // Check
            //   warn!("Account refresh timed out waiting for PositionEnd.");
            // }

            // Reset state *after* logging and storing cancel ID
            r_state.waiting_for_summary_end = false; // Access
            r_state.waiting_for_position_end = false; // Ensure false
            r_state.summary_request_id = None; // Access
            drop(r_state); // Drop lock

            // Cancel summary request if needed
            if let Some(id) = cancel_req_id {
              match encoder.encode_cancel_account_summary(id) {
                Ok(cancel_msg) => { let _ = self.message_broker.send_message(&cancel_msg); },
                Err(e) => error!("Failed to encode cancel summary msg: {:?}", e),
              }
            }

            // --- ADD CancelPositions ---
            // match encoder.encode_cancel_positions() { // <-- COMMENTED OUT FOR TESTING
            //   Ok(cancel_msg) => { let _ = self.message_broker.send_message(&cancel_msg); },
            //   Err(e) => error!("Failed to encode cancel positions msg: {:?}", e),
            // }

            return Err(IBKRError::Timeout("Account refresh timed out".to_string()));
          } // End if timeout_result.timed_out()
          // No else block needed here - if not timed out, loop continues
          // This debug message executes ONLY IF NOT TIMED OUT
          debug!("Refresh wait notified. Waiting flags: Summary={}", r_state.waiting_for_summary_end); // Position flag removed
        } // End while loop


      // After loop, check final state
      // This block runs if the loop terminated because the condition became false (refresh completed)
      if r_state.waiting_for_summary_end { // Position check removed
        // This case means loop ended, but flags somehow still set (e.g., timeout occurred *after* last check but before this block)
        warn!("Refresh loop finished but flags not cleared (Summary={}). Resetting.", r_state.waiting_for_summary_end); // Position flag removed
        r_state.waiting_for_summary_end = false;
        r_state.waiting_for_position_end = false; // Ensure false
        if start_time.elapsed() >= wait_timeout {
          return Err(IBKRError::Timeout("Account refresh timed out (consistency check)".to_string()));
        } else {
          // This path is less likely if timeout didn't happen, maybe a race?
          return Err(IBKRError::InternalError("Refresh state inconsistency after wait".to_string()));
        }
      } else {
        info!("Account refresh completed successfully (Summary only)."); // Adjusted message
      }
      // Ensure summary_request_id is cleared even on success
      r_state.summary_request_id = None;

    } // Release lock (implicit drop of r_state)

    Ok(())
  } // End refresh function

  pub fn refresh_positions(&self) -> Result<(), IBKRError> {
    info!("Starting position-only refresh...");

    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let position_msg = encoder.encode_request_positions()?;

    {
      let mut r_state = self.refresh_state.lock();
      // Check if any refresh operation is already running
      if r_state.waiting_for_summary_end || r_state.waiting_for_position_end {
        return Err(IBKRError::AlreadyRunning("Refresh operation already in progress".to_string()));
      }
      r_state.waiting_for_position_end = true; // Wait for PositionEnd
      // Summary flags remain false/None
    }

    self.message_broker.send_message(&position_msg)?;

    let wait_timeout = Duration::from_secs(10); // Shorter timeout for positions only?
    let start_time = std::time::Instant::now();

    { // Re-lock state for waiting
      let mut r_state = self.refresh_state.lock(); // Acquired lock

      // Wait until positions are complete, or timeout
      while r_state.waiting_for_position_end && start_time.elapsed() < wait_timeout {
        let remaining_timeout = wait_timeout.checked_sub(start_time.elapsed()).unwrap_or(Duration::from_millis(1));
        let timeout_result = self.refresh_cond.wait_for(&mut r_state, remaining_timeout); // Waits, re-acquires lock

        if timeout_result.timed_out() { // Checks timeout
          if r_state.waiting_for_position_end { // Uses r_state
            warn!("Position refresh timed out waiting for PositionEnd.");
            r_state.waiting_for_position_end = false; // Uses r_state
            drop(r_state); // Drops lock

            // Cancel positions request
            match encoder.encode_cancel_positions() {
              Ok(cancel_msg) => { let _ = self.message_broker.send_message(&cancel_msg); },
              Err(e) => error!("Failed to encode cancel positions msg: {:?}", e),
            }
            return Err(IBKRError::Timeout("Position refresh timed out".to_string())); // Returns
          } else {
            // This branch means timed_out() was true, but waiting_for_position_end was somehow false.
            // This is unexpected but could happen in race conditions. Log it and break.
            debug!("Position refresh wait timed out, but condition met concurrently.");
            break; // Exit loop as the condition we were waiting on is false.
          }
        }
        // If not timed out (i.e., notified), just let the loop continue to check the condition again.
        debug!("Position refresh wait notified. Waiting for PositionEnd? {}", r_state.waiting_for_position_end);
      } // End while loop

      // After loop, check final state
      // This block runs if the loop terminated because the condition became false (refresh completed)
      if r_state.waiting_for_position_end {
        // This case means loop ended, but flag somehow still set (e.g., timeout occurred *after* last check but before this block)
        warn!("Position refresh loop finished but flag not cleared. Resetting.");
        r_state.waiting_for_position_end = false; // Reset flag
        if start_time.elapsed() >= wait_timeout {
          return Err(IBKRError::Timeout("Position refresh timed out (consistency check)".to_string()));
        } else {
          return Err(IBKRError::InternalError("Position refresh state inconsistency after wait".to_string()));
        }
      } else {
        info!("Position refresh completed successfully.");
      }
    } // Release lock (implicit drop)

    Ok(())
  } // End refresh_positions function


  pub fn start_background_updates(&self) -> Result<(), IBKRError> {
    if self.is_subscribed.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
      warn!("Attempted to start background updates when already subscribed.");
      return Ok(()); // Or return an error like AlreadyRunning
    }
    info!("Starting background account updates...");
    // Get account ID safely - use default if empty which might be intended for some setups
    let account_id = self.account_state.read().account_id.clone();
    if account_id.is_empty() {
      warn!("Attempting to start background updates but account ID is not yet known. Using empty account code.");
    }
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    // Subscribe=true, pass account_id (can be empty)
    let msg = encoder.encode_request_account_data(true, &account_id)?;
    self.message_broker.send_message(&msg)

  }

  pub fn stop_background_updates(&self) -> Result<(), IBKRError> {
    if self.is_subscribed.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst).is_err() {
      warn!("Attempted to stop background updates when not subscribed.");
      return Ok(()); // Or return an error like NotRunning
    }
    info!("Stopping background account updates...");
    // Get account ID safely - use default if empty
    let account_id = self.account_state.read().account_id.clone();
    if account_id.is_empty() {
      warn!("Attempting to stop background updates but account ID is not known. Using empty account code.");
    }
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    // Subscribe=false, pass account_id (can be empty)
    let msg = encoder.encode_request_account_data(false, &account_id)?;
    self.message_broker.send_message(&msg)

  }


  // --- Observer Notification Helpers ---
  fn notify_account_update(&self, info: &AccountInfo) {
    // read() returns guard directly
    let observers = self.observers.read();
    for observer in observers.values() {
      observer.on_account_update(info);
    }
  }

  fn notify_position_update(&self, position: &Position) {
    // read() returns guard directly
    let observers = self.observers.read();
    for observer in observers.values() {
      observer.on_position_update(position);
    }
  }

  // This notifies with the Execution object, which might be updated with commission later
  fn notify_execution(&self, execution: &Execution) {
    // read() returns guard directly
    let observers = self.observers.read();
    for observer in observers.values() {
      observer.on_execution(execution);
    }
  }

  fn check_and_notify_account_update(&self) {
    match self.get_account_info() {
      Ok(info) => self.notify_account_update(&info),
      Err(e) => {
        // read() returns guard directly
        if self.account_state.read().values.is_empty() || self.account_state.read().last_updated.is_none() {
          debug!("Account state not ready for notification: {:?}", e);
        } else {
          error!("Failed to create AccountInfo for notification after update: {:?}", e);
        }
      }
    }
  }
}

// --- Implement AccountHandler Trait ---
impl AccountHandler for AccountManager {
  fn account_value(&self, key: &str, value: &str, currency: Option<&str>, account_name: &str) {
    debug!("Handler: Account Value: Key={}, Value={}, Currency={:?}, Account={}", key, value, currency, account_name);
    let updated_value = AccountValue {
      key: key.to_string(),
      value: value.to_string(),
      currency: currency.map(|s| s.to_string()),
      account_id: account_name.to_string(),
    };

    // write() returns guard directly
    let mut state = self.account_state.write();
    // Optional: Check if state.is_poisoned()

    if state.account_id.is_empty() {
      state.account_id = account_name.to_string();
      info!("Account ID set to: {}", account_name); // Log when account ID is first set
    } else if state.account_id != account_name {
      // If we are subscribed to a specific account, ignore messages from others.
      // If subscribed to "All" (empty account code), accept any account ID.
      // This check might need refinement depending on how "All" subscriptions work with multiple accounts.
      if !state.account_id.is_empty() { // Only warn if we expected a specific account
        warn!("Received account value for unexpected account ID: {} (expected {})", account_name, state.account_id);
        return;
      } else {
        // If our state.account_id was empty, but we received a message for a specific account,
        // update our state's account_id. This handles the case where ManagedAccounts hasn't arrived yet.
        state.account_id = account_name.to_string();
        info!("Account ID updated to: {}", account_name);
      }
    }
    state.values.insert(key.to_string(), updated_value);
    state.last_updated = Some(Utc::now());
    drop(state); // Release lock

    self.check_and_notify_account_update();
  }

  fn portfolio_value(&self, contract: &Contract, quantity: f64, market_price: f64, market_value: f64, average_cost: f64, unrealized_pnl: f64, realized_pnl: f64, account_name: &str) {
    let position_key = contract.con_id.to_string();
    debug!("Handler: Portfolio Value: Account={}, ConID={}, Symbol={}, Qty={}, MktPx={}, MktVal={}, AvgCost={}, UnPNL={}, RealPNL={}",
           account_name, contract.con_id, contract.symbol, quantity, market_price, market_value, average_cost, unrealized_pnl, realized_pnl);

    let updated_at = Utc::now();
    let mut state = self.account_state.write(); // Get write guard

    if state.account_id.is_empty() { state.account_id = account_name.to_string(); }
    else if !state.account_id.is_empty() && state.account_id != account_name { // Ignore if not the expected account
      warn!("Received portfolio value for unexpected account ID: {} (expected {})", account_name, state.account_id);
      return;
    }

    let updated_position = Position { symbol: contract.symbol.clone(), contract: contract.clone(), quantity, average_cost, market_price, market_value, unrealized_pnl, realized_pnl, updated_at };

    // *** FIX: Structure the update and clone ***
    let position_to_notify: Position;
    { // Inner scope for the mutable borrow from entry()
      let position_entry = state.portfolio.entry(position_key)
        .or_insert_with(|| updated_position.clone()); // Clone only on insert
      *position_entry = updated_position; // Overwrite/update the entry in the map

      // Clone the *data* from the entry *before* further mutable borrows of state
      position_to_notify = position_entry.clone();
    } // position_entry borrow ends here

    // Now we can safely borrow state mutably again
    state.last_updated = Some(updated_at);

    drop(state); // Release lock

    self.notify_position_update(&position_to_notify);
  }

  fn position(&self, account: &str, contract: &Contract, quantity: f64, avg_cost: f64) {
    let position_key = contract.con_id.to_string();
    debug!("Handler: Position: Account={}, ConID={}, Symbol={}, Qty={}, AvgCost={}",
           account, contract.con_id, contract.symbol, quantity, avg_cost);

    let updated_at = Utc::now();
    let mut state = self.account_state.write(); // Get write guard

    if state.account_id.is_empty() { state.account_id = account.to_string(); }
    else if !state.account_id.is_empty() && state.account_id != account { // Ignore if not the expected account
      warn!("Received position for unexpected account ID: {} (expected {})", account, state.account_id);
      return;
    }

    // *** FIX: Apply similar fix structure here ***
    let position_to_notify: Position;
    { // Inner scope for mutable borrow from entry()
      let position_entry = state.portfolio.entry(position_key).or_insert_with(|| {
        Position {
          symbol: contract.symbol.clone(), contract: contract.clone(), quantity,
          average_cost: avg_cost, market_price: 0.0, market_value: 0.0, // Initialize market data
          unrealized_pnl: 0.0, realized_pnl: 0.0, updated_at,
        }
      });

      // Update existing fields from the 'position' message
      position_entry.quantity = quantity;
      position_entry.average_cost = avg_cost;
      // Market price/value/PnL are updated by portfolioValue, not 'position'
      position_entry.updated_at = updated_at; // Update timestamp within entry

      // Clone the data before further mutable borrows of state
      position_to_notify = position_entry.clone();
    } // position_entry borrow ends here


    // Now safely borrow state mutably again
    state.last_updated = Some(updated_at);

    drop(state); // Release lock

    self.notify_position_update(&position_to_notify);
  }

  fn account_update_time(&self, time_stamp: &str) {
    debug!("Handler: Account Update Time: {}", time_stamp);
    // write() returns guard directly
    let mut state = self.account_state.write();
    if let Ok(naive_time) = chrono::NaiveTime::parse_from_str(time_stamp, "%H:%M:%S") {
      let today = Utc::now().date_naive();
      if let Some(datetime_utc) = Utc.from_local_datetime(&today.and_time(naive_time)).single() {
        state.last_updated = Some(datetime_utc);
      } else {
        warn!("Could not form valid DateTime from account update time: {}", time_stamp);
        state.last_updated = Some(Utc::now()); // Fallback to now
      }
    } else {
      warn!("Could not parse account update time: {}", time_stamp);
      state.last_updated = Some(Utc::now()); // Fallback to now
    }
  }

  fn account_download_end(&self, account: &str) {
    debug!("Handler: Account Download End: {}", account);
    // read() returns guard directly
    let state = self.account_state.read();
    let state_account_id = state.account_id.clone();
    // Drop guard early if possible
    drop(state);

    // Usually called after a subscription starts. We might not need to do much here
    // if we rely on refresh() for initial population. Check if account matches.
    if state_account_id == account || state_account_id.is_empty() { // Accept if matches or if state ID is not set yet
      info!("Account download finished for account '{}'", account);
      // Optionally trigger a notification if observers care about this event
      // self.check_and_notify_account_update(); // Probably not needed here, as values would have arrived via account_value/portfolio_value
    } else {
      warn!("Received AccountDownloadEnd for unexpected account: {} (expected {})", account, state_account_id);
    }
  }

  fn managed_accounts(&self, accounts_list: &str) {
    debug!("Handler: Managed Accounts: {}", accounts_list);
    // Potentially store this list if needed, or set the primary account_id if not already set
    if self.account_state.read().account_id.is_empty() {
      let accounts: Vec<&str> = accounts_list.split(',').filter(|s| !s.is_empty()).collect();
      if let Some(first_account) = accounts.first() {
        // Check again after getting write lock
        let mut state = self.account_state.write();
        if state.account_id.is_empty() {
          state.account_id = first_account.to_string();
          info!("Account ID inferred from managed accounts: {}", first_account);
        }
      }
    }
  }

  fn position_end(&self) {
    debug!("Handler: Position End");
    let mut r_state = self.refresh_state.lock();
    // Optional: Check if r_state.is_poisoned()

    if r_state.waiting_for_position_end { // <-- RESTORED FOR TESTING
      info!("PositionEnd received, marking position refresh as complete.");
      r_state.waiting_for_position_end = false;
      // Check if this was the *last* thing we were waiting for (Summary should be false)
      if !r_state.waiting_for_summary_end {
        info!("PositionEnd: All refresh operations complete. Notifying waiter.");
        self.refresh_cond.notify_all();
      } else {
        // This case shouldn't happen in this test scenario
        warn!("PositionEnd: Still waiting for SummaryEnd (unexpected).");
        self.refresh_cond.notify_all(); // Notify anyway
      }
    } else {
      // This can happen normally during background updates if not in a blocking refresh call
      debug!("PositionEnd received but not currently waiting for it (e.g., during background updates or outside a refresh).");
    }
    // debug!("Handler: Position End received (ignored during summary-only test).");
    debug!("Handler: Position End received (ignored during summary-only test with full tags).");
  }

  fn account_summary(&self, req_id: i32, account: &str, tag: &str, value: &str, currency: &str) {
    // Restore processing for summary-only test
    debug!("Handler: Account Summary: ReqId={}, Account={}, Tag={}, Value={}, Currency={}",
           req_id, account, tag, value, currency);
    let currency_opt = if currency.is_empty() { None } else { Some(currency) };
    self.account_value(tag, value, currency_opt, account);
  }

  fn account_summary_end(&self, req_id: i32) {
    // Restore processing for summary-only test
    debug!("Handler: Account Summary End: ReqId={}", req_id);
    // lock() returns guard directly
    let mut r_state = self.refresh_state.lock();

    if r_state.waiting_for_summary_end && r_state.summary_request_id == Some(req_id) {
      info!("AccountSummaryEnd received for request {}, marking summary refresh as complete.", req_id);
      r_state.summary_request_id = None; // Clear the ID
      r_state.waiting_for_summary_end = false;
      // Check if this was the *last* thing we were waiting for (Position check removed)
      // if !r_state.waiting_for_position_end {
      info!("AccountSummaryEnd: Summary refresh complete. Notifying waiter."); // Adjusted message
      self.refresh_cond.notify_all();
      // } else {
      //   info!("AccountSummaryEnd: Still waiting for PositionEnd.");
      // }
    } else {
      warn!("AccountSummaryEnd received for ReqID {} but not waiting for it or ID mismatch (Waiting: {:?}, Expected: {:?})",
            req_id, r_state.waiting_for_summary_end, r_state.summary_request_id);
      // If we received an unexpected summary end, ensure the waiting flag is cleared if the ID matches
      // This handles cases where the refresh might have timed out and cancelled, but the end message still arrived.
      if r_state.summary_request_id == Some(req_id) {
        warn!("Clearing summary wait flag due to unexpected SummaryEnd for matching ReqID {}", req_id);
        r_state.waiting_for_summary_end = false;
        r_state.summary_request_id = None;
        // Check if this completes the overall refresh now (Position check removed)
        // if !r_state.waiting_for_position_end {
        warn!("Notifying waiter after clearing unexpected SummaryEnd.");
        self.refresh_cond.notify_all();
        // }
      }
    }
  }

  fn pnl(&self, _req_id: i32, daily_pnl: f64, unrealized_pnl: Option<f64>, realized_pnl: Option<f64>) {
    debug!("Handler: PnL: Daily={}, Unrealized={:?}, Realized={:?}", daily_pnl, unrealized_pnl, realized_pnl);
    let mut state = self.account_state.write(); // Get write guard
    let account_id = state.account_id.clone(); // Clone needed data early
    if account_id.is_empty() {
      warn!("Received PnL update but account ID is not known yet.");
      // Decide whether to drop the update or store it temporarily? For now, drop.
      return;
    }

    // *** FIX: Get base_currency BEFORE the closure ***
    let base_currency = state.values.get("Currency").map(|v| v.value.clone());

    // Closure captures account_id (clone) but borrows state mutably inside
    let mut update_val = |key: &str, val: f64, curr: Option<String>| {
      // This borrows `state` mutably when called
      state.values.insert(key.to_string(), AccountValue {
        key: key.to_string(),
        value: val.to_string(),
        currency: curr,
        account_id: account_id.clone(), // Use captured account_id
      });
    };

    // Now call the closure - safe because immutable borrow finished
    update_val("DailyPnL", daily_pnl, base_currency.clone());
    if let Some(pnl) = unrealized_pnl {
      update_val("UnrealizedPnL", pnl, base_currency.clone());
    }
    if let Some(pnl) = realized_pnl {
      update_val("RealizedPnL", pnl, base_currency); // Consumes last base_currency clone
    }

    // Final mutable borrow is fine
    state.last_updated = Some(Utc::now());
    drop(state); // Explicit drop before notification

    self.check_and_notify_account_update();
  }

  fn pnl_single(&self, _req_id: i32, pos_idx: i32, _daily_pnl: f64, _unrealized_pnl: Option<f64>, _realized_pnl: Option<f64>, _value: f64) {
    warn!("Handler: PnLSingle received - updating position by index '{}' is not reliably implemented. Use PortfolioValue updates.", pos_idx);
    // This message provides PnL for a position identified by its index (pos_idx) in the TWS portfolio monitor.
    // Matching this index to a contract ID reliably is difficult without maintaining the exact order TWS uses.
    // It's generally better to rely on portfolioValue messages which include the Contract object.
    // If needed, one could *try* to find the position by iterating `state.portfolio.values()` and comparing `market_value` (value)
    // but this is prone to errors if multiple positions have similar market values.
  }

  // --- ExecutionHandler Implementation ---

  fn execution_details(&self, req_id: i32, _contract: &Contract, execution: &Execution) {
    debug!("Handler: Execution Details: ReqID={}, ExecID={}, OrderID={}, Symbol={}, Side={}, Qty={}, Px={}",
           req_id, execution.execution_id, execution.order_id, execution.symbol, execution.side, execution.quantity, execution.price);

    // Store the base execution details, keyed by execution_id
    let mut exec_state = self.executions_state.lock();
    if let Some(state) = exec_state.get_mut(&req_id) {
      if state.waiting {
        // Insert the new execution or update if (somehow) received again
        // We clone here because the commission_report might modify it later
        state.executions.insert(execution.execution_id.clone(), execution.clone());
        // Don't notify observers yet, wait for potential commission report merge
      } else {
        // This can happen if details arrive after timeout/cleanup
        warn!("Received execution details for ReqID {} which is no longer in a waiting state.", req_id);
      }
    } // else: ReqID not found or not waiting, ignore.
  }

  fn execution_details_end(&self, req_id: i32) {
    debug!("Handler: Execution Details End: ReqID={}", req_id);
    let mut exec_state = self.executions_state.lock();
    if let Some(state) = exec_state.get_mut(&req_id) {
      if state.waiting {
        state.end_received = true; // Mark end received
        // DO NOT set state.waiting = false here yet. We wait for the grace period in get_executions_filtered.
        self.executions_cond.notify_all(); // Notify waiter (get_executions_filtered) that end arrived
      } else {
        warn!("Received execution details end for ReqID {} which is no longer in a waiting state.", req_id);
      }
    } // else: ReqID not found
  }

  fn commission_report(&self, exec_id: &str, commission: f64, currency: &str,
                       _yield: Option<f64>, _yield_redemption: Option<f64>) {
    debug!("Handler: Commission Report: ExecID={}, Commission={}, Currency={}",
           exec_id, commission, currency);

    let mut exec_state = self.executions_state.lock();
    let mut updated_execution : Option<Execution> = None;

    // Iterate through *all* active requests to find the matching execution_id
    // This is necessary because commission reports don't have a req_id
    for (_req_id, state) in exec_state.iter_mut() {
      if let Some(execution) = state.executions.get_mut(exec_id) {
        // Found the matching execution, update its commission fields
        execution.commission = Some(commission);
        execution.commission_currency = Some(currency.to_string());
        debug!("Merged commission into Execution: {}", exec_id);
        // Clone the updated execution to notify observers *after* releasing the lock
        updated_execution = Some(execution.clone());
        // We might potentially signal the condition variable here if the grace period wait
        // wants to wake up early, but it's simpler to let the grace period timeout.
        // self.executions_cond.notify_all();
        break; // Assume exec_id is unique across requests for this purpose
      }
    }

    // Drop the lock before notifying observers
    drop(exec_state);

    if let Some(exec) = updated_execution {
      // Notify observers with the *updated* execution data
      self.notify_execution(&exec);
    } else {
      // Commission report arrived but matching execution wasn't found (yet?).
      // This could happen if commission arrives before execDetails.
      // We currently don't store pending commissions, so this report might be lost
      // for the get_daily_executions call if it arrives very early.
      // For long-running observers, this isn't an issue as they get both separately.
      warn!("Received CommissionReport for ExecID {} but no matching execution found in active requests.", exec_id);
    }
  }

  // Implement other multi-account/position methods if needed, similar to single versions
  // fn position_multi(...) { ... }
  // fn position_multi_end(...) { ... }
  // fn account_update_multi(...) { ... }
  // fn account_update_multi_end(...) { ... }

  /// Handles errors related to account data requests.
  fn handle_error(&self, req_id: i32, code: ClientErrorCode, msg: &str) {
    warn!("AccountHandler received error: ReqID={}, Code={:?}, Msg={}", req_id, code, msg);

    // Handle errors related to specific requests (e.g., summary, executions)
    if req_id > 0 {
      // Check if it's for an execution request
      {
        let mut exec_state = self.executions_state.lock();
        if let Some(state) = exec_state.get_mut(&req_id) {
          error!("API Error for execution request {}: Code={:?}, Msg={}", req_id, code, msg);
          // Mark the request as failed and notify waiter
          state.waiting = false; // Stop waiting
          state.end_received = true; // Consider it ended due to error
          // Store error? Maybe not needed if waiter gets IBKRError::ApiError
          self.executions_cond.notify_all();
          // Don't remove state here, let the waiter handle cleanup
          return; // Handled as execution error
        }
      } // Release exec_state lock

      // Check if it's for an account summary request
      {
        let mut r_state = self.refresh_state.lock();
        if r_state.summary_request_id == Some(req_id) {
          error!("API Error for account summary request {}: Code={:?}, Msg={}", req_id, code, msg);
          // Mark the summary part of the refresh as failed
          r_state.waiting_for_summary_end = false; // Stop waiting for summary
          r_state.summary_request_id = None; // Clear the ID
          // Notify the refresh waiter
          self.refresh_cond.notify_all();
          return; // Handled as summary error
        }
      } // Release r_state lock
    }

    // If not matched to a specific request, log as a general account error
    error!("Unhandled Account error: ReqID={}, Code={:?}, Msg={}", req_id, code, msg);
    // TODO: Notify observers of general account errors?
  }
}
