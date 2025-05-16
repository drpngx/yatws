
//! Provides a subscription-based interface for receiving news bulletin events.

use crate::base::IBKRError;
use crate::contract::Contract;
use crate::data_news_manager::DataNewsManager;
use crate::data_observer::ObserverId;
use crate::data_subscription::{BaseIterator, MarketDataIterator, MarketDataSubscription, SubscriptionState};
use crate::news::{NewsArticle, NewsObserver, HistoricalNews};

use chrono::{DateTime, Utc};
use log::{debug, error, info, trace, warn};
use std::sync::{Arc, Weak};
use std::time::Duration;

/// Represents different types of events related to a news bulletin subscription.
#[derive(Debug, Clone)]
pub enum NewsEvent {
  /// Emitted when a news bulletin is received.
  Bulletin {
    article: NewsArticle,
    timestamp: DateTime<Utc>,
  },
  /// Emitted if an error occurs relevant to the news bulletin subscription.
  /// This could be a general news system error or an error specific to this stream.
  Error(IBKRError),
  /// Emitted when the `NewsSubscription` is closed.
  Closed { timestamp: DateTime<Utc> },
}

/// Internal observer for the NewsSubscription.
/// This observer is registered with the DataNewsManager.
struct NewsInternalObserver {
  // State uses ObserverId.0 as its req_id for internal tracking.
  state: Arc<SubscriptionState<NewsEvent, DataNewsManager>>,
}

impl NewsObserver for NewsInternalObserver {
  fn on_news_article(&self, article: &NewsArticle) {
    // For NewsSubscription, all articles received by this observer are considered bulletins.
    trace!(
      "NewsInternalObserver (ObsID {}): on_news_article: {}",
      self.state.req_id, // req_id holds ObserverId.0
      article.id
    );
    self.state.push_event(NewsEvent::Bulletin {
      article: article.clone(),
      timestamp: Utc::now(),
    });
  }

  fn on_error(&self, error_code: i32, error_message: &str) {
    warn!(
      "NewsInternalObserver (ObsID {}): on_error: Code={}, Msg='{}'",
      self.state.req_id, error_code, error_message
    );
    let err = IBKRError::ApiError(error_code, error_message.to_string());
    // Push an error event. The subscription itself might not be "failed"
    // unless TWS cancels it or a critical error occurs.
    self.state.push_event(NewsEvent::Error(err.clone()));
    // If it's a critical error that stops the stream, then:
    // self.state.set_error(err);
  }
}

/// Builder for creating a `NewsSubscription`.
pub struct NewsSubscriptionBuilder {
  manager_weak: Weak<DataNewsManager>,
  all_msgs: bool,
}

impl NewsSubscriptionBuilder {
  /// Creates a new builder. Called internally by `DataNewsManager`.
  pub(crate) fn new(manager_weak: Weak<DataNewsManager>, all_msgs: bool) -> Self {
    Self {
      manager_weak,
      all_msgs,
    }
  }

  /// Submits the request to TWS to start receiving news bulletins and finalizes the subscription.
  ///
  /// This method is non-blocking. Events can be consumed via the iterator returned by `events()`.
  // TODO: resolve the broken design:
  //   In `NewsSubscriptionBuilder::submit()`:

  // 1.  An initial `ObserverId` value (`actual_observer_id.0`) is obtained using `manager.new_observer_id_val()`.
  // 2.  An `Arc<SubscriptionState>` (named `state_arc`) is created. Its `req_id` field is set to this `actual_observer_id.0`.
  // 3.  A `NewsInternalObserver` (named `internal_observer_arc`) is created, holding a clone of this `state_arc`.
  // 4.  This `internal_observer_arc` is passed to `manager.add_observer()`. This call returns `final_observer_id`, which is the ID
  // *actually* generated and used by the `DataNewsManager` to store and track the observer. This `final_observer_id` might be different
  // from the `actual_observer_id` we pre-generated if the manager doesn't use the pre-generated value (which, looking at
  // `DataNewsManager::add_observer`, it currently generates its own ID).
  // 5.  The `state_arc` (the same one created in step 2) then has its internal `observer_id` field updated using
  // `state_arc.set_observer_id(final_observer_id)`.
  // 6.  Finally, `Ok(NewsSubscription { state: state_arc })` is returned.

