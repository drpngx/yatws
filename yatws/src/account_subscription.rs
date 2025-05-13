
//! Provides a subscription-based interface for receiving account-related events.
//!
//! The `AccountSubscription` struct allows users to get a focused stream of
//! account updates, including summary changes, position updates, and executions.

use crate::account::{AccountInfo, AccountObserver, Execution, Position};
use crate::account_manager::AccountManager;
use crate::base::IBKRError;
use chrono::{DateTime, Utc};
use crossbeam_channel::{Receiver, Sender, TryRecvError, RecvError};
use log::{debug, error, info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::collections::HashMap;

/// Represents different types of updates or events related to a subscribed account.
#[derive(Debug, Clone)]
pub enum AccountEvent {
  /// Emitted when the overall account summary (equity, cash, margin, etc.) is updated.
  SummaryUpdate {
    info: AccountInfo,
    timestamp: DateTime<Utc>,
  },
  /// Emitted when a specific portfolio position is updated.
  PositionUpdate {
    position: Position,
    timestamp: DateTime<Utc>,
  },
  /// Emitted when new execution details are received for the account.
  ExecutionUpdate {
    execution: Execution,
    timestamp: DateTime<Utc>,
  },
  /// Emitted if an error occurs relevant to the account subscription or a general account-level error.
  Error {
    error: IBKRError,
    timestamp: DateTime<Utc>,
  },
  /// Emitted when the `AccountSubscription` is closed.
  Closed {
    account_id: String,
    timestamp: DateTime<Utc>,
  },
}

struct InnerAccountObserver {
  event_sender: Sender<AccountEvent>,
  account_id_cache: Arc<parking_lot::RwLock<String>>, // To store account_id for Closed event
  // To update last known states directly from observer
  last_summary_cache: Arc<parking_lot::RwLock<Option<AccountInfo>>>,
  last_positions_cache: Arc<parking_lot::RwLock<HashMap<String, Position>>>,
}

impl AccountObserver for InnerAccountObserver {
  fn on_account_update(&self, info: &AccountInfo) {
    debug!("InnerAccountObserver: on_account_update for account {}", info.account_id);
    // Update caches
    *self.account_id_cache.write() = info.account_id.clone();
    *self.last_summary_cache.write() = Some(info.clone());

    if self.event_sender.send(AccountEvent::SummaryUpdate { info: info.clone(), timestamp: Utc::now() }).is_err() {
      warn!("AccountSubscription: Failed to send SummaryUpdate event, receiver likely dropped.");
    }
  }

  fn on_position_update(&self, position: &Position) {
    debug!("InnerAccountObserver: on_position_update for symbol {}", position.symbol);
    // Update cache
    self.last_positions_cache.write().insert(position.contract.con_id.to_string(), position.clone());

    if self.event_sender.send(AccountEvent::PositionUpdate { position: position.clone(), timestamp: Utc::now() }).is_err() {
      warn!("AccountSubscription: Failed to send PositionUpdate event, receiver likely dropped.");
    }
  }

  fn on_execution(&self, execution: &Execution) {
    debug!("InnerAccountObserver: on_execution for order_id {}", execution.order_id);
    if self.event_sender.send(AccountEvent::ExecutionUpdate { execution: execution.clone(), timestamp: Utc::now() }).is_err() {
      warn!("AccountSubscription: Failed to send ExecutionUpdate event, receiver likely dropped.");
    }
  }

  // Optional: Handle on_commission_report_for_order if AccountSubscription should emit specific commission events
  // For now, commissions are merged into Executions by AccountManager, and then an ExecutionUpdate is sent.

  // Optional: Handle general errors from AccountManager if needed, though they might be too broad
  // fn on_error(&self, error: &IBKRError) { ... }
}

/// Provides a focused, resource-managed way to handle the lifecycle and stream of events
/// for a specific trading account.
pub struct AccountSubscription {
  account_manager: Arc<AccountManager>,
  observer_id: Option<usize>,
  event_receiver: Receiver<AccountEvent>,
  event_sender: Sender<AccountEvent>, // Keep sender to send Closed event
  is_closed: Arc<AtomicBool>,
  account_id_cache: Arc<parking_lot::RwLock<String>>,
  last_summary_cache: Arc<parking_lot::RwLock<Option<AccountInfo>>>,
  last_positions_cache: Arc<parking_lot::RwLock<HashMap<String, Position>>>,
}

impl AccountSubscription {
  /// Creates a new subscription to account events.
  ///
  /// This method ensures that the underlying `AccountManager` is subscribed to TWS
  /// for account updates. It then registers an internal observer to process these
  /// updates into a stream of `AccountEvent`s.
  ///
  /// An initial attempt is made to populate the `last_known_summary` and
  /// `last_known_positions` from the `AccountManager`.
  pub fn new(account_manager: Arc<AccountManager>) -> Result<Self, IBKRError> {
    info!("Creating new AccountSubscription.");
    // Ensure the AccountManager's general subscription is active.
    // This call is blocking and fetches initial data into AccountManager.
    account_manager.subscribe_account_updates()?;
    debug!("AccountManager subscription active for new AccountSubscription.");

    let (event_sender, event_receiver) = crossbeam_channel::unbounded();

    let account_id_cache = Arc::new(parking_lot::RwLock::new(String::new()));
    let last_summary_cache = Arc::new(parking_lot::RwLock::new(None));
    let last_positions_cache = Arc::new(parking_lot::RwLock::new(HashMap::new()));

    // Populate initial state from AccountManager after its subscription is confirmed
    if let Ok(initial_info) = account_manager.get_account_info() {
      *account_id_cache.write() = initial_info.account_id.clone();
      *last_summary_cache.write() = Some(initial_info);
      debug!("AccountSubscription: Initial summary populated.");
    } else {
      warn!("AccountSubscription: Could not get initial account info.");
    }
    if let Ok(initial_positions) = account_manager.list_open_positions() {
      let mut cache_write = last_positions_cache.write();
      for pos in initial_positions {
        cache_write.insert(pos.contract.con_id.to_string(), pos);
      }
      debug!("AccountSubscription: Initial positions populated (count: {}).", cache_write.len());
    } else {
      warn!("AccountSubscription: Could not get initial positions list.");
    }


    let observer = InnerAccountObserver {
      event_sender: event_sender.clone(),
      account_id_cache: Arc::clone(&account_id_cache),
      last_summary_cache: Arc::clone(&last_summary_cache),
      last_positions_cache: Arc::clone(&last_positions_cache),
    };

    let observer_id = account_manager.add_observer(observer);
    debug!("AccountSubscription: Inner observer registered with ID {}.", observer_id);

    Ok(Self {
      account_manager,
      observer_id: Some(observer_id),
      event_receiver,
      event_sender,
      is_closed: Arc::new(AtomicBool::new(false)),
      account_id_cache,
      last_summary_cache,
      last_positions_cache,
    })
  }

  /// Returns the account ID that this subscription is monitoring.
  /// This ID is sourced from the `AccountManager`.
  pub fn account_id(&self) -> String {
    self.account_id_cache.read().clone()
  }

  /// Returns a clone of the most recent `AccountInfo` summary received by this subscription.
  /// Returns `None` if no summary has been processed yet.
  pub fn last_known_summary(&self) -> Option<AccountInfo> {
    self.last_summary_cache.read().clone()
  }

  /// Returns a clone of the list of most recent `Position` objects received by this subscription.
  /// Returns an empty vector if no positions have been processed.
  pub fn last_known_positions(&self) -> Vec<Position> {
    self.last_positions_cache.read().values().cloned().collect()
  }

  /// Blocks until the next `AccountEvent` is available from the subscription's internal channel.
  ///
  /// # Errors
  /// Returns `RecvError` if the channel is disconnected (e.g., subscription closed).
  pub fn next_event(&self) -> Result<AccountEvent, RecvError> {
    self.event_receiver.recv()
  }

  /// Attempts to receive the next `AccountEvent` without blocking.
  ///
  /// # Errors
  /// Returns `TryRecvError::Empty` if no event is currently available.
  /// Returns `TryRecvError::Disconnected` if the channel is disconnected.
  pub fn try_next_event(&self) -> Result<AccountEvent, TryRecvError> {
    self.event_receiver.try_recv()
  }

  /// Returns a clone of the `crossbeam_channel::Receiver` for `AccountEvent`s.
  /// This allows for more flexible event consumption patterns, such as integration
  /// into `select!` statements or having multiple event processors.
  pub fn events(&self) -> Receiver<AccountEvent> {
    self.event_receiver.clone()
  }

  /// Explicitly closes the account subscription.
  ///
  /// This unregisters its internal observer from the `AccountManager` and sends
  /// a final `AccountEvent::Closed` on its event channel.
  ///
  /// Note: This does not stop the `AccountManager`'s underlying subscription to TWS,
  /// as that might be shared. It only cleans up resources specific to this
  /// `AccountSubscription` instance.
  ///
  /// Returns `Ok(())` if successful, or an error if already closed or observer removal fails.
  pub fn close(&mut self) -> Result<(), IBKRError> {
    if self.is_closed.compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed).is_err() {
      debug!("AccountSubscription already closed or being closed.");
      return Ok(()); // Already closed or in process of closing
    }

    info!("Closing AccountSubscription for account '{}'.", self.account_id_cache.read());

    if let Some(id) = self.observer_id.take() {
      if self.account_manager.remove_observer(id) {
        debug!("AccountSubscription: Inner observer {} removed.", id);
      } else {
        warn!("AccountSubscription: Failed to remove observer {}. It might have already been removed.", id);
      }
    }

    let closed_event = AccountEvent::Closed {
      account_id: self.account_id_cache.read().clone(),
      timestamp: Utc::now(),
    };

    // Attempt to send Closed event, ignore error if receiver is already gone
    if self.event_sender.send(closed_event).is_err() {
      debug!("AccountSubscription: Could not send Closed event, receiver likely dropped.");
    }
    // The channel will naturally disconnect for receivers when `self` (and thus `event_sender`) is dropped.

    Ok(())
  }
}

impl Drop for AccountSubscription {
  fn drop(&mut self) {
    if let Err(e) = self.close() {
      error!("Error closing AccountSubscription on drop: {:?}", e);
    }
  }
}
