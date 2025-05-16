// yatws/src/data_news_manager.rs

//! Manages requests for news providers, articles, and historical news.
//!
//! The `DataNewsManager` allows fetching:
//! -   A list of available news providers via `get_news_providers()`.
//! -   The content of a specific news article via `get_news_article()`.
//! -   Historical news headlines for a contract via `get_historical_news()`.
//!
//! It also supports subscribing to live news bulletins via `request_news_bulletins()`
//! and receiving tick-based news updates if a market data stream (via `DataMarketManager`)
//! includes the news tick type.
//!
//! # Observers
//!
//! For streaming news (bulletins or tick-based news), an [`NewsObserver`] can be registered.
//! -   `on_news_article()`: Called when a news bulletin or a news tick is received.
//!     The `NewsArticle` struct attempts to unify these different news sources.
//!
//! # Example: Getting News Providers and an Article
//!
//! ```no_run
//! use yatws::{IBKRClient, IBKRError, contract::Contract};
//! use std::time::Duration;
//!
//! fn main() -> Result<(), IBKRError> {
//!     let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
//!     let news_mgr = client.data_news();
//!     let ref_data_mgr = client.data_ref(); // For con_id
//!
//!     // 1. Get News Providers
//!     match news_mgr.get_news_providers() {
//!         Ok(providers) => {
//!             if providers.is_empty() {
//!                 println!("No news providers available.");
//!             } else {
//!                 println!("Available News Providers:");
//!                 for provider in &providers {
//!                     println!("  Code: {}, Name: {}", provider.code, provider.name);
//!                 }
//!                 // Example: Get an article from the first provider (if any news exists)
//!                 // This requires knowing a valid article_id for that provider.
//!                 // For a real example, you'd likely get article_id from historical_news or a news tick.
//!                 // if let Some(first_provider) = providers.first() {
//!                 //     match news_mgr.get_news_article(&first_provider.code, "SOME_ARTICLE_ID", &[]) {
//!                 //         Ok(article_data) => println!("Article Content (type {}): {}...",
//!                 //             article_data.article_type,
//!                 //             article_data.article_text.chars().take(100).collect::<String>()
//!                 //         ),
//!                 //         Err(e) => eprintln!("Error getting article: {:?}", e),
//!                 //     }
//!                 // }
//!             }
//!         }
//!         Err(e) => eprintln!("Error getting news providers: {:?}", e),
//!     }
//!
//!     // 2. Get Historical News for a contract (e.g., AAPL)
//!     let contract_spec = Contract::stock("AAPL");
//!     if let Ok(details_list) = ref_data_mgr.get_contract_details(&contract_spec) {
//!         if let Some(details) = details_list.first() {
//!             let con_id = details.contract.con_id;
//!             // Assuming we have provider codes from the previous step
//!             let provider_codes = "BRFG,BRFUPDN"; // Example provider codes
//!
//!             match news_mgr.get_historical_news(con_id, provider_codes, None, None, 10, &[]) {
//!                 Ok(articles) => {
//!                     println!("\nHistorical News for AAPL (con_id {}):", con_id);
//!                     for article_info in articles.iter().take(3) { // Print first 3
//!                         println!("  Time: {}, Provider: {}, ID: {}, Headline: {}",
//!                                  article_info.time, article_info.provider_code,
//!                                  article_info.article_id, article_info.headline);
//!                     }
//!                 }
//!                 Err(e) => eprintln!("Error getting historical news: {:?}", e),
//!             }
//!         } else { eprintln!("Could not get contract details for AAPL."); }
//!     } else { eprintln!("Error fetching contract details for AAPL."); }
//!
//!     Ok(())
//! }
//! ```

use crate::base::IBKRError;
use crate::conn::MessageBroker;
use crate::protocol_decoder::ClientErrorCode;
use crate::news::{NewsProvider, NewsArticle, NewsArticleData, HistoricalNews, NewsObserver};
use crate::handler::{NewsDataHandler};
use crate::protocol_encoder::Encoder;
use crate::data_observer::ObserverId;
use crate::news_subscription;