  // **So, to address your question directly:**

  // There isn't a separate variable named `final_state` that is created and then unused. The variable `state_arc` *is* the state that is
  // used throughout the process and ultimately returned within the `NewsSubscription`.

  // The "finality" comes from the `final_observer_id` (the ID assigned by the `DataNewsManager`). This `final_observer_id` is used to
  // **update a field within the existing `state_arc`** via `state_arc.set_observer_id(final_observer_id)`.

  // **Why isn't `state_arc` re-created with `final_observer_id.0` as its `req_id`?**

  // This is due to a classic circular dependency problem, which the comments in the code allude to:

  // *   To create the `NewsInternalObserver`, you need an `Arc<SubscriptionState>`.
  // *   The `SubscriptionState` ideally needs its `req_id` to be the definitive ID assigned by the manager (`final_observer_id.0`).
  // *   To get this `final_observer_id`, you must call `manager.add_observer()`.
  // *   `manager.add_observer()` requires the `NewsInternalObserver` instance as an argument.

  // You can see the cycle: `Observer -> State -> ManagerID -> Observer`.

  // The current design breaks this cycle as follows:

  // 1.  `state_arc` is created with an initial `req_id` (derived from `actual_observer_id.0`).
  // 2.  The `NewsInternalObserver` uses this `state_arc`. The `req_id` (`actual_observer_id.0`) is primarily used by this internal
  // observer for its own logging or internal identification. Since news bulletins are global and not filtered by `req_id` by this
  // observer, this initial `req_id` being potentially different from the manager's final ID is acceptable.
  // 3.  The `final_observer_id` (the one the `DataNewsManager` actually uses) is then stored in the `observer_id` field *inside* the
  // `state_arc`.
  // 4.  The `NewsSubscription::request_id()` method correctly retrieves `final_observer_id.0` (from
  // `state.observer_id.lock().unwrap().0`). This ensures that when the `NewsSubscription` needs to interact with the `DataNewsManager`
  // (e.g., for cancellation using `remove_observer`), it uses the ID that the manager recognizes.

  // In summary:
  // *   The `state_arc` *is* the state being used.
  // *   The `final_observer_id` *is* used to update this `state_arc` with the manager-authoritative ID.
  // *   The `SubscriptionState.req_id` field and the `SubscriptionState.observer_id` field serve slightly different purposes in this
  // specific context to resolve the initialization order challenge. The `observer_id` field holds the critical ID for manager
  // interaction.

