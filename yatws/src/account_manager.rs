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

use crate::account::{AccountInfo, AccountState, AccountValue, Position, AccountObserver, Execution, ExecutionFilter, AccountValueKey}; // Added AccountValueKey
use crate::base::IBKRError;
use crate::contract::Contract;
use crate::handler::AccountHandler;
use crate::conn::MessageBroker;
use crate::protocol_decoder::ClientErrorCode; // Added import
use parking_lot::{RwLock, Mutex, Condvar};
use crate::protocol_encoder::Encoder;
use crate::account_subscription::AccountSubscription;

use crate::parser_client::contains_ignore_ascii_case;
use chrono::{Utc, TimeZone};
use log::{debug, error, info, warn};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicUsize, AtomicU8, Ordering, AtomicBool};
use std::thread::ThreadId;
use std::time::Duration;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubscriptionState {
  Unsubscribed = 0,
  Initializing = 1,
  Stopping = 2,
  Subscribed = 3,
}

impl SubscriptionState {
  #[inline]
  fn from_u8(val: u8) -> Self {
    match val {
      1 => Self::Initializing,
      2 => Self::Stopping,
      3 => Self::Subscribed,
      _ => Self::Unsubscribed,
    }
  }
}

#[derive(Debug, Clone)]
enum PnlSingleFilter {
  Disabled,
  All,
  ConIds(Arc<HashSet<i32>>),
}

impl PnlSingleFilter {
  #[inline]
  fn matches(&self, con_id: i32) -> bool {
    match self {
      Self::Disabled => false,
      Self::All => true,
      Self::ConIds(set) => set.contains(&con_id),
    }
  }
}

// State for tracking subscription and initial fetch operations
#[derive(Debug, Default)]
struct UpdateState {
  // ID of the active reqAccountSummary request used for continuous updates
  // and cancellation. Also used to identify the initial fetch request.
  current_summary_req_id: Option<i32>,
  // True once `reqAccountSummary` has actually been sent on the wire
  summary_sent: bool,
  // ID of the active `reqPnl` (account-level daily PnL) subscription
  current_pnl_req_id: Option<i32>,
  // Last daily PnL req_id rejected by TWS (prevents redundant cancelPnl if rejected mid-send)
  last_rejected_pnl_req_id: Option<i32>,
  // True once `reqPositions` has actually been sent on the wire
  positions_requested: bool,
  // True if TWS explicitly rejected `reqPositions` during the current initial fetch
  position_rejected: bool,
  // Flags used *only* during the initial blocking fetch in subscribe_account_updates
  waiting_for_initial_summary_end: bool,
  waiting_for_initial_position_end: bool,
  // Flag to prevent concurrent initial fetches (owned exclusively by InitialFetchGuard)
  is_initial_fetch_active: bool,
  // Thread owning the active initial fetch (prevents self-wait in single-threaded callbacks)
  initial_fetch_owner: Option<ThreadId>,
  // Error encountered during the initial blocking fetch (e.g., TWS error 322)
  initial_fetch_error: Option<IBKRError>,
}

#[derive(Debug, Default)]
struct ManualPositionRefreshState {
  waiting: bool,
  error: Option<IBKRError>,
}

/// RAII guard ensuring `subscription_state` and `UpdateState::is_initial_fetch_active`
/// are always reset by the owning thread *after* any cleanup messages have been sent.
struct InitialFetchGuard<'a> {
  mgr: &'a AccountManager,
  armed: bool,
}

impl<'a> InitialFetchGuard<'a> {
  fn new(mgr: &'a AccountManager) -> Self {
    Self { mgr, armed: true }
  }

  fn disarm(&mut self) {
    self.armed = false;
  }
}

impl Drop for InitialFetchGuard<'_> {
  fn drop(&mut self) {
    if self.armed {
      self.mgr.initial_fetch_abort.store(true, Ordering::Release);
      self.mgr.set_subscription_state(SubscriptionState::Stopping);
      self.mgr.cancel_active_pnl_single_subscriptions();
      let mut u_state = self.mgr.update_state.lock();
      u_state.is_initial_fetch_active = false;
      u_state.initial_fetch_owner = None;
      u_state.current_summary_req_id = None;
      u_state.summary_sent = false;
      u_state.current_pnl_req_id = None;
      u_state.last_rejected_pnl_req_id = None;
      u_state.positions_requested = false;
      u_state.position_rejected = false;
      u_state.waiting_for_initial_summary_end = false;
      u_state.waiting_for_initial_position_end = false;
      u_state.initial_fetch_error = None;
      self.mgr.initial_fetch_abort.store(false, Ordering::Release);
      self.mgr.set_subscription_state(SubscriptionState::Unsubscribed);
      self.mgr.update_cond.notify_all();
    }
  }
}

struct StoppingGuard<'a> {
  mgr: &'a AccountManager,
}

impl Drop for StoppingGuard<'_> {
  fn drop(&mut self) {
    let mut u_state = self.mgr.update_state.lock();
    if self.mgr.subscription_state() == SubscriptionState::Stopping {
      self.mgr.set_subscription_state(SubscriptionState::Unsubscribed);
    }
    let _ = &mut *u_state;
    self.mgr.update_cond.notify_all();
  }
}

struct ExecutionRequestGuard<'a> {
  mgr: &'a AccountManager,
  req_id: i32,
  armed: bool,
}

impl Drop for ExecutionRequestGuard<'_> {
  fn drop(&mut self) {
    if self.armed {
      self.mgr.executions_state.lock().remove(&self.req_id);
    }
  }
}

// State for tracking execution requests
#[derive(Debug, Default)]
struct ExecutionRequestState {
  waiting: bool,
  executions: HashMap<String, Execution>, // Store by ExecID for merging
  end_received: bool, // Track if execDetailsEnd arrived
  error: Option<IBKRError>,
}

pub struct AccountManager {
  message_broker: Arc<MessageBroker>,
  account_state: RwLock<AccountState>,
  observers: RwLock<HashMap<usize, Box<dyn AccountObserver + Send + Sync>>>,
  next_observer_id: AtomicUsize,
  update_state: Mutex<UpdateState>, // State for subscription and initial fetch
  update_cond: Condvar, // Condvar for initial fetch / stopping transitions
  executions_state: Mutex<HashMap<i32, ExecutionRequestState>>, // State for execution requests (ReqID -> State)
  executions_cond: Condvar, // Condvar for execution requests
  subscription_state: AtomicU8, // Single atomic SubscriptionState (Unsubscribed/Initializing/Stopping/Subscribed)
  initial_fetch_abort: AtomicBool, // Fast atomic abort flag checked during initial PnLSingle fan-out & sends
  manual_position_refresh_state: Mutex<ManualPositionRefreshState>,
  manual_position_refresh_cond: Condvar,
  pnl_single_subscriptions: RwLock<HashMap<i32, i32>>, // con_id -> pnl_req_id
  pnl_single_req_id_to_con_id: RwLock<HashMap<i32, i32>>, // pnl_req_id -> con_id
  last_rejected_pnl_single_req_id: AtomicI32, // Last PnLSingle req_id rejected by TWS
  pnl_filter: RwLock<PnlSingleFilter>, // Stores the current P&L filter (Disabled / All / ConIds)
  pre_liquidation_warning_received: AtomicBool, // Flag if code 2148 was received this session
}

/// Manages account summary, portfolio positions, P&L, and execution data.
///
/// Accessed via [`IBKRClient::account()`](crate::IBKRClient::account()).
///
/// See the [module-level documentation](index.html) for an overview of interaction patterns.
impl AccountManager {
  #[inline]
  fn subscription_state(&self) -> SubscriptionState {
    SubscriptionState::from_u8(self.subscription_state.load(Ordering::Acquire))
  }

  #[inline]
  fn set_subscription_state(&self, state: SubscriptionState) {
    self.subscription_state.store(state as u8, Ordering::Release);
  }

  #[inline]
  fn is_subscribed(&self) -> bool {
    self.subscription_state() == SubscriptionState::Subscribed
  }

  #[inline]
  #[cfg(test)]
  fn is_initializing(&self) -> bool {
    self.subscription_state() == SubscriptionState::Initializing
  }

