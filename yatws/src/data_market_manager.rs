// yatws/src/data_market_manager.rs

//! Manages requests for real-time and historical market data.
//!
//! The `DataMarketManager` provides methods to:
//! -   Request streaming market data (ticks) via `request_market_data()` and cancel with `cancel_market_data()`.
//! -   Fetch a snapshot quote (Bid/Ask/Last) via the blocking `get_quote()`.
//! -   Request streaming 5-second real-time bars via `request_real_time_bars()` and cancel with `cancel_real_time_bars()`.
//! -   Fetch a specific number of 5-second real-time bars via the blocking `get_realtime_bars()`.
//! -   Request streaming tick-by-tick data (Last, AllLast, BidAsk, MidPoint) via `request_tick_by_tick_data()` and cancel with `cancel_tick_by_tick_data()`.
//! -   Request streaming market depth (Level II) data via `request_market_depth()` and cancel with `cancel_market_depth()`.
//! -   Fetch historical bar data via the blocking `get_historical_data()` and cancel with `cancel_historical_data()`.
//! -   Generic blocking methods `get_market_data()`, `get_tick_by_tick_data()`, `get_market_depth()` allow waiting for custom conditions.
//!
//! # Data Types and Market Data Type Setting
//!
//! TWS can provide market data in different "types":
//! 1.  **RealTime (1)**: Live, streaming market data. Requires market data subscriptions.
//! 2.  **Frozen (2)**: Market data is frozen at the close of the previous day.
//! 3.  **Delayed (3)**: Delayed market data.
//! 4.  **DelayedFrozen (4)**: Delayed market data, frozen at the close of the previous day.
//!
//! Most methods in this manager accept an optional `MarketDataType` parameter. If provided,
//! the manager will attempt to set TWS to this data type before making the request.
//! If `None`, `MarketDataType::RealTime` is typically assumed or the current TWS setting is used.
//! The `set_market_data_type_if_needed()` helper handles this.
//!
//! # Observers
//!
//! For streaming data (e.g., from `request_market_data`, `request_real_time_bars`),
//! you would typically implement an observer pattern. The `MarketDataHandler` trait methods
//! within `DataMarketManager` are called by the message processing loop. To consume this
//! data in your application, you would:
//! 1.  Create your own struct that implements a custom observer trait (e.g., `MyMarketObserver`).
//! 2.  Pass an `Arc` or `Weak` reference of your observer to `DataMarketManager` (e.g., via an `add_observer` method, which is not explicitly shown in the current `DataMarketManager` but is a common pattern).
//! 3.  The `MarketDataHandler` methods in `DataMarketManager` would then call the appropriate methods on your registered observers.
//!     (Currently, observer notification is commented out in the provided code, e.g., `// self.notify_observers(req_id);`)
//!
//! # Blocking vs. Streaming
//!
//! -   **`get_*` methods** (e.g., `get_quote`, `get_historical_data`, `get_realtime_bars`) are generally **blocking**.
//!     They send a request and wait for the complete response or a timeout.
//! -   **`request_*` methods** (e.g., `request_market_data`, `request_real_time_bars`) are **non-blocking/streaming**.
//!     They send a request and return a request ID. Data arrives asynchronously and is processed by the `MarketDataHandler`
//!     methods, which would then typically forward it to registered observers.
//!
//! # Example: Getting a Quote
//!
//! ```no_run
//! use yatws::{IBKRClient, IBKRError, contract::Contract, data::MarketDataType};
//! use std::time::Duration;
//!
//! fn main() -> Result<(), IBKRError> {
//!     let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
//!     let market_data_mgr = client.data_market();
//!
//!     let contract = Contract::stock("AAPL");
//!     let timeout = Duration::from_secs(10);
//!
//!     // Request a delayed quote
//!     match market_data_mgr.get_quote(&contract, Some(MarketDataType::Delayed), timeout) {
//!         Ok((bid, ask, last)) => {
//!             println!("AAPL Quote (Delayed): Bid={:?}, Ask={:?}, Last={:?}", bid, ask, last);
//!         }
//!         Err(e) => eprintln!("Error getting quote: {:?}", e),
//!     }
//!     Ok(())
//! }
//! ```

use crate::base::IBKRError;
use crate::conn::MessageBroker;
use crate::contract::{Bar, Contract, ContractDetails, ScanData, ScannerInfo, WhatToShow, BarSize};
use crate::scan_parameters::ScanParameterResponse;
use crate::data::{
  MarketDataInfo, MarketDataType, MarketDepthRow,
  MarketDepthInfo,
  RealTimeBarInfo,
  TickAttrib, TickAttribBidAsk, TickAttribLast, TickByTickData, HistoricalTick,
  TickByTickInfo,
  TickOptionComputationData, TickType, HistogramEntry, HistogramDataRequestState, HistoricalTicksRequestState,
  HistoricalDataRequestState, ScannerInfoState, GenericTickType, TickByTickRequestType, DurationUnit, TimePeriodUnit,
};
use crate::data_subscription::{
  TickDataSubscriptionBuilder, RealTimeBarSubscriptionBuilder, TickByTickSubscriptionBuilder,
  MarketDepthSubscriptionBuilder, HistoricalDataSubscriptionBuilder, MultiSubscriptionBuilder,
};
use crate::handler::{MarketDataHandler};
use crate::protocol_encoder::Encoder;
use crate::protocol_decoder::ClientErrorCode;
use crate::base::IBKRError::Timeout;
use crate::min_server_ver::min_server_ver;
use parking_lot::{Condvar, Mutex};
use chrono::{Utc, TimeZone};
use std::collections::HashMap;
use std::sync::{Arc, Weak, atomic::{AtomicUsize, Ordering}}; // Added Weak
use std::time::Duration;
use parking_lot::RwLock; // Added for observer maps
use crate::data_observer::{ // Added for observers
  ObserverId, MarketDataObserver, RealTimeBarsObserver, TickByTickObserver, MarketDepthObserver,
  HistoricalDataObserver, HistoricalTicksObserver,
};
use crate::rate_limiter::CounterRateLimiter;
use log::{debug, info, trace, warn};


// --- Helper Trait for Generic Waiting ---

/// Internal trait used by `wait_for_completion` to interact with different streaming state types.
/// It defines common methods for checking completion status and accessing error information.
trait CompletableState: Clone + Send + 'static {
  /// Checks if the operation associated with this state is considered complete.
  /// This could be due to successful data retrieval, an error, or a user-defined condition.
  fn is_completed(&self) -> bool;
  /// Marks the operation associated with this state as complete.
  fn mark_completed(&mut self);
  /// Retrieves an error associated with this state, if any.
  fn get_error(&self) -> Option<IBKRError>;
}

// Implement the trait for each streaming type that supports blocking waits
impl CompletableState for MarketDataInfo {
  fn is_completed(&self) -> bool { self.completed || self.quote_received /* Also consider quote flag */ }
  fn mark_completed(&mut self) { self.completed = true; }
  fn get_error(&self) -> Option<IBKRError> {
    match (self.error_code, self.error_message.as_ref()) {
      (Some(code), Some(msg)) => Some(IBKRError::ApiError(code, msg.clone())),
      _ => None,
    }
  }
}

impl CompletableState for RealTimeBarInfo {
  fn is_completed(&self) -> bool { self.completed }
  fn mark_completed(&mut self) { self.completed = true; }
  fn get_error(&self) -> Option<IBKRError> {
    match (self.error_code, self.error_message.as_ref()) {
      (Some(code), Some(msg)) => Some(IBKRError::ApiError(code, msg.clone())),
      _ => None,
    }
  }
}

impl CompletableState for TickByTickInfo {
  fn is_completed(&self) -> bool { self.completed }
  fn mark_completed(&mut self) { self.completed = true; }
  fn get_error(&self) -> Option<IBKRError> {
    match (self.error_code, self.error_message.as_ref()) {
      (Some(code), Some(msg)) => Some(IBKRError::ApiError(code, msg.clone())),
      _ => None,
    }
  }
}

impl CompletableState for MarketDepthInfo {
  fn is_completed(&self) -> bool { self.completed }
  fn mark_completed(&mut self) { self.completed = true; }
  fn get_error(&self) -> Option<IBKRError> {
    match (self.error_code, self.error_message.as_ref()) {
      (Some(code), Some(msg)) => Some(IBKRError::ApiError(code, msg.clone())),
      _ => None,
    }
  }
}
// Note: HistoricalDataRequestState uses a different wait mechanism, so doesn't need this trait currently.

impl CompletableState for ScannerInfoState {
  fn is_completed(&self) -> bool { self.completed }
  fn mark_completed(&mut self) { self.completed = true; }
  fn get_error(&self) -> Option<IBKRError> {
    match (self.error_code, self.error_message.as_ref()) {
      (Some(code), Some(msg)) => Some(IBKRError::ApiError(code, msg.clone())),
      _ => None,
    }
  }
}

impl CompletableState for HistogramDataRequestState {
  fn is_completed(&self) -> bool { self.completed }
  fn mark_completed(&mut self) { self.completed = true; }
  fn get_error(&self) -> Option<IBKRError> {
    match (self.error_code, self.error_message.as_ref()) {
      (Some(code), Some(msg)) => Some(IBKRError::ApiError(code, msg.clone())),
      _ => None,
    }
  }
}

impl CompletableState for HistoricalTicksRequestState {
  fn is_completed(&self) -> bool { self.completed || self.error_code.is_some() }
  fn mark_completed(&mut self) { self.completed = true; }
  fn get_error(&self) -> Option<IBKRError> {
    match (self.error_code, self.error_message.as_ref()) {
      (Some(code), Some(msg)) => Some(IBKRError::ApiError(code, msg.clone())),
      _ => None,
    }
  }
}


// --- Helper Trait for Downcasting MarketStream Enum ---

/// Internal helper trait to enable downcasting from the `MarketStream` enum
/// to a specific underlying subscription state type (e.g., `MarketDataSubscription`).
/// This is used within generic functions like `wait_for_completion`.
trait TryIntoStateHelper<T> {
  /// Attempts to get a mutable reference to the specific state type `T`.
  fn try_into_state_helper_mut(&mut self) -> Option<&mut T>;
}

// Implement the helper trait for each target state type
impl TryIntoStateHelper<MarketDataInfo> for MarketStream {
  fn try_into_state_helper_mut(&mut self) -> Option<&mut MarketDataInfo> {
    match self { MarketStream::TickData(s) => Some(s), _ => None }
  }
}
impl TryIntoStateHelper<RealTimeBarInfo> for MarketStream {
  fn try_into_state_helper_mut(&mut self) -> Option<&mut RealTimeBarInfo> {
    match self { MarketStream::RealTimeBars(s) => Some(s), _ => None }
  }
}
impl TryIntoStateHelper<TickByTickInfo> for MarketStream {
  fn try_into_state_helper_mut(&mut self) -> Option<&mut TickByTickInfo> {
    match self { MarketStream::TickByTick(s) => Some(s), _ => None }
  }
}
impl TryIntoStateHelper<MarketDepthInfo> for MarketStream {
  fn try_into_state_helper_mut(&mut self) -> Option<&mut MarketDepthInfo> {
    match self { MarketStream::MarketDepth(s) => Some(s), _ => None }
  }
}
impl TryIntoStateHelper<ScannerInfoState> for MarketStream {
  fn try_into_state_helper_mut(&mut self) -> Option<&mut ScannerInfoState> {
    match self { MarketStream::Scanner(s) => Some(s), _ => None }
  }
}
impl TryIntoStateHelper<HistogramDataRequestState> for MarketStream {
  fn try_into_state_helper_mut(&mut self) -> Option<&mut HistogramDataRequestState> {
    match self { MarketStream::HistogramData(s) => Some(s), _ => None }
  }
}
impl TryIntoStateHelper<HistoricalTicksRequestState> for MarketStream {
  fn try_into_state_helper_mut(&mut self) -> Option<&mut HistoricalTicksRequestState> {
    match self { MarketStream::HistoricalTicks(s) => Some(s), _ => None }
  }
}
// Add others if needed

// Helper methods on the enum to simplify access in the generic wait function
impl MarketStream {
  /// Tries to get a mutable reference to the specific underlying state type `S`
  /// (e.g., `MarketDataSubscription`) from this `MarketStream` enum variant.
  fn try_get_mut<S>(&mut self) -> Option<&mut S>
  where
    MarketStream: TryIntoStateHelper<S>, // Use helper trait
  {
    self.try_into_state_helper_mut()
  }
}

/// Enum representing the different types of active market data streams
/// managed by `DataMarketManager`. Each variant holds the specific state
/// for that streaming type.
#[derive(Debug)]
enum MarketStream {
  /// State for a standard tick-based market data subscription.
  TickData(MarketDataInfo),
  /// State for a real-time bars subscription.
  RealTimeBars(RealTimeBarInfo),
  /// State for a tick-by-tick data subscription.
  TickByTick(TickByTickInfo),
  MarketDepth(MarketDepthInfo),
  HistoricalData(HistoricalDataRequestState),
  Scanner(ScannerInfoState),
  OptionCalc(OptionCalculationState),
  HistogramData(HistogramDataRequestState),
  HistoricalTicks(HistoricalTicksRequestState),
}

/// Holds the state for an option calculation request (implied volatility or option price).
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct OptionCalculationState {
  req_id: i32,
  contract: Contract, // Contract for which calculation is requested
  result: Option<TickOptionComputationData>, // Stores the result from tickOptionComputation
  completed: bool,
  error_code: Option<i32>,
  error_message: Option<String>,
}

impl OptionCalculationState {
  fn new(req_id: i32, contract: Contract) -> Self {
    Self {
      req_id,
      contract,
      result: None,
      completed: false,
      error_code: None,
      error_message: None,
    }
  }
}

impl CompletableState for OptionCalculationState {
  fn is_completed(&self) -> bool { self.completed || self.result.is_some() }
  fn mark_completed(&mut self) { self.completed = true; }
  fn get_error(&self) -> Option<IBKRError> {
    match (self.error_code, self.error_message.as_ref()) {
      (Some(code), Some(msg)) => Some(IBKRError::ApiError(code, msg.clone())),
      _ => None,
    }
  }
}

impl TryIntoStateHelper<OptionCalculationState> for MarketStream {
  fn try_into_state_helper_mut(&mut self) -> Option<&mut OptionCalculationState> {
    match self { MarketStream::OptionCalc(s) => Some(s), _ => None }
  }
}


/// Manages requests for real-time and historical market data from TWS.
///
/// This includes handling subscriptions for streaming ticks, real-time bars,
/// tick-by-tick data, market depth, and fetching historical data.
/// It also manages the market data type setting (e.g., real-time, delayed, frozen).
///
/// Accessed via [`IBKRClient::data_market()`](crate::IBKRClient::data_market()).
///
/// See the [module-level documentation](index.html) for more details on interaction patterns.
pub struct DataMarketManager {
  message_broker: Arc<MessageBroker>,
  // State for active subscriptions
  subscriptions: Mutex<HashMap<i32, MarketStream>>,
  // Condvar primarily for blocking data requests (historical, get_quote, etc.)
  request_cond: Condvar,
  // State for the connection's current market data type
  current_market_data_type: Mutex<MarketDataType>,
  // Condvar for waiting on market data type changes
  market_data_type_cond: Condvar,
  // State for scanner parameters request (global, as response has no req_id)
  scanner_parameters_xml: Mutex<Option<String>>,
  scanner_parameters_cond: Condvar,
  // Optional: Observer pattern for streaming data
  market_data_observers: RwLock<HashMap<ObserverId, Box<dyn MarketDataObserver + Send + Sync>>>,
  realtime_bars_observers: RwLock<HashMap<ObserverId, Box<dyn RealTimeBarsObserver + Send + Sync>>>,
  tick_by_tick_observers: RwLock<HashMap<ObserverId, Box<dyn TickByTickObserver + Send + Sync>>>,
  market_depth_observers: RwLock<HashMap<ObserverId, Box<dyn MarketDepthObserver + Send + Sync>>>,
  historical_data_observers: RwLock<HashMap<ObserverId, Box<dyn HistoricalDataObserver + Send + Sync>>>,
  historical_ticks_observers: RwLock<HashMap<ObserverId, Box<dyn HistoricalTicksObserver + Send + Sync>>>,

  next_observer_id: AtomicUsize,
  self_weak: Weak<Self>, // For creating Weak<DataMarketManager> for subscriptions

  market_data_limiter: Arc<CounterRateLimiter>,
  historical_limiter: Arc<CounterRateLimiter>,
}

/// Represents a market quote, typically containing Bid, Ask, and Last prices.
/// Each field is an `Option<f64>` because not all prices may be available for all instruments
/// or at all times.
pub type Quote = (Option<f64>, Option<f64>, Option<f64>); // (Bid, Ask, Last)

impl DataMarketManager {
  /// Creates a new `DataMarketManager`.
  ///
  /// This is typically called internally when an `IBKRClient` is created.
  pub(crate) fn new(
    message_broker: Arc<MessageBroker>,
    market_data_limiter: Arc<CounterRateLimiter>,
    historical_limiter: Arc<CounterRateLimiter>
  ) -> Arc<Self> {
    Arc::new_cyclic(|weak_self_ref| DataMarketManager {
      message_broker,
      subscriptions: Mutex::new(HashMap::new()),
      request_cond: Condvar::new(),
      current_market_data_type: Mutex::new(MarketDataType::RealTime), // Default to RealTime
      market_data_type_cond: Condvar::new(),
      scanner_parameters_xml: Mutex::new(None), // Initialize scanner params state
      scanner_parameters_cond: Condvar::new(),
      market_data_observers: RwLock::new(HashMap::new()),
      realtime_bars_observers: RwLock::new(HashMap::new()),
      tick_by_tick_observers: RwLock::new(HashMap::new()),
      market_depth_observers: RwLock::new(HashMap::new()),
      historical_data_observers: RwLock::new(HashMap::new()),
      historical_ticks_observers: RwLock::new(HashMap::new()),
      next_observer_id: AtomicUsize::new(0),
      self_weak: weak_self_ref.clone(),
      market_data_limiter,
      historical_limiter,
    })
  }

  // Helper to get next request ID, used by subscription builders too.
  // Made public within crate for subscription module access.
  pub(crate) fn next_request_id(&self) -> i32 {
    self.message_broker.next_request_id()
  }

  // --- Helper to set market data type if needed ---

  /// Sets the TWS market data type (e.g., RealTime, Delayed, Frozen) if it's different
  /// from the `desired_type`. Blocks until the change is confirmed by TWS or a timeout occurs.
  ///
  /// # Arguments
  /// * `desired_type` - The target `MarketDataType`.
  /// * `timeout` - Maximum duration to wait for TWS to confirm the type change.
  ///
  /// # Errors
  /// Returns `IBKRError::ConfigurationError` if `MarketDataType::Unknown` is requested.
  /// Returns `IBKRError::Unsupported` if the connected TWS version doesn't support type changes.
  /// Returns `IBKRError::Timeout` if TWS doesn't confirm within the timeout.
  /// Returns other `IBKRError` variants for communication issues.
  pub(crate) fn set_market_data_type_if_needed(
    &self,
    desired_type: MarketDataType
  ) -> Result<(), IBKRError> {
    if desired_type == MarketDataType::Unknown {
      return Err(IBKRError::ConfigurationError("Cannot request Unknown market data type".to_string()));
    }

    let current_type_guard = self.current_market_data_type.lock();

    if *current_type_guard == desired_type {
      debug!("Market data type already set to {:?}, no change needed.", desired_type);
      return Ok(());
    }

    info!("Current market data type is {:?}, requesting change to {:?}", *current_type_guard, desired_type);
    let server_version = self.message_broker.get_server_version()?;
    if server_version < crate::min_server_ver::min_server_ver::MARKET_DATA_TYPE {
      return Err(IBKRError::Unsupported(format!(
        "Server version {} does not support changing market data type (requires {}).",
        server_version, crate::min_server_ver::min_server_ver::MARKET_DATA_TYPE
      )));
    }

    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_request_market_data_type(desired_type)?;
    self.message_broker.send_message(&request_msg)?;

    return Ok(());
  }

  // --- Helper to wait for completion (mainly for historical data) ---

