// yatws/src/account_manager.rs

//! Manages account-related data, including account summary, portfolio positions, and Profit & Loss (P&L).
//!
//! The `AccountManager` allows subscribing to continuous updates for account values and positions.
//! It also provides methods to fetch snapshots of this data.
//!
//! # Subscription and Data Flow
//!
//! -   To receive continuous updates, call `subscribe_account_updates()`. This method
//!     performs an initial blocking fetch of account summary and positions. Once complete,
//!     updates will arrive asynchronously.
//! -   Registered [`AccountObserver`]s will be notified of changes to account values
//!     (`on_account_update`), positions (`on_position_update`), and executions (`on_execution`).
//! -   Data can also be fetched on-demand using methods like `get_account_info()`,
//!     `list_open_positions()`, and `get_day_executions()`. These methods typically
//!     ensure that a subscription is active before returning data.
//!
//! # Key Data Structures
//!
//! -   [`AccountInfo`]: A summary of key account metrics (equity, buying power, etc.).
//! -   [`Position`]: Details of a single position in the portfolio.
//! -   [`Execution`]: Information about a single trade execution.
//! -   [`AccountValue`]: A key-value pair representing a single account metric from TWS.
//!
//! # Example: Getting Account Summary and Positions
//!
//! ```no_run
//! use yatws::{IBKRClient, IBKRError};
//!
//! fn main() -> Result<(), IBKRError> {
//!     let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
//!     let acct_mgr = client.account();
//!
//!     // Subscribe to updates (includes initial fetch)
//!     // This is often called implicitly by getter methods if not already subscribed.
//!     // acct_mgr.subscribe_account_updates()?;
//!
//!     // Get account summary
//!     match acct_mgr.get_account_info() {
//!         Ok(info) => println!("Account Info: {:#?}", info),
//!         Err(e) => eprintln!("Error getting account info: {:?}", e),
//!     }
//!
//!     // List open positions
//!     match acct_mgr.list_open_positions() {
//!         Ok(positions) => {
//!             if positions.is_empty() {
//!                 println!("No open positions.");
//!             } else {
//!                 println!("Open Positions:");
//!                 for pos in positions {
//!                     println!("  Symbol: {}, Qty: {}", pos.symbol, pos.quantity);
//!                 }
//!             }
//!         }
//!         Err(e) => eprintln!("Error listing positions: {:?}", e),
//!     }
//!     Ok(())
//! }
//! ```

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


// State for tracking subscription and initial fetch operations
#[derive(Debug, Default)]
struct UpdateState {
  // ID of the active reqAccountSummary request used for continuous updates
  // and cancellation. Also used to identify the initial fetch request.
  current_summary_req_id: Option<i32>,
  // Flags used *only* during the initial blocking fetch in subscribe_account_updates
  waiting_for_initial_summary_end: bool,
  waiting_for_initial_position_end: bool,
  // Flag to prevent concurrent initial fetches
  is_initial_fetch_active: bool,
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
  update_state: Mutex<UpdateState>, // State for subscription and initial fetch
  update_cond: Condvar, // Condvar for initial fetch wait
  executions_state: Mutex<HashMap<i32, ExecutionRequestState>>, // State for execution requests (ReqID -> State)
  executions_cond: Condvar, // Condvar for execution requests
  is_subscribed: AtomicBool, // Track if continuous updates are active
  is_initializing: AtomicBool, // Track if subscribe_account_updates initial fetch is running
  // State for manual position refresh
  manual_position_refresh_waiting: Mutex<bool>,
  manual_position_refresh_cond: Condvar,
}

/// Manages account summary, portfolio positions, P&L, and execution data.
///
/// Accessed via [`IBKRClient::account()`].
///
/// See the [module-level documentation](index.html) for an overview of interaction patterns.
impl AccountManager {
  /// Creates a new `AccountManager`.
  ///
  /// This is typically called internally when an `IBKRClient` is created.
  ///
  /// # Arguments
  /// * `message_broker` - Shared `MessageBroker` for communication with TWS.
  /// * `initial_account_id` - Optional account ID to pre-configure. If `None`,
  ///   the account ID will be inferred from the first account messages received from TWS.
  pub(crate) fn new(message_broker: Arc<MessageBroker>, initial_account_id: Option<String>) -> Arc<Self> {
    let initial_state = AccountState {
      account_id: initial_account_id.unwrap_or_default(),
      ..Default::default()
    };
    Arc::new(AccountManager {
      message_broker,
      account_state: RwLock::new(initial_state),
      observers: RwLock::new(HashMap::new()),
      next_observer_id: AtomicUsize::new(1),
      update_state: Mutex::new(UpdateState::default()),
      update_cond: Condvar::new(),
      executions_state: Mutex::new(HashMap::new()),
      executions_cond: Condvar::new(),
      is_subscribed: AtomicBool::new(false),
      is_initializing: AtomicBool::new(false), // Initialize the new flag
      manual_position_refresh_waiting: Mutex::new(false),
      manual_position_refresh_cond: Condvar::new(),
    })
  }