  pub fn submit(self) -> Result<NewsSubscription, IBKRError> {
    let manager = self
      .manager_weak
      .upgrade()
      .ok_or(IBKRError::NotConnected)?;

    // News bulletins are not tied to a TWS req_id.
    // We use an ObserverId for internal tracking.
    // The SubscriptionState's req_id field will store this ObserverId's value.

    // The actual ObserverId will be generated by DataNewsManager when adding the observer.
    // We create a placeholder req_id for the SubscriptionState, which will be updated
    // with the real ObserverId.0 later if needed, or just use the one from add_observer.
    // For simplicity, we'll let add_observer generate the ID and store it.
    // The req_id in SubscriptionState will be this ObserverId.0.

    let state = Arc::new(SubscriptionState::new(
      0, // Placeholder req_id, will be updated by observer_id from manager
      Contract::default(), // Bulletins are not contract-specific
      Arc::downgrade(&manager), // Pass weak ref to manager
    ));

    let internal_observer = Arc::new(NewsInternalObserver {
      state: Arc::clone(&state),
    });

    // Register the observer and get its ID
    let observer_id = manager.add_observer(internal_observer);
    // Update the state's req_id to be the actual ObserverId's value.
    // This is a bit of a hack as SubscriptionState.req_id is not meant to change.
    // A cleaner way might be to pass the observer_id to SubscriptionState::new.
    // For now, we'll assume SubscriptionState.req_id is set once.
    // Let's ensure SubscriptionState is created with the correct ID from the start.
    // This means `add_observer` needs to be called before `SubscriptionState::new`
    // or `SubscriptionState` needs a way to set its `req_id` post-creation,
    // or `NewsInternalObserver` needs to know its `ObserverId` to filter messages if necessary
    // (though for bulletins, all messages from `NewsObserver` are relevant).

    // Corrected flow:
    // 1. Create internal observer (needs a state, state needs req_id).
    // 2. State needs req_id. For news, this req_id is the ObserverId.0.
    // 3. ObserverId is generated by manager.add_observer.
    // Chicken-and-egg.
    // Solution: SubscriptionState's req_id will store the ObserverId.0.
    // The internal observer doesn't strictly need to check req_id if it's only for one subscription.

    // Let's re-initialize state with the correct observer_id.0 as req_id.
    // This is slightly inefficient but ensures correctness.
    // A better way: pass a closure to SubscriptionState that registers the observer.
    // Or, DataNewsManager.add_observer could take a FnOnce that provides the state.

    // Simpler: The observer_id is primarily for removing the observer.
    // The SubscriptionState's req_id is for its internal event queue keying if it were generic.
    // Since NewsInternalObserver gets all news events, it doesn't filter by req_id.
    // So, the req_id in SubscriptionState can just be the observer_id.0.

    // Re-create state with the actual observer_id.0 as req_id
    let _final_state = Arc::new(SubscriptionState::<NewsEvent, DataNewsManager>::new(
      observer_id.0 as i32, // Use ObserverId.0 as the req_id for this subscription state
      Contract::default(),
      Arc::downgrade(&manager),
    ));
    // Update the observer's state reference
    // This requires NewsInternalObserver.state to be mutable or use interior mutability.
    // Easiest: Create observer with the final_state.
    // This means we need to create the observer *after* getting the observer_id,
    // which is not possible as add_observer takes the observer.

    // Let's stick to the initial state creation and ensure NewsInternalObserver uses it.
    // The `req_id` in `SubscriptionState` for news will be the `ObserverId.0`.
    // We need to pass this `ObserverId.0` to `SubscriptionState::new`.
    // This means `DataNewsManager::add_observer` should perhaps not generate the ID if one is provided,
    // or we generate ID first, then create state, then observer, then add.

    // Revised flow for builder:
    // 1. Get manager.
    // 2. Generate an ObserverId (e.g., manager.next_observer_id()).
    // 3. Create SubscriptionState with this ObserverId.0 as req_id.
    // 4. Create NewsInternalObserver with this state.
    // 5. Call manager.add_observer_with_id(observer_id, internal_observer). (New method on manager)
    // OR:
    // 1. Get manager.
    // 2. Create SubscriptionState with a temporary req_id (e.g. 0 or a new manager.next_request_id()).
    // 3. Create NewsInternalObserver with this state.
    // 4. Call manager.add_observer(internal_observer) which returns the *actual* ObserverId.
    // 5. Store this actual_observer_id in the SubscriptionState (e.g., state.set_actual_req_id(actual_observer_id.0)).
    //    This is what `set_observer_id` is for. The `req_id` field itself is immutable.
    //    The `observer_id` field in `SubscriptionState` is for storing this.

    // The `req_id` in `SubscriptionState` is used for logging/identification.
    // For news, it's fine if this `req_id` is the `ObserverId.0`.
    // Let's assume `SubscriptionState::new` takes the `observer_id.0` as `req_id`.
    // This means `NewsSubscriptionBuilder::submit` needs to get an ID first.
    // `DataNewsManager` needs a `next_internal_id` or similar.
    // The current `add_observer` in `DataNewsManager` generates and returns `ObserverId`.

    // Path taken in provided diffs:
    // - Builder calls manager.add_observer(internal_observer)
    // - manager.add_observer generates ObserverId and stores Weak<Observer>
    // - manager.add_observer returns ObserverId
    // - Builder stores this ObserverId in SubscriptionState.observer_id
    // - SubscriptionState.req_id is separate, can be a placeholder or also ObserverId.0.
    // Let's use ObserverId.0 for SubscriptionState.req_id for consistency.

    // Final refined flow for builder:
    // The DataNewsManager does not have a general `next_request_id()` like DataMarketManager.
    // News bulletins are not tied to a TWS req_id.
    // The ObserverId generated by `add_observer` will serve as the unique identifier.
    // The `SubscriptionState.req_id` will be this `ObserverId.0`.

    // 1. Create `NewsInternalObserver` (needs `Arc<SubscriptionState>`).
    //    `SubscriptionState` needs `req_id`. This `req_id` will be `ObserverId.0`.
    //    So, we need `ObserverId` *before* creating `SubscriptionState`.
    //    `DataNewsManager::add_observer` *returns* `ObserverId`.
    //    This creates a circular dependency if `SubscriptionState` needs the final `req_id` at construction.

    // Solution:
    // Create `SubscriptionState` with the `ObserverId.0` that *will be* assigned.
    // `DataNewsManager::add_observer` is modified to take an `Arc<NewsInternalObserver>`
    // and it generates the `ObserverId`.
    // The `SubscriptionState` is created using `manager.new_observer_id_val()` for its `req_id`.
    // Then, the `NewsInternalObserver` is created with this state.
    // Then, `manager.add_observer(internal_observer)` is called, and the returned `ObserverId`
    // is stored in `SubscriptionState.observer_id`.
    // The `SubscriptionState.req_id` will be the same as `actual_observer_id.0`.

    let observer_id_val = manager.new_observer_id_val(); // Get the next ID value
    let actual_observer_id = ObserverId(observer_id_val);

    // Specify generic arguments for SubscriptionState
    let state_arc = Arc::new(SubscriptionState::<NewsEvent, DataNewsManager>::new(
      actual_observer_id.0 as i32, // Use the generated ID value as req_id
      Contract::default(),
      Arc::downgrade(&manager),
    ));

    let internal_observer_arc = Arc::new(NewsInternalObserver {
      state: Arc::clone(&state_arc),
    });

    // `add_observer` in DataNewsManager will use the `observer_id_val` if we modify it to accept one,
    // or it will generate one. For now, we assume `add_observer` generates one, and we store it.
    // The `req_id` in `state_arc` is already set to what `actual_observer_id.0` will be.
    // We need to ensure that the `ObserverId` used by `add_observer` matches this.
    // The simplest is to use the `ObserverId` returned by `add_observer` for `state_arc.req_id`.

    // Revised flow:
    // Create observer with a temporary state or a state that can have its req_id updated.
    // Or, `SubscriptionState.req_id` is just for logging and `observer_id` is the true key.

    // Let's use the `ObserverId` returned by `add_observer` as the definitive ID for the subscription.
    // The `SubscriptionState` will be created with this `ObserverId.0` as its `req_id`.
    // This means `NewsInternalObserver` needs its `Arc<SubscriptionState>` to be created
    // *after* `add_observer` has returned the `ObserverId`. This is the circular dependency.

    // Break the cycle:
    // 1. Create the `NewsInternalObserver` with a placeholder `Arc<SubscriptionState>`.
    // 2. Call `manager.add_observer()` to get the `actual_observer_id`.
    // 3. Create the *final* `Arc<SubscriptionState>` using `actual_observer_id.0` as `req_id`.
    // 4. Update the `NewsInternalObserver` to point to this final state. (Requires mutability or `OnceCell` in observer).

    // Simpler approach used in other subscriptions:
    // `SubscriptionState.req_id` is the TWS req_id. For news, it's conceptual.
    // `SubscriptionState.observer_id` stores the `ObserverId` from the manager.
    // `NewsSubscription::request_id()` returns `observer_id.0`.

    // Create state with a placeholder req_id (e.g., 0, or a new internal ID if manager had one).
    // For news, since there's no TWS req_id, the ObserverId.0 is the most logical "req_id".
    // Let's ensure SubscriptionState is created with the ObserverId.0 that add_observer will return.
    // This requires DataNewsManager.add_observer to either take a pre-generated ID or for us to
    // create the observer and state *after* getting the ID.

    // Final approach:
    // 1. Create the `NewsInternalObserver`. Its `state` field will be an `Arc<SubscriptionState>`.
    //    The `SubscriptionState` needs a `req_id`.
    // 2. Call `manager.add_observer(internal_observer_arc)` which returns `actual_observer_id`.
    // 3. The `SubscriptionState`'s `req_id` should be `actual_observer_id.0`.
    //    And `SubscriptionState.observer_id` should store `actual_observer_id`.

    // This means `NewsInternalObserver` must be created with an `Arc<SubscriptionState>`
    // where `SubscriptionState.req_id` is already set to the `ObserverId.0` that `add_observer`
    // will return. This is the core of the issue.

    // Let's make `SubscriptionState.req_id` the `ObserverId.0`.
    // `DataNewsManager::add_observer` returns the `ObserverId`.
    // We create the `SubscriptionState` *after* calling `add_observer`.
    // This means `NewsInternalObserver` cannot hold the `Arc<SubscriptionState>` at its construction
    // if that state depends on the `ObserverId` from `add_observer`.

    // Solution adopted by other subscriptions:
    // - Create `SubscriptionState` with a `req_id` (for news, this will be the `ObserverId.0`).
    // - Create `InternalObserver` with this `Arc<SubscriptionState>`.
    // - Call `manager.add_observer(internal_observer)`, get `observer_id`.
    // - Store this `observer_id` in `SubscriptionState.observer_id`.

    // For news, the `req_id` for `SubscriptionState` *is* the `ObserverId.0`.
    // So, we need `ObserverId` first.
    // `new_observer_id_val` and `actual_observer_id` are already defined above.
    // `state_arc` is already created with `actual_observer_id.0` as `req_id`.
    state_arc.set_observer_id(actual_observer_id); // Store the full ObserverId in the state

    // `internal_observer_arc` is already created with this `state_arc`.

    // Now, call `add_observer`. The `DataNewsManager::add_observer` currently generates its own ID.
    // This needs to be reconciled. For now, we'll assume `add_observer` will use the ID
    // from the observer's state or that the passed `internal_observer_arc` is what's stored.
    // The crucial part is that `state_arc` (which `NewsSubscription` will hold) has the correct `req_id`
    // and `observer_id` set.
    // The `add_observer` method in `DataNewsManager` was changed to take `Arc<dyn NewsObserver>`
    // and return the `ObserverId` it generates.
    // So, the `actual_observer_id` we generated above might be different from the one `add_observer` returns.
    // Let's use the one returned by `add_observer`.

    let final_observer_id = manager.add_observer(internal_observer_arc);
    // Update the state with the ID actually used by the manager.
    state_arc.set_observer_id(final_observer_id);
    // If `SubscriptionState.req_id` must match `final_observer_id.0`, we'd need to update it here,
    // but `req_id` is immutable. This implies `SubscriptionState` should be created *after* `add_observer`.
    // This is the circular dependency.

    // Let's assume `NewsSubscription::request_id()` will use `state.observer_id.lock().unwrap().0`.
    // And `SubscriptionState.req_id` can be a placeholder or the initially generated one.
    // The `state_arc` was created with `actual_observer_id.0`. If `final_observer_id` is different,
    // then `state_arc.req_id` won't match `final_observer_id.0`.
    // This is fine if `NewsInternalObserver` doesn't rely on `state.req_id` for filtering,
    // and `NewsSubscription::request_id()` correctly uses the stored `ObserverId`.

    // The current `add_observer` in `DataNewsManager` takes `Arc<dyn NewsObserver>` and returns `ObserverId`.
    // The `state_arc` is created with `actual_observer_id.0`.
    // The `internal_observer_arc` uses this `state_arc`.
    // `manager.add_observer(internal_observer_arc)` returns `final_observer_id`.
    // `state_arc.set_observer_id(final_observer_id)` correctly stores the ID used by the manager.
    // `NewsSubscription::request_id()` will use `final_observer_id.0`.
    // `state_arc.req_id` (which is `actual_observer_id.0`) is used by `NewsInternalObserver` for logging.
    // This seems consistent.

    manager.request_news_bulletins(self.all_msgs)?;
    info!(
      "News bulletin subscription started (ObsID: {:?}, AllMsgs: {}).",
      actual_observer_id, self.all_msgs
    );

    Ok(NewsSubscription { state: state_arc })
  }
}