  /// Waits for a historical data request to complete.
  /// Completion means either all data has been received (`end_received` is true)
  /// or an error has occurred.
  ///
  /// # Arguments
  /// * `req_id` - The request ID of the historical data request.
  /// * `timeout` - Maximum duration to wait.
  ///
  /// # Returns
  /// `Ok(Vec<Bar>)` containing the historical bars if successful.
  /// `Err(IBKRError)` if an error occurs or the request times out.
  fn wait_for_historical_completion(
    &self,
    req_id: i32,
    timeout: Duration,
  ) -> Result<Vec<Bar>, IBKRError> {
    let start_time = std::time::Instant::now();
    let mut guard = self.subscriptions.lock();

    loop {
      // 1. Check if complete *before* waiting
      let maybe_result = if let Some(MarketStream::HistoricalData(state)) = guard.get(&req_id) {
        if state.end_received {
          Some(Ok(state.bars.clone()))
        } else if let (Some(code), Some(msg)) = (state.error_code, state.error_message.as_ref()) {
          Some(Err(IBKRError::ApiError(code, msg.clone())))
        } else {
          None // Not complete, no error yet
        }
      } else {
        // State missing or wrong type
        Some(Err(IBKRError::InternalError(format!("Historical data state for {} missing or invalid during wait", req_id))))
      };

      match maybe_result {
        Some(Ok(result)) => { guard.remove(&req_id); return Ok(result); },
        Some(Err(e)) => { guard.remove(&req_id); return Err(e); },
        None => {} // Not complete, continue
      }

      // 2. Calculate remaining timeout
      let elapsed = start_time.elapsed();
      if elapsed >= timeout {
        guard.remove(&req_id); // Clean up state on timeout
        return Err(IBKRError::Timeout(format!("Historical data request {} timed out after {:?}", req_id, timeout)));
      }
      let remaining_timeout = timeout - elapsed;

      // 3. Wait
      let wait_result = self.request_cond.wait_for(&mut guard, remaining_timeout);

      // 4. Handle timeout after wait (re-check state)
      if wait_result.timed_out() {
        let final_check = if let Some(MarketStream::HistoricalData(state)) = guard.get(&req_id) {
          if state.end_received { Some(Ok(state.bars.clone())) }
          else if let (Some(code), Some(msg)) = (state.error_code, state.error_message.as_ref()) { Some(Err(IBKRError::ApiError(code, msg.clone()))) }
          else { None }
        } else { None };

        guard.remove(&req_id); // Clean up state regardless
        match final_check {
          Some(Ok(result)) => return Ok(result),
          Some(Err(e)) => return Err(e),
          None => return Err(IBKRError::Timeout(format!("Historical data request {} timed out after wait", req_id))),
        }
      }
      // If not timed out, loop continues
    }
  }


  // --- Helper to wait for real-time bars completion ---

  /// Waits for a `get_realtime_bars` request to complete.
  /// Completion means either the target number of bars (`num_bars`) has been received,
  /// an error has occurred, or the request has been explicitly marked as completed.
  ///
  /// # Arguments
  /// * `req_id` - The request ID of the real-time bars request.
  /// * `num_bars` - The target number of bars to receive.
  /// * `timeout` - Maximum duration to wait.
  ///
  /// # Returns
  /// `Ok(Vec<Bar>)` containing the received real-time bars if successful.
  /// `Err(IBKRError)` if an error occurs or the request times out.
  fn wait_for_realtime_bars_completion(
    &self,
    req_id: i32,
    num_bars: usize,
    timeout: Duration,
  ) -> Result<Vec<Bar>, IBKRError> {
    let start_time = std::time::Instant::now();
    let mut guard = self.subscriptions.lock();

    loop {
      // 1. Check state *before* waiting
      let maybe_result = if let Some(MarketStream::RealTimeBars(state)) = guard.get_mut(&req_id) {
        // Check for explicit completion (error or target reached)
        if state.completed {
          debug!("RealTimeBars request {} marked complete (completed=true).", req_id);
          if let (Some(code), Some(msg)) = (state.error_code, state.error_message.as_ref()) {
            Some(Err(IBKRError::ApiError(code, msg.clone()))) // Completed due to error
          } else {
            Some(Ok(state.bars.clone())) // Completed successfully (count reached)
          }
        }
        // Check for error even if not marked completed yet
        else if let (Some(code), Some(msg)) = (state.error_code, state.error_message.as_ref()) {
          debug!("Error found for RealTimeBars request {} before completion signal.", req_id);
          state.completed = true; // Mark completed due to error
          Some(Err(IBKRError::ApiError(code, msg.clone())))
        }
        // Check if target count reached (should normally set 'completed' flag, but double-check)
        else if state.bars.len() >= num_bars {
          debug!("Target bar count ({}) reached for RealTimeBars request {}.", num_bars, req_id);
          state.completed = true; // Mark completed
          Some(Ok(state.bars.clone()))
        }
        // Otherwise, still waiting
        else {
          None
        }
      } else {
        // State missing or wrong type - internal error
        Some(Err(IBKRError::InternalError(format!("RealTimeBars request state for {} missing or invalid during wait", req_id))))
      };

      match maybe_result {
        Some(Ok(result)) => {
          debug!("RealTimeBars request {} successful, removing state.", req_id);
          guard.remove(&req_id);
          return Ok(result);
        },
        Some(Err(e)) => {
          debug!("RealTimeBars request {} failed, removing state. Error: {:?}", req_id, e);
          guard.remove(&req_id);
          return Err(e);
        },
        None => {} // Not complete, continue
      }

      // 2. Calculate remaining timeout
      let elapsed = start_time.elapsed();
      if elapsed >= timeout {
        guard.remove(&req_id); // Clean up state on timeout
        return Err(Timeout(format!("RealTimeBars request {} timed out after {:?} waiting for {} bars", req_id, timeout, num_bars)));
      }
      let remaining_timeout = timeout - elapsed;

      // 3. Wait
      let wait_result = self.request_cond.wait_for(&mut guard, remaining_timeout);

      // 4. Handle timeout after wait (re-check state one last time)
      if wait_result.timed_out() {
        debug!("Wait timed out for RealTimeBars request {}. Performing final state check.", req_id);
        let final_check = if let Some(MarketStream::RealTimeBars(state)) = guard.get(&req_id) {
          if state.completed { // Check completion flag first
            if let (Some(code), Some(msg)) = (state.error_code, state.error_message.as_ref()) {
              Some(Err(IBKRError::ApiError(code, msg.clone())))
            } else {
              Some(Ok(state.bars.clone())) // Completed successfully just before timeout
            }
          } else if let (Some(code), Some(msg)) = (state.error_code, state.error_message.as_ref()) {
            Some(Err(IBKRError::ApiError(code, msg.clone()))) // Error occurred just before timeout
          } else if state.bars.len() >= num_bars {
            Some(Ok(state.bars.clone())) // Target reached just before timeout
          } else {
            None // Genuine timeout
          }
        } else { None }; // State gone

        guard.remove(&req_id); // Clean up state regardless
        return match final_check {
          Some(Ok(result)) => {
            warn!("RealTimeBars request {} completed successfully just before timeout.", req_id);
            Ok(result)
          },
          Some(Err(e)) => {
            warn!("RealTimeBars request {} failed with error just before timeout: {:?}", req_id, e);
            Err(e)
          },
          None => Err(Timeout(format!("RealTimeBars request {} timed out after wait", req_id))),
        };
      }
      // If not timed out, loop continues to re-check state
    }
  }


  // --- Generic Wait Helper ---

  /// Generic helper function for blocking requests that wait for a specific condition to be met
  /// on their subscription state, or until an error occurs or a timeout is reached.
  ///
  /// # Type Parameters
  /// * `S`: The specific subscription state type (e.g., `MarketDataSubscription`, `TickByTickSubscription`)
  ///        that implements `CompletableState` and `Clone`.
  /// * `F`: A closure type `FnMut(&S) -> bool` that checks if the desired completion condition is met.
  ///
  /// # Arguments
  /// * `req_id` - The request ID.
  /// * `timeout` - Maximum duration to wait.
  /// * `completion_check` - A closure that takes a reference to the subscription state `S`
  ///   and returns `true` if the custom completion condition is met, `false` otherwise.
  /// * `request_type_name` - A string identifying the type of request (for logging).
  ///
  /// # Returns
  /// `Ok(S)` containing a clone of the subscription state when the condition is met.
  /// `Err(IBKRError)` if an error occurs, the request times out, or internal issues arise.
  fn wait_for_completion<S, F>(
    &self,
    req_id: i32,
    timeout: Duration,
    completion_check: F,
    request_type_name: &str, // For logging purposes (e.g., "MarketData", "TickByTick")
  ) -> Result<S, IBKRError>
  where
    S: Clone + Send + Sync + 'static + CompletableState, // Added CompletableState bound, Sync for closure
    F: Fn(&S) -> bool, // Closure takes state ref, returns true if complete
  // Ensure the closure can access the correct state type from the enum
    MarketStream: TryIntoStateHelper<S>, // Use the helper trait here
  {
    let start_time = std::time::Instant::now();
    let mut guard = self.subscriptions.lock();

    loop {
      // 1. Check state *before* waiting
      let maybe_result = if let Some(sub_ref) = guard.get_mut(&req_id) {
        // Attempt to get the specific state type (e.g., MarketDataSubscription)
        if let Some(state) = MarketStream::try_get_mut::<S>(sub_ref) {
          // Check for explicit completion flag first (set by error or handler)
          if state.is_completed() {
            debug!("{} request {} marked complete (completed=true).", request_type_name, req_id);
            if let Some(err) = state.get_error() {
              Some(Err(err)) // Completed due to error
            } else {
              Some(Ok(state.clone())) // Completed successfully by handler/logic
            }
          }
          // Check for error even if not marked completed yet
          else if let Some(err) = state.get_error() {
            debug!("Error found for {} request {} before completion signal.", request_type_name, req_id);
            state.mark_completed(); // Mark completed due to error
            Some(Err(err))
          }
          // Check the user-provided completion condition
          else if completion_check(state) {
            debug!("User completion condition met for {} request {}.", request_type_name, req_id);
            state.mark_completed(); // Mark completed
            Some(Ok(state.clone()))
          }
          // Otherwise, still waiting
          else {
            None
          }
        } else {
          // State exists but is the wrong type - internal error
          Some(Err(IBKRError::InternalError(format!("{} request state for {} has unexpected type during wait", request_type_name, req_id))))
        }
      } else {
        // State missing entirely - internal error or already removed
        Some(Err(IBKRError::InternalError(format!("{} request state for {} missing during wait", request_type_name, req_id))))
      };

      match maybe_result {
        Some(Ok(result)) => {
          debug!("{} request {} successful, removing state.", request_type_name, req_id);
          guard.remove(&req_id);
          return Ok(result);
        },
        Some(Err(e)) => {
          debug!("{} request {} failed, removing state. Error: {:?}", request_type_name, req_id, e);
          guard.remove(&req_id);
          return Err(e);
        },
        None => {} // Not complete, continue
      }

      // 2. Calculate remaining timeout
      let elapsed = start_time.elapsed();
      if elapsed >= timeout {
        guard.remove(&req_id); // Clean up state on timeout
        return Err(Timeout(format!("{} request {} timed out after {:?}", request_type_name, req_id, timeout)));
      }
      let remaining_timeout = timeout - elapsed;

      // 3. Wait
      trace!("{} request {} waiting for {:?}...", request_type_name, req_id, remaining_timeout);
      let wait_result = self.request_cond.wait_for(&mut guard, remaining_timeout);

      // 4. Handle timeout after wait (re-check state one last time)
      if wait_result.timed_out() {
        debug!("Wait timed out for {} request {}. Performing final state check.", request_type_name, req_id);
        let final_check = if let Some(sub_ref) = guard.get_mut(&req_id) {
          if let Some(state) = MarketStream::try_get_mut::<S>(sub_ref) {
            if state.is_completed() { // Check completion flag first
              if let Some(err) = state.get_error() { Some(Err(err)) }
              else { Some(Ok(state.clone())) } // Completed successfully just before timeout
            } else if let Some(err) = state.get_error() {
              Some(Err(err)) // Error occurred just before timeout
            } else if completion_check(state) { // Check user condition again
              Some(Ok(state.clone())) // Condition met just before timeout
            } else { None } // Genuine timeout
          } else { None } // Wrong type
        } else { None }; // State gone

        guard.remove(&req_id); // Clean up state regardless
        return match final_check {
          Some(Ok(result)) => {
            warn!("{} request {} completed successfully just before timeout.", request_type_name, req_id);
            Ok(result)
          },
          Some(Err(e)) => {
            warn!("{} request {} failed with error just before timeout: {:?}", request_type_name, req_id, e);
            Err(e)
          },
          None => Err(Timeout(format!("{} request {} timed out after wait", request_type_name, req_id))),
        };
      }
      // If not timed out, loop continues to re-check state
    }
  }


  // --- Helper to wait for quote completion ---

  /// Waits for a `get_quote` request to complete.
  /// Completion for a quote request means either:
  /// - `tickSnapshotEnd` has been received.
  /// - Both Bid and Ask prices have been received (for instruments that might not have a Last price or snapshot end).
  /// - An error has occurred.
  ///
  /// # Arguments
  /// * `req_id` - The request ID of the quote request.
  /// * `timeout` - Maximum duration to wait.
  ///
  /// # Returns
  /// `Ok(Quote)` containing the (Bid, Ask, Last) prices if successful.
  /// `Err(IBKRError)` if an error occurs or the request times out.
  fn wait_for_quote_completion(
    &self,
    req_id: i32,
    timeout: Duration,
  ) -> Result<Quote, IBKRError> {
    let start_time = std::time::Instant::now();
    let mut guard = self.subscriptions.lock();

    loop {
      // 1. Check state *before* waiting
      let maybe_result = if let Some(MarketStream::TickData(state)) = guard.get(&req_id) {
        // Prioritize checking for an explicit completion signal (snapshot_end or error)
        if state.quote_received {
          debug!("Quote request {} marked complete (quote_received=true).", req_id);
          if let (Some(code), Some(msg)) = (state.error_code, state.error_message.as_ref()) {
            // Completed due to error
            Some(Err(IBKRError::ApiError(code, msg.clone())))
          } else {
            // Completed successfully (snapshot_end), return whatever data we have
            Some(Ok((state.bid_price, state.ask_price, state.last_price)))
          }
        }
        // If not explicitly complete, check if we received an error anyway
        else if let (Some(code), Some(msg)) = (state.error_code, state.error_message.as_ref()) {
          debug!("Error found for quote request {} before completion signal.", req_id);
          Some(Err(IBKRError::ApiError(code, msg.clone())))
        }
        // If not complete and no error, check if we have the required Bid/Ask prices (alternative success)
        // This can happen if ticks arrive before snapshot_end, especially for combos without 'Last'
        else if state.bid_price.is_some() && state.ask_price.is_some() {
          debug!("Required Bid/Ask prices received for quote request {} before completion signal.", req_id);
          Some(Ok((state.bid_price, state.ask_price, state.last_price))) // Return last_price if available, otherwise None
        }
        // Otherwise, still waiting
        else {
          None
        }
      } else {
        // State missing or wrong type - internal error
        Some(Err(IBKRError::InternalError(format!("Quote request state for {} missing or invalid during wait", req_id))))
      };

      match maybe_result {
        Some(Ok(result)) => {
          debug!("Quote request {} successful, removing state.", req_id);
          guard.remove(&req_id);
          return Ok(result);
        },
        Some(Err(e)) => {
          debug!("Quote request {} failed, removing state. Error: {:?}", req_id, e);
          guard.remove(&req_id);
          return Err(e);
        },
        None => {} // Not complete, continue
      }

      // 2. Calculate remaining timeout
      let elapsed = start_time.elapsed();
      if elapsed >= timeout {
        guard.remove(&req_id); // Clean up state on timeout
        return Err(Timeout(format!("Quote request {} timed out after {:?}", req_id, timeout)));
      }
      let remaining_timeout = timeout - elapsed;

      // 3. Wait
      let wait_result = self.request_cond.wait_for(&mut guard, remaining_timeout);

      // 4. Handle timeout after wait (re-check state one last time)
      if wait_result.timed_out() {
        debug!("Wait timed out for quote request {}. Performing final state check.", req_id);
        let final_check = if let Some(MarketStream::TickData(state)) = guard.get(&req_id) {
          // Check completion flag first
          if state.quote_received {
            if let (Some(code), Some(msg)) = (state.error_code, state.error_message.as_ref()) {
              Some(Err(IBKRError::ApiError(code, msg.clone())))
            } else {
              Some(Ok((state.bid_price, state.ask_price, state.last_price)))
            }
          }
          // Check error flag
          else if let (Some(code), Some(msg)) = (state.error_code, state.error_message.as_ref()) {
            Some(Err(IBKRError::ApiError(code, msg.clone())))
          }
          // Check if Bid/Ask prices arrived just before timeout
          else if state.bid_price.is_some() && state.ask_price.is_some() {
            Some(Ok((state.bid_price, state.ask_price, state.last_price))) // Return last_price if available, otherwise None
          }
          // Otherwise, it's a genuine timeout
          else { None }
        } else { None }; // State gone

        guard.remove(&req_id); // Clean up state regardless
        return match final_check {
          Some(Ok(result)) => {
            warn!("Quote request {} completed successfully just before timeout.", req_id);
            Ok(result)
          },
          Some(Err(e)) => {
            warn!("Quote request {} failed with error just before timeout: {:?}", req_id, e);
            Err(e)
          },
          None => Err(Timeout(format!("Quote request {} timed out after wait", req_id))),
        };
      }
      // If not timed out, loop continues to re-check state
    }
  }

  // --- Public API Methods ---

  /// Requests streaming market data (ticks) for a contract. This is a non-blocking call.
  ///
  /// Data updates (prices, sizes, etc.) will be delivered via the `MarketDataHandler`
  /// trait methods implemented by this manager, which would typically notify registered observers.
  ///
  /// # Arguments
  /// * `contract` - The [`Contract`] for which to request data.
  /// * `generic_tick_list` - A slice of [`GenericTickType`] enums specifying the types of generic ticks to request.
  ///   An empty string requests a default set of ticks. See TWS API documentation for available tick types.
  /// * `snapshot` - If `true`, requests a single snapshot of current market data.
  ///   If `false`, requests a continuous stream of updates. For simple snapshots, `get_quote()` might be easier.
  /// * `regulatory_snapshot` - If `true`, requests a regulatory snapshot (requires TWS 963+ and specific permissions).
  /// * `mkt_data_options` - A list of `(tag, value)` pairs for additional options (rarely used).
  /// * `market_data_type` - Optional [`MarketDataType`] to request (e.g., RealTime, Delayed).
  ///   If `None`, `MarketDataType::RealTime` is assumed.
  ///
  /// # Returns
  /// The request ID (`i32`) assigned to this market data request. This ID is needed to cancel the stream.
  ///
  /// # Errors
  /// Returns `IBKRError` if the market data type setting fails, the request cannot be encoded,
  /// or the message fails to send.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError, contract::Contract, data::MarketDataType};
  /// # use yatws::data::GenericTickType;
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// # let market_data_mgr = client.data_market();
  /// let contract = Contract::stock("SPY");
  /// let req_id = market_data_mgr.request_market_data(
  ///     &contract,
  ///     &[GenericTickType::RtVolume], // Request RT Volume generic tick
  ///     false, // Streaming
  ///     false,
  ///     &[],
  ///     Some(MarketDataType::Delayed)
  /// )?;
  /// println!("Requested streaming market data for SPY with req_id: {}", req_id);
  /// // ... (register an observer to process incoming ticks for req_id) ...
  /// // market_data_mgr.cancel_market_data(req_id)?;
  /// # Ok(())
  /// # }
  /// ```
  pub fn request_market_data(
    &self,
    contract: &Contract,
    generic_tick_list: &[GenericTickType],
    snapshot: bool,
    regulatory_snapshot: bool,
    mkt_data_options: &[(String, String)],
    market_data_type: Option<MarketDataType>,
  ) -> Result<i32, IBKRError> {
    let desired_mkt_data_type = market_data_type.unwrap_or(MarketDataType::RealTime);
    // Convert generic_tick_list to comma-separated string for logging and encoding
    let generic_tick_list_str = generic_tick_list
      .iter()
      .map(|t| t.to_string())
      .collect::<Vec<String>>()
      .join(",");

    info!("Requesting market data: Contract={}, GenericTicks='{}', Snapshot={}, RegSnapshot={}, Type={:?}",
          contract.symbol, generic_tick_list_str, snapshot, regulatory_snapshot, desired_mkt_data_type);

    // Set market data type if needed before sending the request
    self.set_market_data_type_if_needed(desired_mkt_data_type)?;

    let req_id = self.message_broker.next_request_id();
    self.internal_request_market_data(
      req_id,
      contract,
      generic_tick_list,
      snapshot,
      regulatory_snapshot,
      mkt_data_options,
      desired_mkt_data_type,
    )?;

    Ok(req_id)
  }