use parking_lot::{Condvar, Mutex, RwLock};
use chrono::{Utc, TimeZone};
use std::collections::HashMap;
use std::sync::{Arc, Weak, atomic::{AtomicUsize, Ordering}}; // Added AtomicUsize, Ordering
use std::time::Duration;
use log::{debug, info, trace, warn};

// Import new subscription types
use crate::news_subscription::{HistoricalNewsEvent, HistoricalNewsSubscriptionBuilder};
use crate::data_subscription::SubscriptionState;


// --- State for Pending News Requests ---

/// Internal state for tracking pending news-related requests.
/// This is used by the manager to correlate responses with blocking calls.
#[derive(Debug, Default)]
struct NewsRequestState {
  /// Stores the list of news providers received from `reqNewsProviders`.
  news_providers: Option<Vec<NewsProvider>>,
  /// Stores the content of a specific news article from `reqNewsArticle`.
  /// The `req_id` is part of `NewsArticleData`.
  // Fields for NewsArticle
  news_article: Option<NewsArticleData>,
  /// Stores a list of historical news headlines from `reqHistoricalNews`.
  historical_news_list: Vec<HistoricalNews>,
  /// Flag indicating if `historicalNewsEnd` has been received for a historical news request.
  historical_news_end_received: bool,
  /// Stores an error code if an API error occurred for this request.
  error_code: Option<i32>,
  /// Stores an error message if an API error occurred for this request.
  error_message: Option<String>,
}

/// Manages requests for news providers, articles, historical news, and streaming news bulletins.
///
/// Accessed via [`IBKRClient::data_news()`](crate::IBKRClient::data_news()).
///
/// See the [module-level documentation](index.html) for more details and examples.
pub struct DataNewsManager {
  pub(crate) message_broker: Arc<MessageBroker>,
  request_states: Mutex<HashMap<i32, NewsRequestState>>,
  request_cond: Condvar,
  observers: RwLock<HashMap<ObserverId, Weak<dyn NewsObserver>>>, // Changed to HashMap
  next_observer_id: AtomicUsize, // Added for unique ObserverId generation
  self_weak: Weak<DataNewsManager>, // Added for builder to get weak ref to self
  // Map for historical news event subscriptions
  historical_news_event_subscriptions: Mutex<HashMap<i32, Weak<SubscriptionState<HistoricalNewsEvent, DataNewsManager>>>>,
}

impl DataNewsManager {
  /// Creates a new `DataNewsManager`.
  ///
  /// This is typically called internally when an `IBKRClient` is created.
  pub(crate) fn new(message_broker: Arc<MessageBroker>) -> Arc<Self> {
    Arc::new_cyclic(|weak_self_ref| DataNewsManager {
      message_broker,
      request_states: Mutex::new(HashMap::new()),
      request_cond: Condvar::new(),
      observers: RwLock::new(HashMap::new()),
      next_observer_id: AtomicUsize::new(1),
      self_weak: weak_self_ref.clone(),
      historical_news_event_subscriptions: Mutex::new(HashMap::new()),
    })
  }

  /// Generates a new unique ObserverId value.
  /// Used by NewsSubscriptionBuilder before creating SubscriptionState.
  pub(crate) fn new_observer_id_val(&self) -> usize {
    self.next_observer_id.fetch_add(1, Ordering::SeqCst)
  }