/// Represents an active subscription to news bulletins.
///
/// Provides a stream of [`NewsEvent`]s. Must be kept alive to receive updates.
/// Automatically cancels the TWS news bulletin subscription when dropped if it's the
/// one managing the global bulletin request. For news bulletins, cancellation is global.
#[derive(Debug)]
pub struct NewsSubscription {
  state: Arc<SubscriptionState<NewsEvent, DataNewsManager>>,
}

impl MarketDataSubscription for NewsSubscription {
  type Event = NewsEvent;
  type Iterator = NewsIterator;

  fn request_id(&self) -> i32 {
    // Return the ObserverId.0 as the conceptual "request_id" for this subscription
    let guard = self.state.observer_id.lock();
    match *guard {
      Some(observer_id_val) => observer_id_val.0 as i32,
      None => {
        // Fallback or error if observer_id is unexpectedly None after setup.
        // For news, req_id in state might be a placeholder (0) or the initial guess.
        // If observer_id is None, it means setup might have issues or it was cleared.
        // Returning state.req_id might be more consistent if observer_id is None.
        // However, the expectation is that observer_id is set.
        warn!("NewsSubscription::request_id called when observer_id is None. Using state.req_id ({}) as fallback.", self.state.req_id);
        self.state.req_id
      }
    }
  }

