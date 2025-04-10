// yatws/src/account_manager.rs

use crate::account::{AccountInfo, AccountState, AccountValue, Position, AccountObserver, Execution};
use crate::base::IBKRError;
use crate::contract::Contract;
use crate::handler::AccountHandler;
use crate::order::OrderSide;
use crate::conn::MessageBroker;
use parking_lot::{RwLock, Mutex, Condvar, WaitTimeoutResult};
use crate::protocol_encoder::Encoder;

use chrono::{DateTime, Utc, TimeZone};
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::sync::{Arc};
use std::sync::atomic::{AtomicUsize, Ordering, AtomicBool};
use std::time::Duration;


// Placeholder for ExecutionFilter
#[derive(Debug, Clone, Default)]
pub struct ExecutionFilter {
  // client_id: Option<i32>,
  // acct_code: Option<String>,
  // time: Option<String>, // Format "yyyymmdd-hh:mm:ss"
  // symbol: Option<String>,
  // sec_type: Option<String>,
  // exchange: Option<String>,
  // side: Option<String>,
}
// --- End Placeholder ---


// State for tracking refresh operations
#[derive(Debug, Default)]
struct RefreshState {
  summary_request_id: Option<i32>,
  waiting_for_summary_end: bool,
  waiting_for_position_end: bool,
  // Add flags for other data if needed (e.g., PnL)
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
    })
  }

  // --- Helper to get specific account value ---
  fn get_account_value(&self, key: &str) -> Result<Option<AccountValue>, IBKRError> {
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
    let parse_int_or_err = |key: &str| -> Result<i32, IBKRError> {
      state.values.get(key)
        .ok_or_else(|| IBKRError::InternalError(format!("Key '{}' not found in account values", key)))?
        .value.parse::<i32>()
        .map_err(|e| IBKRError::ParseError(format!("Failed to parse value for key '{}': {}", key, e)))
    };
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
      equity: parse_or_err("NetLiquidation").or_else(|_| parse_or_err("EquityWithLoanValue"))?,
      buying_power: parse_or_err("BuyingPower")?,
      cash_balance: parse_or_err("TotalCashValue")?,
      day_trades_remaining: parse_int_or_minus_one("DayTradesRemaining"),
      leverage: parse_or_zero("Leverage-S"),
      maintenance_margin: parse_or_err("MaintMarginReq").or_else(|_| parse_or_err("FullMaintMarginReq"))?,
      initial_margin: parse_or_err("InitMarginReq").or_else(|_| parse_or_err("FullInitMarginReq"))?,
      excess_liquidity: parse_or_err("ExcessLiquidity").or_else(|_| parse_or_err("FullExcessLiquidity"))?,
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
    Ok(state.portfolio.values().cloned().collect())
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

  pub fn get_executions(&self, _from_date: Option<DateTime<Utc>>) -> Result<Vec<Execution>, IBKRError> {
    warn!("get_executions is a placeholder - requires request/response handling");
    Ok(vec![])
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
    info!("Starting account refresh...");
    let req_id = self.message_broker.next_request_id();

    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let tags = "AccountType,NetLiquidation,TotalCashValue,SettledCash,AccruedCash,BuyingPower,EquityWithLoanValue,PreviousEquityWithLoanValue,GrossPositionValue,ReqTEquity,ReqTMargin,SMA,InitMarginReq,MaintMarginReq,AvailableFunds,ExcessLiquidity,Cushion,FullInitMarginReq,FullMaintMarginReq,FullAvailableFunds,FullExcessLiquidity,LookAheadNextChange,LookAheadInitMarginReq,LookAheadMaintMarginReq,LookAheadAvailableFunds,LookAheadExcessLiquidity,HighestSeverity,DayTradesRemaining,Leverage-S,Currency,DailyPnL,UnrealizedPnL,RealizedPnL";
    let summary_msg = encoder.encode_request_account_summary(req_id, "All", tags)?;
    let position_msg = encoder.encode_request_positions()?;

    {
      let mut r_state = self.refresh_state.lock();
      if r_state.waiting_for_summary_end || r_state.waiting_for_position_end {
        return Err(IBKRError::AlreadyRunning("Refresh already in progress".to_string()));
      }
      r_state.summary_request_id = Some(req_id);
      r_state.waiting_for_summary_end = true;
      r_state.waiting_for_position_end = true;
    } // Lock dropped

    self.message_broker.send_message(&summary_msg)?;
    self.message_broker.send_message(&position_msg)?;

    let wait_timeout = Duration::from_secs(15);
    let start_time = std::time::Instant::now();
    let mut cancel_req_id = None;

    { // Re-lock state for waiting
      let mut r_state = self.refresh_state.lock();

      // Loop while the condition we are waiting for is true AND the overall timeout hasn't been reached
      while (r_state.waiting_for_summary_end || r_state.waiting_for_position_end) && start_time.elapsed() < wait_timeout {
        let remaining_timeout = wait_timeout.checked_sub(start_time.elapsed()).unwrap_or(Duration::from_millis(1));

        // *** Use wait_for ***
        // This waits for a notification OR the timeout duration
        let timeout_result: WaitTimeoutResult = self.refresh_cond.wait_for(&mut r_state, remaining_timeout);


        // Check if the wait *operation itself* timed out
        if timeout_result.timed_out() {
          // The wait timed out. MUST check the condition again.
          if r_state.waiting_for_summary_end || r_state.waiting_for_position_end {
            warn!("Account refresh timed out waiting for SummaryEnd/PositionEnd.");
            cancel_req_id = r_state.summary_request_id;
            r_state.waiting_for_summary_end = false;
            r_state.waiting_for_position_end = false;
            r_state.summary_request_id = None;
            drop(r_state); // Drop lock before cancelling

            // Cancel requests outside lock
            if let Some(id) = cancel_req_id {
              let cancel_encoder = Encoder::new(server_version);
              match cancel_encoder.encode_cancel_account_summary(id) {
                Ok(cancel_msg) => { let _ = self.message_broker.send_message(&cancel_msg); },
                Err(e) => error!("Failed to encode cancel summary msg: {:?}", e),
              }
            }
            match encoder.encode_cancel_positions() {
              Ok(cancel_msg) => { let _ = self.message_broker.send_message(&cancel_msg); },
              Err(e) => error!("Failed to encode cancel positions msg: {:?}", e),
            }
            return Err(IBKRError::Timeout("Account refresh timed out".to_string()));
          } else {
            // Wait timed out, but condition is false. Break loop.
            debug!("Refresh wait timed out, but condition met concurrently.");
            break;
          }
        }
        // If not timed out, a notification was received. The `while` condition
        // will re-evaluate if we still need to wait.
        debug!("Refresh wait notified. Still waiting? {}", r_state.waiting_for_summary_end || r_state.waiting_for_position_end);
      } // End while loop


      // After loop, check final state
      if r_state.waiting_for_summary_end || r_state.waiting_for_position_end {
        warn!("Refresh loop finished but flags not cleared (potentially indicates timeout was hit exactly as condition changed).");
        // Reset flags regardless
        r_state.waiting_for_summary_end = false;
        r_state.waiting_for_position_end = false;
        r_state.summary_request_id = None;
        // Decide whether to return Error or Ok based on whether timeout actually occurred
        if start_time.elapsed() >= wait_timeout {
          return Err(IBKRError::Timeout("Account refresh timed out (consistency check)".to_string()));
        } else {
          // This path is less likely but indicates an issue if reached without timeout
          return Err(IBKRError::InternalError("Refresh state inconsistency after wait".to_string()));
        }
      } else {
        info!("Account refresh completed successfully.");
        r_state.summary_request_id = None;
      }
    } // Release lock

    Ok(())
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
    } else if state.account_id != account_name {
      warn!("Received account value for unexpected account ID: {} (expected {})", account_name, state.account_id);
      return;
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
    else if state.account_id != account_name { /* warn and return */ return; }

    let updated_position = Position { /* ... create updated data ... */ symbol: contract.symbol.clone(), contract: contract.clone(), quantity, average_cost, market_price, market_value, unrealized_pnl, realized_pnl, updated_at };

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
    else if state.account_id != account { /* warn and return */ return; }

    // *** FIX: Apply similar fix structure here ***
    let position_to_notify: Position;
    { // Inner scope for mutable borrow from entry()
      let position_entry = state.portfolio.entry(position_key).or_insert_with(|| {
        Position {
          symbol: contract.symbol.clone(), contract: contract.clone(), quantity,
          average_cost: avg_cost, market_price: 0.0, market_value: 0.0,
          unrealized_pnl: 0.0, realized_pnl: 0.0, updated_at,
        }
      });

      // Update existing fields
      position_entry.quantity = quantity;
      position_entry.average_cost = avg_cost;
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
        state.last_updated = Some(Utc::now());
      }
    } else {
      warn!("Could not parse account update time: {}", time_stamp);
      state.last_updated = Some(Utc::now());
    }
  }

  fn account_download_end(&self, account: &str) {
    debug!("Handler: Account Download End: {}", account);
    // read() returns guard directly
    let state = self.account_state.read();
    let state_account_id = state.account_id.clone();
    // Drop guard early if possible
    drop(state);

    if state_account_id == account {
      info!("Account download finished for {}", account);
      self.check_and_notify_account_update();
    } else {
      warn!("Received AccountDownloadEnd for unexpected account: {} (expected {})", account, state_account_id);
    }
  }

  fn managed_accounts(&self, accounts_list: &str) {
    debug!("Handler: Managed Accounts: {}", accounts_list);
  }

  fn position_end(&self) {
    debug!("Handler: Position End");
    // lock() returns guard directly
    let mut r_state = self.refresh_state.lock();
    // Optional: Check if r_state.is_poisoned()

    if r_state.waiting_for_position_end {
      info!("PositionEnd received, marking position refresh as complete.");
      r_state.waiting_for_position_end = false;
      if !r_state.waiting_for_summary_end {
        info!("PositionEnd: Both summary and positions complete. Notifying refresh waiter.");
        self.refresh_cond.notify_all();
      } else {
        info!("PositionEnd: Waiting for SummaryEnd.");
      }
    } else {
      debug!("PositionEnd received but not waiting for it.");
    }
  }

  fn account_summary(&self, req_id: i32, account: &str, tag: &str, value: &str, currency: &str) {
    debug!("Handler: Account Summary: ReqId={}, Account={}, Tag={}, Value={}, Currency={}",
           req_id, account, tag, value, currency);
    let currency_opt = if currency.is_empty() { None } else { Some(currency) };
    self.account_value(tag, value, currency_opt, account);
  }

  fn account_summary_end(&self, req_id: i32) {
    debug!("Handler: Account Summary End: ReqId={}", req_id);
    // lock() returns guard directly
    let mut r_state = self.refresh_state.lock();

    if r_state.waiting_for_summary_end && r_state.summary_request_id == Some(req_id) {
      info!("AccountSummaryEnd received for request {}, marking summary refresh as complete.", req_id);
      r_state.waiting_for_summary_end = false;
      if !r_state.waiting_for_position_end {
        info!("AccountSummaryEnd: Both summary and positions complete. Notifying refresh waiter.");
        self.refresh_cond.notify_all();
      } else {
        info!("AccountSummaryEnd: Waiting for PositionEnd.");
      }
    } else {
      warn!("AccountSummaryEnd received for ReqID {} but not waiting for it or ID mismatch (Waiting: {:?}, Expected: {:?})",
            req_id, r_state.waiting_for_summary_end, r_state.summary_request_id);
    }
  }

  fn pnl(&self, _req_id: i32, daily_pnl: f64, unrealized_pnl: Option<f64>, realized_pnl: Option<f64>) {
    debug!("Handler: PnL: Daily={}, Unrealized={:?}, Realized={:?}", daily_pnl, unrealized_pnl, realized_pnl);
    let mut state = self.account_state.write(); // Get write guard
    let account_id = state.account_id.clone(); // Clone needed data early

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

  fn pnl_single(&self, _req_id: i32, pos_idx: i32, daily_pnl: f64, unrealized_pnl: Option<f64>, realized_pnl: Option<f64>, value: f64) {
    warn!("Handler: PnLSingle received - updating position by index '{}' is not reliably implemented. Use PortfolioValue updates.", pos_idx);
  }

  // Implement other multi-account/position methods if needed, similar to single versions
  // fn position_multi(...) { ... }
  // fn position_multi_end(...) { ... }
  // fn account_update_multi(...) { ... }
  // fn account_update_multi_end(...) { ... }
}