  /// Registers an observer to receive streaming news updates (bulletins and news ticks).
  ///
  /// Observers implement the [`NewsObserver`] trait. They are held by `Weak` pointers,
  /// so the caller must maintain a strong reference (`Arc`) to the observer
  /// for it to remain active.
  ///
  /// # Arguments
  /// * `observer` - An `Arc` to an object implementing `NewsObserver`.
  ///
  /// # Returns
  /// An `ObserverId` for the registered observer, which can be used with `remove_observer`.
  pub fn add_observer(&self, observer: Arc<dyn NewsObserver>) -> ObserverId {
    let observer_id = ObserverId(self.next_observer_id.fetch_add(1, Ordering::SeqCst));
    let mut observers_map = self.observers.write();
    // Clean up dead observers before adding a new one
    observers_map.retain(|_, weak| weak.strong_count() > 0);
    observers_map.insert(observer_id, Arc::downgrade(&observer));
    debug!("Added news observer with ID: {:?}", observer_id);
    observer_id
  }

  /// Removes a previously registered news observer.
  ///
  /// # Arguments
  /// * `observer_id` - The `ObserverId` returned by `add_observer`.
  ///
  /// # Returns
  /// `true` if an observer with the given ID was found and removed, `false` otherwise.
  pub fn remove_observer(&self, observer_id: ObserverId) -> bool {
    let mut observers_map = self.observers.write();
    let removed = observers_map.remove(&observer_id).is_some();
    if removed {
      debug!("Removed news observer with ID: {:?}", observer_id);
    } else {
      warn!("Attempted to remove non-existent news observer ID: {:?}", observer_id);
    }
    // Clean up dead observers
    observers_map.retain(|_, weak| weak.strong_count() > 0);
    removed
  }

  /// Clears all registered news observers.
  /// After this call, no observers will receive further news updates from this manager.
  pub fn clear_observers(&self) {
    debug!("Clearing all news observers");
    let mut observers = self.observers.write();
    observers.clear();
  }

  // --- Helper to notify observers ---
  fn notify_observers(&self, article: &NewsArticle) {
    let observers_map = self.observers.read();
    for weak_observer in observers_map.values() {
      if let Some(observer) = weak_observer.upgrade() {
        trace!("Notifying observer about news article: {}", article.id);
        observer.on_news_article(article);
      }
      // No cleanup here, do it during add/remove or periodically
    }
  }

  fn notify_observers_of_error(&self, error_code: i32, error_message: &str) {
    let observers_map = self.observers.read();
    for weak_observer in observers_map.values() {
      if let Some(observer) = weak_observer.upgrade() {
        observer.on_error(error_code, error_message);
      }
    }
  }


  // --- Helper to wait for completion ---

  /// Internal helper for blocking calls that wait for a news-related request to complete.
  ///
  /// It waits until the `is_complete_check` closure indicates completion, an API error occurs,
  /// or the timeout is reached.
  ///
  /// # Arguments
  /// * `req_id` - The request ID.
  /// * `timeout` - Maximum duration to wait.
  /// * `is_complete_check` - A closure `Fn(&NewsRequestState) -> Option<Result<R, IBKRError>>`.
  ///   It returns `Some(Ok(result))` when the condition is met, `Some(Err(e))` if the check itself
  ///   determines an error, or `None` to continue waiting.
  ///
  /// # Returns
  /// The result `R` from `is_complete_check` or an `IBKRError` (e.g., Timeout, ApiError).
  fn wait_for_completion<F, R>(
    &self,
    req_id: i32,
    timeout: Duration,
    is_complete_check: F,
  ) -> Result<R, IBKRError>
  where
    F: Fn(&NewsRequestState) -> Option<Result<R, IBKRError>>,
  {
    let start_time = std::time::Instant::now();
    let mut guard = self.request_states.lock();

    loop {
      // 1. Check if complete *before* waiting
      if let Some(state) = guard.get(&req_id) {
        match is_complete_check(state) {
          Some(Ok(result)) => { guard.remove(&req_id); return Ok(result); },
          Some(Err(e)) => { guard.remove(&req_id); return Err(e); },
          None => {} // Not complete yet
        }
        // Check for API error
        if let (Some(code), Some(msg)) = (state.error_code, state.error_message.as_ref()) {
          let err = IBKRError::ApiError(code, msg.clone());
          guard.remove(&req_id); return Err(err);
        }
      } else {
        return Err(IBKRError::InternalError(format!("News request state for {} missing", req_id)));
      }

      // 2. Calculate remaining timeout
      let elapsed = start_time.elapsed();
      if elapsed >= timeout {
        guard.remove(&req_id);
        return Err(IBKRError::Timeout(format!("News request {} timed out", req_id)));
      }
      let remaining_timeout = timeout - elapsed;

      // 3. Wait
      let wait_result = self.request_cond.wait_for(&mut guard, remaining_timeout);

      // 4. Handle timeout after wait
      if wait_result.timed_out() {
        if let Some(state) = guard.get(&req_id) { // Re-check state
          match is_complete_check(state) {
            Some(Ok(result)) => { guard.remove(&req_id); return Ok(result); },
            Some(Err(e)) => { guard.remove(&req_id); return Err(e); },
            None => {}
          }
          if let (Some(code), Some(msg)) = (state.error_code, state.error_message.as_ref()) {
            let err = IBKRError::ApiError(code, msg.clone());
            guard.remove(&req_id); return Err(err);
          }
        }
        guard.remove(&req_id);
        return Err(IBKRError::Timeout(format!("News request {} timed out after wait", req_id)));
      }
    }
  }

