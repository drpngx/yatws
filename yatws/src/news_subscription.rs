// yatws/src/news_subscription.rs
//! Provides a subscription-based interface for receiving news bulletin events.

use crate::base::IBKRError;
use crate::contract::Contract;
use crate::data_news_manager::DataNewsManager;
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
  pub fn submit(self) -> Result<NewsSubscription, IBKRError> {
    let manager = self
      .manager_weak
      .upgrade()
      .ok_or(IBKRError::NotConnected)?;

    // 1. Use a simple unique ID for the req_id - doesn't need to match observer_id
    let req_id = manager.message_broker.next_request_id();

    // 2. Create state with this req_id
    let state_arc = Arc::new(SubscriptionState::<NewsEvent, DataNewsManager>::new(
      req_id,
      Contract::default(),
      Arc::downgrade(&manager),
    ));

    // 3. Create observer with state
    let internal_observer = Arc::new(NewsInternalObserver {
      state: Arc::clone(&state_arc),
    });

    // 4. Register and get observer_id
    let observer_id = manager.add_observer(internal_observer);

    // 5. Store the observer_id in state for cancellation purposes
    state_arc.set_observer_id(observer_id);

    manager.request_news_bulletins(self.all_msgs)?;
    info!(
      "News bulletin subscription started (ObsID: {:?}, AllMsgs: {}).",
      observer_id, self.all_msgs
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

  fn cancel(&mut self) -> Result<(), IBKRError> {
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

  fn cancel(&mut self) -> Result<(), IBKRError> {
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