  /// Internal helper to send market data request message and store state.
  pub(crate) fn internal_request_market_data(
    &self,
    req_id: i32,
    contract: &Contract,
    generic_tick_list: &[GenericTickType],
    snapshot: bool,
    regulatory_snapshot: bool,
    mkt_data_options: &[(String, String)],
    desired_mkt_data_type: MarketDataType,
  ) -> Result<(), IBKRError> {
    // Logging for internal call might be slightly different or rely on caller
    // For simplicity, we assume public method handles primary logging.
    // Ensure market data type is set by the caller (public request_market_data or request_observe_market_data)

    let generic_tick_list_str = generic_tick_list
      .iter()
      .map(|t| t.to_string())
      .collect::<Vec<String>>()
      .join(",");

    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let request_msg = encoder.encode_request_market_data(
      req_id, contract, &generic_tick_list_str, snapshot, regulatory_snapshot, mkt_data_options,
    )?;

    if !self.market_data_limiter.try_acquire() {
      return Err(IBKRError::RateLimited("Market data line limit exceeded".to_string()));
    }
    // Initialize and store state
    {
      let mut subs = self.subscriptions.lock();
      if subs.contains_key(&req_id) {
        return Err(IBKRError::DuplicateRequestId(req_id));
      }
      let mut state = MarketDataInfo::new(
        req_id,
        contract.clone(),
        generic_tick_list.to_vec(), // Store the Vec<GenericTickType>
        snapshot,
        regulatory_snapshot,
        mkt_data_options.to_vec(),
      );
      // Store the requested type in the state
      state.market_data_type = Some(desired_mkt_data_type);
      subs.insert(req_id, MarketStream::TickData(state));
      debug!("Market data subscription added for ReqID: {}, Type: {:?}", req_id, desired_mkt_data_type);
    }

    match self.message_broker.send_message(&request_msg) {
      Ok(()) => { self.market_data_limiter.register_request(req_id); Ok(()) }
      Err(e) => Err(e)
    }
  }

  /// Requests streaming market data (ticks) and blocks until a user-defined completion condition is met.
  ///
  /// This method initiates a market data stream similar to `request_market_data` but then
  /// waits until the `completion_check` closure returns `true`, an error occurs, or the `timeout` is reached.
  /// After the wait, it attempts to cancel the market data stream.
  ///
  /// # Arguments
  /// * `contract`, `generic_tick_list` (as `&[GenericTickType]`), `snapshot`, `regulatory_snapshot`, `mkt_data_options`, `market_data_type`:
  ///   Same as for [`request_market_data`](Self::request_market_data).
  /// * `timeout` - Maximum duration to wait for the completion condition.
  /// * `completion_check` - A closure `FnMut(&MarketDataSubscription) -> bool`. It's called
  ///   repeatedly with the current state of the market data subscription. The wait continues
  ///   until this closure returns `true`.
  ///
  /// # Returns
  /// A clone of the `MarketDataInfo` state when the completion condition is met.
  ///
  /// # Errors
  /// Returns `IBKRError` if the underlying request fails, the wait times out, or other issues occur.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError, contract::Contract, data::{MarketDataType, TickType}};
  /// # use std::time::Duration; use yatws::data::GenericTickType;
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// # let market_data_mgr = client.data_market();
  /// let contract = Contract::stock("MSFT");
  ///
  /// // Wait until we receive at least one Bid and one Ask price
  /// let result_state = market_data_mgr.get_market_data(
  ///     &contract, &[], false, false, &[], Some(MarketDataType::Delayed), Duration::from_secs(20),
  ///     |state| {
  ///         let has_bid = state.ticks.contains_key(&TickType::BidPrice) || state.ticks.contains_key(&TickType::DelayedBid);
  ///         let has_ask = state.ticks.contains_key(&TickType::AskPrice) || state.ticks.contains_key(&TickType::DelayedAsk);
  ///         has_bid && has_ask
  ///     }
  /// )?;
  /// println!("MSFT Data: Bid={:?}, Ask={:?}", result_state.bid_price, result_state.ask_price);
  /// # Ok(())
  /// # }
  /// ```
  pub fn get_market_data<F>( // F changed to Fn from FnMut
    &self,
    contract: &Contract,
    generic_tick_list: &[GenericTickType],
    snapshot: bool, // Note: For true snapshots, get_quote might be simpler
    regulatory_snapshot: bool,
    mkt_data_options: &[(String, String)],
    market_data_type: Option<MarketDataType>,
    timeout: Duration,
    completion_check: F, // Closure: Fn(&MarketDataInfo) -> bool
  ) -> Result<MarketDataInfo, IBKRError>
  where
    F: Fn(&MarketDataInfo) -> bool,
  {
    let desired_mkt_data_type = market_data_type.unwrap_or(MarketDataType::RealTime);
    let generic_tick_list_str = generic_tick_list // For logging
      .iter()
      .map(|t| t.to_string())
      .collect::<Vec<String>>()
      .join(",");
    info!("Requesting blocking market data: Contract={}, GenericTicks='{}', Snapshot={}, Type={:?}, Timeout={:?}",
          contract.symbol, generic_tick_list_str, snapshot, desired_mkt_data_type, timeout);

    // Note: set_market_data_type_if_needed is called *inside* request_market_data

    // 1. Initiate the non-blocking request (gets req_id and stores initial state)
    let req_id = self.request_market_data(
      contract,
      generic_tick_list, // Pass the slice
      snapshot,
      regulatory_snapshot,
      mkt_data_options,
      Some(desired_mkt_data_type), // Pass the type
    ).map_err(|e| { warn!("Error in request_market_data during get_market_data: {:?}", e); e })?;
    debug!("Blocking market data request initiated with ReqID: {}, Type: {:?}", req_id, desired_mkt_data_type);

    // 2. Wait for completion using the generic helper
    let result = self.wait_for_completion(
      req_id,
      timeout,
      completion_check,
      "MarketData",
    );

    // 3. Best effort cancel after completion/timeout/error
    if let Err(e) = self.cancel_market_data(req_id) {
      // Log cancel error but prioritize returning the primary result/error
      warn!("Failed to cancel market data request {} after blocking wait: {:?}", req_id, e);
    }

    result
  }


  /// Cancels an active streaming market data request.
  ///
  /// # Arguments
  /// * `req_id` - The request ID obtained from `request_market_data()` or `get_market_data()`.
  ///
  /// # Errors
  /// Returns `IBKRError` if the cancellation message cannot be encoded or sent.
  /// It logs a warning if the `req_id` is not found (e.g., already cancelled or completed).
  pub fn cancel_market_data(&self, req_id: i32) -> Result<(), IBKRError> {
    info!("Cancelling market data request: ReqID={}", req_id);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_market_data(req_id)?;

    // Remove state *before* sending cancel, or after? Let's remove after success.
    self.message_broker.send_message(&request_msg)?;

    self.market_data_limiter.release(req_id);

    // Remove state
    {
      let mut subs = self.subscriptions.lock();
      if subs.remove(&req_id).is_some() {
        debug!("Removed market data subscription state for ReqID: {}", req_id);
      } else {
        warn!("Attempted to cancel market data for unknown or already removed ReqID: {}", req_id);
      }
    }
    Ok(())
  }


  /// Requests a specific number of 5-second real-time bars and blocks until they are received,
  /// an error occurs, or the timeout is reached.
  ///
  /// TWS API currently only supports 5-second bars for this type of request.
  ///
  /// # Arguments
  /// * `contract` - The [`Contract`] for which to request bars.
  /// * `what_to_show` - A [`WhatToShow`] enum specifying the type of data for bars (e.g., Trades, Midpoint, Bid, Ask).
  /// * `use_rth` - If `true`, only include data from regular trading hours.
  /// * `real_time_bars_options` - A list of `(tag, value)` pairs for additional options.
  /// * `num_bars` - The number of 5-second bars to retrieve. Must be greater than 0.
  /// * `timeout` - Maximum duration to wait for all bars.
  ///
  /// # Returns
  /// A `Vec<Bar>` containing the requested real-time bars.
  ///
  /// # Errors
  /// Returns `IBKRError::ConfigurationError` if `num_bars` is 0.
  /// Returns `IBKRError::Timeout` if not all bars are received within the timeout.
  /// Returns other `IBKRError` variants for communication or encoding issues.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError, contract::Contract};
  /// # use std::time::Duration; use yatws::contract::WhatToShow;
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// # let market_data_mgr = client.data_market();
  /// let contract = Contract::stock("AAPL");
  /// let bars = market_data_mgr.get_realtime_bars(
  ///     &contract, WhatToShow::Trades, true, &[], 2, Duration::from_secs(20)
  /// )?;
  /// println!("Received {} real-time bars for AAPL.", bars.len());
  /// for bar in bars {
  ///     println!("  Time: {}, Close: {}", bar.time.format("%H:%M:%S"), bar.close);
  /// }
  /// # Ok(())
  /// # }
  /// ```
  pub fn get_realtime_bars(
    &self,
    contract: &Contract,
    what_to_show: WhatToShow,
    use_rth: bool,
    real_time_bars_options: &[(String, String)],
    num_bars: usize,
    timeout: Duration,
  ) -> Result<Vec<Bar>, IBKRError> {
    if num_bars == 0 {
      return Err(IBKRError::ConfigurationError("num_bars must be greater than 0".to_string()));
    }
    // let bar_size = 5; // bar_size is now passed from request_real_time_bars or its internal helper
    info!("Requesting {} real time bars: Contract={}, What={}, RTH={}, Timeout={:?}",
          num_bars, contract.symbol, what_to_show, use_rth, timeout); // what_to_show will use Display

    let req_id = self.request_real_time_bars(
      contract,
      what_to_show,
      use_rth,
      real_time_bars_options,
    )?;

    // Update state to mark target_bar_count for blocking
    {
      let mut subs = self.subscriptions.lock();
      if let Some(MarketStream::RealTimeBars(state)) = subs.get_mut(&req_id) {
        state.target_bar_count = Some(num_bars);
      } else {
        return Err(IBKRError::InternalError(format!("State for real time bars req_id {} not found after request.", req_id)));
      }
    }

    // Block and wait for completion
    let result = self.wait_for_realtime_bars_completion(req_id, num_bars, timeout);

    let _ = self.cancel_real_time_bars(req_id);
    result
  }


  /// Requests a single snapshot quote (Bid, Ask, Last prices) for a contract.
  /// This is a blocking call that waits until the quote data is received or a timeout occurs.
  ///
  /// # Arguments
  /// * `contract` - The [`Contract`] for which to request the quote.
  /// * `market_data_type` - Optional [`MarketDataType`] (e.g., RealTime, Delayed).
  ///   If `None`, `MarketDataType::RealTime` is assumed.
  /// * `timeout` - Maximum duration to wait for the quote.
  ///
  /// # Returns
  /// A `Quote` tuple `(Option<f64>, Option<f64>, Option<f64>)` representing
  /// (Bid Price, Ask Price, Last Price). Fields will be `None` if the respective
  /// data is not available.
  ///
  /// # Errors
  /// Returns `IBKRError::Timeout` if the quote is not received within the timeout.
  /// Returns other `IBKRError` variants for issues with market data type setting,
  /// request encoding, or message sending.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError, contract::Contract, data::MarketDataType};
  /// # use std::time::Duration;
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// # let market_data_mgr = client.data_market();
  /// let contract = Contract::stock("GOOG");
  /// match market_data_mgr.get_quote(&contract, Some(MarketDataType::Delayed), Duration::from_secs(10)) {
  ///     Ok((bid, ask, last)) => {
  ///         println!("GOOG Quote: Bid={:?}, Ask={:?}, Last={:?}", bid, ask, last);
  ///     },
  ///     Err(e) => eprintln!("Failed to get quote: {:?}", e),
  /// }
  /// # Ok(())
  /// # }
  /// ```
  pub fn get_quote(
    &self,
    contract: &Contract,
    market_data_type: Option<MarketDataType>,
    timeout: Duration
  ) -> Result<Quote, IBKRError> {
    let desired_mkt_data_type = market_data_type.unwrap_or(MarketDataType::RealTime);
    info!("Requesting quote snapshot for: Contract={}, Type={:?}", contract.symbol, desired_mkt_data_type);

    // Set market data type if needed before sending the request
    self.set_market_data_type_if_needed(desired_mkt_data_type)?;

    let req_id = self.message_broker.next_request_id();

    self.internal_request_market_data(
      req_id, contract, &[], true, false, &[], desired_mkt_data_type
    )?;

    // Mark as blocking quote request
    {
      let mut subs = self.subscriptions.lock();
      if let Some(MarketStream::TickData(state)) = subs.get_mut(&req_id) {
        state.is_blocking_quote_request = true;
      }
    }

    // Block and wait for completion
    let result = self.wait_for_quote_completion(req_id, timeout);

    // Best effort cancel after completion/timeout/error
    // Ignore error here as the state might already be removed by the wait function
    let _ = self.cancel_market_data(req_id);

    result
  }


  /// Requests a stream of 5-second real-time bars for a contract. This is a non-blocking call.
  ///
  /// TWS API currently only supports 5-second bars for this type of request.
  /// Bar data is delivered via the `real_time_bar` method of the `MarketDataHandler` trait,
  /// which would typically notify registered observers.
  ///
  /// # Arguments
  /// * `contract` - The [`Contract`] for which to request bars.
  /// * `what_to_show` - A [`WhatToShow`] enum specifying the type of data for bars.
  /// * `use_rth` - If `true`, only include data from regular trading hours.
  /// * `real_time_bars_options` - A list of `(tag, value)` pairs for additional options.
  ///
  /// # Returns
  /// The request ID (`i32`) assigned to this real-time bars request.
  ///
  /// # Errors
  /// Returns `IBKRError` if the request cannot be encoded or sent.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError, contract::Contract};
  /// # use yatws::contract::WhatToShow; // Added import
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// # let market_data_mgr = client.data_market();
  /// let contract = Contract::stock("TSLA");
  /// let req_id = market_data_mgr.request_real_time_bars(
  ///     &contract, WhatToShow::Trades, true, &[]
  /// )?;
  /// println!("Requested streaming real-time bars for TSLA with req_id: {}", req_id);
  /// // ... (register an observer to process incoming bars for req_id) ...
  /// // market_data_mgr.cancel_real_time_bars(req_id)?;
  /// # Ok(())
  /// # }
  /// ```
  pub fn request_real_time_bars(
    &self,
    contract: &Contract,
    what_to_show: WhatToShow,
    use_rth: bool,
    real_time_bars_options: &[(String, String)],
  ) -> Result<i32, IBKRError> {
    let bar_size = 5; // Hardcoded as per API limitation
    info!("Requesting real time bars: Contract={}, What={}, RTH={}", contract.symbol, what_to_show, use_rth); // what_to_show will use Display
    let req_id = self.next_request_id(); // Use the new helper method

    self.internal_request_real_time_bars(
      req_id,
      contract,
      what_to_show,
      use_rth,
      real_time_bars_options,
      bar_size,
    )?;

    Ok(req_id)
  }

  pub(crate) fn internal_request_real_time_bars(
    &self, req_id: i32, contract: &Contract, what_to_show: WhatToShow, use_rth: bool, real_time_bars_options: &[(String, String)], bar_size: i32
  ) -> Result<(), IBKRError> {
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let request_msg = encoder.encode_request_real_time_bars(
      req_id, contract, bar_size, &what_to_show.to_string(), use_rth, real_time_bars_options,
    )?;

    if !self.market_data_limiter.try_acquire() {
      return Err(IBKRError::RateLimited("Market data line limit exceeded".to_string()));
    }
    {
      let mut subs = self.subscriptions.lock();
      if subs.contains_key(&req_id) { return Err(IBKRError::DuplicateRequestId(req_id)); }
      let state = RealTimeBarInfo {
        req_id,
        contract: contract.clone(),
        bar_size,
        what_to_show, // Store the enum
        use_rth,
        rt_bar_options: real_time_bars_options.to_vec(),
        latest_bar: None,
        bars: Vec::new(),
        target_bar_count: None, // Default for streaming
        completed: false, // Not completed yet
        error_code: None,
        error_message: None,
      };
      subs.insert(req_id, MarketStream::RealTimeBars(state));
    }
    match self.message_broker.send_message(&request_msg) {
      Ok(()) => { self.market_data_limiter.register_request(req_id); Ok(()) }
      Err(e) => Err(e)
    }
  }


  /// Cancels an active streaming real-time bars request.
  ///
  /// # Arguments
  /// * `req_id` - The request ID obtained from `request_real_time_bars()` or `get_realtime_bars()`.
  ///
  /// # Errors
  /// Returns `IBKRError` if the cancellation message cannot be encoded or sent.
  pub fn cancel_real_time_bars(&self, req_id: i32) -> Result<(), IBKRError> {
    info!("Cancelling real time bars request: ReqID={}", req_id);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_real_time_bars(req_id)?;

    self.message_broker.send_message(&request_msg)?;
    self.market_data_limiter.release(req_id);

    {
      let mut subs = self.subscriptions.lock();
      if subs.remove(&req_id).is_some() {
        debug!("Removed real time bar subscription state for ReqID: {}", req_id);
      } else {
        warn!("Attempted to cancel real time bars for unknown or already removed ReqID: {}", req_id);
      }
    }
    Ok(())
  }


  /// Requests a stream of tick-by-tick data for a contract. This is a non-blocking call.
  ///
  /// Tick-by-tick data provides a detailed view of trades, bid/ask changes, or midpoint updates.
  /// Data is delivered via the `tick_by_tick_*` methods of the `MarketDataHandler` trait,
  /// which would typically notify registered observers.
  ///
  /// # Arguments
  /// * `contract` - The [`Contract`] for which to request data.
  /// * `tick_type` - A [`TickByTickRequestType`] enum specifying the type of tick data:
  ///     - `Last`: Last trade ticks.
  ///     - `AllLast`: All last trade ticks (includes non-NBBO).
  ///     - `BidAsk`: Bid and Ask ticks.
  ///     - `MidPoint`: Midpoint ticks.
  /// * `number_of_ticks` - For historical tick-by-tick data, the number of ticks to retrieve.
  ///   Set to `0` for streaming live tick-by-tick data.
  /// * `ignore_size` - For "BidAsk" tick type, if `true`, do not report bid/ask sizes (saves bandwidth).
  ///   Usually `false` for streaming.
  ///
  /// # Returns
  /// The request ID (`i32`) assigned to this tick-by-tick data request.
  ///
  /// # Errors
  /// Returns `IBKRError` if the request cannot be encoded or sent.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError, contract::Contract};
  /// # use yatws::data::TickByTickRequestType; // Added import
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// # let market_data_mgr = client.data_market();
  /// let contract = Contract::stock("NVDA");
  /// let req_id = market_data_mgr.request_tick_by_tick_data(
  ///     &contract, TickByTickRequestType::Last, 0, false // Stream Last trades
  /// )?;
  /// println!("Requested streaming tick-by-tick 'Last' data for NVDA with req_id: {}", req_id);
  /// // ... (register an observer to process incoming ticks for req_id) ...
  /// // market_data_mgr.cancel_tick_by_tick_data(req_id)?;
  /// # Ok(())
  /// # }
  /// ```
  pub fn request_tick_by_tick_data(
    &self,
    contract: &Contract,
    tick_type: TickByTickRequestType,
    number_of_ticks: i32, // 0 for streaming, >0 for historical snapshot
    ignore_size: bool, // Usually false for streaming
  ) -> Result<i32, IBKRError> {
    info!("Requesting tick-by-tick data: Contract={}, Type={}, NumTicks={}, IgnoreSize={}",
          contract.symbol, tick_type, number_of_ticks, ignore_size); // tick_type will use Display
    let req_id = self.next_request_id(); // Use the new helper method

    self.internal_request_tick_by_tick_data(req_id, contract, tick_type, number_of_ticks, ignore_size)?;

    Ok(req_id)
  }

  pub(crate) fn internal_request_tick_by_tick_data(
    &self,
    req_id: i32,
    contract: &Contract,
    tick_type: TickByTickRequestType,
    number_of_ticks: i32,
    ignore_size: bool,
  ) -> Result<(), IBKRError> {
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let request_msg = encoder.encode_request_tick_by_tick_data(
      req_id, contract, &tick_type.to_string(), number_of_ticks, ignore_size,
    )?;

    if !self.market_data_limiter.try_acquire() {
      return Err(IBKRError::RateLimited("Market data line limit exceeded".to_string()));
    }
    {
      let mut subs = self.subscriptions.lock();
      if subs.contains_key(&req_id) { return Err(IBKRError::DuplicateRequestId(req_id)); }
      let state = TickByTickInfo {
        req_id,
        contract: contract.clone(),
        tick_type, // Store the enum
        number_of_ticks,
        ignore_size,
        latest_tick: None,
        ticks: Vec::new(), // Initialize history
        completed: false, // Initialize completion flag
        error_code: None,
        error_message: None,
      };
      subs.insert(req_id, MarketStream::TickByTick(state));
      debug!("Tick-by-tick subscription added for ReqID: {}", req_id);
    }

    match self.message_broker.send_message(&request_msg) {
      Ok(()) => { self.market_data_limiter.register_request(req_id); Ok(()) }
      Err(e) => Err(e)
    }
  }