  // --- Public API Methods ---

  /// Requests and returns a list of available news providers. This is a blocking call.
  ///
  /// # Returns
  /// A `Vec<NewsProvider>` containing codes and names of news providers.
  ///
  /// # Errors
  /// Returns `IBKRError::Timeout` if the provider list is not received within the timeout.
  /// Returns other `IBKRError` variants for communication or encoding issues.
  pub fn get_news_providers(&self) -> Result<Vec<NewsProvider>, IBKRError> {
    info!("Requesting news providers");
    // req_id is not used for this request, but we use one for state tracking
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_request_news_providers()?;

    {
      let mut states = self.request_states.lock();
      if states.contains_key(&req_id) { return Err(IBKRError::DuplicateRequestId(req_id)); }
      states.insert(req_id, NewsRequestState::default());
    }

    self.message_broker.send_message(&request_msg)?;

    let timeout = Duration::from_secs(10);
    self.wait_for_completion(req_id, timeout, |state| {
      // Completion is based on receiving the data itself
      state.news_providers.as_ref().map(|providers| Ok(providers.clone()))
    })
  }

  /// Requests and returns the content of a specific news article. This is a blocking call.
  ///
  /// # Arguments
  /// * `provider_code` - The code of the news provider (e.g., "BRFG", "DJNL").
  /// * `article_id` - The unique ID of the article from the specified provider.
  /// * `news_article_options` - A list of `(tag, value)` pairs for additional options (rarely used).
  ///
  /// # Returns
  /// A `NewsArticleData` struct containing the article type and text.
  ///
  /// # Errors
  /// Returns `IBKRError::Timeout` if the article is not received within the timeout.
  /// Returns other `IBKRError` variants for communication or encoding issues.
  pub fn get_news_article(
    &self,
    provider_code: &str,
    article_id: &str,
    news_article_options: &[(String, String)],
  ) -> Result<NewsArticleData, IBKRError> {
    info!("Requesting news article: Provider={}, ArticleID={}", provider_code, article_id);
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_request_news_article(req_id, provider_code, article_id, news_article_options)?;

    {
      let mut states = self.request_states.lock();
      if states.contains_key(&req_id) { return Err(IBKRError::DuplicateRequestId(req_id)); }
      states.insert(req_id, NewsRequestState::default());
    }

    self.message_broker.send_message(&request_msg)?;

    let timeout = Duration::from_secs(20); // Articles can be large
    self.wait_for_completion(req_id, timeout, |state| {
      state.news_article.as_ref().map(|article| Ok(article.clone()))
    })
  }