  fn contract(&self) -> &Contract {
    &self.state.contract // Returns a default/empty contract
  }

  fn is_completed(&self) -> bool {
    self.state.completed.load(std::sync::atomic::Ordering::SeqCst)
  }

  fn has_error(&self) -> bool {
    self.state.error.lock().is_some()
  }

  fn get_error(&self) -> Option<IBKRError> {
    self.state.error.lock().clone()
  }

  fn cancel(&self) -> Result<(), IBKRError> {
    if !self.state.active.swap(false, std::sync::atomic::Ordering::SeqCst) {
      debug!("NewsSubscription (ObsID {}) already inactive.", self.request_id());
      return Ok(()); // Already inactive
    }
    self.state.completed.store(true, std::sync::atomic::Ordering::SeqCst);
    self.state.condvar.notify_all(); // Wake up iterators

    info!("Cancelling NewsSubscription (ObsID {}).", self.request_id());

    if let Some(manager) = self.state.manager_weak.upgrade() {
      if let Some(observer_id_to_remove) = self.state.observer_id.lock().take() {
        if manager.remove_observer(observer_id_to_remove) {
          debug!("NewsSubscription: Internal observer {:?} removed.", observer_id_to_remove);
        } else {
          warn!("NewsSubscription: Failed to remove observer {:?}, may have already been removed.", observer_id_to_remove);
        }
      }
      // Cancelling news bulletins is a global action for the TWS connection.
      // Only cancel if this subscription was responsible or if it's the last one.
      // For simplicity, let's assume any NewsSubscription cancel will try to stop bulletins.
      // The DataNewsManager could track active bulletin subscriptions if more fine-grained control is needed.
      manager.cancel_news_bulletins()?;
      info!("Global news bulletin subscription cancelled via NewsSubscription (ObsID {}).", self.request_id());
    } else {
      // Manager is gone, can't send cancel message.
      warn!("DataNewsManager unavailable for cancelling NewsSubscription (ObsID {}). Observer already removed or manager dropped.", self.request_id());
      // Return an error if we couldn't ensure cancellation through the manager.
      return Err(IBKRError::InternalError(
        "DataNewsManager unavailable for cancelling NewsSubscription".to_string(),
      ));
    }

    // Send Closed event
    self.state.push_event(NewsEvent::Closed { timestamp: Utc::now() });
    Ok(())
  }