  /// Requests streaming tick-by-tick data and blocks until a user-defined completion condition is met.
  ///
  /// This method initiates a tick-by-tick data stream similar to `request_tick_by_tick_data`
  /// but then waits until the `completion_check` closure returns `true`, an error occurs,
  /// or the `timeout` is reached. After the wait, it attempts to cancel the stream.
  ///
  /// For streaming requests intended to be blocked upon, `number_of_ticks` should typically be `0`.
  ///
  /// # Arguments
  /// * `contract`, `tick_type` (as `TickByTickRequestType`), `number_of_ticks`, `ignore_size`: Same as for `request_tick_by_tick_data`.
  /// * `timeout` - Maximum duration to wait for the completion condition.
  /// * `completion_check` - A closure `FnMut(&TickByTickSubscription) -> bool`. It's called
  ///   repeatedly with the current state of the tick-by-tick subscription. The wait continues
  ///   until this closure returns `true`.
  ///
  /// # Returns
  /// A clone of the `TickByTickSubscription` state when the completion condition is met.
  ///
  /// # Errors
  /// Returns `IBKRError` if the underlying request fails, the wait times out, or other issues occur.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError, contract::Contract, data::TickByTickData};
  /// # use std::time::Duration; use yatws::data::TickByTickRequestType; // Added import
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// # let market_data_mgr = client.data_market();
  /// let contract = Contract::stock("GOOG");
  /// let target_ticks = 5;
  ///
  /// // Wait until we receive at least 5 'Last' ticks
  /// let result_state = market_data_mgr.get_tick_by_tick_data(
  ///     &contract, TickByTickRequestType::Last, 0, false, Duration::from_secs(30),
  ///     |state| state.ticks.len() >= target_ticks
  /// )?;
  /// println!("Received at least {} tick-by-tick 'Last' data for GOOG.", result_state.ticks.len());
  /// # Ok(())
  /// # }
  /// ```
  pub fn get_tick_by_tick_data<F>(
    &self,
    contract: &Contract,
    tick_type: TickByTickRequestType,
    number_of_ticks: i32, // Should be 0 for streaming blocking requests
    ignore_size: bool,
    timeout: Duration,
    completion_check: F, // Closure: Fn(&TickByTickSubscription) -> bool
  ) -> Result<TickByTickInfo, IBKRError>
  where
    F: Fn(&TickByTickInfo) -> bool,
  {
    if number_of_ticks != 0 {
      // This function is intended for blocking on *streaming* data.
      // For historical ticks (number_of_ticks > 0), a different mechanism might be needed.
      warn!("get_tick_by_tick_data called with non-zero number_of_ticks ({}). This function blocks on streaming data (number_of_ticks=0).", number_of_ticks);
    }
    info!("Requesting blocking tick-by-tick data: Contract={}, Type={}, Timeout={:?}",
          contract.symbol, tick_type, timeout); // tick_type will use Display
    // 1. Initiate the non-blocking request
    let req_id = self.request_tick_by_tick_data(
      contract,
      tick_type, // Pass the enum
      number_of_ticks,
      ignore_size,
    )?;
    debug!("Blocking tick-by-tick request initiated with ReqID: {}", req_id);

    // 2. Wait for completion
    let result = self.wait_for_completion(
      req_id,
      timeout,
      completion_check,
      "TickByTick",
    );

    // 3. Best effort cancel
    if let Err(e) = self.cancel_tick_by_tick_data(req_id) {
      warn!("Failed to cancel tick-by-tick request {} after blocking wait: {:?}", req_id, e);
    }

    result
  }


  /// Cancels an active streaming tick-by-tick data request.
  ///
  /// # Arguments
  /// * `req_id` - The request ID obtained from `request_tick_by_tick_data()` or `get_tick_by_tick_data()`.
  ///
  /// # Errors
  /// Returns `IBKRError` if the cancellation message cannot be encoded or sent.
  pub fn cancel_tick_by_tick_data(&self, req_id: i32) -> Result<(), IBKRError> {
    info!("Cancelling tick-by-tick data request: ReqID={}", req_id);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_tick_by_tick_data(req_id)?;

    self.message_broker.send_message(&request_msg)?;
    self.market_data_limiter.release(req_id);
    {
      let mut subs = self.subscriptions.lock();
      if subs.remove(&req_id).is_some() {
        debug!("Removed tick-by-tick subscription state for ReqID: {}", req_id);
      } else {
        warn!("Attempted to cancel tick-by-tick for unknown or already removed ReqID: {}", req_id);
      }
    }
    Ok(())
  }

  /// Requests a stream of market depth data (Level II book). This is a non-blocking call.
  ///
  /// Market depth updates are delivered via the `update_mkt_depth` and `update_mkt_depth_l2`
  /// methods of the `MarketDataHandler` trait, which would typically notify registered observers.
  ///
  /// # Arguments
  /// * `contract` - The [`Contract`] for which to request market depth.
  /// * `num_rows` - The number of rows of market depth data to display on each side (bid/ask).
  /// * `is_smart_depth` - If `true`, requests aggregated depth from SMART routing.
  ///   If `false`, requests depth from the contract's native exchange. SMART depth requires TWS 96 SMART Depth subscription.
  /// * `mkt_depth_options` - A list of `(tag, value)` pairs for additional options.
  ///
  /// # Returns
  /// The request ID (`i32`) assigned to this market depth request.
  ///
  /// # Errors
  /// Returns `IBKRError` if the request cannot be encoded or sent.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError, contract::Contract};
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// # let market_data_mgr = client.data_market();
  /// let contract = Contract::stock("IBM");
  /// let req_id = market_data_mgr.request_market_depth(
  ///     &contract, 5, false, &[] // Request 5 levels of regular depth
  /// )?;
  /// println!("Requested streaming market depth for IBM with req_id: {}", req_id);
  /// // ... (register an observer to process incoming depth updates for req_id) ...
  /// // market_data_mgr.cancel_market_depth(req_id)?;
  /// # Ok(())
  /// # }
  /// ```
  pub fn request_market_depth(
    &self,
    contract: &Contract,
    num_rows: i32,
    is_smart_depth: bool,
    mkt_depth_options: &[(String, String)],
  ) -> Result<i32, IBKRError> {
    info!("Requesting market depth: Contract={}, Rows={}, Smart={}", contract.symbol, num_rows, is_smart_depth);
    let req_id = self.next_request_id(); // Use the new helper method
    self.internal_request_market_depth(req_id, contract, num_rows, is_smart_depth, mkt_depth_options)?;
    Ok(req_id)
  }

  pub(crate) fn internal_request_market_depth(
    &self, req_id: i32, contract: &Contract, num_rows: i32, is_smart_depth: bool, mkt_depth_options: &[(String, String)]
  ) -> Result<(), IBKRError> {


    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let request_msg = encoder.encode_request_market_depth(
      req_id, contract, num_rows, is_smart_depth, mkt_depth_options,
    )?;

    if !self.market_data_limiter.try_acquire() {
      return Err(IBKRError::RateLimited("Market data line limit exceeded".to_string()));
    }
    {
      let mut subs = self.subscriptions.lock();
      if subs.contains_key(&req_id) { return Err(IBKRError::DuplicateRequestId(req_id)); }
      let state = MarketDepthInfo {
        req_id,
        contract: contract.clone(),
        num_rows,
        is_smart_depth,
        mkt_depth_options: mkt_depth_options.to_vec(),
        bid_price: None, ask_price: None, bid_size: None, ask_size: None,
        depth_bids: Vec::new(), depth_asks: Vec::new(),
        completed: false, // Initialize completion flag
        error_code: None, error_message: None,
      };
      subs.insert(req_id, MarketStream::MarketDepth(state));
      debug!("Market depth subscription added for ReqID: {}", req_id);
    }

    match self.message_broker.send_message(&request_msg) {
      Ok(()) => { self.market_data_limiter.register_request(req_id); Ok(()) }
      Err(e) => Err(e)
    }
  }

  /// Requests streaming market depth data and blocks until a user-defined completion condition is met.
  ///
  /// This method initiates a market depth stream similar to `request_market_depth`
  /// but then waits until the `completion_check` closure returns `true`, an error occurs,
  /// or the `timeout` is reached. After the wait, it attempts to cancel the stream.
  ///
  /// # Arguments
  /// * `contract`, `num_rows`, `is_smart_depth`, `mkt_depth_options`: Same as for `request_market_depth`.
  /// * `timeout` - Maximum duration to wait for the completion condition.
  /// * `completion_check` - A closure `FnMut(&MarketDepthSubscription) -> bool`. It's called
  ///   repeatedly with the current state of the market depth subscription. The wait continues
  ///   until this closure returns `true`.
  ///
  /// # Returns
  /// A clone of the `MarketDepthSubscription` state when the completion condition is met.
  ///
  /// # Errors
  /// Returns `IBKRError` if the underlying request fails, the wait times out, or other issues occur.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError, contract::Contract};
  /// # use std::time::Duration;
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// # let market_data_mgr = client.data_market();
  /// let contract = Contract::stock("IBM");
  ///
  /// // Wait until we have at least one bid and one ask level in the depth book
  /// let result_state = market_data_mgr.get_market_depth(
  ///     &contract, 5, false, &[], Duration::from_secs(20),
  ///     |state| !state.depth_bids.is_empty() && !state.depth_asks.is_empty()
  /// )?;
  /// println!("IBM Market Depth: Top Bid Px={:?}, Top Ask Px={:?}",
  ///          result_state.bid_price, result_state.ask_price);
  /// # Ok(())
  /// # }
  /// ```
  pub fn get_market_depth<F>(
    &self,
    contract: &Contract,
    num_rows: i32,
    is_smart_depth: bool,
    mkt_depth_options: &[(String, String)],
    timeout: Duration,
    completion_check: F, // Closure: Fn(&MarketDepthSubscription) -> bool
  ) -> Result<MarketDepthInfo, IBKRError>
  where
    F: Fn(&MarketDepthInfo) -> bool,
  {
    info!("Requesting blocking market depth: Contract={}, Rows={}, Smart={}, Timeout={:?}",
          contract.symbol, num_rows, is_smart_depth, timeout);

    // 1. Initiate the non-blocking request
    let req_id = self.request_market_depth(
      contract,
      num_rows,
      is_smart_depth,
      mkt_depth_options,
    )?;
    debug!("Blocking market depth request initiated with ReqID: {}", req_id);

    // 2. Wait for completion
    let result = self.wait_for_completion(
      req_id,
      timeout,
      completion_check,
      "MarketDepth",
    );

    // 3. Best effort cancel
    // Note: cancel_market_depth needs is_smart_depth, which might be tricky if state removed early.
    // We retrieve it from the result *if successful*, otherwise guess or log warning.
    let smart_for_cancel = match &result {
      Ok(state) => state.is_smart_depth,
      Err(_) => {
        warn!("Could not determine is_smart_depth for cancelling request {}. Assuming false.", req_id);
        false // Best guess
      }
    };
    if let Err(e) = self._cancel_market_depth_internal(req_id, smart_for_cancel) {
      warn!("Failed to cancel market depth request {} after blocking wait: {:?}", req_id, e);
    }


    result
  }


  /// Cancels an active streaming market depth request.
  ///
  /// # Arguments
  /// * `req_id` - The request ID obtained from `request_market_depth()` or `get_market_depth()`.
  ///
  /// # Errors
  /// Returns `IBKRError` if the cancellation message cannot be encoded or sent.
  /// It logs a warning if the `req_id` is not found or if `is_smart_depth` cannot be determined.
  pub fn cancel_market_depth(&self, req_id: i32) -> Result<(), IBKRError> {
    let is_smart_depth = { // Need to check the state for is_smart_depth flag
      let subs = self.subscriptions.lock();
      if let Some(MarketStream::MarketDepth(state)) = subs.get(&req_id) {
        state.is_smart_depth
      } else {
        // If state not found, assume false or return error? Let's assume false.
        warn!("Cannot determine is_smart_depth for cancel_market_depth ReqID: {}. Assuming false.", req_id);
        false
      }
    };
    self._cancel_market_depth_internal(req_id, is_smart_depth)
  }

  pub(crate) fn _cancel_market_depth_internal(&self, req_id: i32, is_smart_depth: bool) -> Result<(), IBKRError> {
    info!("Cancelling market depth request: ReqID={}, Smart={}", req_id, is_smart_depth);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_market_depth(req_id, is_smart_depth)?;

    self.message_broker.send_message(&request_msg)?;
    self.market_data_limiter.release(req_id);

    {
      let mut subs = self.subscriptions.lock();
      if subs.remove(&req_id).is_some() {
        debug!("Removed market depth subscription state for ReqID: {}", req_id);
      } else {
        warn!("Attempted to cancel market depth for unknown or already removed ReqID: {}", req_id);
      }
    }
    Ok(())
  }


  /// Requests historical bar data for a contract. This is a blocking call.
  ///
  /// It waits until all historical bars are received from TWS or a timeout occurs.
  ///
  /// # Arguments
  /// * `contract` - The [`Contract`] for which to request historical data.
  /// * `end_date_time` - Optional `DateTime<Utc>` specifying the end point of the historical data.
  ///   If `None`, data up to the present time is requested.
  /// * `duration` - A [`DurationUnit`] enum specifying the duration of data to request (e.g., `DurationUnit::Day(3)`).
  /// * `bar_size_setting` - A [`crate::contract::BarSize`] enum specifying the size of each bar.
  /// * `what_to_show` - A [`WhatToShow`] enum specifying the type of data to include in bars.
  /// * `use_rth` - If `true`, only include data from regular trading hours.
  /// * `format_date` - Date format: `1` for "yyyyMMdd HH:mm:ss", `2` for system time (seconds since epoch).
  /// * `keep_up_to_date` - If `true`, subscribe to updates for the head bar after the initial data load.
  ///   (Note: `DataMarketManager` currently processes these updates but doesn't have a dedicated observer mechanism for them beyond the initial fetch).
  /// * `market_data_type` - Optional [`MarketDataType`] for the historical data (e.g., Delayed).
  ///   If `None`, `MarketDataType::RealTime` is assumed (which might require subscriptions for some data).
  /// * `chart_options` - A list of `(tag, value)` pairs for additional chart options.
  ///
  /// # Returns
  /// A `Vec<Bar>` containing the historical bars.
  ///
  /// # Errors
  /// Returns `IBKRError::Timeout` if the data is not received within the timeout.
  /// Returns other `IBKRError` variants for issues with market data type setting,
  /// request encoding, or message sending.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError, contract::Contract, data::MarketDataType};
  /// # use yatws::contract::{BarSize, WhatToShow}; // Added imports
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// # let market_data_mgr = client.data_market();
  /// let contract = Contract::stock("IBM");
  /// let bars = market_data_mgr.get_historical_data(
  ///     &contract,
  ///     None, // Up to present
  ///     "3 D", // 3 days of data
  ///     BarSize::Hour1, // 1-hour bars
  ///     WhatToShow::Trades,
  ///     true, // RTH only
  ///     1,    // yyyyMMdd HH:mm:ss format
  ///     false, // Don't keep up to date
  ///     Some(MarketDataType::Delayed),
  ///     &[]
  /// )?;
  /// println!("Received {} historical bars for IBM.", bars.len());
  /// if let Some(bar) = bars.first() {
  ///     println!("First bar: Time={}, Close={}", bar.time, bar.close);
  /// }
  /// # Ok(())
  /// # }
  /// ```
  pub fn get_historical_data(
    &self,
    contract: &Contract,
    end_date_time: Option<chrono::DateTime<chrono::Utc>>, // Use chrono DateTime
    duration: DurationUnit,
    bar_size_setting: crate::contract::BarSize,
    what_to_show: WhatToShow,
    use_rth: bool,
    format_date: i32, // 1 for yyyyMMdd HH:mm:ss, 2 for system time (seconds)
    keep_up_to_date: bool, // Subscribe to updates after initial load
    market_data_type: Option<MarketDataType>, // Added parameter
    chart_options: &[(String, String)], // TagValue list
  ) -> Result<Vec<Bar>, IBKRError> {
    let desired_mkt_data_type = market_data_type.unwrap_or(MarketDataType::RealTime);
    let duration_str = duration.to_string(); // Convert enum to string for logging and encoding
    info!("Requesting historical data: Contract={}, Duration={}, BarSize={}, What={}, KeepUpToDate={}, Type={:?}",
          contract.symbol, duration_str, bar_size_setting, what_to_show, keep_up_to_date, desired_mkt_data_type); // what_to_show will use Display

    // Set market data type if needed before sending the request
    self.set_market_data_type_if_needed(desired_mkt_data_type)?;

    let req_id = self.next_request_id(); // Use the new helper method
    self.internal_request_historical_data(
      req_id, contract, end_date_time, &duration_str, &bar_size_setting.to_string(),
      &what_to_show.to_string(), use_rth, format_date, keep_up_to_date, chart_options, desired_mkt_data_type
    )?;

    // Block and wait for completion
    let timeout = Duration::from_secs(60); // Historical can take time
    self.wait_for_historical_completion(req_id, timeout)
  }

  pub(crate) fn internal_request_historical_data(
    &self, req_id: i32, contract: &Contract, end_date_time: Option<chrono::DateTime<chrono::Utc>>,
    duration_str: &str, bar_size_str: &str, what_to_show_str: &str, use_rth: bool,
    format_date: i32, keep_up_to_date: bool, chart_options: &[(String, String)],
    desired_mkt_data_type: MarketDataType
  ) -> Result<(), IBKRError> {

    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let request_msg = encoder.encode_request_historical_data(
      req_id, contract, end_date_time, duration_str, bar_size_str,
      what_to_show_str, use_rth, format_date, keep_up_to_date, chart_options,
    )?;

    if !self.historical_limiter.try_acquire() {
      return Err(IBKRError::RateLimited("Historical data rate limit exceeded".to_string()));
    }
    {
      let mut subs = self.subscriptions.lock();
      if subs.contains_key(&req_id) { return Err(IBKRError::DuplicateRequestId(req_id)); }
      let mut state = HistoricalDataRequestState {
        req_id,
        contract: contract.clone(),
        // what_to_show: what_to_show, // Store enum if needed in state, or keep as string if only for request
        ..Default::default()
      };
      state.requested_market_data_type = desired_mkt_data_type;
      subs.insert(req_id, MarketStream::HistoricalData(state));
      debug!("Historical data request added for ReqID: {}, Type: {:?}", req_id, desired_mkt_data_type);
    }

    match self.message_broker.send_message(&request_msg) {
      Ok(()) => { self.historical_limiter.register_request(req_id); Ok(()) }
      Err(e) => Err(e)
    }
  }

  /// Cancels an ongoing historical data request.
  ///
  /// # Arguments
  /// * `req_id` - The request ID obtained from `get_historical_data()`.
  ///
  /// # Errors
  /// Returns `IBKRError` if the cancellation message cannot be encoded or sent.
  /// It logs a warning if the `req_id` is not found (e.g., already completed or cancelled).
  pub fn cancel_historical_data(&self, req_id: i32) -> Result<(), IBKRError> {
    info!("Cancelling historical data request: ReqID={}", req_id);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_historical_data(req_id)?;

    self.message_broker.send_message(&request_msg)?;
    self.historical_limiter.release(req_id);

    // Signal the waiting thread (if any) that it was cancelled?
    // The wait loop will eventually time out or see an error.
    // Removing the state might be enough.

    {
      let mut subs = self.subscriptions.lock();
      if let Some(MarketStream::HistoricalData(state)) = subs.get_mut(&req_id) {
        // Optionally set an error state to indicate cancellation
        state.error_code = Some(-1); // Use a custom code for cancellation
        state.error_message = Some("Request cancelled by user".to_string());
        state.end_received = true; // Mark as ended to unblock waiter
        self.request_cond.notify_all(); // Notify waiter immediately
        // Keep state briefly so waiter sees error, waiter cleans up.
        debug!("Marked historical data request {} as cancelled.", req_id);
      } else if subs.remove(&req_id).is_some() {
        debug!("Removed other historical data subscription state for ReqID: {}", req_id);
      }
      else {
        warn!("Attempted to cancel historical data for unknown or already removed ReqID: {}", req_id);
      }
    }
    Ok(())
  }