  /// Requests and returns historical news headlines for a specific contract. This is a blocking call.
  ///
  /// # Arguments
  /// * `con_id` - The TWS contract ID of the instrument.
  /// * `provider_codes` - A comma-separated string of news provider codes to include (e.g., "BRFG,RTRS,DJNL").
  /// * `start_date_time` - Optional `DateTime<Utc>` for the start of the period. If `None`, TWS defaults.
  /// * `end_date_time` - Optional `DateTime<Utc>` for the end of the period. If `None`, TWS defaults (usually current time).
  /// * `total_results` - The maximum number of headlines to return.
  /// * `historical_news_options` - A list of `(tag, value)` pairs for additional options.
  ///
  /// # Returns
  /// A `Vec<HistoricalNews>` containing the news headlines.
  ///
  /// # Errors
  /// Returns `IBKRError::Timeout` if the headlines are not received within the timeout.
  /// Returns `IBKRError::ApiError` for common issues like "Historical news request requires subscription".
  /// Returns other `IBKRError` variants for communication or encoding issues.
  ///
  /// # Example from `gen_goldens.rs`:
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError, contract::Contract, ChronoDuration, Utc};
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// # let news_mgr = client.data_news();
  /// # let ref_data_mgr = client.data_ref();
  /// # let contract_spec = Contract::stock("AAPL");
  /// # let contract_details_list = ref_data_mgr.get_contract_details(&contract_spec)?;
  /// # let con_id = contract_details_list[0].contract.con_id;
  /// let provider_codes = "BRFG,DJNL"; // Example
  /// let start_time = Some(Utc::now() - ChronoDuration::days(7));
  /// let end_time = Some(Utc::now());
  /// let articles = news_mgr.get_historical_news(
  ///     con_id,
  ///     provider_codes,
  ///     start_time,
  ///     end_time,
  ///     10, // Max 10 results
  ///     &[]
  /// )?;
  /// for article in articles {
  ///     println!("[{}] {}: {}", article.provider_code, article.time, article.headline);
  /// }
  /// # Ok(())
  /// # }
  /// ```
  pub fn get_historical_news(
    &self,
    con_id: i32,
    provider_codes: &str,
    start_date_time: Option<chrono::DateTime<chrono::Utc>>,
    end_date_time: Option<chrono::DateTime<chrono::Utc>>,
    total_results: i32,
    historical_news_options: &[(String, String)],
  ) -> Result<Vec<HistoricalNews>, IBKRError> {
    info!("Requesting historical news: ConID={}, Providers={}", con_id, provider_codes);
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_request_historical_news(
      req_id, con_id, provider_codes, start_date_time, end_date_time, total_results, historical_news_options
    )?;

    {
      let mut states = self.request_states.lock();
      if states.contains_key(&req_id) { return Err(IBKRError::DuplicateRequestId(req_id)); }
      states.insert(req_id, NewsRequestState::default());
    }

    self.message_broker.send_message(&request_msg)?;

    let timeout = Duration::from_secs(30); // Can take time
    self.wait_for_completion(req_id, timeout, |state| {
      if state.historical_news_end_received {
        Some(Ok(state.historical_news_list.clone()))
      } else {
        None // Not complete yet
      }
    })
  }

  /// Internal method to request historical news and store subscription state.
  pub(crate) fn internal_request_historical_news_stream(
    &self,
    req_id: i32,
    con_id: i32,
    provider_codes: &str,
    start_date_time: Option<chrono::DateTime<chrono::Utc>>,
    end_date_time: Option<chrono::DateTime<chrono::Utc>>,
    total_results: i32,
    historical_news_options: &[(String, String)],
    state_weak: Weak<SubscriptionState<HistoricalNewsEvent, DataNewsManager>>,
  ) -> Result<(), IBKRError> {
    info!("Internal: Requesting historical news stream: ReqID={}, ConID={}", req_id, con_id);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_request_historical_news(
      req_id, con_id, provider_codes, start_date_time, end_date_time, total_results, historical_news_options
    )?;

    {
      let mut subs = self.historical_news_event_subscriptions.lock();
      if subs.contains_key(&req_id) {
        return Err(IBKRError::DuplicateRequestId(req_id));
      }
      subs.insert(req_id, state_weak);
    }

    self.message_broker.send_message(&request_msg)
  }

  /// Creates a builder for a historical news subscription.
  pub fn subscribe_historical_news_stream(
    &self,
    con_id: i32,
    provider_codes: &str,
    total_results: i32,
  ) -> HistoricalNewsSubscriptionBuilder {
    HistoricalNewsSubscriptionBuilder::new(self.self_weak.clone(), con_id, provider_codes, total_results)
  }

  // --- Streaming Calls (Non-blocking) ---

  /// Subscribes to live news bulletins from TWS. This is a non-blocking call.
  ///
  /// Received bulletins are delivered via the `update_news_bulletin` method of the
  /// `NewsDataHandler` trait, which then notifies registered [`NewsObserver`]s.
  ///
  /// # Arguments
  /// * `all_msgs` - If `true`, receives all news bulletins. If `false`, receives only new bulletins.
  ///
  /// # Errors
  /// Returns `IBKRError` if the request cannot be encoded or sent.
  pub fn request_news_bulletins(&self, all_msgs: bool) -> Result<(), IBKRError> {
    info!("Requesting news bulletins: AllMsgs={}", all_msgs);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_request_news_bulletins(all_msgs)?;
    self.message_broker.send_message(&request_msg)
  }

  /// Cancels the subscription to live news bulletins.
  ///
  /// # Errors
  /// Returns `IBKRError` if the cancellation message cannot be encoded or sent.
  pub fn cancel_news_bulletins(&self) -> Result<(), IBKRError> {
    info!("Cancelling news bulletins");
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_news_bulletins()?;
    self.message_broker.send_message(&request_msg)?;
    Ok(())
  }

  /// Creates a builder for a news bulletin subscription.
  ///
  /// News bulletins are general news items from TWS, not tied to a specific contract.
  /// The subscription provides a stream of [`NewsEvent`](crate::news_subscription::NewsEvent)s.
  ///
  /// # Arguments
  /// * `all_msgs` - If `true`, requests all available historical bulletins upon subscription
  ///   followed by new ones. If `false`, requests only new bulletins.
  pub fn subscribe_news_bulletins_stream(&self, all_msgs: bool) -> news_subscription::NewsSubscriptionBuilder {
      news_subscription::NewsSubscriptionBuilder::new(self.self_weak.clone(), all_msgs)
  }

  /// Client-side cancellation of a historical news stream.
  /// TWS does not provide a specific message to cancel historical news requests.
  pub(crate) fn cancel_historical_news_stream(&self, req_id: i32) {
    info!("Client-side cancelling historical news stream for ReqID: {}", req_id);
    let mut subs = self.historical_news_event_subscriptions.lock();
    if subs.remove(&req_id).is_some() {
        debug!("Removed historical news event subscription state for ReqID: {}", req_id);
    }
  }

  // --- Internal error handling (called by the trait method) ---
  fn _internal_handle_error(&self, req_id: i32, code: ClientErrorCode, msg: &str) {
    // If req_id is 0 or negative, it might be a general news system error not tied to a specific request.
    // Or it could be an error related to news bulletins which also don't use a req_id from client perspective.

    let mut states = self.request_states.lock();
    if req_id > 0 {
        // Check blocking request states first
        if let Some(state) = states.get_mut(&req_id) {
          warn!("API Error received for specific blocking news request {}: Code={:?}, Msg={}", req_id, code, msg);
          state.error_code = Some(code as i32);
          state.error_message = Some(msg.to_string());
          self.request_cond.notify_all(); // Signal waiting thread for this specific request
          return; // Handled
        }

        // Check historical news event subscriptions
        let mut event_subs = self.historical_news_event_subscriptions.lock();
        if let Some(state_weak) = event_subs.get(&req_id) {
            if let Some(state_arc) = state_weak.upgrade() {
                warn!("API Error for historical news subscription {}: Code={:?}, Msg={}", req_id, code, msg);
                let err = IBKRError::ApiError(code as i32, msg.to_string());
                state_arc.push_event(HistoricalNewsEvent::Error(err.clone()));
                state_arc.set_error(err);
            } // Weak ref might be dead if subscription was dropped
            event_subs.remove(&req_id); // Remove on error
            return; // Handled
        }
    } else {
    // If not handled by specific req_id maps, treat as general or bulletin error
      // This error is not for a specific req_id-based request (e.g., get_historical_news).
      // It could be a general news system error, or an error related to news bulletins.
      // Notify all active observers.
      warn!("General News API Error or Bulletin Error: Code={:?}, Msg={}. Notifying all observers.", code, msg);
      self.notify_observers_of_error(code as i32, msg);
      // If it's a bulletin error, the NewsSubscription's internal observer will push a NewsEvent::Error.
    }
  }
}