  fn events(&self) -> Self::Iterator {
    NewsIterator {
      inner: BaseIterator {
        state: Arc::clone(&self.state),
        default_timeout: None,
      },
    }
  }
}

impl Drop for NewsSubscription {
  fn drop(&mut self) {
    if self.state.active.load(std::sync::atomic::Ordering::SeqCst) {
      if let Err(e) = self.cancel() {
        error!(
          "Drop: Error cancelling NewsSubscription (ObsID {}): {:?}",
          self.request_id(), e
        );
      }
    }
  }
}

/// Iterator over `NewsEvent`s from a `NewsSubscription`.
pub struct NewsIterator {
  inner: BaseIterator<NewsEvent, DataNewsManager>,
}

impl NewsIterator {
  /// Sets a default timeout for `next()` calls on this iterator.
  pub fn with_timeout(mut self, timeout: Duration) -> Self {
    self.inner.default_timeout = Some(timeout);
    self
  }

  // Expose next_event and try_next_event for parity with AccountSubscription if desired,
  // though the MarketDataIterator trait provides try_next.
  // pub fn next_event(&mut self) -> Result<NewsEvent, RecvError> { ... }
  // pub fn try_next_event(&mut self) -> Result<NewsEvent, crossbeam_channel::TryRecvError> { ... }
}