  // --- Internal error handling (called by the trait method) ---
  // Renamed to avoid conflict with the trait method name.
  fn _internal_handle_error(&self, req_id: i32, code: ClientErrorCode, msg: &str) {
    if req_id <= 0 { return; } // Ignore general errors not tied to a request

    match code {
      ClientErrorCode::MarketDataNotSubscribedDisplayDelayed => {
        info!("API Info received for market data request {}: Code={:?}, Msg={}", req_id, code, msg);
        return;
      }
      _ => {},
    }

    let mut subs = self.subscriptions.lock();
    if let Some(sub_state) = subs.get_mut(&req_id) {
      warn!("API Error received for market data request {}: Code={:?}, Msg={}", req_id, code, msg);

      // Convert code to i32 for storage
      let error_code_int = code as i32;

      // Determine if the request is blocking *before* taking mutable borrows
      let is_blocking = match sub_state {
        MarketStream::TickData(s) => s.is_blocking_quote_request || s.completed,
        MarketStream::RealTimeBars(s) => s.target_bar_count.is_some() || s.completed,
        MarketStream::TickByTick(s) => s.completed,
        MarketStream::MarketDepth(s) => s.completed,
        MarketStream::HistoricalData(_) => true, // Historical is always blocking in this context
        MarketStream::Scanner(s) => s.completed, // Scanner blocking depends on its completed state
        MarketStream::OptionCalc(s) => s.completed, // Option calculation blocking depends on its completed state
        MarketStream::HistogramData(s) => s.completed,
        MarketStream::HistoricalTicks(s) => s.completed,
      };

      // Extract error fields and completion flag (now safe)
      let (err_code_field, err_msg_field, completion_flag_field) = match sub_state {
        MarketStream::TickData(s) => (&mut s.error_code, &mut s.error_message, &mut s.completed),
        MarketStream::RealTimeBars(s) => (&mut s.error_code, &mut s.error_message, &mut s.completed),
        MarketStream::TickByTick(s) => (&mut s.error_code, &mut s.error_message, &mut s.completed),
        MarketStream::MarketDepth(s) => (&mut s.error_code, &mut s.error_message, &mut s.completed),
        MarketStream::HistoricalData(s) => (&mut s.error_code, &mut s.error_message, &mut s.end_received),
        MarketStream::Scanner(s) => (&mut s.error_code, &mut s.error_message, &mut s.completed),
        MarketStream::OptionCalc(s) => (&mut s.error_code, &mut s.error_message, &mut s.completed),
        MarketStream::HistogramData(s) => (&mut s.error_code, &mut s.error_message, &mut s.completed),
        MarketStream::HistoricalTicks(s) => (&mut s.error_code, &mut s.error_message, &mut s.completed),
      };
      // We could list the types and figure out which to cancel instead.
      self.market_data_limiter.release(req_id);
      self.historical_limiter.release(req_id);

      *err_code_field = Some(error_code_int); // Store the integer code
      *err_msg_field = Some(msg.to_string()); // Store the cloned message string

      // is blocking: (historical, quote, or flexible closure-based)
      // If it's a blocking request, mark end and notify waiter
      if is_blocking {
        // Use the completion flag variable we extracted earlier
        *completion_flag_field = true; // This line caused E0425, now fixed by the previous block
        // Special handling for TickData quote requests
        if let MarketStream::TickData(s) = sub_state {
          if s.is_blocking_quote_request {
            s.quote_received = true; // Mark quote flag too
          }
        }
        debug!("Error received for blocking request {}, marking complete and notifying waiter.", req_id);
        self.request_cond.notify_all(); // Notify waiter immediately
      } else {
        // For non-blocking streaming requests, the error is stored, and observers are notified.
        // Determine the type of stream to notify the correct observers.
        match sub_state {
          MarketStream::TickData(_) => {
            let observers = self.market_data_observers.read();
            for observer in observers.values() { observer.on_error(req_id, error_code_int, msg); }
          }
          MarketStream::RealTimeBars(_) => {
            let observers = self.realtime_bars_observers.read();
            for observer in observers.values() { observer.on_error(req_id, error_code_int, msg); }
          }
          MarketStream::TickByTick(_) => {
            let observers = self.tick_by_tick_observers.read();
            for observer in observers.values() { observer.on_error(req_id, error_code_int, msg); }
          }
          MarketStream::MarketDepth(_) => {
            let observers = self.market_depth_observers.read();
            for observer in observers.values() { observer.on_error(req_id, error_code_int, msg); }
          }
          // HistoricalData, Scanner, OptionCalc, HistogramData, HistoricalTicks are typically blocking or one-shot.
          // Their errors are handled by the blocking wait logic returning an Err.
          // However, if they were requested via request_observe_*, their FilteredObserver will handle on_error.
          // The MarketDataHandler trait methods (e.g., historical_data_end) also set error state.
          // If an error occurs for these types *before* their natural completion/error handling in the handler,
          // we should still notify their specific observers if they exist.
          MarketStream::HistoricalData(_) => {
            let observers = self.historical_data_observers.read();
            for observer in observers.values() { observer.on_error(req_id, error_code_int, msg); }
          }
          MarketStream::HistoricalTicks(_) => {
            let observers = self.historical_ticks_observers.read();
            for observer in observers.values() { observer.on_error(req_id, error_code_int, msg); }
          }
          // Scanner, OptionCalc, HistogramData do not have specific observers in this manager yet.
          _ => {}
        }
      }
    } else {
      // Might be an error for a request that already completed/cancelled/timed out.
      trace!("Received error for unknown or completed market data request ID: {}", req_id);
    }
  }

  // --- Optional: Add observer management ---
  // pub fn add_observer(&self, observer: Weak<dyn MarketDataObserver>) { ... }
  // pub fn remove_observer(&self, observer: Weak<dyn MarketDataObserver>) { ... }
  // fn notify_observers(&self, req_id: i32) { ... }

  // --- Scanner Methods ---

  fn parse_scan_parameters_xml(xml: &str) -> Result<ScanParameterResponse, IBKRError> {
    // from_str does not track line/col.
    quick_xml::de::from_str(xml).map_err(|e| IBKRError::ParseError(e.to_string()))
  }

  /// Requests the XML document containing valid scanner parameters. This is a blocking call.
  ///
  /// The TWS API provides scanner parameters as a single XML string in response to this request.
  /// The response doesn't include a request ID, so this method manages state globally.
  ///
  /// # Arguments
  /// * `timeout` - Maximum duration to wait for the XML response.
  ///
  /// # Returns
  /// A `String` containing the scanner parameters XML.
  ///
  /// # Errors
  /// Returns `IBKRError::Timeout` if the XML is not received within the timeout.
  /// Returns other `IBKRError` variants for communication or encoding issues.
  pub fn get_scanner_parameters(&self, timeout: Duration) -> Result<ScanParameterResponse, IBKRError> {
    info!("Requesting scanner parameters XML...");

    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_request_scanner_parameters()?;

    // Lock and check if parameters are already cached
    let mut params_guard = self.scanner_parameters_xml.lock();
    if let Some(xml) = params_guard.as_ref() {
      info!("Scanner parameters already available, returning cached XML.");
      return Self::parse_scan_parameters_xml(xml);
    }

    // Parameters not cached, send request and wait
    self.message_broker.send_message(&request_msg)?;
    debug!("Scanner parameters request sent. Waiting for response...");

    let start_time = std::time::Instant::now();
    loop {
      // Check if parameters arrived *before* waiting
      if let Some(xml) = params_guard.as_ref() {
        return Self::parse_scan_parameters_xml(xml);
      }

      // Calculate remaining timeout
      let elapsed = start_time.elapsed();
      if elapsed >= timeout {
        return Err(Timeout(format!("Scanner parameters request timed out after {:?}", timeout)));
      }
      let remaining_timeout = timeout - elapsed;

      // Wait for notification from the handler
      let wait_result = self.scanner_parameters_cond.wait_for(&mut params_guard, remaining_timeout);

      if wait_result.timed_out() {
        // Re-check after timeout just in case it arrived right at the end
        if let Some(xml) = params_guard.as_ref() {
          return Self::parse_scan_parameters_xml(xml);
        } else {
          return Err(Timeout(format!("Scanner parameters request timed out after wait", )));
        }
      }
      // If not timed out, loop continues (parameters should be Some now)
    }
  }


  /// Requests a market scanner subscription. This is a non-blocking call.
  ///
  /// Scan results are delivered via the `scanner_data` method of the `MarketDataHandler` trait,
  /// followed by `scanner_data_end` when the initial list is complete.
  ///
  /// # Arguments
  /// * `info` - A [`ScannerInfo`] struct defining the scan parameters.
  ///
  /// # Returns
  /// The request ID (`i32`) assigned to this scanner request.
  ///
  /// # Errors
  /// Returns `IBKRError` if the request cannot be encoded or sent.
  pub fn request_scanner_info(
    &self,
    info: &ScannerInfo,
  ) -> Result<i32, IBKRError> {
    info!("Requesting scanner info: ScanCode={}, Instrument={}, Location={}",
          info.scan_code, info.instrument, info.location_code);
    let req_id = self.message_broker.next_request_id();
    self.internal_request_scanner_info(req_id, info)?;
    Ok(req_id)
  }

  fn internal_request_scanner_info(&self, req_id: i32, info: &ScannerInfo) -> Result<(), IBKRError> {
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let request_msg = encoder.encode_request_scanner_info(req_id, info)?;

    {
      let mut subs = self.subscriptions.lock();
      if subs.contains_key(&req_id) { return Err(IBKRError::DuplicateRequestId(req_id)); }
      let state = ScannerInfoState {
        req_id,
        info: info.clone(),
        results: Vec::new(),
        completed: false,
        error_code: None,
        error_message: None,
      };
      subs.insert(req_id, MarketStream::Scanner(state));
      debug!("Scanner info added for ReqID: {}", req_id);
    }

    self.message_broker.send_message(&request_msg)
  }

  /// Requests market scanner results and blocks until the results are received,
  /// an error occurs, or the timeout is reached.
  ///
  /// # Arguments
  /// * `info` - A [`ScannerInfo`] struct defining the scan parameters.
  /// * `timeout` - Maximum duration to wait for the scan results.
  ///
  /// # Returns
  /// A `Vec<ScanData>` containing the results of the market scan.
  ///
  /// # Errors
  /// Returns `IBKRError::Timeout` if the results are not received within the timeout.
  /// Returns other `IBKRError` variants for communication or encoding issues.
  pub fn get_scanner_results(
    &self,
    info: &ScannerInfo,
    timeout: Duration,
  ) -> Result<Vec<ScanData>, IBKRError> {
    info!("Requesting blocking scanner results: ScanCode={}, Timeout={:?}",
          info.scan_code, timeout);

    // 1. Initiate the non-blocking request
    let req_id = self.request_scanner_info(info)?;
    debug!("Blocking scanner request initiated with ReqID: {}", req_id);

    // 2. Wait for completion (signaled by scanner_data_end)
    let result_state = self.wait_for_completion(
      req_id,
      timeout,
      |state: &ScannerInfoState| state.completed, // Completion check
      "Scanner",
    );

    // 3. Best effort cancel (scanner infos are often one-shot, but cancel anyway)
    if let Err(e) = self.cancel_scanner_info(req_id) {
      warn!("Failed to cancel scanner request {} after blocking wait: {:?}", req_id, e);
    }

    // 4. Extract results from the final state
    result_state.map(|state| state.results)
  }

  /// Cancels an active market scanner info.
  ///
  /// # Arguments
  /// * `req_id` - The request ID obtained from `request_scanner_info()` or `get_scanner_results()`.
  ///
  /// # Errors
  /// Returns `IBKRError` if the cancellation message cannot be encoded or sent.
  pub fn cancel_scanner_info(&self, req_id: i32) -> Result<(), IBKRError> {
    info!("Cancelling scanner info: ReqID={}", req_id);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_scanner_info(req_id)?;

    self.message_broker.send_message(&request_msg)?;

    {
      let mut subs = self.subscriptions.lock();
      if subs.remove(&req_id).is_some() {
        debug!("Removed scanner info state for ReqID: {}", req_id);
      } else {
        warn!("Attempted to cancel scanner for unknown or already removed ReqID: {}", req_id);
      }
    }
    Ok(())
  }

  // --- Option Calculation Methods ---

  /// Calculates the implied volatility for an option given its price and the underlying price.
  /// This is a blocking call.
  ///
  /// # Arguments
  /// * `contract` - The option [`Contract`].
  /// * `option_price` - The market price of the option.
  /// * `under_price` - The current price of the underlying asset.
  /// * `timeout` - Maximum duration to wait for the calculation.
  ///
  /// # Returns
  /// `Ok(TickOptionComputationData)` containing the calculated implied volatility and other greeks.
  /// `Err(IBKRError)` if the calculation fails or times out.
  pub fn calculate_implied_volatility(
    &self,
    contract: &Contract,
    option_price: f64,
    under_price: f64,
    timeout: Duration,
  ) -> Result<TickOptionComputationData, IBKRError> {
    info!("Calculating implied volatility: Contract={}, OptPrice={}, UndPrice={}, Timeout={:?}",
          contract.local_symbol.as_deref().unwrap_or(&contract.symbol), option_price, under_price, timeout);

    if self.message_broker.get_server_version()? < min_server_ver::CALC_IMPLIED_VOLAT {
      return Err(IBKRError::Unsupported("Server version does not support implied volatility calculation.".to_string()));
    }
    let req_id = self.next_request_id(); // Use the new helper method
    self.internal_calculate_implied_volatility(req_id, contract, option_price, under_price)?;

    self.wait_for_option_calc_completion(req_id, timeout, "ImpliedVolatility")
  }

  fn internal_calculate_implied_volatility(&self, req_id: i32, contract: &Contract, option_price: f64, under_price: f64) -> Result<(), IBKRError> {
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let request_msg = encoder.encode_request_calculate_implied_volatility(
      req_id, contract, option_price, under_price
    )?;

    // Initialize and store state
    {
      let mut subs = self.subscriptions.lock();
      if subs.contains_key(&req_id) {
        return Err(IBKRError::DuplicateRequestId(req_id));
      }
      let state = OptionCalculationState::new(req_id, contract.clone());
      subs.insert(req_id, MarketStream::OptionCalc(state));
      debug!("Implied volatility calculation request added for ReqID: {}", req_id);
    }

    self.message_broker.send_message(&request_msg)
  }

  /// Cancels an ongoing implied volatility calculation request.
  pub fn cancel_calculate_implied_volatility(&self, req_id: i32) -> Result<(), IBKRError> {
    info!("Cancelling implied volatility calculation: ReqID={}", req_id);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_calculate_implied_volatility(req_id)?;

    self.message_broker.send_message(&request_msg)?;

    {
      let mut subs = self.subscriptions.lock();
      if subs.remove(&req_id).is_some() {
        debug!("Removed implied volatility calculation state for ReqID: {}", req_id);
      } else {
        warn!("Attempted to cancel implied volatility for unknown or already removed ReqID: {}", req_id);
      }
    }
    Ok(())
  }

  /// Calculates the option price and greeks given its volatility and the underlying price.
  /// This is a blocking call.
  ///
  /// # Arguments
  /// * `contract` - The option [`Contract`].
  /// * `volatility` - The volatility of the option (e.g., 0.20 for 20%).
  /// * `under_price` - The current price of the underlying asset.
  /// * `timeout` - Maximum duration to wait for the calculation.
  ///
  /// # Returns
  /// `Ok(TickOptionComputationData)` containing the calculated option price and other greeks.
  /// `Err(IBKRError)` if the calculation fails or times out.
  pub fn calculate_option_price(
    &self,
    contract: &Contract,
    volatility: f64,
    under_price: f64,
    timeout: Duration,
  ) -> Result<TickOptionComputationData, IBKRError> {
    info!("Calculating option price: Contract={}, Volatility={}, UndPrice={}, Timeout={:?}",
          contract.local_symbol.as_deref().unwrap_or(&contract.symbol), volatility, under_price, timeout);

    if self.message_broker.get_server_version()? < min_server_ver::CALC_OPTION_PRICE {
      return Err(IBKRError::Unsupported("Server version does not support option price calculation.".to_string()));
    }
    let req_id = self.next_request_id(); // Use the new helper method
    self.internal_calculate_option_price(req_id, contract, volatility, under_price)?;

    self.wait_for_option_calc_completion(req_id, timeout, "OptionPrice")
  }

  fn internal_calculate_option_price(&self, req_id: i32, contract: &Contract, volatility: f64, under_price: f64) -> Result<(), IBKRError> {
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let request_msg = encoder.encode_request_calculate_option_price(
      req_id, contract, volatility, under_price
    )?;

    // Initialize and store state
    {
      let mut subs = self.subscriptions.lock();
      if subs.contains_key(&req_id) {
        return Err(IBKRError::DuplicateRequestId(req_id));
      }
      let state = OptionCalculationState::new(req_id, contract.clone());
      subs.insert(req_id, MarketStream::OptionCalc(state));
      debug!("Option price calculation request added for ReqID: {}", req_id);
    }

    self.message_broker.send_message(&request_msg)
  }

  /// Cancels an ongoing option price calculation request.
  pub fn cancel_calculate_option_price(&self, req_id: i32) -> Result<(), IBKRError> {
    info!("Cancelling option price calculation: ReqID={}", req_id);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_calculate_option_price(req_id)?;

    self.message_broker.send_message(&request_msg)?;

    {
      let mut subs = self.subscriptions.lock();
      if subs.remove(&req_id).is_some() {
        debug!("Removed option price calculation state for ReqID: {}", req_id);
      } else {
        warn!("Attempted to cancel option price for unknown or already removed ReqID: {}", req_id);
      }
    }
    Ok(())
  }

  // --- Histogram Data Methods ---

  /// Requests histogram data for a contract. This is a non-blocking call.
  ///
  /// Histogram data provides price distribution over a specified time period.
  /// Data is delivered via the `histogram_data` method of the `MarketDataHandler` trait.
  ///
  /// # Arguments
  /// * `contract` - The [`Contract`] for which to request histogram data.
  /// * `use_rth` - If `true`, include only data from regular trading hours.
  /// * `time_period` - A [`TimePeriodUnit`] enum specifying the time period (e.g., `TimePeriodUnit::Day(3)`).
  /// * `_histogram_options` - Currently unused for `reqHistogramData`. Reserved for future use or alternative methods.
  ///
  /// # Returns
  /// The request ID (`i32`) assigned to this histogram data request.
  ///
  /// # Errors
  /// Returns `IBKRError` if the request cannot be encoded or sent, or if the server version is too low.
  pub fn request_histogram_data(
    &self,
    contract: &Contract,
    use_rth: bool,
    time_period: TimePeriodUnit,
    _histogram_options: &[(String, String)], // Currently unused for REQ_HISTOGRAM_DATA
  ) -> Result<i32, IBKRError> {
    let time_period_str = time_period.to_string(); // Convert enum to string for logging and encoding
    info!("Requesting histogram data: Contract={}, UseRTH={}, TimePeriod='{}'",
          contract.symbol, use_rth, time_period_str); // Log the string representation
    if self.message_broker.get_server_version()? < min_server_ver::HISTOGRAM {
      return Err(IBKRError::Unsupported("Server version does not support histogram data requests.".to_string()));
    }
    let req_id = self.next_request_id(); // Use the new helper method
    self.internal_request_histogram_data(req_id, contract, use_rth, &time_period_str, time_period)?;
    Ok(req_id)
  }

  fn internal_request_histogram_data(
    &self, req_id: i32, contract: &Contract, use_rth: bool, time_period_str: &str, time_period_enum: TimePeriodUnit
  ) -> Result<(), IBKRError> {
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let request_msg = encoder.encode_request_histogram_data(
      req_id, contract, use_rth, time_period_str
    )?;

    // Initialize and store state
    if !self.historical_limiter.try_acquire() {
      return Err(IBKRError::RateLimited("Historical rate limit exceeded".to_string()));
    }
    {
      let mut subs = self.subscriptions.lock();
      if subs.contains_key(&req_id) {
        return Err(IBKRError::DuplicateRequestId(req_id));
      }
      let state = HistogramDataRequestState {
        req_id,
        contract: contract.clone(),
        use_rth,
        time_period: time_period_enum,
        items: Vec::new(),
        completed: false,
        error_code: None,
        error_message: None,
      };
      subs.insert(req_id, MarketStream::HistogramData(state));
      debug!("Histogram data request added for ReqID: {}", req_id);
    }

    match self.message_broker.send_message(&request_msg) {
      Ok(()) => { self.historical_limiter.register_request(req_id); Ok(()) }
      Err(e) => Err(e)
    }
  }