// --- Implement NewsDataHandler Trait ---
impl NewsDataHandler for DataNewsManager {
  fn news_providers(&self, providers: &[NewsProvider]) {
    debug!("Handler: News Providers: Count={}", providers.len());
    // Find the pending request (assuming only one active at a time for this req_id-less response)
    let mut states = self.request_states.lock();
    let mut found_req_id = None;
    for (id, state) in states.iter_mut() {
      if state.news_providers.is_none() && state.error_code.is_none() {
        state.news_providers = Some(providers.to_vec());
        found_req_id = Some(*id);
        break;
      }
    }
    if let Some(req_id) = found_req_id {
      info!("News providers received, matching to request {}. Notifying waiter.", req_id);
      self.request_cond.notify_all();
    } else {
      warn!("Received news providers but no matching pending request found.");
    }
  }

  fn news_article(&self, req_id: i32, article_type: i32, article_text: &str) {
    debug!("Handler: News Article: ReqID={}, Type={}", req_id, article_type);
    let mut states = self.request_states.lock();
    if let Some(state) = states.get_mut(&req_id) {
      state.news_article = Some(NewsArticleData { req_id, article_type, article_text: article_text.to_string() });
      info!("News article received for request {}. Notifying waiter.", req_id);
      self.request_cond.notify_all();
      // Optionally, create a NewsArticle and notify observers, but lacks metadata
      // let article = NewsArticle {
      //     id: format!("{}-{}", "unknown", req_id), // No provider/article ID here
      //     time: Utc::now(), // No timestamp provided
      //     provider_code: "unknown".to_string(),
      //     article_id: req_id.to_string(), // Use req_id as placeholder?
      //     headline: format!("Article ReqID {}", req_id), // Placeholder
      //     content: Some(article_text.to_string()),
      //     article_type: Some(article_type),
      //     extra_data: None,
      // };
      // self.notify_observers(&article);
    } else {
      warn!("Received news article for unknown or completed request ID: {}", req_id);
    }
  }