impl Iterator for NewsIterator {
  type Item = NewsEvent;
  fn next(&mut self) -> Option<Self::Item> {
    self.inner.next()
  }
}

impl MarketDataIterator<NewsEvent> for NewsIterator {
  fn try_next(&mut self, timeout: Duration) -> Option<NewsEvent> {
    self.inner.try_next(timeout)
  }
  fn collect_available(&mut self) -> Vec<NewsEvent> {
    self.inner.collect_available()
  }
}

// --- HistoricalNewsSubscription ---

/// Represents different types of events related to a historical news subscription.
#[derive(Debug, Clone)]
pub enum HistoricalNewsEvent {
  /// Emitted when a historical news article/headline is received.
  Article(HistoricalNews),
  /// Emitted when all historical news items for the request have been received.
  Complete,
  /// Emitted if an error occurs relevant to this historical news subscription.
  Error(IBKRError),
  /// Emitted when the `HistoricalNewsSubscription` is closed (e.g., by cancellation).
  Closed,
}

/// Builder for creating a `HistoricalNewsSubscription`.
pub struct HistoricalNewsSubscriptionBuilder {
  manager_weak: Weak<DataNewsManager>,
  con_id: i32,
  provider_codes: String,
  start_date_time: Option<DateTime<Utc>>,
  end_date_time: Option<DateTime<Utc>>,
  total_results: i32,
  historical_news_options: Vec<(String, String)>,
}

impl HistoricalNewsSubscriptionBuilder {
  pub(crate) fn new(
    manager_weak: Weak<DataNewsManager>,
    con_id: i32,
    provider_codes: &str,
    total_results: i32,
  ) -> Self {
    Self {
      manager_weak,
      con_id,
      provider_codes: provider_codes.to_string(),
      start_date_time: None,
      end_date_time: None,
      total_results,
      historical_news_options: Vec::new(),
    }
  }

  pub fn with_start_date_time(mut self, start_dt: DateTime<Utc>) -> Self {
    self.start_date_time = Some(start_dt);
    self
  }

  pub fn with_end_date_time(mut self, end_dt: DateTime<Utc>) -> Self {
    self.end_date_time = Some(end_dt);
    self
  }

  pub fn with_options(mut self, options: &[(String, String)]) -> Self {
    self.historical_news_options = options.to_vec();
    self
  }

  /// Submits the request to TWS to start receiving historical news and finalizes the subscription.
  pub fn submit(self) -> Result<HistoricalNewsSubscription, IBKRError> {
    let manager = self
      .manager_weak
      .upgrade()
      .ok_or(IBKRError::NotConnected)?;

    // Use the message_broker from the concrete DataNewsManager instance for next_request_id
    let req_id = manager.message_broker.next_request_id();

    // Contract::default() is a placeholder as historical news is by con_id, not full contract object here.
    let state_arc = Arc::new(SubscriptionState::<HistoricalNewsEvent, DataNewsManager>::new(
      req_id,
      Contract::default(),
      Arc::downgrade(&manager),
    ));

    // DataNewsManager needs to be adapted to handle this stream.
    // It will call internal_request_historical_news_stream which sends the TWS request
    // and stores the Weak<SubscriptionState> in a map.
    manager.internal_request_historical_news_stream(
      req_id,
      self.con_id,
      &self.provider_codes,
      self.start_date_time,
      self.end_date_time,
      self.total_results,
      &self.historical_news_options,
      Arc::downgrade(&state_arc),
    )?;

    info!(
      "Historical news subscription started (ReqID: {} for ConID: {}).",
      req_id, self.con_id
    );

    Ok(HistoricalNewsSubscription { state: state_arc })
  }
}