  /// Requests histogram data for a contract and blocks until the data is received,
  /// an error occurs, or the timeout is reached.
  ///
  /// # Arguments
  /// * `contract`, `use_rth`, `time_period` (as `TimePeriodUnit`), `_histogram_options`: Same as for `request_histogram_data`.
  /// * `timeout` - Maximum duration to wait for the histogram data.
  ///
  /// # Returns
  /// A `Vec<HistogramEntry>` containing the histogram data points.
  ///
  /// # Errors
  /// Returns `IBKRError` if the request fails, times out, or other issues occur.
  pub fn get_histogram_data(
    &self,
    contract: &Contract,
    use_rth: bool,
    time_period: TimePeriodUnit,
    _histogram_options: &[(String, String)], // Currently unused for REQ_HISTOGRAM_DATA
    timeout: Duration,
  ) -> Result<Vec<HistogramEntry>, IBKRError> {
    info!("Requesting blocking histogram data: Contract={}, UseRTH={}, TimePeriod='{}', Timeout={:?}",
          contract.symbol, use_rth, time_period, timeout); // Log the enum (uses Display)

    // 1. Initiate the non-blocking request
    let req_id = self.request_histogram_data(
      contract, use_rth, time_period, _histogram_options // Pass the enum
    )?;
    debug!("Blocking histogram data request initiated with ReqID: {}", req_id);

    // 2. Wait for completion (signaled by histogram_data handler setting completed=true)
    let result_state = self.wait_for_completion(
      req_id,
      timeout,
      |s: &HistogramDataRequestState| s.completed, // Completion check
      "HistogramData",
    );

    // 3. Best effort cancel
    if let Err(e) = self.cancel_histogram_data(req_id) {
      warn!("Failed to cancel histogram data request {} after blocking wait: {:?}", req_id, e);
    }

    // 4. Extract results from the final state
    result_state.map(|state| state.items)
  }

  /// Cancels an ongoing histogram data request.
  pub fn cancel_histogram_data(&self, req_id: i32) -> Result<(), IBKRError> {
    info!("Cancelling histogram data request: ReqID={}", req_id);
    if self.message_broker.get_server_version()? < min_server_ver::CANCEL_HISTOGRAM_DATA {
      return Err(IBKRError::Unsupported("Server version does not support histogram data cancellation.".to_string()));
    }
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_histogram_data(req_id)?;

    self.message_broker.send_message(&request_msg)?;
    self.historical_limiter.release(req_id);

    {
      let mut subs = self.subscriptions.lock();
      if subs.remove(&req_id).is_some() {
        debug!("Removed histogram data subscription state for ReqID: {}", req_id);
      } else {
        warn!("Attempted to cancel histogram data for unknown or already removed ReqID: {}", req_id);
      }
    }
    Ok(())
  }

  // --- Historical Ticks Methods ---

  /// Requests historical tick data for a contract. This is a non-blocking call.
  ///
  /// Data is delivered via the `historical_ticks`, `historical_ticks_bid_ask`,
  /// or `historical_ticks_last` methods of the `MarketDataHandler` trait.
  ///
  /// # Arguments
  /// * `contract` - The [`Contract`] for which to request data.
  /// * `start_date_time` - Optional start time for the ticks. Format: "yyyyMMdd HH:mm:ss (zzz)".
  /// * `end_date_time` - Optional end time for the ticks. If `None`, `start_date_time` must be `None` too, and `number_of_ticks` is used.
  /// * `number_of_ticks` - Number of ticks to return (max 1000). Use 0 to get all ticks in the date range.
  /// * `what_to_show` - A [`WhatToShow`] enum specifying the type of ticks: Trades, Midpoint, or BidAsk.
  /// * `use_rth` - If `true`, include only ticks from regular trading hours.
  /// * `ignore_size` - For "BID_ASK" ticks, if `true`, sizes are not returned.
  /// * `misc_options` - A list of `(tag, value)` pairs for additional options (e.g., "tradesLastSale=1").
  ///
  /// # Returns
  /// The request ID (`i32`) assigned to this historical ticks request.
  ///
  /// # Errors
  /// Returns `IBKRError` if the request cannot be encoded, sent, or if server version is too low.
  pub fn request_historical_ticks(
    &self,
    contract: &Contract,
    start_date_time: Option<chrono::DateTime<chrono::Utc>>,
    end_date_time: Option<chrono::DateTime<chrono::Utc>>,
    number_of_ticks: i32,
    what_to_show: WhatToShow,
    use_rth: bool,
    ignore_size: bool,
    misc_options: &[(String, String)],
  ) -> Result<i32, IBKRError> {
    info!("Requesting historical ticks: Contract={}, What={}, NumTicks={}, Start={:?}, End={:?}",
          contract.symbol, what_to_show, number_of_ticks, start_date_time, end_date_time); // what_to_show will use Display
    if self.message_broker.get_server_version()? < min_server_ver::HISTORICAL_TICKS {
      return Err(IBKRError::Unsupported("Server version does not support historical ticks requests.".to_string()));
    }
    let req_id = self.next_request_id(); // Use the new helper method
    self.internal_request_historical_ticks(req_id, contract, start_date_time, end_date_time, number_of_ticks, what_to_show, use_rth, ignore_size, misc_options)?;
    Ok(req_id)
  }

  fn internal_request_historical_ticks(
    &self, req_id: i32, contract: &Contract, start_date_time: Option<chrono::DateTime<chrono::Utc>>,
    end_date_time: Option<chrono::DateTime<chrono::Utc>>, number_of_ticks: i32, what_to_show: WhatToShow,
    use_rth: bool, ignore_size: bool, misc_options: &[(String, String)]
  ) -> Result<(), IBKRError> {
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let request_msg = encoder.encode_request_historical_ticks(
      req_id, contract, start_date_time, end_date_time, number_of_ticks,
      &what_to_show.to_string(), use_rth, ignore_size, misc_options
    )?;

    if !self.historical_limiter.try_acquire() {
      return Err(IBKRError::RateLimited("Historical rate limit exceeded".to_string()));
    }
    // Initialize and store state
    {
      let mut subs = self.subscriptions.lock();
      if subs.contains_key(&req_id) {
        return Err(IBKRError::DuplicateRequestId(req_id));
      }
      let state = HistoricalTicksRequestState::new(
        req_id, contract.clone(), start_date_time, end_date_time, number_of_ticks,
        what_to_show, use_rth, ignore_size, misc_options.to_vec() // Store enum
      );
      subs.insert(req_id, MarketStream::HistoricalTicks(state));
      debug!("Historical ticks request added for ReqID: {}", req_id);
    }

    match self.message_broker.send_message(&request_msg) {
      Ok(()) => { self.historical_limiter.register_request(req_id); Ok(()) }
      Err(e) => Err(e)
    }
  }

  /// Requests historical tick data and blocks until the data is received,
  /// an error occurs, or the timeout is reached.
  ///
  /// # Arguments
  /// * (Same as `request_historical_ticks`)
  /// * `timeout` - Maximum duration to wait for the data.
  ///
  /// # Returns
  /// A `Vec<HistoricalTick>` containing the historical tick data.
  ///
  /// # Errors
  /// Returns `IBKRError` if the request fails, times out, or other issues occur.
  pub fn get_historical_ticks(
    &self,
    contract: &Contract,
    start_date_time: Option<chrono::DateTime<chrono::Utc>>,
    end_date_time: Option<chrono::DateTime<chrono::Utc>>,
    number_of_ticks: i32,
    what_to_show: WhatToShow,
    use_rth: bool,
    ignore_size: bool,
    misc_options: &[(String, String)],
    timeout: Duration,
  ) -> Result<Vec<HistoricalTick>, IBKRError> {
    info!("Requesting blocking historical ticks: Contract={}, What={}, NumTicks={}, Timeout={:?}",
          contract.symbol, what_to_show, number_of_ticks, timeout); // what_to_show will use Display

    // 1. Initiate the non-blocking request
    let req_id = self.request_historical_ticks(
      contract, start_date_time, end_date_time, number_of_ticks,
      what_to_show, use_rth, ignore_size, misc_options // Pass enum
    )?;
    debug!("Blocking historical ticks request initiated with ReqID: {}", req_id);

    // 2. Wait for completion (signaled by handler setting completed=true)
    let result_state = self.wait_for_completion(
      req_id,
      timeout,
      |s: &HistoricalTicksRequestState| s.completed,
      "HistoricalTicks",
    );

    // 3. Best effort cancel (uses cancel_historical_data message)
    // Note: cancel_historical_data will attempt to remove the state.
    // If wait_for_completion already removed it due to error/success, this is fine.
    if let Err(e) = self.cancel_historical_ticks(req_id) {
      warn!("Failed to cancel historical ticks request {} after blocking wait: {:?}", req_id, e);
    }

    // 4. Extract results
    result_state.map(|state| state.ticks)
  }

  /// Cancels an ongoing historical ticks request.
  /// This uses the same underlying TWS message as `cancel_historical_data`.
  pub fn cancel_historical_ticks(&self, req_id: i32) -> Result<(), IBKRError> {
    info!("Cancelling historical ticks request (via cancel_historical_data): ReqID={}", req_id);
    // Historical ticks are cancelled using the CancelHistoricalData message (ID 25)
    self.cancel_historical_data(req_id) // This will also handle state removal
  }

  // --- Subscription Factory Methods ---

  /// Creates a builder for a market data tick subscription.
  pub fn subscribe_market_data(&self, contract: &Contract) -> TickDataSubscriptionBuilder {
    TickDataSubscriptionBuilder::new(self.self_weak.clone(), contract.clone())
  }

  /// Creates a builder for a real-time bars subscription.
  pub fn subscribe_real_time_bars(&self, contract: &Contract, what_to_show: WhatToShow) -> RealTimeBarSubscriptionBuilder {
    RealTimeBarSubscriptionBuilder::new(self.self_weak.clone(), contract.clone(), what_to_show)
  }

  /// Creates a builder for a tick-by-tick data subscription.
  pub fn subscribe_tick_by_tick(&self, contract: &Contract, tick_type: TickByTickRequestType) -> TickByTickSubscriptionBuilder {
    TickByTickSubscriptionBuilder::new(self.self_weak.clone(), contract.clone(), tick_type)
  }

  /// Creates a builder for a market depth subscription.
  pub fn subscribe_market_depth(&self, contract: &Contract, num_rows: i32) -> MarketDepthSubscriptionBuilder {
    MarketDepthSubscriptionBuilder::new(self.self_weak.clone(), contract.clone(), num_rows)
  }

  /// Creates a builder for a historical data subscription.
  pub fn subscribe_historical_data(
    &self,
    contract: &Contract,
    duration: DurationUnit,
    bar_size: BarSize,
    what_to_show: WhatToShow,
  ) -> HistoricalDataSubscriptionBuilder {
    HistoricalDataSubscriptionBuilder::new(self.self_weak.clone(), contract.clone(), duration, bar_size, what_to_show)
  }

  /// Creates a builder for combining multiple subscriptions into a single event stream.
  /// The generic type `T` must be the common event type produced by the iterators being combined.
  pub fn combine_subscriptions<T: Clone + Send + Sync + 'static>(&self) -> MultiSubscriptionBuilder<T, DataMarketManager> {
    MultiSubscriptionBuilder::new()
  }


  // --- Observer Registration Methods ---
  fn new_observer_id(&self) -> ObserverId {
    ObserverId(self.next_observer_id.fetch_add(1, Ordering::SeqCst))
  }

  /// Register an observer for market data tick updates.
  pub fn observe_market_data<T: MarketDataObserver + Send + Sync + 'static>(&self, observer: T) -> ObserverId {
    let observer_id = self.new_observer_id();
    let mut observers = self.market_data_observers.write();
    observers.insert(observer_id, Box::new(observer));
    debug!("Added market data observer with ID: {:?}", observer_id);
    observer_id
  }

  /// Unregister a market data observer.
  pub fn remove_market_data_observer(&self, observer_id: ObserverId) -> bool {
    let mut observers = self.market_data_observers.write();
    let removed = observers.remove(&observer_id).is_some();
    if removed {
      debug!("Removed market data observer with ID: {:?}", observer_id);
    } else {
      warn!("Attempted to remove non-existent market data observer ID: {:?}", observer_id);
    }
    removed
  }

  /// Register an observer for real-time bar updates.
  pub fn observe_realtime_bars<T: RealTimeBarsObserver + Send + Sync + 'static>(&self, observer: T) -> ObserverId {
    let observer_id = self.new_observer_id();
    let mut observers = self.realtime_bars_observers.write();
    observers.insert(observer_id, Box::new(observer));
    debug!("Added real-time bars observer with ID: {:?}", observer_id);
    observer_id
  }

  /// Unregister a real-time bars observer.
  pub fn remove_realtime_bars_observer(&self, observer_id: ObserverId) -> bool {
    let mut observers = self.realtime_bars_observers.write();
    let removed = observers.remove(&observer_id).is_some();
    if removed {
      debug!("Removed real-time bars observer with ID: {:?}", observer_id);
    } else {
      warn!("Attempted to remove non-existent real-time bars observer ID: {:?}", observer_id);
    }
    removed
  }

  /// Register an observer for tick-by-tick data updates.
  pub fn observe_tick_by_tick<T: TickByTickObserver + Send + Sync + 'static>(&self, observer: T) -> ObserverId {
    let observer_id = self.new_observer_id();
    let mut observers = self.tick_by_tick_observers.write();
    observers.insert(observer_id, Box::new(observer));
    debug!("Added tick-by-tick observer with ID: {:?}", observer_id);
    observer_id
  }

  /// Unregister a tick-by-tick data observer.
  pub fn remove_tick_by_tick_observer(&self, observer_id: ObserverId) -> bool {
    let mut observers = self.tick_by_tick_observers.write();
    let removed = observers.remove(&observer_id).is_some();
    if removed {
      debug!("Removed tick-by-tick observer with ID: {:?}", observer_id);
    } else {
      warn!("Attempted to remove non-existent tick-by-tick observer ID: {:?}", observer_id);
    }
    removed
  }

  /// Register an observer for market depth updates.
  pub fn observe_market_depth<T: MarketDepthObserver + Send + Sync + 'static>(&self, observer: T) -> ObserverId {
    let observer_id = self.new_observer_id();
    let mut observers = self.market_depth_observers.write();
    observers.insert(observer_id, Box::new(observer));
    debug!("Added market depth observer with ID: {:?}", observer_id);
    observer_id
  }

  /// Unregister a market depth observer.
  pub fn remove_market_depth_observer(&self, observer_id: ObserverId) -> bool {
    let mut observers = self.market_depth_observers.write();
    let removed = observers.remove(&observer_id).is_some();
    if removed {
      debug!("Removed market depth observer with ID: {:?}", observer_id);
    } else {
      warn!("Attempted to remove non-existent market depth observer ID: {:?}", observer_id);
    }
    removed
  }

  /// Register an observer for historical data updates.
  pub fn observe_historical_data<T: HistoricalDataObserver + Send + Sync + 'static>(&self, observer: T) -> ObserverId {
    let observer_id = self.new_observer_id();
    let mut observers = self.historical_data_observers.write();
    observers.insert(observer_id, Box::new(observer));
    debug!("Added historical data observer with ID: {:?}", observer_id);
    observer_id
  }

  /// Unregister a historical data observer.
  pub fn remove_historical_data_observer(&self, observer_id: ObserverId) -> bool {
    let mut observers = self.historical_data_observers.write();
    let removed = observers.remove(&observer_id).is_some();
    if removed {
      debug!("Removed historical data observer with ID: {:?}", observer_id);
    } else {
      warn!("Attempted to remove non-existent historical data observer ID: {:?}", observer_id);
    }
    removed
  }

  /// Register an observer for historical ticks updates.
  pub fn observe_historical_ticks<T: HistoricalTicksObserver + Send + Sync + 'static>(&self, observer: T) -> ObserverId {
    let observer_id = self.new_observer_id();
    let mut observers = self.historical_ticks_observers.write();
    observers.insert(observer_id, Box::new(observer));
    debug!("Added historical ticks observer with ID: {:?}", observer_id);
    observer_id
  }

  /// Unregister a historical ticks observer.
  pub fn remove_historical_ticks_observer(&self, observer_id: ObserverId) -> bool {
    let mut observers = self.historical_ticks_observers.write();
    let removed = observers.remove(&observer_id).is_some();
    if removed {
      debug!("Removed historical ticks observer with ID: {:?}", observer_id);
    } else {
      warn!("Attempted to remove non-existent historical ticks observer ID: {:?}", observer_id);
    }
    removed
  }

  // --- Request-Observe Methods ---
  /// Request market data and route events from that request to the observer.
  pub fn request_observe_market_data<T: MarketDataObserver + Send + Sync + 'static>(
    &self,
    contract: &Contract,
    generic_tick_list: &[GenericTickType],
    snapshot: bool,
    regulatory_snapshot: bool,
    mkt_data_options: &[(String, String)],
    market_data_type: Option<MarketDataType>,
    observer: T,
  ) -> Result<(i32, ObserverId), IBKRError> {
    struct FilteredObserver<Obs: MarketDataObserver + Send + Sync> { inner: Obs, req_id: i32, }
    impl<Obs: MarketDataObserver + Send + Sync> MarketDataObserver for FilteredObserver<Obs> {
      fn on_tick_price(&self, req_id: i32, tick_type: TickType, price: f64, attrib: TickAttrib) { if req_id == self.req_id { self.inner.on_tick_price(req_id, tick_type, price, attrib); } }
      fn on_tick_size(&self, req_id: i32, tick_type: TickType, size: f64) { if req_id == self.req_id { self.inner.on_tick_size(req_id, tick_type, size); } }
      fn on_tick_string(&self, req_id: i32, tick_type: TickType, value: &str) { if req_id == self.req_id { self.inner.on_tick_string(req_id, tick_type, value); } }
      fn on_tick_generic(&self, req_id: i32, tick_type: TickType, value: f64) { if req_id == self.req_id { self.inner.on_tick_generic(req_id, tick_type, value); } }
      fn on_tick_snapshot_end(&self, req_id: i32) { if req_id == self.req_id { self.inner.on_tick_snapshot_end(req_id); } }
      fn on_market_data_type(&self, req_id: i32, market_data_type: MarketDataType) { if req_id == self.req_id { self.inner.on_market_data_type(req_id, market_data_type); } }
      fn on_error(&self, req_id: i32, error_code: i32, error_message: &str) {
        if req_id == self.req_id { self.inner.on_error(req_id, error_code, error_message); }
      }
    }

    let desired_mkt_data_type = market_data_type.unwrap_or(MarketDataType::RealTime);
    self.set_market_data_type_if_needed(desired_mkt_data_type)?;
    let req_id = self.next_request_id(); // Use the new helper method
    let observer_id = self.observe_market_data(FilteredObserver { inner: observer, req_id });

    self.internal_request_market_data(
      req_id, contract, generic_tick_list, snapshot, regulatory_snapshot,
      mkt_data_options, desired_mkt_data_type
    )?;
    Ok((req_id, observer_id))
  }

  /// Request real-time bars and route events from that request to the observer.
  pub fn request_observe_realtime_bars<T: RealTimeBarsObserver + Send + Sync + 'static>(
    &self,
    contract: &Contract,
    what_to_show: WhatToShow,
    use_rth: bool,
    real_time_bars_options: &[(String, String)],
    observer: T,
  ) -> Result<(i32, ObserverId), IBKRError> {
    struct FilteredObserver<Obs: RealTimeBarsObserver + Send + Sync> { inner: Obs, req_id: i32, }
    impl<Obs: RealTimeBarsObserver + Send + Sync> RealTimeBarsObserver for FilteredObserver<Obs> {
      fn on_bar_update(&self, req_id: i32, bar: &Bar) { if req_id == self.req_id { self.inner.on_bar_update(req_id, bar); } }
      fn on_error(&self, req_id: i32, error_code: i32, error_message: &str) {
        if req_id == self.req_id { self.inner.on_error(req_id, error_code, error_message); }
      }
    }

    let req_id = self.next_request_id(); // Use the new helper method
    let observer_id = self.observe_realtime_bars(FilteredObserver { inner: observer, req_id });
    let bar_size = 5; // Hardcoded as per API limitation for this request type

    self.internal_request_real_time_bars(
      req_id, contract, what_to_show, use_rth, real_time_bars_options, bar_size
    )?;
    Ok((req_id, observer_id))
  }

  /// Request tick-by-tick data and route events from that request to the observer.
  pub fn request_observe_tick_by_tick<T: TickByTickObserver + Send + Sync + 'static>(
    &self,
    contract: &Contract,
    tick_type: TickByTickRequestType,
    number_of_ticks: i32,
    ignore_size: bool,
    observer: T,
  ) -> Result<(i32, ObserverId), IBKRError> {
    struct FilteredObserver<Obs: TickByTickObserver + Send + Sync> { inner: Obs, req_id: i32, }
    impl<Obs: TickByTickObserver + Send + Sync> TickByTickObserver for FilteredObserver<Obs> {
      fn on_tick_by_tick_all_last(&self, req_id: i32, tick_type_val: i32, time: i64, price: f64, size: f64, tick_attrib_last: &TickAttribLast, exchange: &str, special_conditions: &str) {
        if req_id == self.req_id { self.inner.on_tick_by_tick_all_last(req_id, tick_type_val, time, price, size, tick_attrib_last, exchange, special_conditions); }
      }
      fn on_tick_by_tick_bid_ask(&self, req_id: i32, time: i64, bid_price: f64, ask_price: f64, bid_size: f64, ask_size: f64, tick_attrib_bid_ask: &TickAttribBidAsk) {
        if req_id == self.req_id { self.inner.on_tick_by_tick_bid_ask(req_id, time, bid_price, ask_price, bid_size, ask_size, tick_attrib_bid_ask); }
      }
      fn on_tick_by_tick_mid_point(&self, req_id: i32, time: i64, mid_point: f64) {
        if req_id == self.req_id { self.inner.on_tick_by_tick_mid_point(req_id, time, mid_point); }
      }
      fn on_error(&self, req_id: i32, error_code: i32, error_message: &str) {
        if req_id == self.req_id { self.inner.on_error(req_id, error_code, error_message); }
      }
    }

    let req_id = self.next_request_id(); // Use the new helper method
    let observer_id = self.observe_tick_by_tick(FilteredObserver { inner: observer, req_id });

    self.internal_request_tick_by_tick_data(
      req_id, contract, tick_type, number_of_ticks, ignore_size
    )?;
    Ok((req_id, observer_id))
  }

  /// Request market depth data and route events from that request to the observer.
  pub fn request_observe_market_depth<T: MarketDepthObserver + Send + Sync + 'static>(
    &self,
    contract: &Contract,
    num_rows: i32,
    is_smart_depth: bool,
    mkt_depth_options: &[(String, String)],
    observer: T,
  ) -> Result<(i32, ObserverId), IBKRError> {
    struct FilteredObserver<Obs: MarketDepthObserver + Send + Sync> { inner: Obs, req_id: i32, }
    impl<Obs: MarketDepthObserver + Send + Sync> MarketDepthObserver for FilteredObserver<Obs> {
      fn on_update_mkt_depth(&self, req_id: i32, position: i32, operation: i32, side: i32, price: f64, size: f64) {
        if req_id == self.req_id { self.inner.on_update_mkt_depth(req_id, position, operation, side, price, size); }
      }
      fn on_update_mkt_depth_l2(&self, req_id: i32, position: i32, market_maker: &str, operation: i32, side: i32, price: f64, size: f64, is_smart_depth_val: bool) {
        if req_id == self.req_id { self.inner.on_update_mkt_depth_l2(req_id, position, market_maker, operation, side, price, size, is_smart_depth_val); }
      }
      fn on_error(&self, req_id: i32, error_code: i32, error_message: &str) {
        if req_id == self.req_id { self.inner.on_error(req_id, error_code, error_message); }
      }
    }

    let req_id = self.next_request_id(); // Use the new helper method
    let observer_id = self.observe_market_depth(FilteredObserver { inner: observer, req_id });

    self.internal_request_market_depth(
      req_id, contract, num_rows, is_smart_depth, mkt_depth_options
    )?;
    Ok((req_id, observer_id))
  }

  /// Request historical data and route events from that request to the observer.
  pub fn request_observe_historical_data<T: HistoricalDataObserver + Send + Sync + 'static>(
    &self,
    contract: &Contract,
    end_date_time: Option<chrono::DateTime<chrono::Utc>>,
    duration: DurationUnit,
    bar_size_setting: crate::contract::BarSize,
    what_to_show: WhatToShow,
    use_rth: bool,
    format_date: i32,
    keep_up_to_date: bool,
    market_data_type: Option<MarketDataType>,
    chart_options: &[(String, String)],
    observer: T,
  ) -> Result<(i32, ObserverId), IBKRError> {
    struct FilteredObserver<Obs: HistoricalDataObserver + Send + Sync> { inner: Obs, req_id: i32, }
    impl<Obs: HistoricalDataObserver + Send + Sync> HistoricalDataObserver for FilteredObserver<Obs> {
      fn on_historical_data(&self, req_id: i32, bar: &Bar) { if req_id == self.req_id { self.inner.on_historical_data(req_id, bar); } }
      fn on_historical_data_update(&self, req_id: i32, bar: &Bar) { if req_id == self.req_id { self.inner.on_historical_data_update(req_id, bar); } }
      fn on_historical_data_end(&self, req_id: i32, start_date: &str, end_date: &str) { if req_id == self.req_id { self.inner.on_historical_data_end(req_id, start_date, end_date); } }
      fn on_error(&self, req_id: i32, error_code: i32, error_message: &str) {
        if req_id == self.req_id { self.inner.on_error(req_id, error_code, error_message); }
      }
    }

    let desired_mkt_data_type = market_data_type.unwrap_or(MarketDataType::RealTime);
    self.set_market_data_type_if_needed(desired_mkt_data_type)?;
    let req_id = self.next_request_id(); // Use the new helper method
    let observer_id = self.observe_historical_data(FilteredObserver { inner: observer, req_id });
    let duration_str = duration.to_string();
    let bar_size_str = bar_size_setting.to_string();
    let what_to_show_str = what_to_show.to_string();

    self.internal_request_historical_data(
      req_id, contract, end_date_time, &duration_str, &bar_size_str,
      &what_to_show_str, use_rth, format_date, keep_up_to_date, chart_options, desired_mkt_data_type
    )?;
    Ok((req_id, observer_id))
  }

  /// Request historical ticks and route events from that request to the observer.
  pub fn request_observe_historical_ticks<T: HistoricalTicksObserver + Send + Sync + 'static>(
    &self,
    contract: &Contract,
    start_date_time: Option<chrono::DateTime<chrono::Utc>>,
    end_date_time: Option<chrono::DateTime<chrono::Utc>>,
    number_of_ticks: i32,
    what_to_show: WhatToShow,
    use_rth: bool,
    ignore_size: bool,
    misc_options: &[(String, String)],
    observer: T,
  ) -> Result<(i32, ObserverId), IBKRError> {
    struct FilteredObserver<Obs: HistoricalTicksObserver + Send + Sync> { inner: Obs, req_id: i32, }
    impl<Obs: HistoricalTicksObserver + Send + Sync> HistoricalTicksObserver for FilteredObserver<Obs> {
      fn on_historical_ticks_midpoint(&self, req_id: i32, ticks: &[(i64, f64, f64)], done: bool) {
        if req_id == self.req_id { self.inner.on_historical_ticks_midpoint(req_id, ticks, done); }
      }
      fn on_historical_ticks_bid_ask(&self, req_id: i32, ticks: &[(i64, TickAttribBidAsk, f64, f64, f64, f64)], done: bool) {
        if req_id == self.req_id { self.inner.on_historical_ticks_bid_ask(req_id, ticks, done); }
      }
      fn on_historical_ticks_last(&self, req_id: i32, ticks: &[(i64, TickAttribLast, f64, f64, String, String)], done: bool) {
        if req_id == self.req_id { self.inner.on_historical_ticks_last(req_id, ticks, done); }
      }
      fn on_error(&self, req_id: i32, error_code: i32, error_message: &str) {
        if req_id == self.req_id { self.inner.on_error(req_id, error_code, error_message); }
      }
    }

    if self.message_broker.get_server_version()? < min_server_ver::HISTORICAL_TICKS {
      return Err(IBKRError::Unsupported("Server version does not support historical ticks requests.".to_string()));
    }
    let req_id = self.next_request_id(); // Use the new helper method
    let observer_id = self.observe_historical_ticks(FilteredObserver { inner: observer, req_id });

    self.internal_request_historical_ticks(
      req_id, contract, start_date_time, end_date_time, number_of_ticks,
      what_to_show, use_rth, ignore_size, misc_options
    )?;
    Ok((req_id, observer_id))
  }

  // Helper for option calculation completion
  fn wait_for_option_calc_completion(
    &self, req_id: i32, timeout: Duration, request_type_name: &str
  ) -> Result<TickOptionComputationData, IBKRError> {
    let result_state = self.wait_for_completion(
      req_id,
      timeout,
      |s: &OptionCalculationState| s.result.is_some(),
      request_type_name,
    );
    // Best effort cancel
    let cancel_fn = match request_type_name {
      "ImpliedVolatility" => Self::cancel_calculate_implied_volatility,
      "OptionPrice" => Self::cancel_calculate_option_price,
      _ => return Err(IBKRError::InternalError(format!("Unknown option calc type: {}", request_type_name))),
    };
    if let Err(e) = cancel_fn(self, req_id) {
      warn!("Failed to cancel {} request {} after blocking wait: {:?}", request_type_name, req_id, e);
    }
    result_state.and_then(|state| {
      state.result.ok_or_else(|| IBKRError::InternalError(format!("{} result missing for ReqID {}", request_type_name, req_id)))
    })
  }
}