  #[inline]
  fn updates_active(&self) -> bool {
    matches!(
      self.subscription_state(),
      SubscriptionState::Initializing | SubscriptionState::Subscribed
    )
  }

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
      subscription_state: AtomicU8::new(SubscriptionState::Unsubscribed as u8),
      initial_fetch_abort: AtomicBool::new(false),
      manual_position_refresh_state: Mutex::new(ManualPositionRefreshState::default()),
      manual_position_refresh_cond: Condvar::new(),
      pnl_single_subscriptions: RwLock::new(HashMap::new()),
      pnl_single_req_id_to_con_id: RwLock::new(HashMap::new()),
      last_rejected_pnl_single_req_id: AtomicI32::new(-1),
      pnl_filter: RwLock::new(PnlSingleFilter::All),
      pre_liquidation_warning_received: AtomicBool::new(false),
    })
  }

  /// Ensures that account updates are subscribed and the initial data fetch has completed.
  /// Calls `subscribe_account_updates` if not already subscribed.
  fn ensure_subscribed(&self) -> Result<(), IBKRError> {
    if !self.is_subscribed() {
      // Attempt to subscribe only if not already subscribed.
      // The subscribe function itself handles race conditions.
      self.subscribe_account_updates()?;
    }
    // After calling subscribe, we assume the initial fetch is done or an error was returned.
    // Check if the state is populated as a final verification.
    let state = self.account_state.read();
    if state.last_updated.is_none() || state.values.is_empty() {
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
  /// * `key` - The `AccountValueKey` enum variant representing the desired account value.
  ///
  /// # Returns
  /// `Ok(Some(AccountValue))` if the key is found, `Ok(None)` if not, or `Err(IBKRError)`
  /// if not subscribed or other issues occur.
  pub fn get_account_value(&self, key: AccountValueKey) -> Result<Option<AccountValue>, IBKRError> {
    self.ensure_subscribed()?; // Ensure subscription is active
    let state = self.account_state.read();
    let key_str = key.to_string(); // Get the string representation from the enum
    Ok(state.values.get(&key_str).cloned())
  }

  /// Internal helper to subscribe to PnL single for a given con_id.
  /// Reserves `(con_id, req_id)` before sending so fast localhost `PnLSingle` snapshots are never dropped,
  /// and verifies ownership + active state post-send so concurrent cancellations cannot be outraced.
  fn _subscribe_pnl_for_position(&self, con_id: i32, account_id: &str) -> Result<(), IBKRError> {
    if !self.updates_active() || self.initial_fetch_abort.load(Ordering::Acquire) {
      debug!("Skipping PnL single subscription for con_id {}: account updates are not active.", con_id);
      return Ok(());
    }
    if !self.pnl_filter.read().matches(con_id) {
      debug!("Skipping PnL single subscription for con_id {}: excluded by pnl_filter.", con_id);
      return Ok(());
    }
    if self.pnl_single_subscriptions.read().contains_key(&con_id) {
      debug!("PnL single already subscribed for con_id: {}", con_id);
      return Ok(());
    }

    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let req_id = self.message_broker.next_request_id();
    let msg = encoder.encode_request_pnl_single(req_id, account_id, "", con_id)?;

    let pos_key = con_id.to_string();
    let pre_filter_allows = self.pnl_filter.read().matches(con_id);
    let pre_is_open = self
      .account_state
      .read()
      .portfolio
      .get(&pos_key)
      .is_some_and(|p| p.quantity != 0.0);
    if !pre_filter_allows || !pre_is_open {
      return Ok(());
    }

    {
      let mut subs = self.pnl_single_subscriptions.write();
      if !self.updates_active() || self.initial_fetch_abort.load(Ordering::Acquire) || subs.contains_key(&con_id) {
        return Ok(());
      }
      subs.insert(con_id, req_id);
      self.pnl_single_req_id_to_con_id.write().insert(req_id, con_id);
    }

    if let Err(e) = self.message_broker.send_message(&msg) {
      let mut subs = self.pnl_single_subscriptions.write();
      if subs.get(&con_id) == Some(&req_id) {
        subs.remove(&con_id);
      }
      drop(subs);
      self.pnl_single_req_id_to_con_id.write().remove(&req_id);
      return Err(e);
    }

    // Post-send check: if state transitioned to Stopping/Unsubscribed, `pnl_filter` excluded `con_id`,
    // position closed (`quantity == 0.0`), or a concurrent `unsubscribe_pnl` / `stop_account_updates`
    // already drained `subs` BEFORE `msg` hit the wire, ensure `cancelPnlSingle(req_id)` is sent now
    // (unless TWS itself already rejected `req_id`).
    let post_filter_allows = self.pnl_filter.read().matches(con_id);
    let post_is_open = self
      .account_state
      .read()
      .portfolio
      .get(&pos_key)
      .is_some_and(|p| p.quantity != 0.0);
    let (should_abort, send_wire_cancel, remove_rev_map) = {
      let mut subs = self.pnl_single_subscriptions.write();
      let ownership_intact = subs.get(&con_id) == Some(&req_id);
      let rejected_by_tws = !ownership_intact && self.last_rejected_pnl_single_req_id.load(Ordering::Acquire) == req_id;
      let aborting = !self.updates_active()
        || self.initial_fetch_abort.load(Ordering::Acquire)
        || !post_filter_allows
        || !post_is_open;
      if aborting || (!ownership_intact && !rejected_by_tws) {
        if ownership_intact {
          subs.remove(&con_id);
        }
        (true, !rejected_by_tws, ownership_intact)
      } else {
        (false, false, false)
      }
    };
    if remove_rev_map {
      self.pnl_single_req_id_to_con_id.write().remove(&req_id);
    }

    if should_abort {
      if send_wire_cancel {
        if let Ok(cancel_msg) = encoder.encode_cancel_pnl_single(req_id) {
          let _ = self.message_broker.send_message(&cancel_msg);
          info!("Cancelled concurrently-aborted PnL single for con_id: {} (req_id: {})", con_id, req_id);
        }
      }
      return Ok(());
    }

    info!("Requested PnL single for con_id: {} with req_id: {}", con_id, req_id);
    Ok(())
  }

  /// Subscribes `PnLSingle` on the caller thread for all currently open positions matching `pnl_filter`.
  fn sync_pnl_single_for_open_positions(&self) -> Result<(), IBKRError> {
    if !self.updates_active() || self.initial_fetch_abort.load(Ordering::Acquire) {
      return Ok(());
    }
    let filter = self.pnl_filter.read().clone();
    if matches!(filter, PnlSingleFilter::Disabled) {
      return Ok(());
    }
    let (account_id, con_ids_to_sub) = {
      let state = self.account_state.read();
      if state.account_id.is_empty() || state.account_id == "All" {
        return Ok(());
      }
      let mut ids = Vec::new();
      for pos in state.portfolio.values() {
        if pos.quantity == 0.0 {
          continue;
        }
        let con_id = pos.contract.con_id;
        if filter.matches(con_id) {
          ids.push(con_id);
        }
      }
      (state.account_id.clone(), ids)
    };

    for con_id in con_ids_to_sub {
      if !self.updates_active() || self.initial_fetch_abort.load(Ordering::Acquire) {
        debug!("Aborting PnL single fan-out early because account subscription is stopping/aborted.");
        break;
      }
      if let Err(e) = self._subscribe_pnl_for_position(con_id, &account_id) {
        warn!("Failed to subscribe initial PnL single for con_id {}: {:?}", con_id, e);
      }
    }
    Ok(())
  }

  /// Batch-drains and cancels all active PnL single subscriptions without altering `pnl_filter`.
  fn cancel_active_pnl_single_subscriptions(&self) {
    let req_ids: Vec<(i32, i32)> = {
      let mut subs = self.pnl_single_subscriptions.write();
      if subs.is_empty() {
        return;
      }
      self.pnl_single_req_id_to_con_id.write().clear();
      subs.drain().collect()
    };

    let Ok(server_version) = self.message_broker.get_server_version() else {
      return;
    };
    let encoder = Encoder::new(server_version);
    for (con_id, req_id) in req_ids {
      match encoder.encode_cancel_pnl_single(req_id) {
        Ok(msg) => {
          if let Err(e) = self.message_broker.send_message(&msg) {
            warn!("Error sending cancel PnL single for con_id {} (req_id {}): {:?}", con_id, req_id, e);
          } else {
            info!("Cancelled PnL single for con_id: {} (req_id: {})", con_id, req_id);
          }
        }
        Err(e) => warn!("Error encoding cancel PnL single for con_id {}: {:?}", con_id, e),
      }
    }
  }

  /// Internal helper to unsubscribe PnL single for a given con_id.
  fn _unsubscribe_pnl_for_position(&self, con_id: i32) -> Result<(), IBKRError> {
    let req_id_opt = self.pnl_single_subscriptions.write().remove(&con_id);

    if let Some(req_id) = req_id_opt {
      self.pnl_single_req_id_to_con_id.write().remove(&req_id);

      let server_version = self.message_broker.get_server_version()?;
      let encoder = Encoder::new(server_version);
      let msg = encoder.encode_cancel_pnl_single(req_id)?;
      self.message_broker.send_message(&msg)?;
      info!("Cancelled PnL single for con_id: {} (req_id: {})", con_id, req_id);
    } else {
      debug!("No active PnL single subscription found to cancel for con_id: {}", con_id);
    }
    Ok(())
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
    self.build_account_info()
  }

  /// Builds an `AccountInfo` snapshot directly from `account_state` without calling `ensure_subscribed()`.
  /// Safe to call from reader-thread handlers (`check_and_notify_account_update`) without risking re-entrancy.
  fn build_account_info(&self) -> Result<AccountInfo, IBKRError> {
    let state = self.account_state.read();

    if state.last_updated.is_none() || state.values.is_empty() {
      return Err(IBKRError::InternalError("Account state not populated after subscription attempt.".to_string()));
    }

    // Helper closures remain the same conceptually, but access `state` guard directly
    let parse_or_zero = |key: &str| -> f64 {
      match state.values.get(key) {
        Some(v) => match v.value.parse::<f64>() {
          Ok(parsed) => parsed,
          Err(_) => {
            warn!("Failed to parse '{}' with value '{}', using 0.0", key, v.value);
            0.0
          }
        },
        None => 0.0 // Silently return 0.0 when key not found
      }
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

    // Populate the expanded AccountInfo struct
    Ok(AccountInfo {
      // --- Core Identifiers ---
      account_id: state.account_id.clone(),
      account_type: get_string_or_default("AccountType"),
      base_currency: get_string_or_default("Currency"),

      // --- Key Balances & Equity ---
      net_liquidation: parse_or_zero("NetLiquidation"),
      total_cash_value: parse_or_zero("TotalCashValue"),
      settled_cash: parse_or_zero("SettledCash"),
      accrued_cash: parse_or_zero("AccruedCash"),
      buying_power: parse_or_zero("BuyingPower"),
      equity_with_loan_value: parse_or_zero("EquityWithLoanValue"),
      previous_equity_with_loan_value: parse_or_zero("PreviousEquityWithLoanValue"),
      gross_position_value: parse_or_zero("GrossPositionValue"),

      // --- Margin Requirements ---
      reg_t_equity: parse_or_zero("ReqTEquity"), // Assuming ReqTEquity maps here
      reg_t_margin: parse_or_zero("ReqTMargin"), // Assuming ReqTMargin maps here
      sma: parse_or_zero("SMA"),
      init_margin_req: parse_or_zero("InitMarginReq"),
      maint_margin_req: parse_or_zero("MaintMarginReq"),
      full_init_margin_req: parse_or_zero("FullInitMarginReq"),
      full_maint_margin_req: parse_or_zero("FullMaintMarginReq"),

      // --- Available Funds & Liquidity ---
      available_funds: parse_or_zero("AvailableFunds"),
      excess_liquidity: parse_or_zero("ExcessLiquidity"),
      cushion: parse_or_zero("Cushion"),
      full_available_funds: parse_or_zero("FullAvailableFunds"),
      full_excess_liquidity: parse_or_zero("FullExcessLiquidity"),

      // --- Look Ahead Margin ---
      look_ahead_next_change: get_string_or_default("LookAheadNextChange"),
      look_ahead_init_margin_req: parse_or_zero("LookAheadInitMarginReq"),
      look_ahead_maint_margin_req: parse_or_zero("LookAheadMaintMarginReq"),
      look_ahead_available_funds: parse_or_zero("LookAheadAvailableFunds"),
      look_ahead_excess_liquidity: parse_or_zero("LookAheadExcessLiquidity"),

      // --- Other ---
      highest_severity: parse_int_or_minus_one("HighestSeverity"),
      day_trades_remaining: parse_int_or_minus_one("DayTradesRemaining"),
      leverage_s: parse_or_zero("Leverage-S"),
      daily_pnl: parse_or_zero("DailyPnL"),
      unrealized_pnl: parse_or_zero("UnrealizedPnL"),
      realized_pnl: parse_or_zero("RealizedPnL"),

      // --- Timestamp ---
      updated_at: state.last_updated.unwrap_or_else(Utc::now),
    })
  }

  /// Requests the list of managed accounts from TWS.
  ///
  /// The result is received asynchronously by the `managed_accounts` handler,
  /// which typically sets the primary account ID if it's not already known.
  /// This is often called automatically on connection, but can be called manually.
  ///
  /// # Returns
  /// `Ok(())` if the request was sent successfully.
  ///
  /// # Errors
  /// Returns `IBKRError` if there are issues encoding or sending the request.
  pub fn request_managed_accounts(&self) -> Result<(), IBKRError> {
    info!("Requesting managed accounts.");
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_request_managed_accounts()?;
    self.message_broker.send_message(&request_msg)?;
    Ok(())
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
  /// Requires an active subscription.
  pub fn get_net_liquidation(&self) -> Result<f64, IBKRError> {
    self.ensure_subscribed()?;
    self.get_parsed_value("NetLiquidation")
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
  fn abort_manual_position_refresh(&self, err: IBKRError) {
    let mut refresh_state = self.manual_position_refresh_state.lock();
    if refresh_state.waiting {
      refresh_state.waiting = false;
      if refresh_state.error.is_none() {
        refresh_state.error = Some(err);
      }
      self.manual_position_refresh_cond.notify_all();
    }
  }

  pub fn list_open_positions(&self) -> Result<Vec<Position>, IBKRError> {
    self.ensure_subscribed()?; // Ensure basic account info is likely present

    debug!("Requesting manual position refresh.");
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_request_positions()?;

    // Prepare state for waiting (drop mutex before sending to avoid holding lock across I/O
    // and to reset `waiting = false` cleanly on send failure).
    {
      let mut refresh_state = self.manual_position_refresh_state.lock();
      if !self.is_subscribed() {
        return Err(IBKRError::InternalError(
          "Account subscription stopped before manual position refresh could start".to_string(),
        ));
      }
      if refresh_state.waiting {
        warn!("list_open_positions called while another manual refresh is already in progress.");
        return Err(IBKRError::AlreadyRunning("Manual position refresh already in progress".to_string()));
      }
      refresh_state.waiting = true;
      refresh_state.error = None;
    }

    if let Err(e) = self.message_broker.send_message(&request_msg) {
      let mut refresh_state = self.manual_position_refresh_state.lock();
      refresh_state.waiting = false;
      refresh_state.error = None;
      self.manual_position_refresh_cond.notify_all();
      return Err(e);
    }

    // If `stop_account_updates()` / `cleanup_requests()` ran concurrently while `reqPositions` was in flight,
    // cancel the stray position request safely under `StoppingGuard` ONLY if fully `Unsubscribed` (never
    // clobbering a concurrent `Initializing` or `Subscribed` session) and abort rather than hanging 15s.
    if !self.is_subscribed() {
      let stopping_guard_opt = {
        let u_state = self.update_state.lock();
        if self.subscription_state() == SubscriptionState::Unsubscribed && !u_state.is_initial_fetch_active {
          self.set_subscription_state(SubscriptionState::Stopping);
          Some(StoppingGuard { mgr: self })
        } else {
          None
        }
      };
      if let Some(_stopping_guard) = stopping_guard_opt {
        if let Ok(cancel_msg) = encoder.encode_cancel_positions() {
          let _ = self.message_broker.send_message(&cancel_msg);
        }
      }
      let err = {
        let mut refresh_state = self.manual_position_refresh_state.lock();
        refresh_state.waiting = false;
        let existing = refresh_state.error.take();
        self.manual_position_refresh_cond.notify_all();
        existing.unwrap_or_else(|| {
          IBKRError::InternalError("Position refresh cancelled because account updates were stopped".to_string())
        })
      };
      return Err(err);
    }
    debug!("Manual position request sent. Waiting for PositionEnd...");

    // Wait for position_end (or position error / cleanup / cancelPositions) to signal completion
    let wait_timeout = Duration::from_secs(15);
    let start_time = std::time::Instant::now();

    {
      let mut refresh_state = self.manual_position_refresh_state.lock();
      loop {
        if let Some(err) = refresh_state.error.take() {
          refresh_state.waiting = false;
          return Err(err);
        }
        if !refresh_state.waiting {
          debug!("Manual position refresh completed successfully.");
          break;
        }
        let elapsed = start_time.elapsed();
        if elapsed >= wait_timeout {
          warn!("Manual position refresh timed out waiting for PositionEnd.");
          refresh_state.waiting = false;
          return Err(IBKRError::Timeout("Manual position refresh timed out".to_string()));
        }
        let remaining_timeout = wait_timeout - elapsed;
        let _ = self.manual_position_refresh_cond.wait_for(&mut refresh_state, remaining_timeout);
      }
    }

    let state = self.account_state.read();
    Ok(state.portfolio.values().filter(|p| p.quantity != 0.0).cloned().collect())
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

  /// Subscribes to daily Profit and Loss (P&L) updates for the account.
  ///
  /// This method sends a `reqPnl` request to TWS, which will start
  /// continuous updates for the P&L. The updates will be delivered
  /// through the `pnl` method in the `AccountHandler` implementation.
  ///
  /// # Returns
  /// `Ok(())` if the subscription request was sent successfully.
  ///
  /// # Errors
  /// Returns `IBKRError` if the underlying account subscription is not established,
  /// if there are issues communicating with TWS, or if essential account information
  pub fn subscribe_account_pnl(&self) -> Result<(), IBKRError> {
    self.ensure_subscribed()?;
    self.send_account_pnl_request()
  }

  /// Internal helper that validates account state, tracks `current_pnl_req_id`, and sends `reqPnl`
  /// without calling `ensure_subscribed()`.
  fn send_account_pnl_request(&self) -> Result<(), IBKRError> {
    let account_id = {
      let state = self.account_state.read();
      if state.last_updated.is_none() || state.values.is_empty() {
        warn!("send_account_pnl_request: Account state still appears empty after subscription attempt.");
        return Err(IBKRError::InternalError(
          "Account state not populated after subscription. Data might be pending or subscription failed.".to_string()
        ));
      }
      if state.account_id.is_empty() || state.account_id == "All" {
        return Err(IBKRError::InternalError(
          "Account ID not available for PnL subscription. Ensure account summary has been received.".to_string()
        ));
      }
      state.account_id.clone()
    };

    {
      let u_state = self.update_state.lock();
      if !self.updates_active()
        || self.initial_fetch_abort.load(Ordering::Acquire)
        || (u_state.is_initial_fetch_active && u_state.initial_fetch_error.is_some())
      {
        return Err(IBKRError::InternalError(
          "Account subscription stopped before daily PnL request could be sent".to_string(),
        ));
      }
      if u_state.current_pnl_req_id.is_some() {
        debug!("Account daily PnL subscription already active.");
        return Ok(());
      }
    }

    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let req_id = self.message_broker.next_request_id();
    let pnl_msg = encoder.encode_request_pnl(req_id, &account_id, "")?;

    // Reserve `current_pnl_req_id` before sending so errors or concurrent stops can identify/cancel it
    {
      let mut u_state = self.update_state.lock();
      if !self.updates_active()
        || self.initial_fetch_abort.load(Ordering::Acquire)
        || (u_state.is_initial_fetch_active && u_state.initial_fetch_error.is_some())
      {
        return Err(IBKRError::InternalError(
          "Account subscription stopped before daily PnL request could be sent".to_string(),
        ));
      }
      if u_state.current_pnl_req_id.is_some() {
        return Ok(());
      }
      u_state.last_rejected_pnl_req_id = None;
      u_state.current_pnl_req_id = Some(req_id);
    }

    if let Err(e) = self.message_broker.send_message(&pnl_msg) {
      let mut u_state = self.update_state.lock();
      if u_state.current_pnl_req_id == Some(req_id) {
        u_state.current_pnl_req_id = None;
      }
      return Err(e);
    }

    // Post-send check: if `stop_account_updates` / `cleanup_requests` / initial-fetch abort raced
    // with `send_message(&pnl_msg)`, ensure `cancelPnl(req_id)` is sent after `reqPnl` hit the wire
    // (unless TWS itself already rejected `req_id`).
    let (should_abort, send_wire_cancel) = {
      let mut u_state = self.update_state.lock();
      let rejected_by_tws = u_state.last_rejected_pnl_req_id == Some(req_id);
      if rejected_by_tws {
        u_state.last_rejected_pnl_req_id = None;
      }
      let ownership_intact = u_state.current_pnl_req_id == Some(req_id);
      let aborting = !self.updates_active()
        || self.initial_fetch_abort.load(Ordering::Acquire)
        || (u_state.is_initial_fetch_active && u_state.initial_fetch_error.is_some());
      if aborting || (!ownership_intact && !rejected_by_tws) {
        if ownership_intact {
          u_state.current_pnl_req_id = None;
        }
        (true, !rejected_by_tws)
      } else {
        (false, false)
      }
    };

    if should_abort {
      if send_wire_cancel {
        if let Ok(cancel_msg) = encoder.encode_cancel_pnl(req_id) {
          let _ = self.message_broker.send_message(&cancel_msg);
        }
      }
      return Err(IBKRError::InternalError(
        "Account PnL subscription cancelled concurrently while sending".to_string(),
      ));
    }

    Ok(())
  }

  /// Subscribes to real-time Profit and Loss (P&L) updates for individual positions.
  ///
  /// When called, this method will:
  /// 1.  Ensure that the basic account subscription is active (to know the account ID and open positions).
  /// 2.  Update the internal P&L filter based on the `con_id_filter` argument.
  /// 3.  Iterate through all currently open positions:
  ///     *   If a position matches the new filter (or if the filter is `None` for all positions)
  ///         and is not already subscribed for P&L single, a `reqPnlSingle` request will be sent.
  ///     *   If a position was previously subscribed for P&L single but no longer matches the new
  ///         filter, its P&L subscription will be cancelled via `cancelPnlSingle`.
  /// 4.  Subsequently, if new positions are opened:
  ///     *   They will automatically be subscribed for P&L single if they match the currently active filter.
  /// 5.  If a subscribed position is closed (quantity becomes zero):
  ///     *   Its P&L single subscription will automatically be cancelled.
  ///
  /// # Arguments
  /// * `con_id_filter`: An `Option<Vec<i32>>`.
  ///   - `None`: Subscribe to P&L for all current and future open positions.
  ///   - `Some(Vec<i32>)`: Subscribe to P&L only for the positions whose contract IDs (`con_id`)
  ///     are in the provided vector. If a `con_id` in the filter does not correspond to a
  ///     currently open position, it will be ignored until such a position opens.
  ///
  /// # Behavior on Multiple Calls
  /// Calling this method multiple times will replace the existing P&L filter. Subscriptions
  /// will be adjusted accordingly (cancelling old ones, starting new ones). Calling with the
  /// same filter will generally be a no-op for existing subscriptions but will ensure any
  /// newly opened positions matching that filter are subscribed.
  ///
  /// # P&L Data
  /// Received P&L data (daily P&L, unrealized P&L, realized P&L, and market value) will update
  /// the corresponding fields in the `Position` objects. This data will then be available
  /// through methods like `list_open_positions()`.
  ///
  /// # Returns
  /// `Ok(())` if the subscription process was initiated successfully.
  ///
  /// # Errors
  /// Returns `IBKRError` if the underlying account subscription is not established,
  /// if there are issues communicating with TWS, or if essential account information
  /// (like Account ID) is not yet available.
  pub fn subscribe_pnl(&self, con_id_filter: Option<Vec<i32>>) -> Result<(), IBKRError> {
    info!("Subscribing to PnL single with filter: {:?}", con_id_filter);
    self.ensure_subscribed()?; // Ensures account_id and initial positions are likely known

    let new_filter = match con_id_filter {
      None => PnlSingleFilter::All,
      Some(v) => PnlSingleFilter::ConIds(Arc::new(v.into_iter().collect::<HashSet<i32>>())),
    };
    *self.pnl_filter.write() = new_filter.clone();

    let (account_id, con_ids_to_subscribe) = {
      let state = self.account_state.read();
      if state.account_id.is_empty() || state.account_id == "All" {
        return Err(IBKRError::InternalError(
          "Account ID not available for PnL subscription. Ensure account summary has been received.".to_string()
        ));
      }
      let to_sub: HashSet<i32> = state
        .portfolio
        .values()
        .filter(|p| p.quantity != 0.0 && new_filter.matches(p.contract.con_id))
        .map(|p| p.contract.con_id)
        .collect();
      (state.account_id.clone(), to_sub)
    };

    let active_subscriptions = self.pnl_single_subscriptions.read().keys().cloned().collect::<HashSet<i32>>();

    // Unsubscribe those that are active but shouldn't be (or are no longer active positions)
    let con_ids_to_unsubscribe = active_subscriptions.iter()
      .filter(|&con_id| !con_ids_to_subscribe.contains(con_id))
      .cloned()
      .collect::<Vec<i32>>();

    for con_id in con_ids_to_unsubscribe {
      if let Err(e) = self._unsubscribe_pnl_for_position(con_id) {
        warn!("Error unsubscribing PnL for con_id {}: {:?}", con_id, e);
      }
    }

    // Subscribe those that should be active but aren't
    for con_id in con_ids_to_subscribe {
      if !active_subscriptions.contains(&con_id) {
        if let Err(e) = self._subscribe_pnl_for_position(con_id, &account_id) {
          warn!("Error subscribing PnL for con_id {}: {:?}", con_id, e);
        }
      }
    }
    info!("PnL single subscription state updated.");
    Ok(())
  }

  /// Cancels all active single-position P&L subscriptions.
  ///
  /// This method iterates through all currently tracked P&L single subscriptions
  /// and sends a `cancelPnlSingle` request for each. It also disables the
  /// internal P&L filter, meaning no new positions will be automatically
  /// subscribed for P&L updates until `subscribe_pnl` is called again.
  ///
  /// # Returns
  /// `Ok(())` if all cancellation requests were sent successfully. Errors encountered
  /// during cancellation of individual subscriptions will be logged as warnings,
  /// but the method will attempt to cancel all.
  ///
  /// # Errors
  /// Can return `IBKRError` if fundamental issues like server version incompatibility occur,
  /// though individual cancellation send errors are typically logged as warnings.
  pub fn unsubscribe_pnl(&self) -> Result<(), IBKRError> {
    info!("Unsubscribing from all PnL single updates.");
    *self.pnl_filter.write() = PnlSingleFilter::Disabled;
    self.cancel_active_pnl_single_subscriptions();
    Ok(())
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

    // Prepare state for waiting; `exec_guard` removes `req_id` on send failure, timeout, error, or unwind
    {
      let mut exec_state = self.executions_state.lock();
      if exec_state.contains_key(&req_id) {
        // Should not happen if req_id generation is correct
        return Err(IBKRError::DuplicateRequestId(req_id));
      } // Initialize with empty map
      exec_state.insert(req_id, ExecutionRequestState { waiting: true, executions: HashMap::new(), end_received: false, error: None });
    }
    let mut exec_guard = ExecutionRequestGuard { mgr: self, req_id, armed: true };

    self.message_broker.send_message(&request_msg)?;

    // Wait for execDetailsEnd
    let wait_timeout = Duration::from_secs(20); // Adjust timeout as needed
    let commission_grace_period = Duration::from_secs(2); // Time to wait for commissions after end signal
    let start_time = std::time::Instant::now();
    let final_results: Vec<_>;

    { // Scope for locking and waiting
      let mut exec_state = self.executions_state.lock();
      // Wait until execDetailsEnd (or error) is received OR timeout
      while exec_state.get(&req_id).map_or(false, |s| s.waiting && !s.end_received && s.error.is_none()) && start_time.elapsed() < wait_timeout {
        let remaining_timeout = wait_timeout.checked_sub(start_time.elapsed()).unwrap_or(Duration::from_millis(1));
        let timeout_result = self.executions_cond.wait_for(&mut exec_state, remaining_timeout);
        if timeout_result.timed_out() {
          // Check again if the state changed right before timeout
          if exec_state.get(&req_id).map_or(true, |s| s.waiting && !s.end_received && s.error.is_none()) { // Still waiting after timeout
            warn!("Execution request {} timed out waiting for ExecDetailsEnd.", req_id);
            exec_state.remove(&req_id); // Clean up state
            exec_guard.armed = false;
            return Err(IBKRError::Timeout(format!("Execution request {} timed out waiting for end signal", req_id)));
          }
        }
      } // End primary wait loop (execDetailsEnd/error received or timed out)

      // Propagate any API or disconnect error immediately without waiting the commission grace period
      if let Some(err) = exec_state.get_mut(&req_id).and_then(|s| s.error.take()) {
        exec_state.remove(&req_id);
        exec_guard.armed = false;
        return Err(err);
      }

      // Check if execDetailsEnd was actually received
      if exec_state.get(&req_id).map_or(true, |s| !s.end_received) {
        // This means the main loop timed out before end_received was set
        warn!("Execution request {} loop ended without receiving ExecDetailsEnd.", req_id);
        exec_state.remove(&req_id); // Clean up state
        exec_guard.armed = false;
        return Err(IBKRError::Timeout(format!("Execution request {} timed out before end signal", req_id)));
      }

      // *** Start Grace Period Wait for Commissions ***
      info!("ExecDetailsEnd received for {}, waiting grace period ({}ms) for commissions...", req_id, commission_grace_period.as_millis());
      // We don't expect a specific signal here, just wait passively.
      // The Condvar wait will re-acquire the lock if signaled (e.g., on disconnect/cleanup).
      let _grace_result = self.executions_cond.wait_for(&mut exec_state, commission_grace_period);
      info!("Grace period finished for {}.", req_id);

      if let Some(err) = exec_state.get_mut(&req_id).and_then(|s| s.error.take()) {
        exec_state.remove(&req_id);
        exec_guard.armed = false;
        return Err(err);
      }

      // Retrieve results after grace period
      if let Some(state) = exec_state.remove(&req_id) {
        exec_guard.armed = false;
        // Convert the HashMap values into a Vec
        final_results = state.executions.into_values().collect();
        info!("Execution request {} completed successfully with {} executions (commissions merged where available).", req_id, final_results.len());
        // Log missing commissions
        let missing_commissions = final_results.iter().filter(|e| e.commission.is_none()).count();
        if missing_commissions > 0 {
          warn!("Execution request {}: {} executions are missing commission details after grace period.", req_id, missing_commissions);
        }
      } else {
        exec_guard.armed = false;
        // Should not happen if logic is correct, as we checked for end_received above
        error!("Execution request {} state unexpectedly missing after wait.", req_id);
        return Err(IBKRError::InternalError(format!("State missing for execution req {}", req_id)));
      }
    } // Lock released
    Ok(final_results)
  }

  /// Creates a new subscription to account events.
  ///
  /// This method ensures that the underlying `AccountManager` is subscribed to TWS
  /// for account updates. It then registers an internal observer to process these
  /// updates into a stream of `AccountEvent`s.
  ///
  /// An initial attempt is made to populate the `last_known_summary` and
  /// `last_known_positions` from the `AccountManager`.
  pub fn create_account_subscription(account_manager: Arc<AccountManager>) -> Result<AccountSubscription, IBKRError> {
    AccountSubscription::new(account_manager)
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
  /// -   TWS rejects the subscription request (e.g., `IBKRError::ApiError`).
  /// -   The initial fetch times out.
  /// -   An initial fetch is already in progress by another call.
  /// -   There are issues encoding or sending requests to TWS.
  pub fn subscribe_account_updates(&self) -> Result<(), IBKRError> {
    self.subscribe_account_updates_with_timeout(Duration::from_secs(20))
  }

  fn subscribe_account_updates_with_timeout(&self, wait_timeout: Duration) -> Result<(), IBKRError> {
    // Quick check without lock first
    if self.is_subscribed() {
      debug!("Already subscribed to account updates.");
      return Ok(());
    }

    let start_time = std::time::Instant::now();

    // Reserve initial-fetch ownership *before* allocating request IDs or encoding messages
    let mut fetch_guard = {
      let mut u_state = self.update_state.lock();

      if u_state.is_initial_fetch_active {
        warn!("Initial fetch already in progress, returning.");
        return Err(IBKRError::AlreadyRunning("Initial fetch already in progress".to_string()));
      }

      // If a concurrent `stop_account_updates()` is mid-cancellation (`Stopping`), wait for it to finish
      // so its request-ID-less `cancelPositions` cannot clobber our new `reqPositions`.
      while self.subscription_state() == SubscriptionState::Stopping {
        let elapsed = start_time.elapsed();
        if elapsed >= wait_timeout {
          return Err(IBKRError::Timeout("Timed out waiting for concurrent stop_account_updates to complete".to_string()));
        }
        let _ = self.update_cond.wait_for(&mut u_state, wait_timeout - elapsed);
      }

      if self.is_subscribed() {
        info!("Subscription started concurrently, returning.");
        return Ok(());
      }
      if u_state.is_initial_fetch_active {
        warn!("Initial fetch already in progress, returning.");
        return Err(IBKRError::AlreadyRunning("Initial fetch already in progress".to_string()));
      }

      u_state.is_initial_fetch_active = true;
      u_state.initial_fetch_owner = Some(std::thread::current().id());
      u_state.current_summary_req_id = None;
      u_state.summary_sent = false;
      u_state.current_pnl_req_id = None;
      u_state.last_rejected_pnl_req_id = None;
      u_state.positions_requested = false;
      u_state.position_rejected = false;
      u_state.waiting_for_initial_summary_end = true;
      u_state.waiting_for_initial_position_end = true;
      u_state.initial_fetch_error = None;
      self.initial_fetch_abort.store(false, Ordering::Release);
      self.set_subscription_state(SubscriptionState::Initializing);
      InitialFetchGuard::new(self)
    };

    info!("Attempting to subscribe to account updates and perform initial fetch...");
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let req_id = self.message_broker.next_request_id();

    let tags = "AccountType,NetLiquidation,TotalCashValue,SettledCash,AccruedCash,BuyingPower,EquityWithLoanValue,PreviousEquityWithLoanValue,GrossPositionValue,ReqTEquity,ReqTMargin,SMA,InitMarginReq,MaintMarginReq,AvailableFunds,ExcessLiquidity,Cushion,FullInitMarginReq,FullMaintMarginReq,FullAvailableFunds,FullExcessLiquidity,LookAheadNextChange,LookAheadInitMarginReq,LookAheadMaintMarginReq,LookAheadAvailableFunds,LookAheadExcessLiquidity,HighestSeverity,DayTradesRemaining,Leverage";
    let summary_msg = encoder.encode_request_account_summary(req_id, "All", tags).map_err(|e| {
      error!("Failed to encode account summary request: {:?}", e);
      e
    })?;
    let position_msg = encoder.encode_request_positions().map_err(|e| {
      error!("Failed to encode position request: {:?}", e);
      e
    })?;

    // Check if an abort (`cleanup_requests` / `stop_account_updates`) arrived before sending summary,
    // and publish `current_summary_req_id` right before putting `summary_msg` on the wire.
    {
      let mut u_state = self.update_state.lock();
      if let Some(err) = u_state.initial_fetch_error.take() {
        return Err(err);
      }
      u_state.current_summary_req_id = Some(req_id);
    }

    // Send summary request (this also implicitly starts continuous updates)
    if let Err(e) = self.message_broker.send_message(&summary_msg) {
      error!("Failed to send account summary request: {:?}", e);
      let mut u_state = self.update_state.lock();
      u_state.current_summary_req_id = None;
      u_state.summary_sent = false;
      drop(u_state);
      self.cleanup_failed_initial_fetch(&encoder, None, None, false);
      return Err(e);
    }

    // Record that summary was sent and check if summary already failed / aborted before sending positions
    {
      let mut u_state = self.update_state.lock();
      if u_state.current_summary_req_id == Some(req_id) {
        u_state.summary_sent = true;
      }
      if let Some(err) = u_state.initial_fetch_error.take() {
        warn!("Initial fetch aborted before sending position request: {:?}", err);
        let cancel_id = if std::mem::replace(&mut u_state.summary_sent, false) {
          u_state.current_summary_req_id.take()
        } else {
          u_state.current_summary_req_id = None;
          None
        };
        let cancel_pnl = u_state.current_pnl_req_id.take();
        drop(u_state);
        self.cleanup_failed_initial_fetch(&encoder, cancel_id, cancel_pnl, false);
        return Err(err);
      }
    }

    // Send position request (this also implicitly starts continuous updates)
    if let Err(e) = self.message_broker.send_message(&position_msg) {
      error!("Failed to send position request: {:?}", e);
      let (cancel_id, cancel_pnl) = {
        let mut u_state = self.update_state.lock();
        u_state.positions_requested = false;
        let id = if std::mem::replace(&mut u_state.summary_sent, false) {
          u_state.current_summary_req_id.take()
        } else {
          u_state.current_summary_req_id = None;
          None
        };
        (id, u_state.current_pnl_req_id.take())
      };
      self.cleanup_failed_initial_fetch(&encoder, cancel_id, cancel_pnl, false);
      return Err(e);
    }

    {
      let mut u_state = self.update_state.lock();
      // Only mark `positions_requested = true` if an asynchronous position error didn't explicitly reject it
      if !u_state.position_rejected {
        u_state.positions_requested = true;
      }
    }
    info!("Initial account summary (ReqID {}) and position requests sent. Waiting for completion...", req_id);

    // --- Wait for initial fetch completion ---
    let result: Result<(), IBKRError>;
    let mut cancel_summary_req_id: Option<i32> = None;
    let mut cancel_pnl_req_id: Option<i32> = None;
    let mut cancel_positions = false;

    {
      let mut u_state = self.update_state.lock();

      loop {
        if let Some(err) = u_state.initial_fetch_error.take() {
          warn!("Initial account data fetch failed with error: {:?}", err);
          cancel_summary_req_id = if std::mem::replace(&mut u_state.summary_sent, false) {
            u_state.current_summary_req_id.take()
          } else {
            u_state.current_summary_req_id = None;
            None
          };
          cancel_pnl_req_id = u_state.current_pnl_req_id.take();
          cancel_positions = std::mem::replace(&mut u_state.positions_requested, false);
          u_state.waiting_for_initial_summary_end = false;
          u_state.waiting_for_initial_position_end = false;
          self.initial_fetch_abort.store(true, Ordering::Release);
          self.set_subscription_state(SubscriptionState::Stopping);
          result = Err(err);
          break;
        }

        if !u_state.waiting_for_initial_summary_end && !u_state.waiting_for_initial_position_end {
          info!("Initial account data fetch completed successfully.");
          result = Ok(());
          break;
        }

        let elapsed = start_time.elapsed();
        if elapsed >= wait_timeout {
          warn!("Initial account data fetch timed out (SummaryEnd: {}, PositionEnd: {}).",
                !u_state.waiting_for_initial_summary_end, !u_state.waiting_for_initial_position_end);
          cancel_summary_req_id = if std::mem::replace(&mut u_state.summary_sent, false) {
            u_state.current_summary_req_id.take()
          } else {
            u_state.current_summary_req_id = None;
            None
          };
          cancel_pnl_req_id = u_state.current_pnl_req_id.take();
          cancel_positions = std::mem::replace(&mut u_state.positions_requested, false);
          u_state.waiting_for_initial_summary_end = false;
          u_state.waiting_for_initial_position_end = false;
          self.initial_fetch_abort.store(true, Ordering::Release);
          self.set_subscription_state(SubscriptionState::Stopping);
          result = Err(IBKRError::Timeout("Initial account data fetch timed out".to_string()));
          break;
        }

        let remaining_timeout = wait_timeout - elapsed;
        let _ = self.update_cond.wait_for(&mut u_state, remaining_timeout);
        debug!("Initial fetch wait notified. Waiting flags: Summary={}, Position={}, Error={}",
               u_state.waiting_for_initial_summary_end,
               u_state.waiting_for_initial_position_end,
               u_state.initial_fetch_error.is_some());
      }
    } // Release lock (`u_state`) here

    if let Err(err) = result {
      self.cleanup_failed_initial_fetch(&encoder, cancel_summary_req_id, cancel_pnl_req_id, cancel_positions);
      return Err(err);
    }

    // --- Subscribe PnL single for open positions on the caller thread (avoids reader-thread throttling) ---
    let _ = self.sync_pnl_single_for_open_positions();

    // --- Send daily PnL request (without premature `Subscribed` state) ---
    if let Err(e) = self.send_account_pnl_request() {
      warn!("Failed to subscribe to daily PnL after initial account fetch: {:?}. Cleaning up so next call can retry.", e);
      let (cancel_id, cancel_pnl, cancel_pos) = {
        let mut u_state = self.update_state.lock();
        let id = if std::mem::replace(&mut u_state.summary_sent, false) {
          u_state.current_summary_req_id.take()
        } else {
          u_state.current_summary_req_id = None;
          None
        };
        (
          id,
          u_state.current_pnl_req_id.take(),
          std::mem::replace(&mut u_state.positions_requested, false),
        )
      };
      self.cleanup_failed_initial_fetch(&encoder, cancel_id, cancel_pnl, cancel_pos);
      return Err(e);
    }
    info!("Daily PnL subscription request sent.");

    // --- Finalize subscription state atomically via single AtomicU8 transition ---
    {
      let mut u_state = self.update_state.lock();
      if let Some(err) = u_state.initial_fetch_error.take() {
        warn!("Initial account fetch aborted concurrently during PnL setup: {:?}", err);
        let cancel_id = if std::mem::replace(&mut u_state.summary_sent, false) {
          u_state.current_summary_req_id.take()
        } else {
          u_state.current_summary_req_id = None;
          None
        };
        let cancel_pnl = u_state.current_pnl_req_id.take();
        let cancel_pos = std::mem::replace(&mut u_state.positions_requested, false);
        drop(u_state);
        self.cleanup_failed_initial_fetch(&encoder, cancel_id, cancel_pnl, cancel_pos);
        return Err(err);
      }
      self.set_subscription_state(SubscriptionState::Subscribed);
      u_state.is_initial_fetch_active = false;
      u_state.initial_fetch_owner = None;
      self.update_cond.notify_all();
    }
    fetch_guard.disarm();

    // Catch any position that arrived during `send_account_pnl_request()` before `Subscribed` became visible
    let _ = self.sync_pnl_single_for_open_positions();

    // --- Notify observers on successful completion ---
    info!("Initial fetch successful, triggering initial account notification.");
    self.check_and_notify_account_update();

    log::debug!("Final return.");
    Ok(())
  } // End subscribe_account_updates function

  /// Transitions state to `Stopping` while cancelling any active summary / daily-PnL / position streams
  /// from a failed initial fetch (`InitialFetchGuard::drop` transitions to `Unsubscribed` afterwards).
  fn cleanup_failed_initial_fetch(
    &self,
    encoder: &Encoder,
    cancel_summary_req_id: Option<i32>,
    cancel_pnl_req_id: Option<i32>,
    cancel_positions: bool,
  ) {
    self.initial_fetch_abort.store(true, Ordering::Release);
    self.set_subscription_state(SubscriptionState::Stopping);

    if let Some(pnl_id) = cancel_pnl_req_id {
      match encoder.encode_cancel_pnl(pnl_id) {
        Ok(cancel_msg) => { let _ = self.message_broker.send_message(&cancel_msg); },
        Err(e) => error!("Failed to encode cancel PnL msg on failed initial fetch: {:?}", e),
      }
    }
    if let Some(id) = cancel_summary_req_id {
      match encoder.encode_cancel_account_summary(id) {
        Ok(cancel_msg) => { let _ = self.message_broker.send_message(&cancel_msg); },
        Err(e) => error!("Failed to encode cancel summary msg on failed initial fetch: {:?}", e),
      }
    }
    if cancel_positions {
      match encoder.encode_cancel_positions() {
        Ok(cancel_msg) => { let _ = self.message_broker.send_message(&cancel_msg); },
        Err(e) => error!("Failed to encode cancel positions msg on failed initial fetch: {:?}", e),
      }
      self.abort_manual_position_refresh(IBKRError::InternalError(
        "Position refresh cancelled because initial account fetch failed".to_string(),
      ));
    }
    self.cancel_active_pnl_single_subscriptions();
  }

  /// Aborts any in-flight `subscribe_account_updates()` and waits (bounded) for the owning thread
  /// to finish sending its cancellations so streams are never cancelled before they are sent.
  fn abort_in_flight_initial_fetch(&self, abort_error: IBKRError) -> bool {
    let mut u_state = self.update_state.lock();
    if !u_state.is_initial_fetch_active {
      return false;
    }
    self.initial_fetch_abort.store(true, Ordering::Release);
    if u_state.initial_fetch_error.is_none() {
      u_state.initial_fetch_error = Some(abort_error);
    }
    u_state.waiting_for_initial_summary_end = false;
    u_state.waiting_for_initial_position_end = false;
    self.update_cond.notify_all();

    if u_state.initial_fetch_owner != Some(std::thread::current().id()) {
      let deadline = std::time::Instant::now() + Duration::from_secs(2);
      while u_state.is_initial_fetch_active {
        let now = std::time::Instant::now();
        if now >= deadline {
          warn!("Timed out waiting for in-flight initial account fetch to finish cleanup.");
          break;
        }
        let _ = self.update_cond.wait_for(&mut u_state, deadline - now);
      }
    }
    true
  }


  /// Stops receiving continuous updates for account summary and portfolio positions.
  ///
  /// This method sends `cancelAccountSummary`, `cancelPositions`, and `cancelPnl` messages to TWS
  /// to halt the flow of updates initiated by `subscribe_account_updates()`.
  ///
  /// If an initial subscription is currently in progress, it aborts that fetch and waits for its cleanup.
  /// If not currently subscribed, it returns `Ok(())` immediately.
  pub fn stop_account_updates(&self) -> Result<(), IBKRError> {
    if self.subscription_state() == SubscriptionState::Unsubscribed {
      debug!("Not currently subscribed to account updates.");
      return Ok(());
    }

    info!("Attempting to stop account updates...");
    let summary_req_id_to_cancel;
    let pnl_req_id_to_cancel;
    let cancel_positions;
    let _stopping_guard;

    {
      let mut u_state = self.update_state.lock();

      // If an initial fetch is in progress, delegate abort + cancellations to the `InitialFetchGuard` owner
      if u_state.is_initial_fetch_active {
        drop(u_state);
        self.abort_in_flight_initial_fetch(IBKRError::InternalError(
          "Account subscription stopped".to_string(),
        ));
        return Ok(());
      }

      // If another thread is currently sending stop cancellations (`Stopping`), wait until it completes
      // so callers (including `cleanup_requests()`) never return or re-subscribe before `cancelPositions` finishes.
      if self.subscription_state() == SubscriptionState::Stopping {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while self.subscription_state() == SubscriptionState::Stopping {
          let now = std::time::Instant::now();
          if now >= deadline {
            warn!("Timed out waiting for concurrent stop_account_updates to finish.");
            break;
          }
          let _ = self.update_cond.wait_for(&mut u_state, deadline - now);
        }
        return Ok(());
      }

      if self.subscription_state() != SubscriptionState::Subscribed {
        debug!("Subscription already stopped concurrently.");
        return Ok(());
      }

      // Transition `Subscribed -> Stopping` so concurrent `subscribe_account_updates()` waits
      // until `cancelPositions` has been sent before starting a new subscription.
      self.set_subscription_state(SubscriptionState::Stopping);
      _stopping_guard = StoppingGuard { mgr: self };

      summary_req_id_to_cancel = if std::mem::replace(&mut u_state.summary_sent, false) {
        u_state.current_summary_req_id.take()
      } else {
        u_state.current_summary_req_id = None;
        None
      };
      pnl_req_id_to_cancel = u_state.current_pnl_req_id.take();
      cancel_positions = std::mem::replace(&mut u_state.positions_requested, false);
      u_state.waiting_for_initial_summary_end = false;
      u_state.waiting_for_initial_position_end = false;
    } // Release lock

    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let mut cancel_errors = Vec::new();

    // Stop all active PnL single subscriptions without clobbering the user's configured `pnl_filter`
    self.cancel_active_pnl_single_subscriptions();

    if let Some(pnl_req_id) = pnl_req_id_to_cancel {
      info!("Sending cancel daily PnL request (ReqID: {})", pnl_req_id);
      match encoder.encode_cancel_pnl(pnl_req_id) {
        Ok(cancel_msg) => {
          if let Err(e) = self.message_broker.send_message(&cancel_msg) {
            error!("Failed to send cancel PnL request: {:?}", e);
            cancel_errors.push(e);
          }
        }
        Err(e) => {
          error!("Failed to encode cancel PnL request: {:?}", e);
          cancel_errors.push(e);
        }
      }
    }

    if let Some(req_id) = summary_req_id_to_cancel {
      info!("Sending cancel account summary request (ReqID: {})", req_id);
      match encoder.encode_cancel_account_summary(req_id) {
        Ok(cancel_msg) => {
          if let Err(e) = self.message_broker.send_message(&cancel_msg) {
            error!("Failed to send cancel account summary request: {:?}", e);
            cancel_errors.push(e);
          }
        }
        Err(e) => {
          error!("Failed to encode cancel account summary request: {:?}", e);
          cancel_errors.push(e);
        }
      }
    } else {
      warn!("No active summary request ID found to cancel.");
    }

    if cancel_positions {
      info!("Sending cancel positions request");
      match encoder.encode_cancel_positions() {
        Ok(cancel_msg) => {
          if let Err(e) = self.message_broker.send_message(&cancel_msg) {
            error!("Failed to send cancel positions request: {:?}", e);
            cancel_errors.push(e);
          }
        }
        Err(e) => {
          error!("Failed to encode cancel positions request: {:?}", e);
          cancel_errors.push(e);
        }
      }
    }
    self.abort_manual_position_refresh(IBKRError::InternalError(
      "Position refresh cancelled because account updates were stopped".to_string(),
    ));

    if cancel_errors.is_empty() {
      info!("Account updates stopped successfully.");
      Ok(())
    } else {
      Err(cancel_errors.remove(0))
    }
  }

  /// Checks if a pre-liquidation warning (TWS error code 2148) has been received
  /// during the current API session.
  ///
  /// This flag indicates that TWS sent the warning message at least once since
  /// the client connected. It does **not** indicate the current liquidation status
  /// of the account, only that the warning condition was met at some point.
  /// The flag is reset when a new `IBKRClient` session is started.
  /// Check the account info cushion field to see how close you are.
  ///
  /// # Returns
  /// `true` if the warning was received in this session, `false` otherwise.
  pub fn has_received_pre_liquidation_warning(&self) -> bool {
    self.pre_liquidation_warning_received.load(Ordering::Relaxed)
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
    // Only notify when fully `Subscribed` and skip building `AccountInfo` entirely when no observers
    // are registered. Use `build_account_info()` (NOT `get_account_info()`) so a concurrent
    // `stop_account_updates()` / `cleanup_requests()` can NEVER cause the reader thread to re-enter
    // `ensure_subscribed()` -> `subscribe_account_updates()`.
    if !self.is_subscribed() || self.observers.read().is_empty() {
      return;
    }

    match self.build_account_info() {
      Ok(info) => {
        if self.is_subscribed() {
          self.notify_account_update(&info);
        }
      }
      Err(e) => {
        let state = self.account_state.read();
        if state.values.is_empty() || state.last_updated.is_none() {
          debug!("Account state not ready for notification: {:?}", e);
        } else {
          error!("Failed to create AccountInfo for notification after update: {:?}", e);
        }
      }
    }
  }

  /// Cleanly shuts down all active subscriptions and resets internal state.
  ///
  /// This method should be called during client disconnect to ensure:
  /// - All active subscriptions are cancelled
  /// - Internal state is reset to initial values
  /// - No pending requests are left hanging
  ///
  /// This method is safe to call multiple times.
  pub(crate) fn cleanup_requests(&self) -> Result<(), IBKRError> {
    info!("AccountManager: Cleaning up all subscriptions and resetting state...");

    let mut cleanup_errors = Vec::new();

    // 1. If an initial fetch is in progress, abort it with `NotConnected` and wait for its owner
    // to send cancellations cleanly (prevents cancelling streams before they are sent).
    self.abort_in_flight_initial_fetch(IBKRError::NotConnected);

    // 2. Stop continuous account updates (summary + daily PnL + positions + PnLSingle) if subscribed
    if let Err(e) = self.stop_account_updates() {
      warn!("Error stopping account updates during cleanup: {:?}", e);
      cleanup_errors.push(e);
    }

    // 3. Ensure subscription flags and PnL single subscriptions are cleared
    self.set_subscription_state(SubscriptionState::Unsubscribed);
    self.initial_fetch_abort.store(false, Ordering::Release);
    self.pre_liquidation_warning_received.store(false, Ordering::Relaxed);
    self.cancel_active_pnl_single_subscriptions();

    // 4. Cancel any lingering PnL / summary / position state if not already cleared
    let (lingering_pnl_id, lingering_summary_id, lingering_pos) = {
      let mut u_state = self.update_state.lock();
      let summary_id = if std::mem::replace(&mut u_state.summary_sent, false) {
        u_state.current_summary_req_id.take()
      } else {
        u_state.current_summary_req_id = None;
        None
      };
      let pnl_id = u_state.current_pnl_req_id.take();
      u_state.last_rejected_pnl_req_id = None;
      let pos = std::mem::replace(&mut u_state.positions_requested, false);
      u_state.waiting_for_initial_summary_end = false;
      u_state.waiting_for_initial_position_end = false;
      self.update_cond.notify_all();
      (pnl_id, summary_id, pos)
    };

    if lingering_pnl_id.is_some() || lingering_summary_id.is_some() || lingering_pos {
      if let Ok(server_version) = self.message_broker.get_server_version() {
        let encoder = Encoder::new(server_version);
        if let Some(pnl_id) = lingering_pnl_id {
          if let Ok(msg) = encoder.encode_cancel_pnl(pnl_id) {
            let _ = self.message_broker.send_message(&msg);
          }
        }
        if let Some(req_id) = lingering_summary_id {
          if let Ok(msg) = encoder.encode_cancel_account_summary(req_id) {
            let _ = self.message_broker.send_message(&msg);
          }
        }
        if lingering_pos {
          if let Ok(msg) = encoder.encode_cancel_positions() {
            let _ = self.message_broker.send_message(&msg);
          }
        }
      }
    }

    // 5. Cancel any pending execution requests
    {
      let mut exec_state = self.executions_state.lock();
      if !exec_state.is_empty() {
        info!("Cleaning up {} pending execution requests", exec_state.len());

        for state in exec_state.values_mut() {
          if state.error.is_none() {
            state.error = Some(IBKRError::NotConnected);
          }
          state.waiting = false;
          state.end_received = true;
        }

        self.executions_cond.notify_all();
      }
    }

    // 6. Abort any waiting manual position refresh with `NotConnected`
    self.abort_manual_position_refresh(IBKRError::NotConnected);

    // 7. Reset PnL filter to initial default (`All`) and clear mappings (lock `pnl_single_subscriptions` first)
    *self.pnl_filter.write() = PnlSingleFilter::All;
    self.last_rejected_pnl_single_req_id.store(-1, Ordering::Release);
    {
      let mut subs = self.pnl_single_subscriptions.write();
      subs.clear();
      self.pnl_single_req_id_to_con_id.write().clear();
    }

    info!("AccountManager cleanup completed. {} errors encountered.", cleanup_errors.len());

    if let Some(first_error) = cleanup_errors.into_iter().next() {
      Err(first_error)
    } else {
      Ok(())
    }
  }

  /// Internal helper shared by `try_handle_request_error` and `handle_error` for `req_id > 0`.
  /// Returns `true` if `req_id` belonged to an active account request.
  fn handle_owned_request_error(&self, req_id: i32, code: ClientErrorCode, msg: &str) -> bool {
    if req_id <= 0 {
      return false;
    }

    // 1. Check execution requests
    {
      let mut exec_state = self.executions_state.lock();
      if let Some(state) = exec_state.get_mut(&req_id) {
        error!("API Error for execution request {}: Code={:?}, Msg={}", req_id, code, msg);
        if state.error.is_none() {
          state.error = Some(IBKRError::ApiError(code as i32, msg.to_string()));
        }
        state.waiting = false;
        state.end_received = true;
        self.executions_cond.notify_all();
        return true;
      }
    }

    // 2. Check active account summary or account-level PnL request
    let mut teardown_post_subscribe_streams = None;
    let mut stopping_guard_opt = None;
    {
      let mut u_state = self.update_state.lock();
      if u_state.current_summary_req_id == Some(req_id) {
        error!("API Error for account subscription/summary request {}: Code={:?}, Msg={}", req_id, code, msg);
        if u_state.is_initial_fetch_active {
          self.initial_fetch_abort.store(true, Ordering::Release);
          if u_state.initial_fetch_error.is_none() {
            u_state.initial_fetch_error = Some(IBKRError::ApiError(code as i32, msg.to_string()));
          }
          u_state.waiting_for_initial_summary_end = false;
          u_state.waiting_for_initial_position_end = false;
          // Leave `SubscriptionState` as `Initializing` until the owner thread enters `Stopping`
          // for wire cancellations and `InitialFetchGuard::drop` sets `Unsubscribed`.
          self.update_cond.notify_all();
        } else if self.is_subscribed() {
          // Summary stream failed after initial subscription succeeded: transition through `Stopping`
          // so concurrent `ensure_subscribed()` waits until companion streams finish cancelling.
          self.set_subscription_state(SubscriptionState::Stopping);
          stopping_guard_opt = Some(StoppingGuard { mgr: self });
          let pnl_id = u_state.current_pnl_req_id.take();
          let pos = std::mem::replace(&mut u_state.positions_requested, false);
          teardown_post_subscribe_streams = Some((pnl_id, pos));
        } else if self.subscription_state() != SubscriptionState::Stopping {
          self.set_subscription_state(SubscriptionState::Unsubscribed);
        }
        // Summary request itself was rejected/terminated by TWS, so do not send cancelAccountSummary for it
        u_state.current_summary_req_id = None;
        u_state.summary_sent = false;
        drop(u_state);

        if let Some((pnl_id, cancel_pos)) = teardown_post_subscribe_streams {
          if let Ok(server_version) = self.message_broker.get_server_version() {
            let encoder = Encoder::new(server_version);
            if let Some(id) = pnl_id {
              if let Ok(cancel_msg) = encoder.encode_cancel_pnl(id) {
                let _ = self.message_broker.send_message(&cancel_msg);
              }
            }
            if cancel_pos {
              if let Ok(cancel_msg) = encoder.encode_cancel_positions() {
                let _ = self.message_broker.send_message(&cancel_msg);
              }
              self.abort_manual_position_refresh(IBKRError::ApiError(code as i32, msg.to_string()));
            }
          }
          self.cancel_active_pnl_single_subscriptions();
        }
        drop(stopping_guard_opt);
        return true;
      }

      if u_state.current_pnl_req_id == Some(req_id) {
        error!("API Error for account daily PnL request {}: Code={:?}, Msg={}", req_id, code, msg);
        u_state.current_pnl_req_id = None;
        u_state.last_rejected_pnl_req_id = Some(req_id);
        return true;
      }
    }

    // 3. Check PnL single subscriptions (release `pnl_single_req_id_to_con_id` write lock before locking `pnl_single_subscriptions`)
    let con_id_opt = self.pnl_single_req_id_to_con_id.write().remove(&req_id);
    if let Some(con_id) = con_id_opt {
      self.last_rejected_pnl_single_req_id.store(req_id, Ordering::Release);
      let mut subs = self.pnl_single_subscriptions.write();
      if subs.get(&con_id) == Some(&req_id) {
        subs.remove(&con_id);
      }
      drop(subs);
      error!("API Error for PnL single request {} (con_id {}): Code={:?}, Msg={}", req_id, con_id, code, msg);
      return true;
    }

    false
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

    let mut state = self.account_state.write();

    // TWS can emit aggregated tags with account="All" when `reqAccountSummary` uses group="All".
    // Accept those tags as fallbacks without locking `state.account_id` onto "All", rejecting real
    // account IDs, or overwriting values already populated for a specific real account ID.
    if account_name.is_empty() || account_name == "All" {
      if let Some(existing) = state.values.get(key) {
        if !existing.account_id.is_empty() && existing.account_id != "All" {
          debug!(
            "Ignoring aggregated '{}' account value for key '{}' because account '{}' already populated it.",
            account_name, key, existing.account_id
          );
          return;
        }
      }
    } else if state.account_id.is_empty() || state.account_id == "All" {
      state.account_id = account_name.to_string();
      info!("Account ID set to: {}", account_name);
    } else if state.account_id != account_name {
      warn!("Received account value for unexpected account ID: {} (expected {})", account_name, state.account_id);
      return;
    }
    state.values.insert(key.to_string(), updated_value);
    state.last_updated = Some(Utc::now());
    drop(state);

    self.check_and_notify_account_update();
  }

  fn portfolio_value(&self, contract: &Contract, quantity: f64, market_price: f64, market_value: f64, average_cost: f64, unrealized_pnl: f64, realized_pnl: f64, account_name: &str) {
    let position_key = contract.con_id.to_string();
    debug!("Handler: Portfolio Value: Account={}, ConID={}, Symbol={}, Qty={}, MktPx={}, MktVal={}, AvgCost={}, UnPNL={}, RealPNL={}",
           account_name, contract.con_id, contract.symbol, quantity, market_price, market_value, average_cost, unrealized_pnl, realized_pnl);

    let updated_at = Utc::now();
    let has_observers = !self.observers.read().is_empty();
    let mut state_guard = self.account_state.write();

    if state_guard.account_id.is_empty() || state_guard.account_id == "All" {
      if !account_name.is_empty() && account_name != "All" {
        state_guard.account_id = account_name.to_string();
      }
    } else if !account_name.is_empty() && state_guard.account_id != account_name {
      warn!("Received portfolio value for unexpected account ID: {} (expected {})", account_name, state_guard.account_id);
      return;
    }

    let updated_position = Position {
      symbol: contract.symbol.clone(),
      contract: contract.clone(),
      quantity,
      average_cost,
      market_price,
      market_value,
      unrealized_pnl,
      realized_pnl,
      updated_at,
      position_daily_pnl: None,
    };

    let position_to_notify: Option<Position> = if has_observers {
      Some(updated_position.clone())
    } else {
      None
    };
    state_guard.portfolio.insert(position_key, updated_position);

    let con_id = contract.con_id;
    let current_pnl_filter = self.pnl_filter.read().clone();
    let should_be_subscribed_pnl = quantity != 0.0 && current_pnl_filter.matches(con_id);

    let is_currently_subscribed_pnl = self.pnl_single_subscriptions.read().contains_key(&con_id);
    let account_id_clone = state_guard.account_id.clone();

    state_guard.last_updated = Some(updated_at);
    drop(state_guard);

    // Only auto-subscribe PnLSingle on the reader thread once `Subscribed` (during `Initializing`,
    // `sync_pnl_single_for_open_positions` batches them on the caller thread after `PositionEnd`).
    let is_sub = self.is_subscribed();
    if is_sub && should_be_subscribed_pnl && !is_currently_subscribed_pnl {
      if !account_id_clone.is_empty() && account_id_clone != "All" {
        if let Err(e) = self._subscribe_pnl_for_position(con_id, &account_id_clone) {
          warn!("Auto-subscribe PnL (from portfolio_value) failed for con_id {}: {:?}", con_id, e);
        }
      } else {
        warn!("Cannot auto-subscribe PnL (from portfolio_value) for con_id {}: Account ID unknown.", con_id);
      }
    } else if (!self.updates_active() || !should_be_subscribed_pnl) && is_currently_subscribed_pnl {
      if let Err(e) = self._unsubscribe_pnl_for_position(con_id) {
        warn!("Auto-unsubscribe PnL (from portfolio_value) failed for con_id {}: {:?}", con_id, e);
      }
    }

    if let Some(pos) = position_to_notify {
      self.notify_position_update(&pos);
    }
  }

  fn position(&self, account: &str, contract: &Contract, quantity: f64, avg_cost: f64) {
    let position_key = contract.con_id.to_string();
    debug!("Handler: Position: Account={}, ConID={}, Symbol={}, Qty={}, AvgCost={}",
           account, contract.con_id, contract.symbol, quantity, avg_cost);

    let updated_at = Utc::now();
    let has_observers = !self.observers.read().is_empty();
    let mut state_guard = self.account_state.write();

    if state_guard.account_id.is_empty() || state_guard.account_id == "All" {
      if !account.is_empty() && account != "All" {
        state_guard.account_id = account.to_string();
      }
    } else if !account.is_empty() && state_guard.account_id != account {
      warn!("Received position for unexpected account ID: {} (expected {})", account, state_guard.account_id);
      return;
    }

    let position_to_notify: Option<Position>;
    {
      let position_entry = state_guard.portfolio.entry(position_key).or_insert_with(|| {
        Position {
          symbol: contract.symbol.clone(), contract: contract.clone(), quantity,
          average_cost: avg_cost, market_price: 0.0, market_value: 0.0,
          unrealized_pnl: 0.0, realized_pnl: 0.0, updated_at,
          position_daily_pnl: None,
        }
      });

      position_entry.quantity = quantity;
      position_entry.average_cost = avg_cost;
      position_entry.updated_at = updated_at;

      position_to_notify = if has_observers {
        Some(position_entry.clone())
      } else {
        None
      };
    }

    let con_id = contract.con_id;
    let current_pnl_filter = self.pnl_filter.read().clone();
    let should_be_subscribed_pnl = quantity != 0.0 && current_pnl_filter.matches(con_id);
    let is_currently_subscribed_pnl = self.pnl_single_subscriptions.read().contains_key(&con_id);
    let account_id_clone = state_guard.account_id.clone();

    state_guard.last_updated = Some(updated_at);
    drop(state_guard);

    // Only auto-subscribe PnLSingle on the reader thread once `Subscribed` (during `Initializing`,
    // `sync_pnl_single_for_open_positions` batches them on the caller thread after `PositionEnd`).
    let is_sub = self.is_subscribed();
    if is_sub && should_be_subscribed_pnl && !is_currently_subscribed_pnl {
      if !account_id_clone.is_empty() && account_id_clone != "All" {
        if let Err(e) = self._subscribe_pnl_for_position(con_id, &account_id_clone) {
          warn!("Auto-subscribe PnL (from position) failed for con_id {}: {:?}", con_id, e);
        }
      } else {
        warn!("Cannot auto-subscribe PnL (from position) for con_id {}: Account ID unknown.", con_id);
      }
    } else if (!self.updates_active() || !should_be_subscribed_pnl) && is_currently_subscribed_pnl {
      if let Err(e) = self._unsubscribe_pnl_for_position(con_id) {
        warn!("Auto-unsubscribe PnL (from position) failed for con_id {}: {:?}", con_id, e);
      }
    }

    if let Some(pos) = position_to_notify {
      self.notify_position_update(&pos);
    }
  }

  fn account_update_time(&self, time_stamp: &str) {
    debug!("Handler: Account Update Time: {}", time_stamp);
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
    let state = self.account_state.read();
    let state_account_id = state.account_id.clone();
    drop(state);

    if state_account_id == account || state_account_id.is_empty() {
      info!("Account download finished for account '{}'", account);
    } else {
      warn!("Received AccountDownloadEnd for unexpected account: {} (expected {})", account, state_account_id);
    }
  }

  fn managed_accounts(&self, accounts_list: &str) {
    debug!("Handler: Managed Accounts: {}", accounts_list);
    if self.account_state.read().account_id.is_empty() {
      let accounts: Vec<&str> = accounts_list.split(',').filter(|s| !s.is_empty()).collect();
      if let Some(first_account) = accounts.first() {
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
    let mut handled = false;

    // --- Check for Manual Refresh Completion (do not return early: PositionEnd is connection-wide) ---
    {
      let mut refresh_state = self.manual_position_refresh_state.lock();
      if refresh_state.waiting {
        info!("PositionEnd received, completing manual position refresh.");
        refresh_state.waiting = false;
        self.manual_position_refresh_cond.notify_all();
        handled = true;
      }
    }

    // --- Check for Initial Fetch Completion ---
    {
      let mut u_state = self.update_state.lock();
      if u_state.is_initial_fetch_active && u_state.waiting_for_initial_position_end {
        info!("PositionEnd received during initial fetch, marking positions as complete.");
        u_state.waiting_for_initial_position_end = false;
        if !u_state.waiting_for_initial_summary_end {
          info!("PositionEnd: Initial fetch complete. Notifying waiter.");
          self.update_cond.notify_all();
        } else {
          info!("PositionEnd: Still waiting for initial SummaryEnd.");
        }
      } else if !handled {
        debug!("PositionEnd received outside of initial fetch or manual refresh.");
      }
    }
  }

  fn account_summary(&self, req_id: i32, account: &str, tag: &str, value: &str, currency: &str) {
    debug!("Handler: Account Summary: ReqId={}, Account={}, Tag={}, Value={}, Currency={}",
           req_id, account, tag, value, currency);
    // Ignore stale summary rows belonging to a cancelled/timed-out summary request ID
    if self.update_state.lock().current_summary_req_id != Some(req_id) {
      debug!("Ignoring AccountSummary for stale/inactive ReqID {}", req_id);
      return;
    }
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
        if !u_state.waiting_for_initial_position_end {
          info!("AccountSummaryEnd: Initial fetch complete. Notifying waiter.");
          self.update_cond.notify_all();
        } else {
          info!("AccountSummaryEnd: Still waiting for initial PositionEnd.");
        }
      } else {
        warn!("AccountSummaryEnd received for initial fetch ReqID {} but not waiting for summary.", req_id);
      }
    } else if u_state.current_summary_req_id == Some(req_id) {
      debug!("AccountSummaryEnd received for ReqID {} during continuous updates.", req_id);
    } else {
      warn!("AccountSummaryEnd received for unexpected ReqID {} (Expected: {:?}, Initial Fetch Active: {}).",
            req_id, u_state.current_summary_req_id, u_state.is_initial_fetch_active);
    }
  }

  fn pnl(&self, req_id: i32, daily_pnl: f64, unrealized_pnl: Option<f64>, realized_pnl: Option<f64>) {
    debug!("Handler: PnL: ReqId={}, Daily={}, Unrealized={:?}, Realized={:?}", req_id, daily_pnl, unrealized_pnl, realized_pnl);
    if self.update_state.lock().current_pnl_req_id != Some(req_id) {
      debug!("Ignoring PnL update for stale/inactive ReqID {}", req_id);
      return;
    }

    let mut state = self.account_state.write();
    let account_id = state.account_id.clone();
    if account_id.is_empty() {
      warn!("Received PnL update but account ID is not known yet.");
      return;
    }

    let base_currency = state.values.get("Currency").map(|v| v.value.clone());

    let mut update_val = |key: &str, val: f64, curr: Option<String>| {
      state.values.insert(key.to_string(), AccountValue {
        key: key.to_string(),
        value: val.to_string(),
        currency: curr,
        account_id: account_id.clone(),
      });
    };

    update_val("DailyPnL", daily_pnl, base_currency.clone());
    if let Some(pnl) = unrealized_pnl {
      update_val("UnrealizedPnL", pnl, base_currency.clone());
    }
    if let Some(pnl) = realized_pnl {
      update_val("RealizedPnL", pnl, base_currency);
    }

    state.last_updated = Some(Utc::now());
    drop(state);

    self.check_and_notify_account_update();
  }

  fn pnl_single(&self, req_id: i32, pos_idx: i32, daily_pnl_val: f64, unrealized_pnl_val: Option<f64>, realized_pnl_val: Option<f64>, value_val: f64) {
    debug!("Handler: PnLSingle: ReqID={}, PosIdx={}, DailyPnL={}, UnrealizedPnL={:?}, RealizedPnL={:?}, Value={}",
           req_id, pos_idx, daily_pnl_val, unrealized_pnl_val, realized_pnl_val, value_val);

    let con_id_opt = self.pnl_single_req_id_to_con_id.read().get(&req_id).cloned();

    if let Some(con_id) = con_id_opt {
      let has_observers = !self.observers.read().is_empty();
      let mut account_state_guard = self.account_state.write();
      let position_key = con_id.to_string();

      if let Some(position) = account_state_guard.portfolio.get_mut(&position_key) {
        position.position_daily_pnl = Some(daily_pnl_val);
        if let Some(un_pnl) = unrealized_pnl_val {
          position.unrealized_pnl = un_pnl;
        }
        if let Some(r_pnl) = realized_pnl_val {
          position.realized_pnl = r_pnl;
        }
        position.market_value = value_val;
        if position.quantity != 0.0 {
          position.market_price = value_val / position.quantity;
        }
        position.updated_at = Utc::now();

        let position_to_notify = if has_observers {
          Some(position.clone())
        } else {
          None
        };
        drop(account_state_guard);

        if let Some(pos) = position_to_notify {
          self.notify_position_update(&pos);
        }
      } else {
        warn!("PnLSingle received for con_id {} (ReqID {}), but position not found in portfolio.", con_id, req_id);
      }
    } else {
      warn!("PnLSingle received for unknown ReqID: {}. No con_id mapping found. PosIdx was {}.", req_id, pos_idx);
    }
  }

  // --- ExecutionHandler Implementation ---

  fn execution_details(&self, req_id: i32, _contract: &Contract, execution: &Execution) {
    debug!("Handler: Execution Details: ReqID={}, ExecID={}, OrderID={}, Symbol={}, Side={}, Qty={}, Px={}",
           req_id, execution.execution_id, execution.order_id, execution.symbol, execution.side, execution.quantity, execution.price);

    let mut exec_state = self.executions_state.lock();
    if let Some(state) = exec_state.get_mut(&req_id) {
      if state.waiting {
        state.executions.insert(execution.execution_id.clone(), execution.clone());
      } else {
        warn!("Received execution details for ReqID {} which is no longer in a waiting state.", req_id);
      }
    }
  }

  fn execution_details_end(&self, req_id: i32) {
    debug!("Handler: Execution Details End: ReqID={}", req_id);
    let mut exec_state = self.executions_state.lock();
    if let Some(state) = exec_state.get_mut(&req_id) {
      if state.waiting {
        state.end_received = true;
        self.executions_cond.notify_all();
      } else {
        warn!("Received execution details end for ReqID {} which is no longer in a waiting state.", req_id);
      }
    }
  }

  fn commission_report(&self, exec_id: &str, commission: f64, currency: &str,
                       _yield: Option<f64>, _yield_redemption: Option<f64>) {
    debug!("Handler: Commission Report: ExecID={}, Commission={}, Currency={}",
           exec_id, commission, currency);

    let has_observers = !self.observers.read().is_empty();
    let mut exec_state = self.executions_state.lock();
    let mut updated_execution: Option<Execution> = None;
    let mut matched = false;

    for (_req_id, state) in exec_state.iter_mut() {
      if let Some(execution) = state.executions.get_mut(exec_id) {
        execution.commission = Some(commission);
        execution.commission_currency = Some(currency.to_string());
        debug!("Merged commission into Execution: {}", exec_id);
        matched = true;
        if has_observers {
          updated_execution = Some(execution.clone());
        }
        break;
      }
    }

    drop(exec_state);

    if let Some(exec) = updated_execution {
      self.notify_execution(&exec);
    } else if !matched {
      warn!("Received CommissionReport for ExecID {} but no matching execution found in active requests.", exec_id);
    }
  }

  fn try_handle_request_error(&self, req_id: i32, code: ClientErrorCode, msg: &str) -> bool {
    self.handle_owned_request_error(req_id, code, msg)
  }

  /// Handles errors related to account data requests.
  fn handle_error(&self, req_id: i32, code: ClientErrorCode, msg: &str) {
    warn!("AccountHandler received error: ReqID={}, Code={:?}, Msg={}", req_id, code, msg);

    if req_id > 0 && self.handle_owned_request_error(req_id, code, msg) {
      return;
    }

    if req_id <= 0 {
      // Zero-allocation ASCII case-insensitive checks; ignore cancellation errors ("cancel")
      let is_cancel_err = contains_ignore_ascii_case(msg, b"cancel")
        || matches!(
          code,
          ClientErrorCode::FailSendCanPositions
            | ClientErrorCode::FailSendCanAccountData
            | ClientErrorCode::FailSendCanPositionsMulti
            | ClientErrorCode::FailSendCanAccountUpdatesMulti
            | ClientErrorCode::FailSendCancelPnl
            | ClientErrorCode::FailSendCancelPnlSingle
        );

      if !is_cancel_err {
        let is_summary_error = contains_ignore_ascii_case(msg, b"account summary");
        let is_position_error = code == ClientErrorCode::FailSendReqPositions
          || contains_ignore_ascii_case(msg, b"position requests");
        let mut handled = false;

        if is_position_error {
          let mut refresh_state = self.manual_position_refresh_state.lock();
          if refresh_state.waiting {
            error!("API Error during manual position refresh: Code={:?}, Msg={}", code, msg);
            if refresh_state.error.is_none() {
              refresh_state.error = Some(IBKRError::ApiError(code as i32, msg.to_string()));
            }
            refresh_state.waiting = false;
            self.manual_position_refresh_cond.notify_all();
            handled = true;
          }
        }

        if is_summary_error || is_position_error {
          let mut u_state = self.update_state.lock();
          if u_state.is_initial_fetch_active {
            error!("API Error during initial account fetch (ReqID={}): Code={:?}, Msg={}", req_id, code, msg);
            self.initial_fetch_abort.store(true, Ordering::Release);
            if u_state.initial_fetch_error.is_none() {
              u_state.initial_fetch_error = Some(IBKRError::ApiError(code as i32, msg.to_string()));
            }
            u_state.waiting_for_initial_summary_end = false;
            u_state.waiting_for_initial_position_end = false;
            if is_summary_error {
              u_state.current_summary_req_id = None;
              u_state.summary_sent = false;
            }
            if is_position_error {
              u_state.positions_requested = false;
              u_state.position_rejected = true;
            }
            // Leave `SubscriptionState` as `Initializing` until the owner thread enters `Stopping`
            // for wire cancellations and `InitialFetchGuard::drop` sets `Unsubscribed`.
            self.update_cond.notify_all();
            handled = true;
          }
        }

        if handled {
          return;
        }
      }
    }

    // If not matched to a specific request, log as a general account error
    error!("Unhandled Account error: ReqID={}, Code={:?}, Msg={}", req_id, code, msg);
    if code == ClientErrorCode::PreLiquidationWarning {
      info!("Pre-liquidation warning (2148) received this session.");
      self.pre_liquidation_warning_received.store(true, Ordering::Relaxed);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::client::client_manager::ClientManager;
  use crate::conn::Connection;
  use crate::data_fin_manager::DataFundamentalsManager;
  use crate::data_market_manager::DataMarketManager;
  use crate::data_news_manager::DataNewsManager;
  use crate::data_ref_manager::DataRefManager;
  use crate::financial_advisor_manager::FinancialAdvisorManager;
  use crate::handler::MessageHandler;
  use crate::message_parser::process_message;
  use crate::min_server_ver::min_server_ver;
  use crate::order_manager::OrderManager;
  use crate::rate_limiter::{RateLimiterConfig, RateLimiterManager};
  use std::sync::atomic::AtomicUsize;

  struct TestConnectionState {
    connected: bool,
    sent_messages: Vec<Vec<u8>>,
    fail_on_prefix: Option<Vec<u8>>,
    on_send: Option<Box<dyn FnMut(&[u8]) + Send>>,
  }

  #[derive(Clone)]
  struct TestConnection {
    // Separate `state` from `handler` so inbound message handlers that send outbound
    // messages (e.g., `position()` -> `_subscribe_pnl_for_position()`) never deadlock!
    state: Arc<Mutex<TestConnectionState>>,
    handler: Arc<Mutex<Option<MessageHandler>>>,
  }

  impl TestConnection {
    fn new() -> Self {
      Self {
        state: Arc::new(Mutex::new(TestConnectionState {
          connected: true,
          sent_messages: Vec::new(),
          fail_on_prefix: None,
          on_send: None,
        })),
        handler: Arc::new(Mutex::new(None)),
      }
    }

    fn set_on_send<F>(&self, f: F)
    where
      F: FnMut(&[u8]) + Send + 'static,
    {
      self.state.lock().on_send = Some(Box::new(f));
    }

    fn set_fail_on_prefix(&self, prefix: Option<&[u8]>) {
      self.state.lock().fail_on_prefix = prefix.map(|p| p.to_vec());
    }

    fn feed_wire_message(&self, wire: &[u8]) {
      let mut h_guard = self.handler.lock();
      if let Some(h) = h_guard.as_mut() {
        process_message(h, wire).expect("process_message failed");
      }
    }

    fn take_sent_strings(&self) -> Vec<String> {
      self
        .state
        .lock()
        .sent_messages
        .drain(..)
        .map(|b| String::from_utf8_lossy(&b).replace('\0', "|"))
        .collect()
    }
  }

  impl Connection for TestConnection {
    fn is_connected(&self) -> bool {
      self.state.lock().connected
    }

    fn disconnect(&mut self) -> Result<(), IBKRError> {
      self.state.lock().connected = false;
      *self.handler.lock() = None;
      Ok(())
    }

    fn send_message_body(&mut self, data: &[u8]) -> Result<(), IBKRError> {
      let cb_opt;
      {
        let mut g = self.state.lock();
        if !g.connected {
          return Err(IBKRError::NotConnected);
        }
        if let Some(prefix) = g.fail_on_prefix.as_ref() {
          if data.starts_with(prefix) {
            return Err(IBKRError::SocketError("Injected send failure".to_string()));
          }
        }
        g.sent_messages.push(data.to_vec());
        // Temporarily take `on_send` so callback runs without holding `self.state` lock!
        cb_opt = g.on_send.take();
      }
      if let Some(mut cb) = cb_opt {
        cb(data);
        let mut g = self.state.lock();
        if g.on_send.is_none() {
          g.on_send = Some(cb);
        }
      }
      Ok(())
    }

    fn set_message_handler(&mut self, handler: MessageHandler) {
      *self.handler.lock() = Some(handler);
      self.state.lock().connected = true;
    }

    fn get_server_version(&self) -> i32 {
      min_server_ver::MAX_SUPPORTED_VERSION
    }
  }

  fn setup_test_account_manager() -> (Arc<AccountManager>, TestConnection) {
    let conn = TestConnection::new();
    let server_version = conn.get_server_version();
    let broker = Arc::new(MessageBroker::new(Box::new(conn.clone()), None));
    let client_mgr = ClientManager::new(broker.clone());
    let rate_limiter_mgr = Arc::new(RateLimiterManager::new(RateLimiterConfig::default()));
    let (order_mgr, _order_init) = OrderManager::create(broker.clone());
    let account_mgr = AccountManager::new(broker.clone(), None);
    let data_ref_mgr = DataRefManager::new(broker.clone());
    let data_market_mgr = DataMarketManager::new(
      broker.clone(),
      rate_limiter_mgr.get_market_data_limiter(),
      rate_limiter_mgr.get_historical_limiter(),
    );
    let data_news_mgr = DataNewsManager::new(broker.clone(), rate_limiter_mgr.get_historical_limiter());
    let data_fin_mgr = DataFundamentalsManager::new(broker.clone());
    let fin_adv_mgr = FinancialAdvisorManager::new(broker.clone());
    let msg_handler = MessageHandler::new(
      server_version,
      client_mgr,
      account_mgr.clone(),
      order_mgr,
      data_ref_mgr,
      data_market_mgr,
      data_news_mgr,
      data_fin_mgr,
      fin_adv_mgr,
    );
    broker.set_message_handler(msg_handler);
    (account_mgr, conn)
  }

  struct CountingObserver {
    updates: Arc<AtomicUsize>,
  }

  impl AccountObserver for CountingObserver {
    fn on_account_update(&self, _info: &AccountInfo) {
      self.updates.fetch_add(1, Ordering::SeqCst);
    }
    fn on_position_update(&self, _position: &Position) {}
    fn on_execution(&self, _execution: &Execution) {}
  }

  #[test]
  fn test_error_322_rejection_does_not_poison_subscription_and_allows_retry() {
    let (acct_mgr, conn) = setup_test_account_manager();
    let updates_count = Arc::new(AtomicUsize::new(0));
    acct_mgr.add_observer(CountingObserver {
      updates: updates_count.clone(),
    });

    // Attempt 1: When reqPositions ("61\01\0") is sent after reqAccountSummary (ReqID 8000),
    // simulate IB replying from another thread with Error 322 ("Maximum number of account summary requests exceeded").
    let conn_for_err = conn.clone();
    conn.set_on_send(move |data| {
      if data.starts_with(b"61\0") {
        let c = conn_for_err.clone();
        std::thread::spawn(move || {
          std::thread::sleep(Duration::from_millis(15));
          c.feed_wire_message(
            b"4\02\08000\0322\0Error processing request.-'bT' : cause - Maximum number of account summary requests exceeded.\0",
          );
        });
      }
    });

    let err = acct_mgr
      .get_net_liquidation()
      .expect_err("Expected error 322 to fail get_net_liquidation");
    match err {
      IBKRError::ApiError(code, msg) => {
        assert_eq!(code, 322);
        assert!(msg.contains("account summary requests"), "unexpected msg: {}", msg);
      }
      other => panic!("Expected IBKRError::ApiError(322, _), got {:?}", other),
    }

    // Must NOT be marked subscribed or initializing after rejection!
    assert!(!acct_mgr.is_subscribed());
    assert!(!acct_mgr.is_initializing());
    assert!(!acct_mgr.update_state.lock().is_initial_fetch_active);

    let sent_attempt_1 = conn.take_sent_strings();
    // Sent reqAccountSummary (62), reqPositions (61), and cancelPositions (64);
    // must NOT cancel the already-rejected summary ID (63) and must NOT send reqPnl (92).
    assert!(sent_attempt_1.iter().any(|m| m.starts_with("62|1|8000|")));
    assert!(sent_attempt_1.iter().any(|m| m.starts_with("61|1|")));
    assert!(sent_attempt_1.iter().any(|m| m.starts_with("64|1|")));
    assert!(!sent_attempt_1.iter().any(|m| m.starts_with("63|")));
    assert!(!sent_attempt_1.iter().any(|m| m.starts_with("92|")));

    // Attempt 2: Temporary IB rejection is gone; next call must send a fresh request (ReqID 8001) and succeed!
    // Also include an "All" aggregated tag row before the real account ID to verify `[C-7]`.
    let conn_for_ok = conn.clone();
    conn.set_on_send(move |data| {
      if data.starts_with(b"61\0") {
        let c = conn_for_ok.clone();
        std::thread::spawn(move || {
          std::thread::sleep(Duration::from_millis(15));
          c.feed_wire_message(b"63\01\08001\0All\0Currency\0USD\0USD\0");
          c.feed_wire_message(b"63\01\08001\0DU12345\0NetLiquidation\0250000.50\0USD\0");
          c.feed_wire_message(b"64\01\08001\0");
          c.feed_wire_message(b"62\01\0");
        });
      }
    });

    let net_liq = acct_mgr
      .get_net_liquidation()
      .expect("Retry after 322 should send fresh request and succeed");
    assert!((net_liq - 250000.50).abs() < 1e-6);
    assert_eq!(acct_mgr.account_state.read().account_id, "DU12345");
    assert!(acct_mgr.is_subscribed());
    assert!(!acct_mgr.is_initializing());

    let sent_attempt_2 = conn.take_sent_strings();
    assert!(sent_attempt_2.iter().any(|m| m.starts_with("62|1|8001|")));
    assert!(sent_attempt_2.iter().any(|m| m.starts_with("61|1|")));
    assert!(sent_attempt_2.iter().any(|m| m.starts_with("92|8002|DU12345|"))); // reqPnl sent with real account ID

    // Initial notification fired once, and subsequent updates must ALSO notify observers (is_initializing == false)
    assert_eq!(updates_count.load(Ordering::SeqCst), 1);
    acct_mgr.account_value("NetLiquidation", "251000.00", Some("USD"), "DU12345");
    assert_eq!(updates_count.load(Ordering::SeqCst), 2);

    // Stopping account updates must now also cancel daily PnL (`93|8002|`) (`[C-3]`)!
    conn.set_on_send(|_| {});
    acct_mgr.stop_account_updates().unwrap();
    let sent_stop = conn.take_sent_strings();
    assert!(sent_stop.iter().any(|m| m == "93|8002|"), "Expected cancelPnl(8002): {:?}", sent_stop);
    assert!(sent_stop.iter().any(|m| m == "63|1|8001|"), "Expected cancelAccountSummary(8001): {:?}", sent_stop);
    assert!(sent_stop.iter().any(|m| m == "64|1|"), "Expected cancelPositions: {:?}", sent_stop);
  }

  #[test]
  fn test_open_position_during_rejected_fetch_defers_pnl_single_and_does_not_self_deadlock() {
    let (acct_mgr, conn) = setup_test_account_manager();

    // Simulate an open SPY position arriving before Error 322 rejects reqAccountSummary(8000).
    let conn_for_err = conn.clone();
    conn.set_on_send(move |data| {
      if data.starts_with(b"61\0") {
        let c = conn_for_err.clone();
        std::thread::spawn(move || {
          std::thread::sleep(Duration::from_millis(10));
          // Open position (msg 61, v3) during `Initializing`: deferred to caller thread (`[S-2]`),
          // so the reader thread does not send `reqPnlSingle` on a doomed fetch!
          c.feed_wire_message(b"61\03\0DU12345\0265598\0SPY\0STK\0\00.0\0\0\0ARCA\0USD\0SPY\0SPY\0100\0450.0\0");
          c.feed_wire_message(
            b"4\02\08000\0322\0Error processing request.-'bT' : cause - Maximum number of account summary requests exceeded.\0",
          );
        });
      }
    });

    let err = acct_mgr
      .subscribe_account_updates()
      .expect_err("Expected 322 rejection");
    assert!(matches!(err, IBKRError::ApiError(322, _)));
    assert!(!acct_mgr.is_subscribed());
    assert!(!acct_mgr.is_initializing());
    assert!(acct_mgr.pnl_single_subscriptions.read().is_empty());

    let sent = conn.take_sent_strings();
    assert!(!sent.iter().any(|m| m.starts_with("94|")), "Reader thread must defer reqPnlSingle during Initializing: {:?}", sent);

    // Simulate late in-flight PnLSingle (msg 95), AccountSummary (msg 63), and Position (msg 61)
    // arriving on the reader thread while unsubscribed — must return immediately without self-deadlock!
    conn.set_on_send(|_| {});
    let start = std::time::Instant::now();
    conn.feed_wire_message(b"95\08001\0100\012.5\025.0\00.0\045025.0\0");
    conn.feed_wire_message(b"63\01\08000\0DU12345\0NetLiquidation\0100000.0\0USD\0");
    conn.feed_wire_message(b"61\03\0DU12345\0265598\0SPY\0STK\0\00.0\0\0\0ARCA\0USD\0SPY\0SPY\0100\0450.0\0");
    assert!(
      start.elapsed() < Duration::from_millis(1500),
      "Reader-thread handlers must not block in ensure_subscribed when unsubscribed"
    );
    assert!(conn.take_sent_strings().is_empty());
    assert!(acct_mgr.pnl_single_subscriptions.read().is_empty());
  }

  #[test]
  fn test_concurrent_caller_during_error_cleanup_gets_already_running() {
    let (acct_mgr, conn) = setup_test_account_manager();

    // Pause Thread A inside `cancelPositions` ("64\0") during failure cleanup. Because Thread B
    // now checks `is_initial_fetch_active` under `update_state.lock()` *before* touching
    // `MessageBroker::connection` (`[E-3]`), Thread B immediately gets `AlreadyRunning`!
    let (entered_cancel_tx, entered_cancel_rx) = std::sync::mpsc::channel();
    let (release_cancel_tx, release_cancel_rx) = std::sync::mpsc::channel();
    let release_cancel_rx = Arc::new(Mutex::new(release_cancel_rx));

    let conn_cb = conn.clone();
    conn.set_on_send(move |data| {
      if data.starts_with(b"61\0") {
        let c = conn_cb.clone();
        std::thread::spawn(move || {
          std::thread::sleep(Duration::from_millis(5));
          c.feed_wire_message(
            b"4\02\08000\0322\0Error processing request.-'bT' : cause - Maximum number of account summary requests exceeded.\0",
          );
        });
      } else if data.starts_with(b"64\0") {
        let _ = entered_cancel_tx.send(());
        let _ = release_cancel_rx.lock().recv_timeout(Duration::from_secs(2));
      }
    });

    let mgr_a = acct_mgr.clone();
    let handle_a = std::thread::spawn(move || {
      mgr_a.subscribe_account_updates_with_timeout(Duration::from_secs(2))
    });

    entered_cancel_rx
      .recv_timeout(Duration::from_secs(2))
      .expect("Thread A should send cancelPositions");

    let res_b = acct_mgr.subscribe_account_updates_with_timeout(Duration::from_millis(100));
    assert!(
      matches!(res_b, Err(IBKRError::AlreadyRunning(_))),
      "Expected AlreadyRunning while Thread A still owns InitialFetchGuard, got {:?}",
      res_b
    );

    let _ = release_cancel_tx.send(());
    let res_a = handle_a.join().unwrap();
    assert!(matches!(res_a, Err(IBKRError::ApiError(322, _))), "Got {:?}", res_a);
    assert!(!acct_mgr.update_state.lock().is_initial_fetch_active);
  }

  #[test]
  fn test_timeout_cancels_summary_and_positions_without_recursing() {
    let (acct_mgr, conn) = setup_test_account_manager();

    let conn_for_summary = conn.clone();
    conn.set_on_send(move |data| {
      if data.starts_with(b"61\0") {
        // Deterministically deliver Summary + SummaryEnd before entering wait loop; omit PositionEnd!
        conn_for_summary.feed_wire_message(b"63\01\08000\0DU12345\0NetLiquidation\0100000.0\0USD\0");
        conn_for_summary.feed_wire_message(b"64\01\08000\0");
      }
    });

    let err = acct_mgr
      .subscribe_account_updates_with_timeout(Duration::from_millis(50))
      .expect_err("Expected timeout when PositionEnd never arrives");
    assert!(matches!(err, IBKRError::Timeout(_)), "Expected Timeout, got {:?}", err);
    assert!(!acct_mgr.is_subscribed());
    assert!(!acct_mgr.is_initializing());

    let sent = conn.take_sent_strings();
    let summary_reqs = sent.iter().filter(|m| m.starts_with("62|")).count();
    assert_eq!(summary_reqs, 1, "Must not recursively re-subscribe on timeout: {:?}", sent);
    assert!(sent.iter().any(|m| m == "63|1|8000|"), "Must cancel summary req 8000 even if SummaryEnd arrived before PositionEnd timeout: {:?}", sent);
    assert!(sent.iter().any(|m| m == "64|1|"), "Must cancel positions on timeout: {:?}", sent);
    assert!(!sent.iter().any(|m| m.starts_with("92|")), "Must not send reqPnl on timeout: {:?}", sent);
  }

  #[test]
  fn test_pnl_failure_never_exposes_is_subscribed_and_cancels_streams_including_pnl_single() {
    let (acct_mgr, conn) = setup_test_account_manager();

    // Case A: SummaryEnd + PositionEnd arrive with empty summary values -> send_account_pnl_request fails.
    let conn_cb = conn.clone();
    conn.set_on_send(move |data| {
      if data.starts_with(b"61\0") {
        conn_cb.feed_wire_message(b"64\01\08000\0");
        conn_cb.feed_wire_message(b"62\01\0");
      }
    });

    let err = acct_mgr
      .subscribe_account_updates()
      .expect_err("Expected failure when account summary is empty");
    assert!(matches!(err, IBKRError::InternalError(_)), "Got {:?}", err);
    assert!(!acct_mgr.is_subscribed());
    assert!(!acct_mgr.is_initializing());
    let sent_a = conn.take_sent_strings();
    assert!(sent_a.iter().any(|m| m == "63|1|8000|"), "Expected summary cancel: {:?}", sent_a);
    assert!(sent_a.iter().any(|m| m == "64|1|"), "Expected positions cancel: {:?}", sent_a);

    // Case B: Summary + open SPY position succeed (so caller thread sends `reqPnlSingle` 8002),
    // then sending daily `reqPnl` ("92\0") fails on the socket -> must cancel summary, positions, AND `reqPnlSingle`!
    conn.set_fail_on_prefix(Some(b"92\0"));
    let conn_cb2 = conn.clone();
    conn.set_on_send(move |data| {
      if data.starts_with(b"61\0") {
        conn_cb2.feed_wire_message(b"63\01\08001\0DU12345\0NetLiquidation\0100000.0\0USD\0");
        conn_cb2.feed_wire_message(b"61\03\0DU12345\0265598\0SPY\0STK\0\00.0\0\0\0ARCA\0USD\0SPY\0SPY\0100\0450.0\0");
        conn_cb2.feed_wire_message(b"64\01\08001\0");
        conn_cb2.feed_wire_message(b"62\01\0");
      }
    });

    let err_b = acct_mgr
      .subscribe_account_updates()
      .expect_err("Expected SocketError when reqPnl send fails");
    assert!(matches!(err_b, IBKRError::SocketError(_)), "Got {:?}", err_b);
    assert!(!acct_mgr.is_subscribed());
    assert!(!acct_mgr.is_initializing());
    assert!(acct_mgr.pnl_single_subscriptions.read().is_empty());
    let sent_b = conn.take_sent_strings();
    assert!(sent_b.iter().any(|m| m.starts_with("94|8002|")), "Expected caller-thread reqPnlSingle: {:?}", sent_b);
    assert!(sent_b.iter().any(|m| m == "95|8002|"), "Expected cancelPnlSingle(8002) on PnL send error: {:?}", sent_b);
    assert!(sent_b.iter().any(|m| m == "63|1|8001|"), "Expected summary cancel on PnL send error: {:?}", sent_b);
    assert!(sent_b.iter().any(|m| m == "64|1|"), "Expected positions cancel on PnL send error: {:?}", sent_b);
  }

  #[test]
  fn test_cleanup_requests_during_and_before_sends_never_races() {
    let (acct_mgr, conn) = setup_test_account_manager();
    let mgr_for_cleanup = acct_mgr.clone();

    // Part 1: `cleanup_requests()` while waiting in initial fetch -> owner cancels 8000 + positions once
    let (pos_sent_tx, pos_sent_rx) = std::sync::mpsc::channel();
    conn.set_on_send(move |data| {
      if data.starts_with(b"61\0") {
        let _ = pos_sent_tx.send(());
      }
    });
    let cleanup_handle = std::thread::spawn(move || {
      pos_sent_rx.recv_timeout(Duration::from_secs(2)).expect("reqPositions should be sent");
      mgr_for_cleanup.cleanup_requests()
    });

    let err = acct_mgr
      .subscribe_account_updates_with_timeout(Duration::from_secs(5))
      .expect_err("Expected cleanup_requests to abort waiting initial fetch");
    cleanup_handle.join().unwrap().expect("cleanup_requests should succeed");
    assert!(matches!(err, IBKRError::NotConnected), "Got {:?}", err);
    assert!(!acct_mgr.is_subscribed());
    assert!(!acct_mgr.is_initializing());

    let sent = conn.take_sent_strings();
    assert_eq!(sent.iter().filter(|m| *m == "63|1|8000|").count(), 1, "Sent: {:?}", sent);
    assert_eq!(sent.iter().filter(|m| *m == "64|1|").count(), 1, "Sent: {:?}", sent);

    // Part 2 (`[C-2]`): `stop_account_updates()` called right after `summary_msg` (8001) is sent,
    // *before* `position_msg` is sent -> must cancel summary 8001 ONLY and never send `reqPositions` or `cancelPositions`!
    let mgr_stop = acct_mgr.clone();
    conn.set_on_send(move |data| {
      if data.starts_with(b"62\0") {
        let m = mgr_stop.clone();
        std::thread::spawn(move || {
          let _ = m.stop_account_updates();
        });
        // Deterministically wait until `stop_account_updates()` sets `initial_fetch_abort`
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !mgr_stop.initial_fetch_abort.load(Ordering::Acquire) {
          assert!(std::time::Instant::now() < deadline, "Timed out waiting for initial_fetch_abort");
          std::thread::yield_now();
        }
      }
    });

    let err2 = acct_mgr
      .subscribe_account_updates_with_timeout(Duration::from_secs(2))
      .expect_err("Expected stop_account_updates to abort initial fetch before position_msg");
    assert!(matches!(err2, IBKRError::InternalError(_)), "Got {:?}", err2);
    let sent2 = conn.take_sent_strings();
    assert!(sent2.iter().any(|m| m.starts_with("62|1|8001|")), "Sent2: {:?}", sent2);
    assert!(sent2.iter().any(|m| m == "63|1|8001|"), "Sent2: {:?}", sent2);
    assert!(!sent2.iter().any(|m| m.starts_with("61|")), "Must not send reqPositions after abort: {:?}", sent2);
    assert!(!sent2.iter().any(|m| m.starts_with("64|")), "Must not send cancelPositions when reqPositions was never sent: {:?}", sent2);
  }

  #[test]
  fn test_req_id_ownership_routing_cancel_collision_and_pnl_single_error_cleanup() {
    let (acct_mgr, conn) = setup_test_account_manager();

    // 1 (`[C-4]`): Error 321 on owned ReqID 8000 with an arbitrary/renamed TWS ProGuard tag (`-'zQ'`)
    // and NO "account summary" substring -> still routed by `req_id` ownership!
    let conn_cb = conn.clone();
    conn.set_on_send(move |data| {
      if data.starts_with(b"61\0") {
        let c = conn_cb.clone();
        std::thread::spawn(move || {
          std::thread::sleep(Duration::from_millis(10));
          // Also inject a stale `-1` cancel error (`[C-4]`) first: must NOT abort or clear state!
          c.feed_wire_message(b"4\02\0-1\0322\0Can't cancel position requests when not subscribed\0");
          c.feed_wire_message(b"4\02\08000\0321\0Error validating request.-'zQ' : cause - Invalid tag\0");
        });
      }
    });
    let err_321 = acct_mgr
      .subscribe_account_updates_with_timeout(Duration::from_secs(2))
      .expect_err("Expected 321 error on owned req_id 8000 to abort initial fetch immediately");
    assert!(matches!(err_321, IBKRError::ApiError(321, _)), "Got {:?}", err_321);
    let sent_321 = conn.take_sent_strings();
    // Stale cancel error did NOT clear `positions_requested`, so `cancelPositions` ("64|1|") WAS sent!
    assert!(sent_321.iter().any(|m| m == "64|1|"), "Sent: {:?}", sent_321);

    // 2 (`[C-4]`): Request-ID-less (`id = -1`) Error 322 for `reqPositions` ("Maximum number of position requests exceeded")
    let conn_cb2 = conn.clone();
    conn.set_on_send(move |data| {
      if data.starts_with(b"61\0") {
        let c = conn_cb2.clone();
        std::thread::spawn(move || {
          std::thread::sleep(Duration::from_millis(10));
          c.feed_wire_message(
            b"4\02\0-1\0322\0Error processing request.-'zP' : cause - Maximum number of position requests exceeded\0",
          );
        });
      }
    });
    let err_pos = acct_mgr
      .subscribe_account_updates_with_timeout(Duration::from_secs(2))
      .expect_err("Expected -1 position error 322 to abort initial fetch immediately");
    assert!(matches!(err_pos, IBKRError::ApiError(322, _)), "Got {:?}", err_pos);
    let sent_pos = conn.take_sent_strings();
    assert!(sent_pos.iter().any(|m| m == "63|1|8001|"), "Sent: {:?}", sent_pos);
    assert!(!sent_pos.iter().any(|m| m == "64|1|"), "Sent: {:?}", sent_pos);

    // 3 (`[C-3]` & `[C-4]` & `[S-3]`): Subscribe with an open position, verify `PnLSingle` error cleans up map,
    // verify `list_open_positions` wakes on position error + send failure, and verify oversized/control-char error sanitization!
    let conn_cb3 = conn.clone();
    conn.set_on_send(move |data| {
      if data.starts_with(b"61\0") {
        conn_cb3.feed_wire_message(b"63\01\08002\0DU12345\0NetLiquidation\0100000.0\0USD\0");
        conn_cb3.feed_wire_message(b"61\03\0DU12345\0265598\0SPY\0STK\0\00.0\0\0\0ARCA\0USD\0SPY\0SPY\0100\0450.0\0");
        conn_cb3.feed_wire_message(b"64\01\08002\0");
        conn_cb3.feed_wire_message(b"62\01\0");
      }
    });
    acct_mgr.subscribe_account_updates().expect("Subscription should succeed");
    let _ = conn.take_sent_strings();
    let spy_pnl_req_id = *acct_mgr.pnl_single_subscriptions.read().get(&265598).expect("SPY PnLSingle active");

    // Reject `spy_pnl_req_id` with Error 322 -> must remove from `pnl_single_subscriptions` (`[C-3]`)!
    let pnl_err_wire = format!("4\02\0{}\0322\0Max PnL single requests exceeded\0", spy_pnl_req_id);
    conn.feed_wire_message(pnl_err_wire.as_bytes());
    assert!(!acct_mgr.pnl_single_subscriptions.read().contains_key(&265598));
    assert!(!acct_mgr.pnl_single_req_id_to_con_id.read().contains_key(&spy_pnl_req_id));

    // `list_open_positions` send failure (`[C-7]`) must NOT leave `waiting = true`!
    conn.set_fail_on_prefix(Some(b"61\0"));
    assert!(matches!(acct_mgr.list_open_positions(), Err(IBKRError::SocketError(_))));
    conn.set_fail_on_prefix(None);

    // `list_open_positions` position error (`[C-4]`) + control-char/oversized sanitization (`[S-3]`)!
    let conn_cb4 = conn.clone();
    conn.set_on_send(move |data| {
      if data.starts_with(b"61\0") {
        let c = conn_cb4.clone();
        std::thread::spawn(move || {
          std::thread::sleep(Duration::from_millis(10));
          let mut big_msg = b"4\02\0-1\0322\0Maximum number of position requests exceeded \x1b[31m\n".to_vec();
          big_msg.extend(std::iter::repeat(b'X').take(5000));
          big_msg.push(0);
          c.feed_wire_message(&big_msg);
        });
      }
    });
    let pos_refresh_err = acct_mgr
      .list_open_positions()
      .expect_err("Expected list_open_positions to fail immediately on position error");
    match pos_refresh_err {
      IBKRError::ApiError(322, msg) => {
        assert!(msg.len() <= 4100, "Oversized error msg must be capped, len={}", msg.len());
        assert!(!msg.contains('\x1b') && !msg.contains('\n'), "Control chars must be stripped");
      }
      other => panic!("Expected ApiError(322, _), got {:?}", other),
    }
  }

  #[test]
  fn test_in_flight_summary_error_during_position_send_cancels_positions_and_exec_error_propagates() {
    let (acct_mgr, conn) = setup_test_account_manager();

    // `[NEW-1]`: Summary Error 322 arrives *synchronously while `send_message(&position_msg)` is in flight*!
    // Because `position_msg` succeeded on the wire (`position_rejected == false`), `cancelPositions` MUST still be sent!
    let conn_cb = conn.clone();
    conn.set_on_send(move |data| {
      if data.starts_with(b"61\0") {
        conn_cb.feed_wire_message(
          b"4\02\08000\0322\0Error processing request.-'bT' : cause - Maximum number of account summary requests exceeded.\0",
        );
      }
    });
    let err = acct_mgr
      .subscribe_account_updates()
      .expect_err("Expected Error 322");
    assert!(matches!(err, IBKRError::ApiError(322, _)));
    let sent = conn.take_sent_strings();
    assert!(sent.iter().any(|m| m == "64|1|"), "Must still cancel positions when summary error arrived mid-send: {:?}", sent);

    // `[NEW-3]`: Error 321 on `get_day_executions()` (ReqID 8001, msg type 7) propagates `Err(ApiError(321, _))`
    // immediately without waiting the 2-second commission grace period!
    let conn_cb2 = conn.clone();
    conn.set_on_send(move |data| {
      if data.starts_with(b"7\0") {
        conn_cb2.feed_wire_message(b"4\02\08001\0321\0Invalid execution filter time format\0");
      }
    });
    let exec_start = std::time::Instant::now();
    let exec_err = acct_mgr
      .get_day_executions()
      .expect_err("Expected ApiError(321, _) on execution request");
    assert!(matches!(exec_err, IBKRError::ApiError(321, _)), "Got {:?}", exec_err);
    assert!(exec_start.elapsed() < Duration::from_millis(1500), "Must not wait 2s grace period on execution error");
  }

  #[test]
  fn test_all_account_summary_row_never_overwrites_real_account_and_stale_pnl_ignored() {
    let (acct_mgr, conn) = setup_test_account_manager();

    // Multi-account / FA scenario (`#1`): TWS sends DU12345 NetLiquidation=250000.0, DU99999=100000.0,
    // and then an aggregated `All` row NetLiquidation=350000.0 in BASE currency.
    let conn_cb = conn.clone();
    conn.set_on_send(move |data| {
      if data.starts_with(b"61\0") {
        conn_cb.feed_wire_message(b"63\01\08000\0DU12345\0NetLiquidation\0250000.0\0USD\0");
        conn_cb.feed_wire_message(b"63\01\08000\0DU99999\0NetLiquidation\0100000.0\0USD\0");
        conn_cb.feed_wire_message(b"63\01\08000\0All\0NetLiquidation\0350000.0\0BASE\0");
        conn_cb.feed_wire_message(b"63\01\08000\0All\0Currency\0USD\0USD\0");
        conn_cb.feed_wire_message(b"64\01\08000\0");
        conn_cb.feed_wire_message(b"62\01\0");
      }
    });

    let net_liq = acct_mgr.get_net_liquidation().expect("Should subscribe and return DU12345 equity");
    assert!((net_liq - 250000.0).abs() < 1e-6, "Aggregated All row must not overwrite DU12345: {}", net_liq);
    assert_eq!(acct_mgr.get_account_info().unwrap().base_currency, "USD");

    // Stale daily PnL frame (`#10b`) with wrong req_id (9999 instead of active 8001) must be ignored,
    // while active req_id (8001) updates DailyPnL!
    conn.feed_wire_message(b"94\09999\0-999.0\0-500.0\00.0\0");
    assert!(acct_mgr.get_daily_pnl().is_err(), "Stale PnL req_id 9999 must be ignored");
    conn.feed_wire_message(b"94\08001\01234.5\0500.0\0250.0\0");
    assert!((acct_mgr.get_daily_pnl().unwrap() - 1234.5).abs() < 1e-6);
  }

  #[test]
  fn test_pnl_single_filter_disabled_preserved_across_stop_and_post_send_cancel_race() {
    let (acct_mgr, conn) = setup_test_account_manager();

    let conn_cb = conn.clone();
    conn.set_on_send(move |data| {
      if data.starts_with(b"61\0") {
        conn_cb.feed_wire_message(b"63\01\08000\0DU12345\0NetLiquidation\0100000.0\0USD\0");
        conn_cb.feed_wire_message(b"64\01\08000\0");
        conn_cb.feed_wire_message(b"62\01\0");
      }
    });
    acct_mgr.subscribe_account_updates().unwrap();
    let _ = conn.take_sent_strings();

    // `#6`: `unsubscribe_pnl()` sets `PnlSingleFilter::Disabled`, so new positions do NOT auto-subscribe
    acct_mgr.unsubscribe_pnl().unwrap();
    conn.set_on_send(|_| {});
    conn.feed_wire_message(b"61\03\0DU12345\0265598\0SPY\0STK\0\00.0\0\0\0ARCA\0USD\0SPY\0SPY\0100\0450.0\0");
    assert!(acct_mgr.pnl_single_subscriptions.read().is_empty());
    assert!(conn.take_sent_strings().is_empty());

    // Re-enabling via `subscribe_pnl(None)` subscribes open SPY position, and `stop_account_updates()`
    // cancels active `PnLSingle` WITHOUT resetting `pnl_filter` to `Disabled`!
    acct_mgr.subscribe_pnl(None).unwrap();
    assert!(acct_mgr.pnl_single_subscriptions.read().contains_key(&265598));
    let _ = conn.take_sent_strings();

    acct_mgr.stop_account_updates().unwrap();
    assert!(acct_mgr.pnl_single_subscriptions.read().is_empty());
    let _ = conn.take_sent_strings();

    // Restarting account updates still auto-subscribes open SPY because `pnl_filter` stayed `All`!
    let conn_cb2 = conn.clone();
    conn.set_on_send(move |data| {
      if data.starts_with(b"61\0") {
        conn_cb2.feed_wire_message(b"63\01\08003\0DU12345\0NetLiquidation\0100000.0\0USD\0");
        conn_cb2.feed_wire_message(b"64\01\08003\0");
        conn_cb2.feed_wire_message(b"62\01\0");
      }
    });
    acct_mgr.subscribe_account_updates().unwrap();
    assert!(acct_mgr.pnl_single_subscriptions.read().contains_key(&265598));
    let _ = conn.take_sent_strings();

    // `#2`: Concurrent `unsubscribe_pnl()` racing on another thread *while `reqPnlSingle` ("94\0") is in flight*
    // must send `cancelPnlSingle` after `reqPnlSingle` hits the wire and leave maps empty!
    let mgr_race = acct_mgr.clone();
    let unsub_handle_slot: Arc<Mutex<Option<std::thread::JoinHandle<()>>>> = Arc::new(Mutex::new(None));
    let unsub_handle_slot_cb = unsub_handle_slot.clone();
    conn.set_on_send(move |data| {
      if data.starts_with(b"94\0") {
        let m = mgr_race.clone();
        let h = std::thread::spawn(move || {
          m.unsubscribe_pnl().unwrap();
        });
        // Deterministically wait until the concurrent `unsubscribe_pnl()` drains `pnl_single_subscriptions`
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !mgr_race.pnl_single_subscriptions.read().is_empty() {
          assert!(std::time::Instant::now() < deadline, "Timed out waiting for concurrent unsubscribe_pnl drain");
          std::thread::yield_now();
        }
        *unsub_handle_slot_cb.lock() = Some(h);
      }
    });
    conn.feed_wire_message(b"61\03\0DU12345\0756733\0QQQ\0STK\0\00.0\0\0\0NASDAQ\0USD\0QQQ\0QQQ\050\0400.0\0");
    if let Some(h) = unsub_handle_slot.lock().take() {
      h.join().unwrap();
    }
    assert!(acct_mgr.pnl_single_subscriptions.read().is_empty(), "Post-send check must clean up QQQ PnLSingle");
    let race_sent = conn.take_sent_strings();
    let last_cancel_idx = race_sent.iter().rposition(|m| m.starts_with("95|")).expect("Expected cancelPnlSingle");
    let req_idx = race_sent.iter().position(|m| m.starts_with("94|")).expect("Expected reqPnlSingle");
    assert!(last_cancel_idx > req_idx, "cancelPnlSingle must follow reqPnlSingle: {:?}", race_sent);
  }

  #[test]
  fn test_manual_position_refresh_aborts_on_stop_and_position_end_wakes_both() {
    let (acct_mgr, conn) = setup_test_account_manager();

    let conn_cb = conn.clone();
    conn.set_on_send(move |data| {
      if data.starts_with(b"61\0") {
        conn_cb.feed_wire_message(b"63\01\08000\0DU12345\0NetLiquidation\0100000.0\0USD\0");
        conn_cb.feed_wire_message(b"64\01\08000\0");
        conn_cb.feed_wire_message(b"62\01\0");
      }
    });
    acct_mgr.subscribe_account_updates().unwrap();
    let _ = conn.take_sent_strings();

    // `#3`: `list_open_positions()` waiting for `PositionEnd` must wake immediately when `stop_account_updates()` runs!
    let (refresh_sent_tx, refresh_sent_rx) = std::sync::mpsc::channel();
    conn.set_on_send(move |data| {
      if data.starts_with(b"61\0") {
        let _ = refresh_sent_tx.send(());
      }
    });
    let mgr_refresh = acct_mgr.clone();
    let refresh_handle = std::thread::spawn(move || mgr_refresh.list_open_positions());
    refresh_sent_rx.recv_timeout(Duration::from_secs(2)).expect("list_open_positions should send reqPositions");

    let stop_start = std::time::Instant::now();
    acct_mgr.stop_account_updates().unwrap();
    let refresh_res = refresh_handle.join().unwrap();
    assert!(refresh_res.is_err(), "list_open_positions must abort when account updates stop");
    assert!(stop_start.elapsed() < Duration::from_millis(1500), "list_open_positions must not hang 15s");

    // `#3`: `position_end()` must wake BOTH `manual_position_refresh_state` and `waiting_for_initial_position_end`
    {
      let mut r = acct_mgr.manual_position_refresh_state.lock();
      r.waiting = true;
      let mut u = acct_mgr.update_state.lock();
      u.is_initial_fetch_active = true;
      u.waiting_for_initial_position_end = true;
      u.waiting_for_initial_summary_end = false;
    }
    acct_mgr.position_end();
    assert!(!acct_mgr.manual_position_refresh_state.lock().waiting);
    assert!(!acct_mgr.update_state.lock().waiting_for_initial_position_end);
    acct_mgr.update_state.lock().is_initial_fetch_active = false;
  }

  #[test]
  fn test_executions_send_failure_cleans_up_state_and_utf8_error_sanitization() {
    let (acct_mgr, conn) = setup_test_account_manager();

    // `#5`: `get_day_executions()` cleans up `executions_state` when `send_message` fails
    conn.set_fail_on_prefix(Some(b"7\0"));
    let err = acct_mgr.get_day_executions().expect_err("Expected SocketError on execution send failure");
    assert!(matches!(err, IBKRError::SocketError(_)));
    assert!(acct_mgr.executions_state.lock().is_empty(), "executions_state must not leak on send error");
    conn.set_fail_on_prefix(None);

    // `#8`: `sanitize_error_msg` borrows clean non-ASCII UTF-8 without allocating, strips Unicode C1 controls,
    // and safely truncates multi-byte UTF-8 (`€`, 3 bytes) straddling byte 4096.
    use crate::parser_client::sanitize_error_msg;
    let utf8_clean = "Order rejected: €125,000.00 exceeds margin — ‘test’";
    assert!(matches!(sanitize_error_msg(utf8_clean), std::borrow::Cow::Borrowed(s) if s == utf8_clean));

    let c1_msg = "Bad\u{009B}31mCSI sequence";
    let c1_sanitized = sanitize_error_msg(c1_msg);
    assert!(matches!(c1_sanitized, std::borrow::Cow::Owned(_)));
    assert_eq!(c1_sanitized, "Bad 31mCSI sequence");

    let mut boundary_str = "A".repeat(4094);
    boundary_str.push('€'); // bytes 4094..4097 straddle 4096!
    boundary_str.push_str("tail");
    let truncated = sanitize_error_msg(&boundary_str);
    assert_eq!(truncated.len(), 4094 + 3);
    assert!(truncated.ends_with("..."));
  }

  #[test]
  fn test_observer_notification_never_reenters_subscribe_and_post_subscribe_teardown() {
    let (acct_mgr, conn) = setup_test_account_manager();
    let updates_count = Arc::new(AtomicUsize::new(0));
    acct_mgr.add_observer(CountingObserver {
      updates: updates_count.clone(),
    });

    // Subscribe with two open positions (SPY=265598, QQQ=756733)
    let conn_cb = conn.clone();
    conn.set_on_send(move |data| {
      if data.starts_with(b"61\0") {
        conn_cb.feed_wire_message(b"63\01\08000\0DU12345\0NetLiquidation\0100000.0\0USD\0");
        conn_cb.feed_wire_message(b"61\03\0DU12345\0265598\0SPY\0STK\0\00.0\0\0\0ARCA\0USD\0SPY\0SPY\0100\0450.0\0");
        conn_cb.feed_wire_message(b"61\03\0DU12345\0756733\0QQQ\0STK\0\00.0\0\0\0NASDAQ\0USD\0QQQ\0QQQ\050\0400.0\0");
        conn_cb.feed_wire_message(b"64\01\08000\0");
        conn_cb.feed_wire_message(b"62\01\0");
      }
    });
    acct_mgr.subscribe_account_updates().unwrap();
    let _ = conn.take_sent_strings();

    // `[N-3]`: `subscribe_pnl(Some(vec![265598]))` unsubscribes QQQ (756733) and keeps SPY (265598)
    let qqq_req_id = *acct_mgr.pnl_single_subscriptions.read().get(&756733).unwrap();
    conn.set_on_send(|_| {});
    acct_mgr.subscribe_pnl(Some(vec![265598])).unwrap();
    assert!(acct_mgr.pnl_single_subscriptions.read().contains_key(&265598));
    assert!(!acct_mgr.pnl_single_subscriptions.read().contains_key(&756733));
    let filter_sent = conn.take_sent_strings();
    assert!(filter_sent.contains(&format!("95|{}|", qqq_req_id)));

    // `[L-2]`: Error 200 (`NoSecurityDefinitionFound`) on SPY's `PnLSingle` req_id cleans up both maps
    let spy_req_id = *acct_mgr.pnl_single_subscriptions.read().get(&265598).unwrap();
    let err_200 = format!("4\02\0{}\0200\0No security definition has been found for the request\0", spy_req_id);
    conn.feed_wire_message(err_200.as_bytes());
    assert!(!acct_mgr.pnl_single_subscriptions.read().contains_key(&265598));
    assert!(!acct_mgr.pnl_single_req_id_to_con_id.read().contains_key(&spy_req_id));

    // Re-subscribe SPY PnLSingle, then trigger post-subscribe summary error 322 on ReqID 8000:
    // must tear down daily PnL, positions, and SPY PnLSingle, and transition cleanly to `Unsubscribed`!
    acct_mgr.subscribe_pnl(Some(vec![265598])).unwrap();
    let spy_req_id_2 = *acct_mgr.pnl_single_subscriptions.read().get(&265598).unwrap();
    let daily_pnl_req_id = acct_mgr.update_state.lock().current_pnl_req_id.unwrap();
    let _ = conn.take_sent_strings();

    conn.feed_wire_message(b"4\02\08000\0322\0Account summary stream terminated\0");
    assert!(!acct_mgr.is_subscribed());
    assert!(acct_mgr.pnl_single_subscriptions.read().is_empty());
    let teardown_sent = conn.take_sent_strings();
    assert!(teardown_sent.contains(&format!("93|{}|", daily_pnl_req_id)), "Sent: {:?}", teardown_sent);
    assert!(teardown_sent.contains(&"64|1|".to_string()), "Sent: {:?}", teardown_sent);
    assert!(teardown_sent.contains(&format!("95|{}|", spy_req_id_2)), "Sent: {:?}", teardown_sent);

    // `[H-1]`: Even with an active observer registered, `build_account_info()` / `account_value()` while
    // `Unsubscribed` must NEVER call `ensure_subscribed()` -> `subscribe_account_updates()` on the reader thread!
    let start = std::time::Instant::now();
    acct_mgr.account_value("NetLiquidation", "105000.0", Some("USD"), "DU12345");
    assert!(start.elapsed() < Duration::from_millis(1500));
    assert!(!acct_mgr.is_subscribed());
    assert!(conn.take_sent_strings().is_empty(), "Reader thread must never re-subscribe on account_value");
  }
}