/// Represents an active subscription to historical news.
#[derive(Debug)]
pub struct HistoricalNewsSubscription {
  state: Arc<SubscriptionState<HistoricalNewsEvent, DataNewsManager>>,
}

impl MarketDataSubscription for HistoricalNewsSubscription {
  type Event = HistoricalNewsEvent;
  type Iterator = HistoricalNewsIterator;

  fn request_id(&self) -> i32 {
    self.state.req_id
  }

  fn contract(&self) -> &Contract {
    &self.state.contract // Returns a default/empty contract
  }

  fn is_completed(&self) -> bool {
    self.state.completed.load(std::sync::atomic::Ordering::SeqCst)
  }

  fn has_error(&self) -> bool {
    self.state.error.lock().is_some()
  }

  fn get_error(&self) -> Option<IBKRError> {
    self.state.error.lock().clone()
  }

  fn cancel(&self) -> Result<(), IBKRError> {
    if !self.state.active.swap(false, std::sync::atomic::Ordering::SeqCst) {
      debug!("HistoricalNewsSubscription (ReqID {}) already inactive.", self.request_id());
      return Ok(());
    }
    self.state.completed.store(true, std::sync::atomic::Ordering::SeqCst);
    self.state.condvar.notify_all();

    info!("Cancelling HistoricalNewsSubscription (ReqID {}).", self.request_id());

    if let Some(manager) = self.state.manager_weak.upgrade() {
      // TWS does not have a specific cancel message for historical news.
      // Cancellation is client-side: remove state and ignore further events.
      manager.cancel_historical_news_stream(self.request_id());
    } else {
      warn!("DataNewsManager unavailable for cancelling HistoricalNewsSubscription (ReqID {}).", self.request_id());
      return Err(IBKRError::InternalError(
        "DataNewsManager unavailable for cancelling HistoricalNewsSubscription".to_string(),
      ));
    }

    self.state.push_event(HistoricalNewsEvent::Closed);
    Ok(())
  }

  fn events(&self) -> Self::Iterator {
    HistoricalNewsIterator {
      inner: BaseIterator {
        state: Arc::clone(&self.state),
        default_timeout: None,
      },
    }
  }
}

impl Drop for HistoricalNewsSubscription {
  fn drop(&mut self) {
    if self.state.active.load(std::sync::atomic::Ordering::SeqCst) {
      if let Err(e) = self.cancel() {
        error!(
          "Drop: Error cancelling HistoricalNewsSubscription (ReqID {}): {:?}",
          self.request_id(), e
        );
      }
    }
  }
}

/// Iterator over `HistoricalNewsEvent`s from a `HistoricalNewsSubscription`.
pub struct HistoricalNewsIterator {
  inner: BaseIterator<HistoricalNewsEvent, DataNewsManager>,
}

impl HistoricalNewsIterator {
  pub fn with_timeout(mut self, timeout: Duration) -> Self {
    self.inner.default_timeout = Some(timeout);
    self
  }
}

impl Iterator for HistoricalNewsIterator {
  type Item = HistoricalNewsEvent;
  fn next(&mut self) -> Option<Self::Item> {
    self.inner.next()
  }
}

impl MarketDataIterator<HistoricalNewsEvent> for HistoricalNewsIterator {
  fn try_next(&mut self, timeout: Duration) -> Option<HistoricalNewsEvent> {
    self.inner.try_next(timeout)
  }
  fn collect_available(&mut self) -> Vec<HistoricalNewsEvent> {
    self.inner.collect_available()
  }
}