  /// Ensures that account updates are subscribed and the initial data fetch has completed.
  /// Calls `subscribe_account_updates` if not already subscribed.
  fn ensure_subscribed(&self) -> Result<(), IBKRError> {
    if !self.is_subscribed.load(Ordering::Relaxed) {
      // Attempt to subscribe only if not already subscribed.
      // The subscribe function itself handles race conditions.
      self.subscribe_account_updates()?;
    }
    // After calling subscribe, we assume the initial fetch is done or an error was returned.
    // Check if the state is populated as a final verification.
    let state = self.account_state.read();
    if state.last_updated.is_none() || state.values.is_empty() || state.portfolio.is_empty() {
      // This might happen if subscribe finished but data hasn't arrived yet, or if subscribe failed silently.
      // Or if subscribe was called concurrently and the current thread didn't wait.
      // For simplicity, return an error indicating data isn't ready.
      // A more robust solution might involve waiting here, but that adds complexity.
      warn!("ensure_subscribed: Account state still appears empty after subscription attempt.");
      return Err(IBKRError::InternalError("Account state not populated after subscription. Data might be pending or subscription failed.".to_string()));
    }
    Ok(())
  }

  // --- Helper to get specific account value ---

  /// Retrieves a specific account value by its key.
  ///
  /// Account values are key-value pairs received from TWS (e.g., "NetLiquidation", "BuyingPower").
  /// This method provides direct access to the raw `AccountValue` struct.
  ///
  /// Requires an active subscription via `subscribe_account_updates()` or implicit
  /// call by other getter methods.
  ///
  /// # Arguments
  /// * `key` - The TWS tag name for the desired account value.
  ///
  /// # Returns
  /// `Ok(Some(AccountValue))` if the key is found, `Ok(None)` if not, or `Err(IBKRError)`
  /// if not subscribed or other issues occur.
  pub fn get_account_value(&self, key: &str) -> Result<Option<AccountValue>, IBKRError> {
    self.ensure_subscribed()?; // Ensure subscription is active
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

  /// Retrieves a consolidated summary of account information.
  ///
  /// This includes metrics like account ID, type, base currency, equity, buying power,
  /// cash balance, margin requirements, and P&L figures.
  ///
  /// Requires an active subscription to account updates (implicitly calls
  /// `subscribe_account_updates()` if not already subscribed).
  ///
  /// # Returns
  /// An `AccountInfo` struct containing the summary.
  ///
  /// # Errors
  /// Returns `IBKRError` if the subscription fails, data is not yet populated,
  /// or if essential account values are missing or cannot be parsed.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError};
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// let account_info = client.account().get_account_info()?;
  /// println!("Account ID: {}", account_info.account_id);
  /// println!("Equity: {}", account_info.equity);
  /// # Ok(())
  /// # }
  /// ```
  pub fn get_account_info(&self) -> Result<AccountInfo, IBKRError> {
    self.ensure_subscribed()?;
    let state = self.account_state.read();

    // Check again after ensuring subscription, in case it failed silently or data is missing
    if state.last_updated.is_none() || state.values.is_empty() {
      return Err(IBKRError::InternalError("Account state not populated after subscription attempt.".to_string()));
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

  /// Retrieves the current buying power for the account.
  /// Requires an active subscription.
  pub fn get_buying_power(&self) -> Result<f64, IBKRError> {
    self.ensure_subscribed()?;
    self.get_parsed_value("BuyingPower")
  }

  /// Retrieves the total cash balance for the account.
  /// Requires an active subscription.
  pub fn get_cash_balance(&self) -> Result<f64, IBKRError> {
    self.ensure_subscribed()?;
    self.get_parsed_value("TotalCashValue")
  }

  /// Retrieves the net liquidation value (equity) of the account.
  /// Attempts to parse "NetLiquidation" first, then "EquityWithLoanValue" as a fallback.
  /// Requires an active subscription.
  pub fn get_equity(&self) -> Result<f64, IBKRError> {
    self.ensure_subscribed()?;
    self.get_parsed_value("NetLiquidation")
      .or_else(|_| self.get_parsed_value("EquityWithLoanValue"))
  }

  /// Retrieves a list of all open positions in the account's portfolio.
  ///
  /// This method ensures that an account subscription is active and then requests
  /// a fresh snapshot of positions from TWS. It blocks until the position data
  /// is received and updated.
  ///
  /// Positions with zero quantity are filtered out.
  ///
  /// # Returns
  /// A `Vec<Position>` containing all open positions.
  ///
  /// # Errors
  /// Returns `IBKRError` if the subscription fails, the position refresh times out,
  /// or other communication issues occur.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError};
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// let positions = client.account().list_open_positions()?;
  /// for pos in positions {
  ///     println!("Symbol: {}, Quantity: {}, Avg Cost: {}",
  ///              pos.symbol, pos.quantity, pos.average_cost);
  /// }
  /// # Ok(())
  /// # }
  /// ```
  pub fn list_open_positions(&self) -> Result<Vec<Position>, IBKRError> {
    self.ensure_subscribed()?; // Ensure basic account info is likely present

    debug!("Requesting manual position refresh.");
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_request_positions()?;

    // Prepare state for waiting
    let mut waiting_guard = self.manual_position_refresh_waiting.lock();
    if *waiting_guard {
      // Another refresh is already in progress
      warn!("list_open_positions called while another manual refresh is already in progress.");
      return Err(IBKRError::AlreadyRunning("Manual position refresh already in progress".to_string()));
    }
    *waiting_guard = true; // Mark as waiting

    // Send the request *after* marking as waiting but *before* starting the wait loop
    self.message_broker.send_message(&request_msg)?;
    debug!("Manual position request sent. Waiting for PositionEnd...");

    // Wait for position_end to signal completion
    let wait_timeout = Duration::from_secs(15); // Adjust timeout as needed
    let start_time = std::time::Instant::now();

    while *waiting_guard && start_time.elapsed() < wait_timeout {
      let remaining_timeout = wait_timeout.checked_sub(start_time.elapsed()).unwrap_or(Duration::from_millis(1));
      let timeout_result = self.manual_position_refresh_cond.wait_for(&mut waiting_guard, remaining_timeout);
      if timeout_result.timed_out() {
        // Check again if the state changed right before timeout
        if *waiting_guard { // Still waiting after timeout
          warn!("Manual position refresh timed out waiting for PositionEnd.");
          *waiting_guard = false; // Reset waiting flag on timeout
          return Err(IBKRError::Timeout("Manual position refresh timed out".to_string()));
        }
      }
      // If woken up, the loop condition (*waiting_guard) will be checked again.
    }

    // Check if we exited the loop because waiting became false (success)
    if !*waiting_guard {
      debug!("Manual position refresh completed successfully.");
      // Drop the lock explicitly before reading account state
      drop(waiting_guard);
      // Read the updated positions
      let state = self.account_state.read();
      Ok(state.portfolio.values().filter(|p| p.quantity != 0.0).cloned().collect())
    } else {
      // Should not happen if timeout logic is correct, but handle defensively
      error!("Manual position refresh loop ended unexpectedly while still marked as waiting.");
      *waiting_guard = false; // Reset flag
      Err(IBKRError::InternalError("Manual position refresh state inconsistency".to_string()))
    }
  }


  /// Retrieves the daily Profit & Loss (P&L) for the account.
  /// Requires an active subscription.
  pub fn get_daily_pnl(&self) -> Result<f64, IBKRError> {
    self.ensure_subscribed()?;
    self.get_parsed_value("DailyPnL")
  }

  /// Retrieves the unrealized Profit & Loss (P&L) for the account.
  /// Requires an active subscription.
  pub fn get_unrealized_pnl(&self) -> Result<f64, IBKRError> {
    self.ensure_subscribed()?;
    self.get_parsed_value("UnrealizedPnL")
  }

  /// Retrieves the realized Profit & Loss (P&L) for the account.
  /// Requires an active subscription.
  pub fn get_realized_pnl(&self) -> Result<f64, IBKRError> {
    self.ensure_subscribed()?;
    self.get_parsed_value("RealizedPnL")
  }

  /// Requests and returns execution details for the current trading day,
  /// attempting to merge them with commission data.
  ///
  /// This method uses a default [`ExecutionFilter`] which typically requests all
  /// executions for the current client connection. For more specific filtering,
  /// use `get_executions_filtered()`.
  ///
  /// This is a blocking call. It waits for TWS to send all execution details
  /// and then includes a short grace period to allow for commission reports to arrive
  /// and be merged.
  ///
  /// **Note on Commissions:** Due to the asynchronous nature of TWS messages,
  /// commission reports might arrive after this function returns. While a grace
  /// period is included, it's not a guarantee that all commissions will be merged
  /// immediately. Observers registered via `add_observer` might see `Execution`
  /// objects updated later when their corresponding commission data is received.
  ///
  /// This method does *not* require a prior call to `subscribe_account_updates()`.
  ///
  /// # Returns
  /// A `Vec<Execution>` containing the day's executions.
  ///
  /// # Errors
  /// Returns `IBKRError` if the request times out or other communication issues occur.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError};
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// let executions = client.account().get_day_executions()?;
  /// for exec in executions {
  ///     println!("Exec ID: {}, Symbol: {}, Qty: {}, Price: {}, Commission: {:?}",
  ///              exec.execution_id, exec.symbol, exec.quantity, exec.price, exec.commission);
  /// }
  /// # Ok(())
  /// # }
  /// ```
  /// Note: This does NOT require prior subscription via `subscribe_account_updates`.
  pub fn get_day_executions(&self) -> Result<Vec<Execution>, IBKRError> {
    // Execution requests are independent of the account summary/position subscription
    let filter = ExecutionFilter::default(); // Use default filter
    self.get_executions_filtered(&filter)
  }

  /// Requests and returns execution details merged with commission data, based on the provided filter.
  ///
  /// This is a blocking call. It waits for TWS to send all execution details
  /// matching the filter and then includes a short grace period to allow for
  /// commission reports to arrive and be merged.
  ///
  /// See `get_day_executions()` for notes on commission merging.
  /// This method does *not* require a prior call to `subscribe_account_updates()`.
  ///
  /// # Arguments
  /// * `filter` - An [`ExecutionFilter`] specifying criteria for the executions to request
  ///   (e.g., client ID, account code, time, symbol, security type, exchange).
  ///
  /// # Returns
  /// A `Vec<Execution>` containing the filtered executions.
  ///
  /// # Errors
  /// Returns `IBKRError` if the request times out or other communication issues occur.
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


  /// Adds an observer to receive asynchronous notifications about account updates,
  /// position changes, and new executions.
  ///
  /// Observers implement the [`AccountObserver`] trait. Multiple observers can be added.
  ///
  /// # Arguments
  /// * `observer` - A boxed trait object implementing `AccountObserver`.
  ///
  /// # Returns
  /// A unique `usize` ID for the registered observer, which can be used with
  /// `remove_observer()`.
  pub fn add_observer<T: AccountObserver + Send + Sync + 'static>(&self, observer: T) -> usize {
    let observer_id = self.next_observer_id.fetch_add(1, Ordering::SeqCst);
    let mut observers = self.observers.write();
    observers.insert(observer_id, Box::new(observer));
    debug!("Added account observer with ID: {}", observer_id);
    observer_id
  }

  /// Removes a previously registered account observer.
  ///
  /// # Arguments
  /// * `observer_id` - The ID returned by `add_observer()`.
  ///
  /// # Returns
  /// `true` if an observer with the given ID was found and removed, `false` otherwise.
  pub fn remove_observer(&self, observer_id: usize) -> bool {
    let mut observers = self.observers.write();
    let removed = observers.remove(&observer_id).is_some();
    if removed {
      debug!("Removed account observer with ID: {}", observer_id);
    } else {
      warn!("Attempted to remove non-existent observer ID: {}", observer_id);
    }
    removed
  }

  /// Subscribes to continuous updates for account summary and portfolio positions from TWS.
  ///
  /// This method initiates two subscriptions:
  /// 1.  Account Summary: Uses `reqAccountSummary` to get a wide range of account values
  ///     (equity, P&L, margins, etc.) and receive continuous updates.
  /// 2.  Positions: Uses `reqPositions` to get current portfolio positions and receive
  ///     continuous updates.
  ///
  /// **Behavior:**
  /// -   If not already subscribed, this function performs an **initial blocking fetch**
  ///     of both account summary and all positions. It waits until TWS signals the end
  ///     of these initial data bursts (`accountSummaryEnd` and `positionEnd`) or a
  ///     timeout occurs.
  /// -   Once the initial fetch is complete (or if already subscribed), the method returns.
  ///     Account and position data will then continue to be updated asynchronously in the
  ///     background. Registered [`AccountObserver`]s will be notified of these changes.
  /// -   If called while already subscribed, it returns `Ok(())` immediately without
  ///     re-fetching.
  /// -   If called while an initial fetch is already in progress by another thread,
  ///     it returns `Err(IBKRError::AlreadyRunning)`.
  ///
  /// **Note on `reqAccountUpdates`:** This method does *not* use the TWS `reqAccountUpdates`
  /// message. `reqAccountUpdates` is designed for a single subscriber per account and
  /// can conflict if multiple clients (or even multiple managers within one client)
  /// attempt to use it. Instead, this library uses `reqAccountSummary` and `reqPositions`
  /// which are more flexible for multiple subscribers and provide similar continuous updates.
  ///
  /// # Returns
  /// `Ok(())` if the subscription is successful (or already active).
  ///
  /// # Errors
  /// Returns `IBKRError` if:
  /// -   The initial fetch times out.
  /// -   An initial fetch is already in progress by another call.
  /// -   There are issues encoding or sending requests to TWS.
  pub fn subscribe_account_updates(&self) -> Result<(), IBKRError> {
    // Quick check without lock first
    if self.is_subscribed.load(Ordering::Relaxed) {
      debug!("Already subscribed to account updates.");
      return Ok(());
    }

    info!("Attempting to subscribe to account updates and perform initial fetch...");
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let req_id = self.message_broker.next_request_id();
    let initial_fetch_started: bool; // Tracks if *this* thread initiated the fetch
    let mut result = Ok(());
    let mut timed_out = false; // Tracks if the initial fetch wait timed out
    let mut cancel_req_id_on_timeout = None; // Store req_id if timeout occurs

    { // Scope for locking update_state to initiate requests
      let mut u_state = self.update_state.lock();

      // Double-check subscription and initial fetch status *after* acquiring lock
      if self.is_subscribed.load(Ordering::SeqCst) {
        info!("Subscription started concurrently, returning.");
        return Ok(());
      }
      if u_state.is_initial_fetch_active {
        warn!("Initial fetch already in progress, returning.");
        return Err(IBKRError::AlreadyRunning("Initial fetch already in progress".to_string()));
      }

      // Mark initial fetch as active *within the lock*
      u_state.is_initial_fetch_active = true;
      u_state.current_summary_req_id = Some(req_id);
      u_state.waiting_for_initial_summary_end = true;
      u_state.waiting_for_initial_position_end = true;
      initial_fetch_started = true; // Mark that *this* thread initiated the fetch
      self.is_initializing.store(true, Ordering::SeqCst); // Set initializing flag

      // Prepare messages
      // Request a comprehensive set of tags, including PnL
      let tags = "AccountType,NetLiquidation,TotalCashValue,SettledCash,AccruedCash,BuyingPower,EquityWithLoanValue,PreviousEquityWithLoanValue,GrossPositionValue,ReqTEquity,ReqTMargin,SMA,InitMarginReq,MaintMarginReq,AvailableFunds,ExcessLiquidity,Cushion,FullInitMarginReq,FullMaintMarginReq,FullAvailableFunds,FullExcessLiquidity,LookAheadNextChange,LookAheadInitMarginReq,LookAheadMaintMarginReq,LookAheadAvailableFunds,LookAheadExcessLiquidity,HighestSeverity,DayTradesRemaining,Leverage-S,Currency,DailyPnL,UnrealizedPnL,RealizedPnL".to_string();

      // Send summary request (this also implicitly starts continuous updates)
      match encoder.encode_request_account_summary(req_id, "All", &tags) {
        Ok(summary_msg) => {
          if let Err(e) = self.message_broker.send_message(&summary_msg) {
            error!("Failed to send account summary request: {:?}", e);
            u_state.is_initial_fetch_active = false; // Reset state on error
            u_state.current_summary_req_id = None;
            return Err(e);
          }
        },
        Err(e) => {
          error!("Failed to encode account summary request: {:?}", e);
          u_state.is_initial_fetch_active = false; // Reset state on error
          u_state.current_summary_req_id = None;
          return Err(e);
        }
      }

      // Send position request (this also implicitly starts continuous updates)
      match encoder.encode_request_positions() {
        Ok(position_msg) => {
          if let Err(e) = self.message_broker.send_message(&position_msg) {
            error!("Failed to send position request: {:?}", e);
            // Attempt to cancel summary if it was sent
            if let Some(id) = u_state.current_summary_req_id {
              if let Ok(cancel_msg) = encoder.encode_cancel_account_summary(id) {
                let _ = self.message_broker.send_message(&cancel_msg);
              }
            }
            u_state.is_initial_fetch_active = false; // Reset state on error
            u_state.current_summary_req_id = None;
            return Err(e);
          }
        },
        Err(e) => {
          error!("Failed to encode position request: {:?}", e);
          // Attempt to cancel summary if it was sent
          if let Some(id) = u_state.current_summary_req_id {
            if let Ok(cancel_msg) = encoder.encode_cancel_account_summary(id) {
              let _ = self.message_broker.send_message(&cancel_msg);
            }
          }
          u_state.is_initial_fetch_active = false; // Reset state on error
          u_state.current_summary_req_id = None;
          return Err(e);
        }
      }
      info!("Initial account summary (ReqID {}) and position requests sent. Waiting for completion...", req_id);
    } // Release update_state lock before waiting

    // --- Wait for initial fetch completion ---
    if initial_fetch_started {
      let wait_timeout = Duration::from_secs(20); // Increased timeout
      let start_time = std::time::Instant::now();

      { // Scope for waiting lock starts
        let mut u_state = self.update_state.lock(); // Re-acquire lock for waiting

        while (u_state.waiting_for_initial_summary_end || u_state.waiting_for_initial_position_end)
          && start_time.elapsed() < wait_timeout
        {
          let remaining_timeout = wait_timeout.checked_sub(start_time.elapsed()).unwrap_or(Duration::from_millis(1));
          let timeout_result = self.update_cond.wait_for(&mut u_state, remaining_timeout); // Wait

          if timeout_result.timed_out() {
            warn!("Initial account data fetch timed out (SummaryEnd: {}, PositionEnd: {}).",
                  !u_state.waiting_for_initial_summary_end, !u_state.waiting_for_initial_position_end);

            // Store the ID needed for cancellation later, outside the lock
            if u_state.waiting_for_initial_summary_end {
              cancel_req_id_on_timeout = u_state.current_summary_req_id;
            }

            // Set flag and reset state *within the lock*
            timed_out = true;
            u_state.is_initial_fetch_active = false;
            u_state.waiting_for_initial_summary_end = false;
            u_state.waiting_for_initial_position_end = false;
            u_state.current_summary_req_id = None; // Clear ID on timeout

            // Set error result
            result = Err(IBKRError::Timeout("Initial account data fetch timed out".to_string()));
            // No drop(u_state) here! Let the scope handle it.
            break; // Exit wait loop
          }
          // If not timed out, log and continue loop
          debug!("Initial fetch wait notified. Waiting flags: Summary={}, Position={}", u_state.waiting_for_initial_summary_end, u_state.waiting_for_initial_position_end);
        } // End while loop

        // After loop, check final state (still holding lock `u_state`)
        if !timed_out { // Only check consistency if timeout didn't happen inside the loop
          if u_state.waiting_for_initial_summary_end || u_state.waiting_for_initial_position_end {
            // This means loop ended, but flags somehow still set (e.g., spurious wakeup?)
            // Or maybe timeout occurred *after* last check but before this block? Check elapsed time again.
            if start_time.elapsed() >= wait_timeout {
              warn!("Initial fetch loop finished but flags not cleared and timeout elapsed. Resetting.");
              timed_out = true; // Mark as timed out for logic below
              result = Err(IBKRError::Timeout("Initial account fetch timed out (consistency check)".to_string()));
              cancel_req_id_on_timeout = u_state.current_summary_req_id; // Store ID for cancellation
            } else {
              // This path is less likely, maybe a race or spurious wakeup?
              warn!("Initial fetch loop finished but flags not cleared (Summary={}, Position={}). Resetting.", u_state.waiting_for_initial_summary_end, u_state.waiting_for_initial_position_end);
              result = Err(IBKRError::InternalError("Initial fetch state inconsistency after wait".to_string()));
            }
            // Reset state fully on any error discovered here
            u_state.is_initial_fetch_active = false;
            u_state.waiting_for_initial_summary_end = false;
            u_state.waiting_for_initial_position_end = false;
            u_state.is_initial_fetch_active = false;
            u_state.waiting_for_initial_summary_end = false;
            u_state.waiting_for_initial_position_end = false;
            u_state.current_summary_req_id = None; // Clear ID on error
          } else {
            // Success path: Loop finished, flags cleared, no timeout detected
            info!("Initial account data fetch completed successfully.");
            // Mark subscription as fully active *only on success*
            self.is_subscribed.store(true, Ordering::SeqCst);
            u_state.is_initial_fetch_active = false; // Mark fetch as complete
            // Keep current_summary_req_id for potential cancellation later by stop_account_updates
          }
        }
        // If timed_out is true (either from loop or consistency check), ensure state is reset
        if timed_out {
          u_state.is_initial_fetch_active = false;
          u_state.waiting_for_initial_summary_end = false;
          u_state.waiting_for_initial_position_end = false;
          // current_summary_req_id should already be None if timeout occurred,
          // but set it again just in case the consistency check path was taken.
          u_state.current_summary_req_id = None;
        }
      } // Release lock (`u_state`) here

      // --- Clear initializing flag ---
      // Do this *after* releasing the lock and *before* potential notification
      self.is_initializing.store(false, Ordering::SeqCst);

      // --- Send cancellations outside the lock if a timeout occurred ---
      if timed_out {
        if let Some(id) = cancel_req_id_on_timeout {
          match encoder.encode_cancel_account_summary(id) {
            Ok(cancel_msg) => { let _ = self.message_broker.send_message(&cancel_msg); },
            Err(e) => error!("Failed to encode cancel summary msg on timeout: {:?}", e),
          }
        }
        match encoder.encode_cancel_positions() {
          Ok(cancel_msg) => { let _ = self.message_broker.send_message(&cancel_msg); },
          Err(e) => error!("Failed to encode cancel positions msg on timeout: {:?}", e),
        }
      } else if result.is_ok() {
        // --- Notify observers on successful completion ---
        info!("Initial fetch successful, triggering initial account notification.");
        self.check_and_notify_account_update();
      }
    } // End if initial_fetch_started
    log::debug!("Final return.");

    result // Return Ok(()) on success, or Err(IBKRError::Timeout/InternalError) on failure
  } // End subscribe_account_updates function


  /// Stops receiving continuous updates for account summary and portfolio positions.
  ///
  /// This method sends `cancelAccountSummary` and `cancelPositions` messages to TWS
  /// to halt the flow of updates initiated by `subscribe_account_updates()`.
  ///
  /// If not currently subscribed, it returns `Ok(())` immediately.
  ///
  /// # Returns
  /// `Ok(())` if the cancellation requests were sent successfully (or if not subscribed).
  ///
  /// # Errors
  /// Returns `IBKRError` if there are issues encoding or sending the cancellation messages.
  pub fn stop_account_updates(&self) -> Result<(), IBKRError> {
    // Quick check first
    if !self.is_subscribed.load(Ordering::Relaxed) {
      debug!("Not currently subscribed to account updates.");
      return Ok(());
    }

    info!("Attempting to stop account updates...");
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let summary_req_id_to_cancel;

    { // Scope for lock
      let mut u_state = self.update_state.lock();

      // Double check subscription status after lock
      if !self.is_subscribed.load(Ordering::SeqCst) {
        debug!("Subscription stopped concurrently.");
        return Ok(());
      }

      // Clear subscription flag first
      self.is_subscribed.store(false, Ordering::SeqCst);

      // Get the summary request ID to cancel it
      summary_req_id_to_cancel = u_state.current_summary_req_id.take(); // Take ownership and clear from state

      // Reset any lingering initial fetch flags (shouldn't be active, but just in case)
      u_state.is_initial_fetch_active = false;
      u_state.waiting_for_initial_summary_end = false;
      u_state.waiting_for_initial_position_end = false;

    } // Release lock

    // Send cancellation messages outside the lock
    let mut cancel_errors = Vec::new();

    if let Some(req_id) = summary_req_id_to_cancel {
      info!("Sending cancel account summary request (ReqID: {})", req_id);
      match encoder.encode_cancel_account_summary(req_id) {
        Ok(cancel_msg) => {
          if let Err(e) = self.message_broker.send_message(&cancel_msg) {
            error!("Failed to send cancel account summary request: {:?}", e);
            cancel_errors.push(e);
          }
        },
        Err(e) => {
          error!("Failed to encode cancel account summary request: {:?}", e);
          cancel_errors.push(e);
        }
      }
    } else {
      warn!("No active summary request ID found to cancel.");
    }

    info!("Sending cancel positions request");
    match encoder.encode_cancel_positions() {
      Ok(cancel_msg) => {
        if let Err(e) = self.message_broker.send_message(&cancel_msg) {
          error!("Failed to send cancel positions request: {:?}", e);
          cancel_errors.push(e);
        }
      },
      Err(e) => {
        error!("Failed to encode cancel positions request: {:?}", e);
        cancel_errors.push(e);
      }
    }

    if cancel_errors.is_empty() {
      info!("Account updates stopped successfully.");
      Ok(())
    } else {
      // Combine errors? For now, return the first one.
      Err(cancel_errors.remove(0))
    }
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
    // Skip notification if initial fetch is still running to prevent re-entrance
    if self.is_initializing.load(Ordering::Relaxed) {
      debug!("Skipping account notification during initialization.");
      return;
    }

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

    // --- Check for Manual Refresh Completion ---
    { // Scope for manual refresh lock
      let mut waiting_guard = self.manual_position_refresh_waiting.lock();
      if *waiting_guard {
        info!("PositionEnd received, completing manual position refresh.");
        *waiting_guard = false; // Clear the flag
        self.manual_position_refresh_cond.notify_all(); // Notify the waiting list_open_positions call
        // We assume PositionEnd is only for one thing at a time (either initial or manual)
        // If it could be for both, logic would need adjustment.
        return; // Handled as manual refresh completion
      }
      // else: Not waiting for a manual refresh, proceed to check initial fetch.
    } // Release manual refresh lock

    // --- Check for Initial Fetch Completion ---
    { // Scope for initial fetch lock
      let mut u_state = self.update_state.lock();
      if u_state.is_initial_fetch_active && u_state.waiting_for_initial_position_end {
        info!("PositionEnd received during initial fetch, marking positions as complete.");
        u_state.waiting_for_initial_position_end = false;
        // Check if this was the *last* thing we were waiting for during initial fetch
        if !u_state.waiting_for_initial_summary_end {
          info!("PositionEnd: Initial fetch complete. Notifying waiter.");
          self.update_cond.notify_all();
        } else {
          info!("PositionEnd: Still waiting for initial SummaryEnd.");
        }
      } else {
        // Received during continuous updates or when not actively fetching/refreshing
        debug!("PositionEnd received outside of initial fetch or manual refresh.");
      }
    } // Release initial fetch lock
  }

  fn account_summary(&self, req_id: i32, account: &str, tag: &str, value: &str, currency: &str) {
    debug!("Handler: Account Summary: ReqId={}, Account={}, Tag={}, Value={}, Currency={}",
           req_id, account, tag, value, currency);
    let currency_opt = if currency.is_empty() { None } else { Some(currency) };
    self.account_value(tag, value, currency_opt, account);
  }

  fn account_summary_end(&self, req_id: i32) {
    debug!("Handler: Account Summary End: ReqId={}", req_id);
    let mut u_state = self.update_state.lock();

    if u_state.is_initial_fetch_active && u_state.current_summary_req_id == Some(req_id) {
      if u_state.waiting_for_initial_summary_end {
        info!("AccountSummaryEnd received for initial fetch request {}, marking summary as complete.", req_id);
        u_state.waiting_for_initial_summary_end = false;
        // Check if this was the *last* thing we were waiting for during initial fetch
        if !u_state.waiting_for_initial_position_end {
          info!("AccountSummaryEnd: Initial fetch complete. Notifying waiter.");
          self.update_cond.notify_all();
        } else {
          info!("AccountSummaryEnd: Still waiting for initial PositionEnd.");
        }
      } else {
        // SummaryEnd received for the correct req_id, but we weren't waiting for it (e.g., already timed out/completed).
        warn!("AccountSummaryEnd received for initial fetch ReqID {} but not waiting for summary.", req_id);
      }
      // Do NOT clear current_summary_req_id here, it's needed for cancellation.
    } else if u_state.current_summary_req_id == Some(req_id) {
      // Received during continuous updates for the active subscription ID
      debug!("AccountSummaryEnd received for ReqID {} during continuous updates.", req_id);
    } else {
      // Received for an unknown or outdated request ID
      warn!("AccountSummaryEnd received for unexpected ReqID {} (Expected: {:?}, Initial Fetch Active: {}).",
            req_id, u_state.current_summary_req_id, u_state.is_initial_fetch_active);
      // If it matches an ID we *thought* was active but fetch isn't marked active, maybe reset flags?
      if u_state.current_summary_req_id == Some(req_id) && !u_state.is_initial_fetch_active {
        warn!("Clearing lingering wait flags due to unexpected SummaryEnd for matching ReqID {}.", req_id);
        u_state.waiting_for_initial_summary_end = false;
        u_state.waiting_for_initial_position_end = false;
        // Don't notify here as we weren't actively waiting.
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

      // Check if it's for the active account summary/subscription request
      {
        let mut u_state = self.update_state.lock();
        if u_state.current_summary_req_id == Some(req_id) {
          error!("API Error for account subscription/summary request {}: Code={:?}, Msg={}", req_id, code, msg);
          // If this happens during initial fetch, mark it as failed and notify waiter
          if u_state.is_initial_fetch_active {
            u_state.waiting_for_initial_summary_end = false; // Stop waiting
            u_state.waiting_for_initial_position_end = false; // Stop waiting for positions too
            u_state.is_initial_fetch_active = false; // Mark fetch as failed/inactive
            self.update_cond.notify_all(); // Notify the subscribe waiter about the error
          }
          // Regardless of initial fetch, clear the problematic request ID
          u_state.current_summary_req_id = None;
          // Mark as unsubscribed since the request failed
          self.is_subscribed.store(false, Ordering::SeqCst);
          return; // Handled as account subscription error
        }
      } // Release u_state lock
    }

    // If not matched to a specific request, log as a general account error
    error!("Unhandled Account error: ReqID={}, Code={:?}, Msg={}", req_id, code, msg);
    // TODO: Notify observers of general account errors?
  }
}