  fn historical_news(&self, req_id: i32, time: &str, provider_code: &str, article_id: &str, headline: &str) {
    trace!("Handler: Historical News Item: ReqID={}", req_id);
    // Check blocking request states
    let mut blocking_states = self.request_states.lock();
    if let Some(state) = blocking_states.get_mut(&req_id) {
        state.historical_news_list.push(HistoricalNews {
            time: time.to_string(),
            provider_code: provider_code.to_string(),
            article_id: article_id.to_string(),
            headline: headline.to_string(),
        });
        return; // Handled by blocking call state
    }
    drop(blocking_states); // Release lock

    // Check event stream subscriptions
    let event_subs = self.historical_news_event_subscriptions.lock();
    if let Some(state_weak) = event_subs.get(&req_id) {
        if let Some(state_arc) = state_weak.upgrade() {
            let item = HistoricalNews {
                time: time.to_string(),
                provider_code: provider_code.to_string(),
                article_id: article_id.to_string(),
                headline: headline.to_string(),
            };
            state_arc.push_event(HistoricalNewsEvent::Article(item));
        }
    }
  }

  fn historical_news_end(&self, req_id: i32, has_more: bool) {
    debug!("Handler: Historical News End: ReqID={}, HasMore={}", req_id, has_more);
    // Check blocking request states
    let mut blocking_states = self.request_states.lock();
    if let Some(state) = blocking_states.get_mut(&req_id) {
        state.historical_news_end_received = true;
        info!("Historical news end received for blocking request {}. Notifying waiter.", req_id);
        self.request_cond.notify_all();
        // Note: blocking_states.remove(&req_id) happens in wait_for_completion
        return; // Handled by blocking call state
    }
    drop(blocking_states); // Release lock

    // Check event stream subscriptions
    let mut event_subs = self.historical_news_event_subscriptions.lock();
    if let Some(state_weak) = event_subs.remove(&req_id) { // Remove as it's complete
        if let Some(state_arc) = state_weak.upgrade() {
            state_arc.push_event(HistoricalNewsEvent::Complete);
            state_arc.mark_completed_and_inactive();
        }
    }
  }