// --- Implement MarketDataHandler Trait for DataMarketManager ---
impl MarketDataHandler for DataMarketManager {

  // --- Tick Data ---
  fn tick_price(&self, req_id: i32, tick_type: TickType, price: f64, attrib: TickAttrib) {
    trace!("Handler: Tick Price: ID={}, Type={:?}, Price={}, Attrib={:?}", req_id, tick_type, price, attrib);
    debug!("Data Market: Tick Price: ID={}, Type={:?}, Price={}, Attrib={:?}", req_id, tick_type, price, attrib);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::TickData(state)) = subs.get_mut(&req_id) {
      // Update MarketDataInfo state
      // Map tick_type enum to fields in MarketDataSubscriptionState
      match tick_type {
        TickType::BidPrice => state.bid_price = Some(price),
        TickType::AskPrice => state.ask_price = Some(price),
        TickType::LastPrice => state.last_price = Some(price),
        TickType::High => state.high_price = Some(price),
        TickType::Low => state.low_price = Some(price),
        TickType::ClosePrice => state.close_price = Some(price),
        TickType::OpenTick => state.open_price = Some(price),
        TickType::DelayedBid => state.bid_price = Some(price), // Update main field for delayed
        TickType::DelayedAsk => state.ask_price = Some(price), // Update main field for delayed
        TickType::DelayedLast => state.last_price = Some(price), // Update main field for delayed
        TickType::DelayedHighPrice => state.high_price = Some(price), // Update main field for delayed
        TickType::DelayedLowPrice => state.low_price = Some(price), // Update main field for delayed
        TickType::DelayedClose => state.close_price = Some(price), // Update main field for delayed
        TickType::DelayedOpen => state.open_price = Some(price), // Update main field for delayed
        TickType::MarkPrice => { /* Maybe store separately? */ },
        TickType::BidYield | TickType::AskYield | TickType::LastYield => { /* Store yield separately? */ },
        TickType::EtfNavClose | TickType::EtfNavPriorClose | TickType::EtfNavBid | TickType::EtfNavAsk | TickType::EtfNavLast | TickType::EtfNavFrozenLast | TickType::EtfNavHigh | TickType::EtfNavLow => { /* Store NAV separately? */ },
        TickType::AuctionPrice => { /* Store auction price separately? */ },
        TickType::CreditmanMarkPrice | TickType::CreditmanSlowMarkPrice => { /* Store creditman price separately? */ },
        _ => trace!("Unhandled price tick_type {:?} in tick_price for ReqID {}", tick_type, req_id),
      }
      // TODO: Store attrib if needed? Maybe only store latest?
      state.ticks.entry(tick_type).or_default().push((price, attrib.clone())); // Store tick history using enum key

      // If this is for a blocking quote request, check if we have all needed data
      if state.is_blocking_quote_request && !state.quote_received {
        if state.bid_price.is_some() && state.ask_price.is_some() && state.last_price.is_some() {
          debug!("All required ticks received for blocking quote ReqID {}. Notifying waiter.", req_id);
          // We don't set quote_received here, let snapshot_end do that,
          // but we notify in case snapshot_end is delayed or missed.
          self.request_cond.notify_all();
        }
      } else {
        // Notify for general blocking requests if not a quote request
        self.request_cond.notify_all();
      }

      // Notify observers
      let observers = self.market_data_observers.read();
      for observer in observers.values() {
        observer.on_tick_price(req_id, tick_type, price, attrib.clone());
      }
    } else {
      // warn!("Received tick_price for unknown or non-tick subscription ID: {}", req_id);
    }
  }

  fn tick_size(&self, req_id: i32, tick_type: TickType, size: f64) {
    trace!("Handler: Tick Size: ID={}, Type={:?}, Size={}", req_id, tick_type, size);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::TickData(state)) = subs.get_mut(&req_id) {
      // Update MarketDataInfo state
      match tick_type {
        TickType::BidSize => state.bid_size = Some(size),
        TickType::AskSize => state.ask_size = Some(size),
        TickType::LastSize => state.last_size = Some(size),
        TickType::Volume => state.volume = Some(size),
        TickType::OptionCallOpenInterest => state.call_open_interest = Some(size),
        TickType::OptionPutOpenInterest => state.put_open_interest = Some(size),
        TickType::OptionCallVolume => state.call_volume = Some(size),
        TickType::OptionPutVolume => state.put_volume = Some(size),
        TickType::AverageVolume => state.avg_volume = Some(size), // Assuming ID 21 maps here
        TickType::ShortableShares => state.shortable_shares = Some(size), // Assuming ID 89 maps here
        TickType::FuturesOpenInterest => state.futures_open_interest = Some(size), // Assuming ID 86 maps here
        TickType::DelayedBidSize => state.bid_size = Some(size), // Update main field
        TickType::DelayedAskSize => state.ask_size = Some(size), // Update main field
        TickType::DelayedLastSize => state.last_size = Some(size), // Update main field
        TickType::DelayedVolume => state.volume = Some(size), // Update main field
        TickType::ShortTermVolume3Minutes => state.short_term_volume_3_min = Some(size),
        TickType::ShortTermVolume5Minutes => state.short_term_volume_5_min = Some(size),
        TickType::ShortTermVolume10Minutes => state.short_term_volume_10_min = Some(size),
        TickType::AuctionVolume | TickType::AuctionImbalance | TickType::RegulatoryImbalance => { /* Store separately? */ },
        TickType::AverageOptionVolume => { /* Store separately? */ },
        _ => trace!("Unhandled size tick_type {:?} in tick_size", tick_type),
      }
      state.sizes.entry(tick_type).or_default().push(size); // Store size history using enum key
      self.request_cond.notify_all(); // Notify waiters

      // Notify observers
      let observers = self.market_data_observers.read();
      for observer in observers.values() {
        observer.on_tick_size(req_id, tick_type, size);
      }
    } else {
      // warn!("Received tick_size for unknown or non-tick subscription ID: {}", req_id);
    }
  }

  fn tick_string(&self, req_id: i32, tick_type: TickType, value: &str) {
    trace!("Handler: Tick String: ID={}, Type={:?}, Value='{}'", req_id, tick_type, value);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::TickData(state)) = subs.get_mut(&req_id) {
      // Update MarketDataInfo state
      match tick_type {
        TickType::LastTimestamp | TickType::DelayedLastTimestamp => {
          if let Ok(ts) = value.parse::<i64>() {
            state.last_timestamp = Some(ts);
          } else {
            warn!("Failed to parse timestamp string '{}' for {:?} ReqID {}", value, tick_type, req_id);
          }
        },
        TickType::LastRegulatoryTime => state.last_reg_time = Some(value.to_string()),
        TickType::RtVolume => { /* Parse RTVolume string: price;size;time;totalVolume;vwap;singleTrade */ },
        TickType::RtTradeVolume => { /* Parse RTTradeVolume string */ },
        TickType::IbDividends => { /* Parse dividend string: past12m,next12m,nextDate,nextAmt */ },
        TickType::News => { /* Parse news string: time;provider;articleId;headline;extraData */
                               // Example parsing (adjust based on actual format)
                               let parts: Vec<&str> = value.split(';').collect();
                               if parts.len() >= 4 {
                                 if let Ok(ts) = parts[0].parse::<i64>() {
                                   state.latest_news_time = Some(ts);
                                   // Could store full TickNewsData if needed
                                 }
                               }
        },
        TickType::BidExchange | TickType::AskExchange | TickType::LastExchange => state.last_exchange = Some(value.to_string()),
        _ => trace!("Unhandled string tick_type {:?} in tick_string", tick_type),
      }

      // Notify observers
      let observers = self.market_data_observers.read();
      for observer in observers.values() {
        observer.on_tick_string(req_id, tick_type, value);
      }
    } else {
      // warn!("Received tick_string for unknown or non-tick subscription ID: {}", req_id);
    }
  }

  fn tick_generic(&self, req_id: i32, tick_type: TickType, value: f64) {
    trace!("Handler: Tick Generic: ID={}, Type={:?}, Value={}", req_id, tick_type, value);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::TickData(state)) = subs.get_mut(&req_id) {
      // Update MarketDataInfo state
      match tick_type {
        TickType::OptionHistoricalVolatility | TickType::RtHistoricalVolatility => { /* Store vol separately? */ },
        TickType::OptionImpliedVolatility => { /* Store vol separately? */ },
        TickType::IndexFuturePremium => { /* Store premium separately? */ },
        TickType::Shortable => { /* Store shortable status separately? */ },
        TickType::Halted => state.halted = Some(value != 0.0), // 0=Not Halted, 1/2=Halted, -1=Unknown
        TickType::TradeCount => state.trade_count = Some(value as i64),
        TickType::TradeRate => state.trade_rate = Some(value),
        TickType::VolumeRate => state.volume_rate = Some(value),
        TickType::BondFactorMultiplier => { /* Store multiplier separately? */ },
        TickType::EstimatedIpoMidpoint | TickType::FinalIpoPrice => { /* Store IPO price separately? */ },
        _ => trace!("Unhandled generic tick_type {:?} in tick_generic", tick_type),
      }

      // Notify observers
      let observers = self.market_data_observers.read();
      for observer in observers.values() {
        observer.on_tick_generic(req_id, tick_type, value);
      }
    } else {
      // warn!("Received tick_generic for unknown or non-tick subscription ID: {}", req_id);
    }
  }

  fn tick_efp(&self, req_id: i32, tick_type: TickType, basis_points: f64, _formatted_basis_points: &str,
              _implied_futures_price: f64, _hold_days: i32, _future_last_trade_date: &str,
              _dividend_impact: f64, _dividends_to_last_trade_date: f64) {
    trace!("Handler: Tick EFP: ID={}, Type={:?}, BasisPts={}", req_id, tick_type, basis_points);
    // EFP data doesn't typically fit into the standard MarketDataSubscription fields.
    // An observer pattern or dedicated callback might be better here.
    // For now, just log it. Store if needed.
  }

  fn tick_option_computation(&self, req_id: i32, data: TickOptionComputationData) {
    trace!("Handler: Tick Option Computation: ID={}, Type={:?}", req_id, data.tick_type);
    let mut subs = self.subscriptions.lock();
    match subs.get_mut(&req_id) {
      Some(MarketStream::TickData(state)) => {
        // Store the whole computation data struct for regular market data stream
        state.option_computation = Some(data.clone());
        // Notify general waiters if this tick was part of a broader request
        self.request_cond.notify_all();

        // Notify MarketDataObservers if they are interested in option computations via generic ticks
        // This part is a bit ambiguous in the design. Assuming generic tick observers handle this.
        // If a specific OptionComputationObserver was defined, it would be notified here.
        // For now, let's assume it's covered by on_tick_generic or similar if mapped to a TickType.
        // The `data` itself contains a `tick_type` field.
      }
      Some(MarketStream::OptionCalc(state)) => {
        // This is a dedicated option calculation request
        state.result = Some(data);
        state.completed = true; // Mark as completed since we got the result
        debug!("Option calculation result received for ReqID {}. Notifying waiter.", req_id);
        self.request_cond.notify_all(); // Notify the specific waiter for this calculation
      }
      _ => {
        warn!("Received tick_option_computation for unknown or mismatched subscription ID: {}", req_id);
      }
    }
  }

  fn tick_snapshot_end(&self, req_id: i32) {
    debug!("Handler: Tick Snapshot End: ID={}", req_id);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::TickData(state)) = subs.get_mut(&req_id) {
      state.snapshot_end_received = true;
      // If this was a blocking quote snapshot request, mark as complete and notify waiter.
      if state.is_blocking_quote_request {
        state.quote_received = true;
        debug!("Snapshot end received for blocking quote ReqID {}. Notifying waiter.", req_id);
        self.request_cond.notify_all();
      }

      // Notify observers
      let observers = self.market_data_observers.read();
      for observer in observers.values() {
        observer.on_tick_snapshot_end(req_id);
      }
      self.market_data_limiter.release(req_id);
    } else {
      // warn!("Received tick_snapshot_end for unknown or non-tick subscription ID: {}", req_id);
    }
  }

  fn market_data_type(&self, _req_id: i32, market_data_type: MarketDataType) {
    // Note: The req_id in this message corresponds to the *original* market data request ID,
    // not the ReqMarketDataType message itself. We update the global state and notify observers.
    // The _req_id parameter from the handler trait is the one associated with the original data request.
    let req_id_for_observers = _req_id; // Use the passed req_id for observer notification
    debug!("Handler: Market Data Type Received: Type={:?}", market_data_type);

    // Update the manager's current market data type state
    {
      let mut current_type_guard = self.current_market_data_type.lock();
      if *current_type_guard != market_data_type {
        info!("Updating connection market data type from {:?} to {:?}", *current_type_guard, market_data_type);
        *current_type_guard = market_data_type;
        // Notify any threads waiting for this specific type change
        self.market_data_type_cond.notify_all();
      } else {
        trace!("Received market data type confirmation for current type: {:?}", market_data_type);
      }
    }

    // We don't need to update individual subscription states here, as they
    // already store the *requested* type. The global state reflects the *active* type.

    // Notify observers associated with the original request ID
    let observers = self.market_data_observers.read();
    for observer in observers.values() {
      observer.on_market_data_type(req_id_for_observers, market_data_type);
    }
  }

  fn tick_req_params(&self, req_id: i32, min_tick: f64, bbo_exchange: &str, snapshot_permissions: i32) {
    debug!("Handler: Tick Req Params: ID={}, MinTick={}, BBOExch={}, Permissions={}", req_id, min_tick, bbo_exchange, snapshot_permissions);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::TickData(state)) = subs.get_mut(&req_id) {
      // Store snapshot permissions if this was a snapshot request
      if state.snapshot {
        state.snapshot_permissions = Some(snapshot_permissions);
      }
      // min_tick and bbo_exchange might be useful context but aren't typically stored as live data.
    } else {
      // warn!("Received tick_req_params for unknown or non-tick subscription ID: {}", req_id);
    }
  }

  // --- Real Time Bars ---
  fn real_time_bar(&self, req_id: i32, time: i64, open: f64, high: f64, low: f64, close: f64,
                   volume: f64, wap: f64, count: i32) {
    trace!("Handler: Real Time Bar: ID={}, Time={}, O={}, H={}, L={}, C={}", req_id, time, open, high, low, close);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::RealTimeBars(state)) = subs.get_mut(&req_id) {
      let bar = Bar {
        time: Utc.timestamp_opt(time, 0).single().unwrap_or_else(|| {
          warn!("Failed to parse timestamp {} for real time bar ReqID {}. Using Utc::now().", time, req_id);
          Utc::now()
        }),
        open, high, low, close,
        volume: volume as i64, // Convert f64 back to i64
        wap,
        count,
      };
      state.latest_bar = Some(bar.clone()); // Store latest separately
      state.bars.push(bar); // Add to the collected list

      // Check if this is a blocking request and if the target count is reached
      if let Some(target_count) = state.target_bar_count {
        if !state.completed && state.bars.len() >= target_count {
          debug!("Target bar count ({}) reached for blocking RealTimeBars ReqID {}. Notifying waiter.", target_count, req_id);
          state.completed = true;
          self.request_cond.notify_all();
        }
      }

      // Notify observers
      let observers = self.realtime_bars_observers.read();
      if let Some(latest_bar) = &state.latest_bar { // Ensure bar is present
        for observer in observers.values() {
          observer.on_bar_update(req_id, latest_bar);
        }
      }
    } else {
      // warn!("Received real_time_bar for unknown or non-RTBar subscription ID: {}", req_id);
    }
  }

  // --- Historical Data ---
  fn historical_data(&self, req_id: i32, bar: &Bar) {
    trace!("Handler: Historical Data Bar: ID={}, Time={}", req_id, bar.time);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::HistoricalData(state)) = subs.get_mut(&req_id) {
      state.bars.push(bar.clone());
      // Don't notify yet, wait for end.

      // Notify observers for each bar
      let observers = self.historical_data_observers.read();
      for observer in observers.values() {
        observer.on_historical_data(req_id, bar);
      }
    } else {
      // warn!("Received historical_data for unknown or non-historical subscription ID: {}", req_id);
    }
  }

  fn historical_data_update(&self, req_id: i32, bar: &Bar) {
    debug!("Handler: Historical Data Update Bar: ID={}, Time={}", req_id, bar.time);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::HistoricalData(state)) = subs.get_mut(&req_id) {
      // Handle update - e.g., replace last bar or append if time is newer
      if let Some(last_bar) = state.bars.last_mut() {
        if bar.time == last_bar.time {
          *last_bar = bar.clone();
        } else if bar.time > last_bar.time {
          state.bars.push(bar.clone());
        } else {
          warn!("Received out-of-order historical update for ReqID {}: UpdateTime={}, LastBarTime={}", req_id, bar.time, last_bar.time);
        }
      } else {
        state.bars.push(bar.clone()); // First update
      }
      state.update_received = true;

      // Notify observers about update
      let observers = self.historical_data_observers.read();
      for observer in observers.values() {
        observer.on_historical_data_update(req_id, bar);
      }
    } else {
      // warn!("Received historical_data_update for unknown or non-historical subscription ID: {}", req_id);
    }
  }

  fn historical_data_end(&self, req_id: i32, start_date: &str, end_date: &str) {
    debug!("Handler: Historical Data End: ID={}, Start={}, End={}", req_id, start_date, end_date);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::HistoricalData(state)) = subs.get_mut(&req_id) {
      state.start_date = start_date.to_string();
      state.end_date = end_date.to_string();
      state.end_received = true;
      info!("Historical data end received for request {}. Notifying waiter.", req_id);

      // Notify observers
      let observers = self.historical_data_observers.read();
      for observer in observers.values() {
        observer.on_historical_data_end(req_id, start_date, end_date);
      }
      self.historical_limiter.release(req_id);
      self.request_cond.notify_all(); // Signal waiting thread
    } else {
      // warn!("Received historical_data_end for unknown or non-historical subscription ID: {}", req_id);
    }
  }

  // This is for REQ_HISTORICAL_TICKS with whatToShow = "MIDPOINT"
  fn historical_ticks(&self, req_id: i32, ticks_data: &[(i64, f64, f64)], done: bool) {
    debug!("Handler: Historical Ticks (MidPoint): ID={}, Count={}, Done={}", req_id, ticks_data.len(), done);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::HistoricalTicks(state)) = subs.get_mut(&req_id) {
      for &(time, price, size) in ticks_data {
        state.ticks.push(HistoricalTick::MidPoint { time, price, size });
      }
      if done {
        state.completed = true;
        info!("Historical Ticks (MidPoint) end received for request {}. Notifying waiter.", req_id);
        self.request_cond.notify_all();

        // Notify observers
        let observers = self.historical_ticks_observers.read();
        for observer in observers.values() {
          observer.on_historical_ticks_midpoint(req_id, ticks_data, done);
        }
      }
    } else {
      warn!("Received historical_ticks (MidPoint) for unknown or non-HistoricalTicks subscription ID: {}", req_id);
    }
  }

  // This is for REQ_HISTORICAL_TICKS with whatToShow = "BID_ASK"
  fn historical_ticks_bid_ask(&self, req_id: i32, ticks_data: &[(i64, TickAttribBidAsk, f64, f64, f64, f64)], done: bool) {
    debug!("Handler: Historical Ticks BidAsk: ID={}, Count={}, Done={}", req_id, ticks_data.len(), done);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::HistoricalTicks(state)) = subs.get_mut(&req_id) {
      for &(time, ref tick_attrib_bid_ask, price_bid, price_ask, size_bid, size_ask) in ticks_data {
        state.ticks.push(HistoricalTick::BidAsk {
          time,
          tick_attrib_bid_ask: tick_attrib_bid_ask.clone(),
          price_bid,
          price_ask,
          size_bid,
          size_ask,
        });
      }
      if done {
        state.completed = true;
        info!("Historical Ticks (BidAsk) end received for request {}. Notifying waiter.", req_id);
        self.request_cond.notify_all();

        // Notify observers
        let observers = self.historical_ticks_observers.read();
        for observer in observers.values() {
          observer.on_historical_ticks_bid_ask(req_id, ticks_data, done);
        }
      }
    } else {
      warn!("Received historical_ticks_bid_ask for unknown or non-HistoricalTicks subscription ID: {}", req_id);
    }
  }

  // This is for REQ_HISTORICAL_TICKS with whatToShow = "TRADES"
  fn historical_ticks_last(&self, req_id: i32, ticks_data: &[(i64, TickAttribLast, f64, f64, String, String)], done: bool) {
    debug!("Handler: Historical Ticks Last: ID={}, Count={}, Done={}", req_id, ticks_data.len(), done);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::HistoricalTicks(state)) = subs.get_mut(&req_id) {
      for &(time, ref tick_attrib_last, price, size, ref exchange, ref special_conditions) in ticks_data {
        state.ticks.push(HistoricalTick::Trade {
          time,
          price,
          size,
          tick_attrib_last: tick_attrib_last.clone(),
          exchange: exchange.clone(),
          special_conditions: special_conditions.clone(),
        });
      }
      if done {
        state.completed = true;
        info!("Historical Ticks (Last) end received for request {}. Notifying waiter.", req_id);
        self.request_cond.notify_all();

        // Notify observers
        let observers = self.historical_ticks_observers.read();
        for observer in observers.values() {
          observer.on_historical_ticks_last(req_id, ticks_data, done);
        }
      }
    } else {
      warn!("Received historical_ticks_last for unknown or non-HistoricalTicks subscription ID: {}", req_id);
    }
  }

  // --- Tick By Tick ---
  fn tick_by_tick_all_last(&self, req_id: i32, tick_type: i32, time: i64, price: f64, size: f64,
                           tick_attrib_last: TickAttribLast, exchange: &str, special_conditions: &str) {
    trace!("Handler: TickByTick AllLast: ID={}, Time={}, Px={}, Sz={}", req_id, time, price, size);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::TickByTick(state)) = subs.get_mut(&req_id) {
      let data = if tick_type == 1 {
        TickByTickData::Last { time, price, size, tick_attrib_last: tick_attrib_last.clone(), exchange: exchange.to_string(), special_conditions: special_conditions.to_string() }
      } else {
        TickByTickData::AllLast { time, price, size, tick_attrib_last: tick_attrib_last.clone(), exchange: exchange.to_string(), special_conditions: special_conditions.to_string() }
      };
      state.ticks.push(data.clone()); // Store history
      state.latest_tick = Some(data);
      self.request_cond.notify_all(); // Notify waiters

      // Notify observers
      let observers = self.tick_by_tick_observers.read();
      for observer in observers.values() {
        observer.on_tick_by_tick_all_last(req_id, tick_type, time, price, size,
                                          &tick_attrib_last, exchange, special_conditions);
      }
    } else {
      // warn!("Received tick_by_tick_all_last for unknown or non-TBT subscription ID: {}", req_id);
    }
  }

  fn tick_by_tick_bid_ask(&self, req_id: i32, time: i64, bid_price: f64, ask_price: f64, bid_size: f64,
                          ask_size: f64, tick_attrib_bid_ask: TickAttribBidAsk) {
    trace!("Handler: TickByTick BidAsk: ID={}, Time={}, BidPx={}, AskPx={}", req_id, time, bid_price, ask_price);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::TickByTick(state)) = subs.get_mut(&req_id) {
      let data = TickByTickData::BidAsk { time, bid_price, ask_price, bid_size, ask_size, tick_attrib_bid_ask: tick_attrib_bid_ask.clone() };
      state.ticks.push(data.clone()); // Store history
      state.latest_tick = Some(data);
      self.request_cond.notify_all(); // Notify waiters

      // Notify observers
      let observers = self.tick_by_tick_observers.read();
      for observer in observers.values() {
        observer.on_tick_by_tick_bid_ask(req_id, time, bid_price, ask_price, bid_size,
                                         ask_size, &tick_attrib_bid_ask);
      }
    } else {
      // warn!("Received tick_by_tick_bid_ask for unknown or non-TBT subscription ID: {}", req_id);
    }
  }

  fn tick_by_tick_mid_point(&self, req_id: i32, time: i64, mid_point: f64) {
    trace!("Handler: TickByTick MidPoint: ID={}, Time={}, MidPt={}", req_id, time, mid_point);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::TickByTick(state)) = subs.get_mut(&req_id) {
      let data = TickByTickData::MidPoint { time, mid_point };
      state.ticks.push(data.clone()); // Store history
      state.latest_tick = Some(data);
      self.request_cond.notify_all(); // Notify waiters

      // Notify observers
      let observers = self.tick_by_tick_observers.read();
      for observer in observers.values() {
        observer.on_tick_by_tick_mid_point(req_id, time, mid_point);
      }
    } else {
      // warn!("Received tick_by_tick_mid_point for unknown or non-TBT subscription ID: {}", req_id);
    }
  }

  // --- Market Depth ---
  fn update_mkt_depth(&self, req_id: i32, position: i32, operation: i32, side: i32, price: f64, size: f64) {
    trace!("Handler: MktDepth L1 Update: ID={}, Pos={}, Op={}, Side={}, Px={}, Sz={}", req_id, position, operation, side, price, size);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::MarketDepth(state)) = subs.get_mut(&req_id) {
      // Update L1 fields directly (simple approach, assumes position 0)
      if position == 0 {
        match side {
          1 => { // Bid
            state.bid_price = Some(price);
            state.bid_size = Some(size);
          },
          0 => { // Ask
            state.ask_price = Some(price);
            state.ask_size = Some(size);
          },
          _ => {}
        }
      }
      // Update L2 depth book (more complex logic needed here for insert/update/delete)
      let depth_list = if side == 1 { &mut state.depth_bids } else { &mut state.depth_asks };
      let row = MarketDepthRow { position, operation, side, price, size, market_maker: String::new(), is_smart_depth: None };

      match operation {
        0 => { // Insert
          if let Some(idx) = depth_list.iter().position(|r| r.position >= position) {
            depth_list.insert(idx, row);
            // Shift positions of subsequent rows? TWS usually sends updates or deletes for shifts.
          } else {
            depth_list.push(row);
          }
        },
        1 => { // Update
          if let Some(existing_row) = depth_list.iter_mut().find(|r| r.position == position) {
            existing_row.price = price;
            existing_row.size = size;
            // Keep other fields
          } else {
            warn!("Market Depth Update for non-existent position {} on ReqID {}", position, req_id);
            // Optionally insert if update is for missing row?
            depth_list.push(row); // Insert as fallback
            depth_list.sort_by_key(|r| r.position); // Keep sorted
          }
        },
        2 => { // Delete
          depth_list.retain(|r| r.position != position);
        },
        _ => warn!("Unknown market depth operation: {}", operation),
      }
      // Trim list if it exceeds num_rows? TWS usually manages this.
      self.request_cond.notify_all(); // Notify waiters

      // Notify observers
      let observers = self.market_depth_observers.read();
      for observer in observers.values() {
        observer.on_update_mkt_depth(req_id, position, operation, side, price, size);
      }
    } else {
      // warn!("Received update_mkt_depth for unknown or non-Depth subscription ID: {}", req_id);
    }
  }

  fn update_mkt_depth_l2(&self, req_id: i32, position: i32, market_maker: &str, operation: i32,
                         side: i32, price: f64, size: f64, is_smart_depth: bool) {
    trace!("Handler: MktDepth L2 Update: ID={}, Pos={}, MM={}, Op={}, Side={}, Px={}, Sz={}, Smart={}",
           req_id, position, market_maker, operation, side, price, size, is_smart_depth);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::MarketDepth(state)) = subs.get_mut(&req_id) {
      // Update L2 depth book
      let depth_list = if side == 1 { &mut state.depth_bids } else { &mut state.depth_asks };
      let row = MarketDepthRow { position, operation, side, price, size, market_maker: market_maker.to_string(), is_smart_depth: Some(is_smart_depth) };

      match operation {
        0 => { // Insert
          // Find insert position, handle duplicates? TWS usually sends unique rows
          if let Some(idx) = depth_list.iter().position(|r| r.position >= position) {
            depth_list.insert(idx, row);
          } else {
            depth_list.push(row);
          }
        },
        1 => { // Update
          if let Some(existing_row) = depth_list.iter_mut().find(|r| r.position == position && r.market_maker == market_maker) {
            existing_row.price = price;
            existing_row.size = size;
            existing_row.is_smart_depth = Some(is_smart_depth);
          } else {
            warn!("Market Depth L2 Update for non-existent pos/mm {}/{} on ReqID {}", position, market_maker, req_id);
            depth_list.push(row); // Insert as fallback
            depth_list.sort_by_key(|r| r.position); // Keep sorted
          }
        },
        2 => { // Delete
          depth_list.retain(|r| !(r.position == position && r.market_maker == market_maker));
        },
        _ => warn!("Unknown market depth L2 operation: {}", operation),
      }
      self.request_cond.notify_all(); // Notify waiters

      // Notify observers
      let observers = self.market_depth_observers.read();
      for observer in observers.values() {
        observer.on_update_mkt_depth_l2(req_id, position, market_maker, operation,
                                        side, price, size, is_smart_depth);
      }
    } else {
      // warn!("Received update_mkt_depth_l2 for unknown or non-Depth subscription ID: {}", req_id);
    }
  }

  // --- Other Market Data ---
  fn delta_neutral_validation(&self, req_id: i32, /* con_id: i32, delta: f64, price: f64 */) {
    debug!("Handler: Delta Neutral Validation: ID={}", req_id);
    // Usually just logged, doesn't update typical subscription state.
  }

  fn histogram_data(&self, req_id: i32, items: &[(f64, f64)]) {
    debug!("Handler: Histogram Data Received: ID={}, ItemCount={}", req_id, items.len());
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::HistogramData(state)) = subs.get_mut(&req_id) {
      state.items = items.iter().map(|&(price, size)| HistogramEntry { price, size }).collect();
      state.completed = true; // Mark as completed once data is received
      info!("Histogram data received for request {}. Notifying waiter.", req_id);
      self.request_cond.notify_all(); // Signal waiting thread (for get_histogram_data)
      self.historical_limiter.release(req_id);
    } else {
      warn!("Received histogram_data for unknown or non-histogram subscription ID: {}", req_id);
    }
  }

  fn scanner_parameters(&self, xml: &str) {
    info!("Handler: Scanner Parameters XML received ({} bytes)", xml.len());
    // Store the received XML and notify any waiting threads (get_scanner_parameters)
    let mut params_guard = self.scanner_parameters_xml.lock();
    *params_guard = Some(xml.to_string());
    self.scanner_parameters_cond.notify_all();
    debug!("Scanner parameters stored and condition notified.");
  }

  fn scanner_data(&self, req_id: i32, rank: i32, contract_details: &ContractDetails, distance: &str,
                  benchmark: &str, projection: &str, legs_str: Option<&str>) {
    trace!("Handler: Scanner Data Row: ID={}, Rank={}, Symbol={}", req_id, rank, contract_details.contract.symbol);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::Scanner(state)) = subs.get_mut(&req_id) {
      let scan_data = ScanData {
        rank,
        contract_details: contract_details.clone(),
        distance: distance.to_string(),
        benchmark: benchmark.to_string(),
        projection: projection.to_string(),
        legs_str: legs_str.unwrap_or("").to_string(),
      };
      state.results.push(scan_data);
      // Don't notify here, wait for scanner_data_end
    } else {
      // warn!("Received scanner_data for unknown or non-scanner subscription ID: {}", req_id);
    }
  }

  fn scanner_data_end(&self, req_id: i32) {
    debug!("Handler: Scanner Data End: ID={}", req_id);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketStream::Scanner(state)) = subs.get_mut(&req_id) {
      state.completed = true;
      info!("Scanner data end received for request {}. Notifying waiter.", req_id);
      self.request_cond.notify_all(); // Signal waiting thread (for get_scanner_results)
    } else {
      // warn!("Received scanner_data_end for unknown or non-scanner subscription ID: {}", req_id);
    }
  }

  /// Handles errors related to market data requests.
  /// This is the implementation of the trait method.
  fn handle_error(&self, req_id: i32, code: ClientErrorCode, msg: &str) {
    // Delegate the actual state update to the manager's internal helper method
    self._internal_handle_error(req_id, code, msg);
  }

  // --- Rerouting ---
  fn reroute_mkt_data_req(&self, req_id: i32, con_id: i32, exchange: &str) {
    info!("Handler: Reroute Mkt Data Req: ID={}, ConID={}, Exch={}", req_id, con_id, exchange);
    // Notification, usually just logged.
  }

  fn reroute_mkt_depth_req(&self, req_id: i32, con_id: i32, exchange: &str) {
    info!("Handler: Reroute Mkt Depth Req: ID={}, ConID={}, Exch={}", req_id, con_id, exchange);
    // Notification, usually just logged.
  }
}