  fn update_news_bulletin(&self, msg_id: i32, msg_type: i32, news_message: &str, origin_exch: &str) {
    // This is streaming data - log or pass to observer
    info!("Handler: News Bulletin Update: ID={}, Type={}, Origin={}, Msg='{}'", msg_id, msg_type, origin_exch, news_message);
    // Attempt to parse into a NewsArticle if desired, format is unstructured.
    // Example: Might look for patterns, but highly unreliable.
    // Let's create a basic article and notify.
    let article = NewsArticle {
      id: format!("BULLETIN/{}", msg_id), // Create a unique ID
      time: Utc::now(), // Timestamp is arrival time
      provider_code: origin_exch.to_string(), // Use origin exchange as provider?
      article_id: msg_id.to_string(),
      headline: format!("Bulletin Type {}: {}", msg_type, news_message.chars().take(50).collect::<String>()), // Truncate msg for headline
      content: Some(news_message.to_string()), // Full message as content
      article_type: Some(0), // Assume plain text
      extra_data: Some(format!("Origin: {}, Type: {}", origin_exch, msg_type)),
    };
    self.notify_observers(&article);
  }

  fn tick_news(&self, _req_id: i32, time_stamp: i64, provider_code: &str, article_id: &str, headline: &str, extra_data: &str) {
    // This is streaming data -> Notify observers
    if let Some(time) = Utc.timestamp_opt(time_stamp, 0).single() {
      let article = NewsArticle {
        id: format!("{}/{}", provider_code, article_id), // Combine for unique ID
        time,
        provider_code: provider_code.to_string(),
        article_id: article_id.to_string(),
        headline: headline.to_string(),
        content: None, // Content not available from tick
        article_type: None, // Type not available from tick
        extra_data: Some(extra_data.to_string()).filter(|s| !s.is_empty()), // Only store if non-empty
      };
      debug!("Handler: Tick News -> Notifying observers: {}", article.id);
      self.notify_observers(&article);
    } else {
      warn!("Failed to parse timestamp {} for tick news", time_stamp);
    }
  }

  /// Handles errors related to news data requests.
  /// This is the implementation of the trait method.
  fn handle_error(&self, req_id: i32, code: ClientErrorCode, msg: &str) {
    // Delegate to the internal helper
    self._internal_handle_error(req_id, code, msg);
  }
}
