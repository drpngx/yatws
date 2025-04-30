// yatws/src/data_market_manager.rs
use crate::base::IBKRError;
use crate::conn::MessageBroker;
use crate::contract::{Bar, Contract, ContractDetails};
use crate::data::{
  MarketDataSubscription, RealTimeBarSubscription, TickByTickSubscription, MarketDepthSubscription,
  HistoricalDataRequestState, TickAttrib, TickAttribLast, TickAttribBidAsk, TickOptionComputationData,
  MarketDataTypeEnum, MarketDepthRow, TickByTickData,
};
use crate::handler::MarketDataHandler;
use crate::protocol_encoder::Encoder;
use crate::protocol_decoder::ClientErrorCode; // Added import
use crate::base::IBKRError::Timeout; // Import Timeout variant directly
use parking_lot::{Condvar, Mutex};
use chrono::{Utc, TimeZone};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use log::{debug, info, trace, warn}; // Removed unused 'error'


// --- Helper Trait for Generic Waiting ---
// Allows the generic wait_for_completion function to access common state fields/methods.
trait CompletableState: Clone + Send + 'static {
  fn is_completed(&self) -> bool;
  fn mark_completed(&mut self);
  fn get_error(&self) -> Option<IBKRError>;
}

// Implement the trait for each subscription type that supports blocking waits
impl CompletableState for MarketDataSubscription {
  fn is_completed(&self) -> bool { self.completed || self.quote_received /* Also consider quote flag */ }
  fn mark_completed(&mut self) { self.completed = true; }
  fn get_error(&self) -> Option<IBKRError> {
    match (self.error_code, self.error_message.as_ref()) {
      (Some(code), Some(msg)) => Some(IBKRError::ApiError(code, msg.clone())),
      _ => None,
    }
  }
}

impl CompletableState for RealTimeBarSubscription {
  fn is_completed(&self) -> bool { self.completed }
  fn mark_completed(&mut self) { self.completed = true; }
  fn get_error(&self) -> Option<IBKRError> {
    match (self.error_code, self.error_message.as_ref()) {
      (Some(code), Some(msg)) => Some(IBKRError::ApiError(code, msg.clone())),
      _ => None,
    }
  }
}

impl CompletableState for TickByTickSubscription {
  fn is_completed(&self) -> bool { self.completed }
  fn mark_completed(&mut self) { self.completed = true; }
  fn get_error(&self) -> Option<IBKRError> {
    match (self.error_code, self.error_message.as_ref()) {
      (Some(code), Some(msg)) => Some(IBKRError::ApiError(code, msg.clone())),
      _ => None,
    }
  }
}

impl CompletableState for MarketDepthSubscription {
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


// --- Helper Trait for Downcasting MarketSubscription Enum ---
// Private helper trait to implement the downcasting logic
trait TryIntoStateHelper<T> {
  fn try_into_state_helper_mut(&mut self) -> Option<&mut T>;
}

// Implement the helper trait for each target state type
impl TryIntoStateHelper<MarketDataSubscription> for MarketSubscription {
  fn try_into_state_helper_mut(&mut self) -> Option<&mut MarketDataSubscription> {
    match self { MarketSubscription::TickData(s) => Some(s), _ => None }
  }
}
impl TryIntoStateHelper<RealTimeBarSubscription> for MarketSubscription {
  fn try_into_state_helper_mut(&mut self) -> Option<&mut RealTimeBarSubscription> {
    match self { MarketSubscription::RealTimeBars(s) => Some(s), _ => None }
  }
}
impl TryIntoStateHelper<TickByTickSubscription> for MarketSubscription {
  fn try_into_state_helper_mut(&mut self) -> Option<&mut TickByTickSubscription> {
    match self { MarketSubscription::TickByTick(s) => Some(s), _ => None }
  }
}
impl TryIntoStateHelper<MarketDepthSubscription> for MarketSubscription {
  fn try_into_state_helper_mut(&mut self) -> Option<&mut MarketDepthSubscription> {
    match self { MarketSubscription::MarketDepth(s) => Some(s), _ => None }
  }
}
// Add others if needed


// Helper methods on the enum to simplify access in the generic wait function
impl MarketSubscription {
  // Tries to get a mutable reference to the specific state type S
  fn try_get_mut<S>(&mut self) -> Option<&mut S>
  where
    MarketSubscription: TryIntoStateHelper<S>, // Use helper trait
  {
    self.try_into_state_helper_mut()
  }
}


#[derive(Debug)]
enum MarketSubscription {
  TickData(MarketDataSubscription),
  RealTimeBars(RealTimeBarSubscription),
  TickByTick(TickByTickSubscription),
  MarketDepth(MarketDepthSubscription),
  HistoricalData(HistoricalDataRequestState),
  // Add Scanner, Histogram etc. if needed
}

pub struct DataMarketManager {
  message_broker: Arc<MessageBroker>,
  // State for active subscriptions
  subscriptions: Mutex<HashMap<i32, MarketSubscription>>,
  // Condvar primarily for historical data requests that block
  request_cond: Condvar,
  // Optional: Observer pattern for streaming data
  // observers: RwLock<Vec<Weak<dyn MarketDataObserver>>>,
}

// Define the return type for get_quote
pub type Quote = (Option<f64>, Option<f64>, Option<f64>); // (Bid, Ask, Last)

impl DataMarketManager {
  pub fn new(message_broker: Arc<MessageBroker>) -> Arc<Self> {
    Arc::new(DataMarketManager {
      message_broker,
      subscriptions: Mutex::new(HashMap::new()),
      request_cond: Condvar::new(),
      // observers: RwLock::new(Vec::new()),
    })
  }

  // --- Helper to wait for completion (mainly for historical data) ---
  fn wait_for_historical_completion(
    &self,
    req_id: i32,
    timeout: Duration,
  ) -> Result<Vec<Bar>, IBKRError> {
    let start_time = std::time::Instant::now();
    let mut guard = self.subscriptions.lock();

    loop {
      // 1. Check if complete *before* waiting
      let maybe_result = if let Some(MarketSubscription::HistoricalData(state)) = guard.get(&req_id) {
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
        let final_check = if let Some(MarketSubscription::HistoricalData(state)) = guard.get(&req_id) {
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
      let maybe_result = if let Some(MarketSubscription::RealTimeBars(state)) = guard.get_mut(&req_id) {
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
        let final_check = if let Some(MarketSubscription::RealTimeBars(state)) = guard.get(&req_id) {
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
  // This helper abstracts the common waiting logic for blocking requests.
  // It takes the request ID, timeout, and a closure to check the completion condition.
  fn wait_for_completion<S, F>(
    &self,
    req_id: i32,
    timeout: Duration,
    mut completion_check: F,
    request_type_name: &str, // For logging purposes (e.g., "MarketData", "TickByTick")
  ) -> Result<S, IBKRError>
  where
    S: Clone + Send + 'static + CompletableState, // Added CompletableState bound
    F: FnMut(&S) -> bool, // Closure takes state ref, returns true if complete
  // Ensure the closure can access the correct state type from the enum
    MarketSubscription: TryIntoStateHelper<S>, // Use the helper trait here
  {
    let start_time = std::time::Instant::now();
    let mut guard = self.subscriptions.lock();

    loop {
      // 1. Check state *before* waiting
      let maybe_result = if let Some(sub_ref) = guard.get_mut(&req_id) {
        // Attempt to get the specific state type (e.g., MarketDataSubscription)
        if let Some(state) = MarketSubscription::try_get_mut::<S>(sub_ref) {
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
          if let Some(state) = MarketSubscription::try_get_mut::<S>(sub_ref) {
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
  // Note: get_quote uses a specialized wait because its completion is tied to snapshot_end
  // or receiving specific ticks, not a flexible closure. We keep it separate.
  fn wait_for_quote_completion(
    &self,
    req_id: i32,
    timeout: Duration,
  ) -> Result<Quote, IBKRError> {
    let start_time = std::time::Instant::now();
    let mut guard = self.subscriptions.lock();

    loop {
      // 1. Check state *before* waiting
      let maybe_result = if let Some(MarketSubscription::TickData(state)) = guard.get(&req_id) {
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
        // If not complete and no error, check if we have all required prices (alternative success)
        // This can happen if ticks arrive before snapshot_end
        else if state.bid_price.is_some() && state.ask_price.is_some() && state.last_price.is_some() {
          debug!("All required prices received for quote request {} before completion signal.", req_id);
          Some(Ok((state.bid_price, state.ask_price, state.last_price)))
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
        let final_check = if let Some(MarketSubscription::TickData(state)) = guard.get(&req_id) {
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
          // Check if all prices arrived just before timeout
          else if state.bid_price.is_some() && state.ask_price.is_some() && state.last_price.is_some() {
            Some(Ok((state.bid_price, state.ask_price, state.last_price)))
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

  /// Requests streaming market data (ticks). Non-blocking. Returns req_id.
  pub fn request_market_data(
    &self,
    contract: &Contract,
    generic_tick_list: &str, // e.g., "100,101,104,106,165,233,236,258"
    snapshot: bool,
    regulatory_snapshot: bool, // Requires TWS 963+
    mkt_data_options: &[(String, String)], // TagValue list
  ) -> Result<i32, IBKRError> {
    info!("Requesting market data: Contract={}, Snapshot={}, RegSnapshot={}", contract.symbol, snapshot, regulatory_snapshot);
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let request_msg = encoder.encode_request_market_data(
      req_id, contract, generic_tick_list, snapshot, regulatory_snapshot, mkt_data_options,
    )?;

    // Initialize and store state
    {
      let mut subs = self.subscriptions.lock();
      if subs.contains_key(&req_id) {
        return Err(IBKRError::DuplicateRequestId(req_id));
      }
      let state = MarketDataSubscription::new(
        req_id,
        contract.clone(),
        generic_tick_list.to_string(),
        snapshot,
        regulatory_snapshot,
        mkt_data_options.to_vec(),
      );
      subs.insert(req_id, MarketSubscription::TickData(state));
      debug!("Market data subscription added for ReqID: {}", req_id);
    }

    self.message_broker.send_message(&request_msg)?;
    Ok(req_id)
  }

  /// Requests streaming market data (ticks) and blocks until a completion condition is met.
  pub fn get_market_data<F>(
    &self,
    contract: &Contract,
    generic_tick_list: &str,
    snapshot: bool, // Note: For true snapshots, get_quote might be simpler
    regulatory_snapshot: bool,
    mkt_data_options: &[(String, String)],
    timeout: Duration,
    completion_check: F, // Closure: FnMut(&MarketDataSubscription) -> bool
  ) -> Result<MarketDataSubscription, IBKRError>
  where
    F: FnMut(&MarketDataSubscription) -> bool,
  {
    info!("Requesting blocking market data: Contract={}, Snapshot={}, Timeout={:?}",
          contract.symbol, snapshot, timeout);

    // 1. Initiate the non-blocking request (gets req_id and stores initial state)
    let req_id = self.request_market_data(
      contract,
      generic_tick_list,
      snapshot,
      regulatory_snapshot,
      mkt_data_options,
    )?;
    debug!("Blocking market data request initiated with ReqID: {}", req_id);

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


  /// Cancels a streaming market data request.
  pub fn cancel_market_data(&self, req_id: i32) -> Result<(), IBKRError> {
    info!("Cancelling market data request: ReqID={}", req_id);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_market_data(req_id)?;

    // Remove state *before* sending cancel, or after? Let's remove after success.
    self.message_broker.send_message(&request_msg)?;

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


  /// Requests a specific number of 5-second real-time bars. Blocks until the bars are received, an error occurs, or timeout.
  pub fn get_realtime_bars(
    &self,
    contract: &Contract,
    what_to_show: &str, // "TRADES", "MIDPOINT", "BID", "ASK"
    use_rth: bool,
    real_time_bars_options: &[(String, String)],
    num_bars: usize, // Number of bars to wait for
    timeout: Duration, // Total timeout for the operation
  ) -> Result<Vec<Bar>, IBKRError> {
    if num_bars == 0 {
      return Err(IBKRError::ConfigurationError("num_bars must be greater than 0".to_string()));
    }
    let bar_size = 5; // Hardcoded as per API limitation
    info!("Requesting {} real time bars: Contract={}, What={}, RTH={}, Timeout={:?}",
          num_bars, contract.symbol, what_to_show, use_rth, timeout);
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let request_msg = encoder.encode_request_real_time_bars(
      req_id, contract, bar_size, what_to_show, use_rth, real_time_bars_options,
    )?;

    // Initialize and store state, marking target count
    {
      let mut subs = self.subscriptions.lock();
      if subs.contains_key(&req_id) { return Err(IBKRError::DuplicateRequestId(req_id)); }
      let state = RealTimeBarSubscription {
        req_id,
        contract: contract.clone(),
        bar_size,
        what_to_show: what_to_show.to_string(),
        use_rth,
        rt_bar_options: real_time_bars_options.to_vec(),
        latest_bar: None,
        bars: Vec::with_capacity(num_bars), // Pre-allocate
        target_bar_count: Some(num_bars), // Mark target for blocking
        completed: false, // Not completed yet
        error_code: None,
        error_message: None,
      };
      subs.insert(req_id, MarketSubscription::RealTimeBars(state));
      debug!("Blocking real time bar request added for ReqID: {}", req_id);
    }

    // Send the request
    self.message_broker.send_message(&request_msg)?;

    // Block and wait for completion
    let result = self.wait_for_realtime_bars_completion(req_id, num_bars, timeout);

    // Best effort cancel after completion/timeout/error
    // Ignore error here as the state might already be removed by the wait function
    let _ = self.cancel_real_time_bars(req_id);

    result
  }



  /// Gets a single quote (Bid, Ask, Last) for a contract using a snapshot request. Blocks until data is received or timeout.
  pub fn get_quote(&self, contract: &Contract, timeout: Duration) -> Result<Quote, IBKRError> {
    info!("Requesting quote snapshot for: Contract={}", contract.symbol);
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    // Use an empty generic tick list for snapshot requests.
    // This often requests all available generic ticks for the snapshot.
    let generic_tick_list = "";
    let snapshot = false; // Request snapshot
    let regulatory_snapshot = false; // Typically false for simple quotes
    let mkt_data_options: Vec<(String, String)> = Vec::new(); // No options needed usually

    let request_msg = encoder.encode_request_market_data(
      req_id, contract, generic_tick_list, snapshot, regulatory_snapshot, &mkt_data_options,
    )?;

    // Initialize and store state, marking as a blocking quote request
    {
      let mut subs = self.subscriptions.lock();
      if subs.contains_key(&req_id) {
        return Err(IBKRError::DuplicateRequestId(req_id));
      }
      let mut state = MarketDataSubscription::new(
        req_id,
        contract.clone(),
        generic_tick_list.to_string(),
        snapshot,
        regulatory_snapshot,
        mkt_data_options,
      );
      state.is_blocking_quote_request = true; // Mark this specifically
      subs.insert(req_id, MarketSubscription::TickData(state));
      debug!("Blocking quote request added for ReqID: {}", req_id);
    }

    // Send the request
    self.message_broker.send_message(&request_msg)?;

    // Block and wait for completion
    let result = self.wait_for_quote_completion(req_id, timeout);

    // Best effort cancel after completion/timeout/error
    // Ignore error here as the state might already be removed by the wait function
    let _ = self.cancel_market_data(req_id);

    result
  }


  /// Requests streaming 5-second real-time bars. Non-blocking. Returns req_id.
  pub fn request_real_time_bars(
    &self,
    contract: &Contract,
    // bar_size: i32, // API currently only supports 5
    what_to_show: &str, // "TRADES", "MIDPOINT", "BID", "ASK"
    use_rth: bool,
    real_time_bars_options: &[(String, String)],
  ) -> Result<i32, IBKRError> {
    let bar_size = 5; // Hardcoded as per API limitation
    info!("Requesting real time bars: Contract={}, What={}, RTH={}", contract.symbol, what_to_show, use_rth);
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let request_msg = encoder.encode_request_real_time_bars(
      req_id, contract, bar_size, what_to_show, use_rth, real_time_bars_options,
    )?;

    {
      let mut subs = self.subscriptions.lock();
      if subs.contains_key(&req_id) { return Err(IBKRError::DuplicateRequestId(req_id)); }
      let state = RealTimeBarSubscription {
        req_id,
        contract: contract.clone(),
        bar_size,
        what_to_show: what_to_show.to_string(),
        use_rth,
        rt_bar_options: real_time_bars_options.to_vec(),
        latest_bar: None,
        bars: Vec::new(), // Initialize empty vec for streaming
        target_bar_count: None, // Not a blocking request by default
        completed: false, // Not completed yet
        error_code: None,
        error_message: None,
      };
      subs.insert(req_id, MarketSubscription::RealTimeBars(state));
      debug!("Streaming real time bar subscription added for ReqID: {}", req_id);
    }

    self.message_broker.send_message(&request_msg)?;
    Ok(req_id)
  }

  /// Cancels streaming real-time bars.
  pub fn cancel_real_time_bars(&self, req_id: i32) -> Result<(), IBKRError> {
    info!("Cancelling real time bars request: ReqID={}", req_id);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_real_time_bars(req_id)?;

    self.message_broker.send_message(&request_msg)?;

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


  /// Requests streaming tick-by-tick data. Non-blocking. Returns req_id.
  pub fn request_tick_by_tick_data(
    &self,
    contract: &Contract,
    tick_type: &str, // "Last", "AllLast", "BidAsk", "MidPoint"
    number_of_ticks: i32, // 0 for streaming, >0 for historical snapshot
    ignore_size: bool, // Usually false for streaming
  ) -> Result<i32, IBKRError> {
    info!("Requesting tick-by-tick data: Contract={}, Type={}, NumTicks={}, IgnoreSize={}",
          contract.symbol, tick_type, number_of_ticks, ignore_size);
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let request_msg = encoder.encode_request_tick_by_tick_data(
      req_id, contract, tick_type, number_of_ticks, ignore_size,
    )?;

    {
      let mut subs = self.subscriptions.lock();
      if subs.contains_key(&req_id) { return Err(IBKRError::DuplicateRequestId(req_id)); }
      let state = TickByTickSubscription {
        req_id,
        contract: contract.clone(),
        tick_type: tick_type.to_string(),
        number_of_ticks,
        ignore_size,
        latest_tick: None,
        ticks: Vec::new(), // Initialize history
        completed: false, // Initialize completion flag
        error_code: None,
        error_message: None,
      };
      subs.insert(req_id, MarketSubscription::TickByTick(state));
      debug!("Tick-by-tick subscription added for ReqID: {}", req_id);
    }

    self.message_broker.send_message(&request_msg)?;
    Ok(req_id)
  }

  /// Requests streaming tick-by-tick data and blocks until a completion condition is met.
  pub fn get_tick_by_tick_data<F>(
    &self,
    contract: &Contract,
    tick_type: &str, // "Last", "AllLast", "BidAsk", "MidPoint"
    number_of_ticks: i32, // Should be 0 for streaming blocking requests
    ignore_size: bool,
    timeout: Duration,
    completion_check: F, // Closure: FnMut(&TickByTickSubscription) -> bool
  ) -> Result<TickByTickSubscription, IBKRError>
  where
    F: FnMut(&TickByTickSubscription) -> bool,
  {
    if number_of_ticks != 0 {
      // This function is intended for blocking on *streaming* data.
      // For historical ticks (number_of_ticks > 0), a different mechanism might be needed.
      warn!("get_tick_by_tick_data called with non-zero number_of_ticks ({}). This function blocks on streaming data (number_of_ticks=0).", number_of_ticks);
      // Proceed anyway, but the behavior might not be as expected for historical snapshots.
    }
    info!("Requesting blocking tick-by-tick data: Contract={}, Type={}, Timeout={:?}",
          contract.symbol, tick_type, timeout);

    // 1. Initiate the non-blocking request
    let req_id = self.request_tick_by_tick_data(
      contract,
      tick_type,
      number_of_ticks, // Pass through, though usually 0
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


  /// Cancels streaming tick-by-tick data.
  pub fn cancel_tick_by_tick_data(&self, req_id: i32) -> Result<(), IBKRError> {
    info!("Cancelling tick-by-tick data request: ReqID={}", req_id);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_tick_by_tick_data(req_id)?;

    self.message_broker.send_message(&request_msg)?;

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

  /// Requests streaming market depth. Non-blocking. Returns req_id.
  pub fn request_market_depth(
    &self,
    contract: &Contract,
    num_rows: i32,
    is_smart_depth: bool,
    mkt_depth_options: &[(String, String)],
  ) -> Result<i32, IBKRError> {
    info!("Requesting market depth: Contract={}, Rows={}, Smart={}", contract.symbol, num_rows, is_smart_depth);
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let request_msg = encoder.encode_request_market_depth(
      req_id, contract, num_rows, is_smart_depth, mkt_depth_options,
    )?;

    {
      let mut subs = self.subscriptions.lock();
      if subs.contains_key(&req_id) { return Err(IBKRError::DuplicateRequestId(req_id)); }
      let state = MarketDepthSubscription {
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
      subs.insert(req_id, MarketSubscription::MarketDepth(state));
      debug!("Market depth subscription added for ReqID: {}", req_id);
    }

    self.message_broker.send_message(&request_msg)?;
    Ok(req_id)
  }

  /// Requests streaming market depth and blocks until a completion condition is met.
  pub fn get_market_depth<F>(
    &self,
    contract: &Contract,
    num_rows: i32,
    is_smart_depth: bool,
    mkt_depth_options: &[(String, String)],
    timeout: Duration,
    completion_check: F, // Closure: FnMut(&MarketDepthSubscription) -> bool
  ) -> Result<MarketDepthSubscription, IBKRError>
  where
    F: FnMut(&MarketDepthSubscription) -> bool,
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


  /// Cancels streaming market depth.
  pub fn cancel_market_depth(&self, req_id: i32) -> Result<(), IBKRError> {
    let is_smart_depth = { // Need to check the state for is_smart_depth flag
      let subs = self.subscriptions.lock();
      if let Some(MarketSubscription::MarketDepth(state)) = subs.get(&req_id) {
        state.is_smart_depth
      } else {
        // If state not found, assume false or return error? Let's assume false.
        warn!("Cannot determine is_smart_depth for cancel_market_depth ReqID: {}. Assuming false.", req_id);
        false
      }
    };

    info!("Cancelling market depth request: ReqID={}, Smart={}", req_id, is_smart_depth);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_market_depth(req_id, is_smart_depth)?;

    self.message_broker.send_message(&request_msg)?;

    {
      let mut subs = self.subscriptions.lock();
      if subs.remove(&req_id).is_some() {
        debug!("Removed market depth subscription state for ReqID: {}", req_id);
      } else {
        warn!("Attempted to cancel market depth for unknown or already removed ReqID: {}", req_id);
      }
    }
    // Internal helper to avoid code duplication, as cancel needs is_smart_depth
    self._cancel_market_depth_internal(req_id, is_smart_depth)
  }

  fn _cancel_market_depth_internal(&self, req_id: i32, is_smart_depth: bool) -> Result<(), IBKRError> {
    info!("Cancelling market depth request: ReqID={}, Smart={}", req_id, is_smart_depth);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_market_depth(req_id, is_smart_depth)?;

    self.message_broker.send_message(&request_msg)?;

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


  /// Requests historical bar data. Blocks until data is received or timeout.
  pub fn get_historical_data(
    &self,
    contract: &Contract,
    end_date_time: Option<chrono::DateTime<chrono::Utc>>, // Use chrono DateTime
    duration_str: &str, // e.g., "1 Y", "3 M", "60 D", "3600 S"
    bar_size_setting: &str, // e.g., "1 day", "30 mins", "1 secs"
    what_to_show: &str, // e.g., "TRADES", "MIDPOINT", "BID_ASK"
    use_rth: bool,
    format_date: i32, // 1 for yyyyMMdd HH:mm:ss, 2 for system time (seconds)
    keep_up_to_date: bool, // Subscribe to updates after initial load
    _chart_options: &[(String, String)], // TagValue list
  ) -> Result<Vec<Bar>, IBKRError> {
    info!("Requesting historical data: Contract={}, Duration={}, BarSize={}, What={}",
          contract.symbol, duration_str, bar_size_setting, what_to_show);
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let request_msg = encoder.encode_request_historical_data(
      req_id, contract, end_date_time, duration_str, bar_size_setting,
      what_to_show, use_rth, format_date, keep_up_to_date, // chart_options,
    )?;

    {
      let mut subs = self.subscriptions.lock();
      if subs.contains_key(&req_id) { return Err(IBKRError::DuplicateRequestId(req_id)); }
      let state = HistoricalDataRequestState {
        req_id,
        contract: contract.clone(),
        ..Default::default()
      };
      subs.insert(req_id, MarketSubscription::HistoricalData(state));
      debug!("Historical data request added for ReqID: {}", req_id);
    }

    self.message_broker.send_message(&request_msg)?;

    // Block and wait for completion
    let timeout = Duration::from_secs(60); // Historical can take time
    self.wait_for_historical_completion(req_id, timeout)
  }

  /// Cancels a historical data request.
  pub fn cancel_historical_data(&self, req_id: i32) -> Result<(), IBKRError> {
    info!("Cancelling historical data request: ReqID={}", req_id);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_historical_data(req_id)?;

    self.message_broker.send_message(&request_msg)?;

    // Signal the waiting thread (if any) that it was cancelled?
    // The wait loop will eventually time out or see an error.
    // Removing the state might be enough.

    {
      let mut subs = self.subscriptions.lock();
      if let Some(MarketSubscription::HistoricalData(state)) = subs.get_mut(&req_id) {
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

    let mut subs = self.subscriptions.lock();
    if let Some(sub_state) = subs.get_mut(&req_id) {
      warn!("API Error received for market data request {}: Code={:?}, Msg={}", req_id, code, msg);

      // Convert code to i32 for storage
      let error_code_int = code as i32;

      // Extract error fields
      let (err_code_field, err_msg_field) = match sub_state {
        MarketSubscription::TickData(s) => (&mut s.error_code, &mut s.error_message),
        MarketSubscription::RealTimeBars(s) => (&mut s.error_code, &mut s.error_message),
        MarketSubscription::TickByTick(s) => (&mut s.error_code, &mut s.error_message),
        MarketSubscription::MarketDepth(s) => (&mut s.error_code, &mut s.error_message),
        MarketSubscription::HistoricalData(s) => (&mut s.error_code, &mut s.error_message),
      };

      *err_code_field = Some(error_code_int); // Store the integer code
      *err_msg_field = Some(msg.to_string()); // Store the cloned message string

      // Determine if it's any kind of blocking request (historical, quote, or flexible closure-based)
      let is_blocking = match sub_state {
        MarketSubscription::HistoricalData(_) => true, // Always blocking
        MarketSubscription::TickData(s) => s.is_blocking_quote_request || s.completed, // Quote or flexible
        MarketSubscription::RealTimeBars(s) => s.target_bar_count.is_some() || s.completed, // Fixed count or flexible
        MarketSubscription::TickByTick(s) => s.completed, // Flexible only
        MarketSubscription::MarketDepth(s) => s.completed, // Flexible only
      };


      // If it's a blocking request, mark end and notify waiter
      if is_blocking {
        match sub_state {
          MarketSubscription::HistoricalData(s) => { s.end_received = true; },
          MarketSubscription::TickData(s) => { s.completed = true; s.quote_received = true; }, // Mark both flags
          MarketSubscription::RealTimeBars(s) => { s.completed = true; },
          MarketSubscription::TickByTick(s) => { s.completed = true; },
          MarketSubscription::MarketDepth(s) => { s.completed = true; },
        }
        debug!("Error received for blocking request {}, marking complete and notifying waiter.", req_id);
        self.request_cond.notify_all();
      }
      // For non-blocking streaming requests, the error is just stored.
    } else {
      // Might be an error for a request that already completed/cancelled/timed out.
      trace!("Received error for unknown or completed market data request ID: {}", req_id);
    }
  }

  // --- Optional: Add observer management ---
  // pub fn add_observer(&self, observer: Weak<dyn MarketDataObserver>) { ... }
  // pub fn remove_observer(&self, observer: Weak<dyn MarketDataObserver>) { ... }
  // fn notify_observers(&self, req_id: i32) { ... }
}


// --- Implement MarketDataHandler Trait for DataMarketManager ---
impl MarketDataHandler for DataMarketManager {

  // --- Tick Data ---
  fn tick_price(&self, req_id: i32, tick_type: i32, price: f64, attrib: TickAttrib) {
    trace!("Handler: Tick Price: ID={}, Type={}, Price={}, Attrib={:?}", req_id, tick_type, price, attrib);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketSubscription::TickData(state)) = subs.get_mut(&req_id) {
      // Map tick_type to fields in MarketDataSubscriptionState
      match tick_type {
        1 => state.bid_price = Some(price),  // BID
        2 => state.ask_price = Some(price),  // ASK
        4 => state.last_price = Some(price), // LAST
        6 => state.high_price = Some(price), // HIGH
        7 => state.low_price = Some(price),  // LOW
        9 => state.close_price = Some(price),// CLOSE
        14 => state.open_price = Some(price), // OPEN_TICK
        // ... map other tick types (DELAYED_BID, DELAYED_ASK, etc.) if needed
        _ => trace!("Unhandled tick_type {} in tick_price for ReqID {}", tick_type, req_id),
      }
      // TODO: Store attrib if needed
      state.ticks.entry(tick_type).or_default().push((price, attrib.clone())); // Store tick history

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
      // self.notify_observers(req_id); // If using observer pattern
    } else {
      // warn!("Received tick_price for unknown or non-tick subscription ID: {}", req_id);
    }
  }

  fn tick_size(&self, req_id: i32, tick_type: i32, size: f64) {
    trace!("Handler: Tick Size: ID={}, Type={}, Size={}", req_id, tick_type, size);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketSubscription::TickData(state)) = subs.get_mut(&req_id) {
      match tick_type {
        0 => state.bid_size = Some(size),  // BID_SIZE
        3 => state.ask_size = Some(size),  // ASK_SIZE
        5 => state.last_size = Some(size), // LAST_SIZE
        8 => state.volume = Some(size),    // VOLUME
        27 => state.call_open_interest = Some(size),
        28 => state.put_open_interest = Some(size),
        29 => state.call_volume = Some(size),
        30 => state.put_volume = Some(size),
        21 => state.avg_volume = Some(size), // Map AVG_VOLUME to this? Check TWS docs
        37 => state.shortable_shares = Some(size), // SHORTABLE -> SHORTABLE_SHARES
        48 => state.futures_open_interest = Some(size), // RT_VOLUME -> FUTURES_OPEN_INTEREST? Check TWS docs
        // ... map other size types (DELAYED_BID_SIZE, RT_TRD_VOLUME etc.)
        _ => trace!("Unhandled tick_type {} in tick_size", tick_type),
      }
      state.sizes.entry(tick_type).or_default().push(size); // Store size history
      self.request_cond.notify_all(); // Notify waiters
      // self.notify_observers(req_id);
    } else {
      // warn!("Received tick_size for unknown or non-tick subscription ID: {}", req_id);
    }
  }

  fn tick_string(&self, req_id: i32, tick_type: i32, value: &str) {
    trace!("Handler: Tick String: ID={}, Type={}, Value='{}'", req_id, tick_type, value);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketSubscription::TickData(state)) = subs.get_mut(&req_id) {
      match tick_type {
        45 => { // LAST_TIMESTAMP
          if let Ok(ts) = value.parse::<i64>() {
            state.last_timestamp = Some(ts);
          } else {
            warn!("Failed to parse LAST_TIMESTAMP '{}' for ReqID {}", value, req_id);
          }
        },
        59 => state.last_reg_time = Some(value.to_string()), // IB_DIVIDENDS -> last_reg_time? Check TWS. Placeholder.
        // ... map RT_TRADE_VOLUME tick types if they send string data ...
        _ => trace!("Unhandled tick_type {} in tick_string", tick_type),
      }
      // self.notify_observers(req_id);
    } else {
      // warn!("Received tick_string for unknown or non-tick subscription ID: {}", req_id);
    }
  }

  fn tick_generic(&self, req_id: i32, tick_type: i32, value: f64) {
    trace!("Handler: Tick Generic: ID={}, Type={}, Value={}", req_id, tick_type, value);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketSubscription::TickData(state)) = subs.get_mut(&req_id) {
      match tick_type {
        10 => state.bid_price = Some(value), // OPTION_IMPLIED_VOL -> bid_price? Unlikely. Needs mapping.
        11 => state.ask_price = Some(value), // OPTION_IMPLIED_VOL -> ask_price? Unlikely. Needs mapping.
        31 => state.trade_count = Some(value as i64), // AUCTION_IMBALANCE -> trade_count? Unlikely. Needs mapping.
        // ... map other generic types ...
        _ => trace!("Unhandled tick_type {} in tick_generic", tick_type),
      }
      // self.notify_observers(req_id);
    } else {
      // warn!("Received tick_generic for unknown or non-tick subscription ID: {}", req_id);
    }
  }

  fn tick_efp(&self, req_id: i32, tick_type: i32, basis_points: f64, _formatted_basis_points: &str,
              _implied_futures_price: f64, _hold_days: i32, _future_last_trade_date: &str,
              _dividend_impact: f64, _dividends_to_last_trade_date: f64) {
    trace!("Handler: Tick EFP: ID={}, Type={}, BasisPts={}", req_id, tick_type, basis_points);
    // EFP data doesn't typically fit into the standard MarketDataSubscription fields.
    // An observer pattern or dedicated callback might be better here.
    // For now, just log it.
  }

  fn tick_option_computation(&self, req_id: i32, data: TickOptionComputationData) {
    trace!("Handler: Tick Option Computation: ID={}, Type={}", req_id, data.tick_type);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketSubscription::TickData(state)) = subs.get_mut(&req_id) {
      state.option_computation = Some(data);
      // self.notify_observers(req_id);
    } else {
      // warn!("Received tick_option_computation for unknown or non-tick subscription ID: {}", req_id);
    }
  }

  fn tick_snapshot_end(&self, req_id: i32) {
    debug!("Handler: Tick Snapshot End: ID={}", req_id);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketSubscription::TickData(state)) = subs.get_mut(&req_id) {
      state.snapshot_end_received = true;
      // If this was a blocking quote snapshot request, mark as complete and notify waiter.
      if state.is_blocking_quote_request {
        state.quote_received = true;
        debug!("Snapshot end received for blocking quote ReqID {}. Notifying waiter.", req_id);
        self.request_cond.notify_all();
      }
    } else {
      // warn!("Received tick_snapshot_end for unknown or non-tick subscription ID: {}", req_id);
    }
  }

  fn market_data_type(&self, req_id: i32, market_data_type: MarketDataTypeEnum) {
    debug!("Handler: Market Data Type: ID={}, Type={:?}", req_id, market_data_type);
    let mut subs = self.subscriptions.lock();
    // Store the type in all relevant subscription types?
    if let Some(MarketSubscription::TickData(state)) = subs.get_mut(&req_id) {
      state.market_data_type = Some(market_data_type);
    } else if let Some(MarketSubscription::MarketDepth(_state)) = subs.get_mut(&req_id) {
      // Market depth might also care about frozen status
      // state.market_data_type = Some(market_data_type); // Add field if needed
    }
    // No need to notify observers for this meta-information usually.
  }

  fn tick_req_params(&self, req_id: i32, min_tick: f64, bbo_exchange: &str, snapshot_permissions: i32) {
    debug!("Handler: Tick Req Params: ID={}, MinTick={}, BBOExch={}, Permissions={}", req_id, min_tick, bbo_exchange, snapshot_permissions);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketSubscription::TickData(state)) = subs.get_mut(&req_id) {
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
    if let Some(MarketSubscription::RealTimeBars(state)) = subs.get_mut(&req_id) {
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
      // self.notify_observers(req_id); // Or specific bar observer for streaming
    } else {
      // warn!("Received real_time_bar for unknown or non-RTBar subscription ID: {}", req_id);
    }
  }

  // --- Historical Data ---
  fn historical_data(&self, req_id: i32, bar: &Bar) {
    trace!("Handler: Historical Data Bar: ID={}, Time={}", req_id, bar.time);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketSubscription::HistoricalData(state)) = subs.get_mut(&req_id) {
      state.bars.push(bar.clone());
      // Don't notify yet, wait for end.
    } else {
      // warn!("Received historical_data for unknown or non-historical subscription ID: {}", req_id);
    }
  }

  fn historical_data_update(&self, req_id: i32, bar: &Bar) {
    debug!("Handler: Historical Data Update Bar: ID={}, Time={}", req_id, bar.time);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketSubscription::HistoricalData(state)) = subs.get_mut(&req_id) {
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
      // self.notify_observers(req_id); // Notify observer about update
    } else {
      // warn!("Received historical_data_update for unknown or non-historical subscription ID: {}", req_id);
    }
  }

  fn historical_data_end(&self, req_id: i32, start_date: &str, end_date: &str) {
    debug!("Handler: Historical Data End: ID={}, Start={}, End={}", req_id, start_date, end_date);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketSubscription::HistoricalData(state)) = subs.get_mut(&req_id) {
      state.start_date = start_date.to_string();
      state.end_date = end_date.to_string();
      state.end_received = true;
      info!("Historical data end received for request {}. Notifying waiter.", req_id);
      self.request_cond.notify_all(); // Signal waiting thread
    } else {
      // warn!("Received historical_data_end for unknown or non-historical subscription ID: {}", req_id);
    }
  }

  fn historical_ticks(&self, req_id: i32, ticks: &[(i64, f64, f64)], done: bool) {
    debug!("Handler: Historical Ticks: ID={}, Count={}, Done={}", req_id, ticks.len(), done);
    // Store or process historical ticks. Similar logic to historical bars.
    // Needs state in DataMarketManager if blocking is required.
    if done {
      info!("Historical ticks end received for request {}. Notifying waiter.", req_id);
      // self.request_cond.notify_all();
    }
  }

  fn historical_ticks_bid_ask(&self, req_id: i32, ticks: &[(i64, TickAttribBidAsk, f64, f64, f64, f64)], done: bool) {
    debug!("Handler: Historical Ticks BidAsk: ID={}, Count={}, Done={}", req_id, ticks.len(), done);
    if done {
      info!("Historical ticks bidask end received for request {}. Notifying waiter.", req_id);
      // self.request_cond.notify_all();
    }
  }

  fn historical_ticks_last(&self, req_id: i32, ticks: &[(i64, TickAttribLast, f64, f64, String, String)], done: bool) {
    debug!("Handler: Historical Ticks Last: ID={}, Count={}, Done={}", req_id, ticks.len(), done);
    if done {
      info!("Historical ticks last end received for request {}. Notifying waiter.", req_id);
      // self.request_cond.notify_all();
    }
  }

  // --- Tick By Tick ---
  fn tick_by_tick_all_last(&self, req_id: i32, tick_type: i32, time: i64, price: f64, size: f64,
                           tick_attrib_last: TickAttribLast, exchange: &str, special_conditions: &str) {
    trace!("Handler: TickByTick AllLast: ID={}, Time={}, Px={}, Sz={}", req_id, time, price, size);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketSubscription::TickByTick(state)) = subs.get_mut(&req_id) {
      let data = if tick_type == 1 {
        TickByTickData::Last { time, price, size, tick_attrib_last, exchange: exchange.to_string(), special_conditions: special_conditions.to_string() }
      } else {
        TickByTickData::AllLast { time, price, size, tick_attrib_last, exchange: exchange.to_string(), special_conditions: special_conditions.to_string() }
      };
      state.ticks.push(data.clone()); // Store history
      state.latest_tick = Some(data);
      self.request_cond.notify_all(); // Notify waiters
      // self.notify_observers(req_id);
    } else {
      // warn!("Received tick_by_tick_all_last for unknown or non-TBT subscription ID: {}", req_id);
    }
  }

  fn tick_by_tick_bid_ask(&self, req_id: i32, time: i64, bid_price: f64, ask_price: f64, bid_size: f64,
                          ask_size: f64, tick_attrib_bid_ask: TickAttribBidAsk) {
    trace!("Handler: TickByTick BidAsk: ID={}, Time={}, BidPx={}, AskPx={}", req_id, time, bid_price, ask_price);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketSubscription::TickByTick(state)) = subs.get_mut(&req_id) {
      let data = TickByTickData::BidAsk { time, bid_price, ask_price, bid_size, ask_size, tick_attrib_bid_ask };
      state.ticks.push(data.clone()); // Store history
      state.latest_tick = Some(data);
      self.request_cond.notify_all(); // Notify waiters
      // self.notify_observers(req_id);
    } else {
      // warn!("Received tick_by_tick_bid_ask for unknown or non-TBT subscription ID: {}", req_id);
    }
  }

  fn tick_by_tick_mid_point(&self, req_id: i32, time: i64, mid_point: f64) {
    trace!("Handler: TickByTick MidPoint: ID={}, Time={}, MidPt={}", req_id, time, mid_point);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketSubscription::TickByTick(state)) = subs.get_mut(&req_id) {
      let data = TickByTickData::MidPoint { time, mid_point };
      state.ticks.push(data.clone()); // Store history
      state.latest_tick = Some(data);
      self.request_cond.notify_all(); // Notify waiters
      // self.notify_observers(req_id);
    } else {
      // warn!("Received tick_by_tick_mid_point for unknown or non-TBT subscription ID: {}", req_id);
    }
  }

  // --- Market Depth ---
  fn update_mkt_depth(&self, req_id: i32, position: i32, operation: i32, side: i32, price: f64, size: f64) {
    trace!("Handler: MktDepth L1 Update: ID={}, Pos={}, Op={}, Side={}, Px={}, Sz={}", req_id, position, operation, side, price, size);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketSubscription::MarketDepth(state)) = subs.get_mut(&req_id) {
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
      // self.notify_observers(req_id);
    } else {
      // warn!("Received update_mkt_depth for unknown or non-Depth subscription ID: {}", req_id);
    }
  }

  fn update_mkt_depth_l2(&self, req_id: i32, position: i32, market_maker: &str, operation: i32,
                         side: i32, price: f64, size: f64, is_smart_depth: bool) {
    trace!("Handler: MktDepth L2 Update: ID={}, Pos={}, MM={}, Op={}, Side={}, Px={}, Sz={}, Smart={}",
           req_id, position, market_maker, operation, side, price, size, is_smart_depth);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketSubscription::MarketDepth(state)) = subs.get_mut(&req_id) {
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
      // self.notify_observers(req_id);
    } else {
      // warn!("Received update_mkt_depth_l2 for unknown or non-Depth subscription ID: {}", req_id);
    }
  }

  // --- Other Market Data ---
  fn delta_neutral_validation(&self, req_id: i32) {
    debug!("Handler: Delta Neutral Validation: ID={}", req_id);
    // Usually just logged, doesn't update typical subscription state.
  }

  fn histogram_data(&self, req_id: i32) {
    debug!("Handler: Histogram Data Received: ID={}", req_id);
    // Placeholder, needs HistogramEntry struct and state handling if blocking needed.
  }

  fn scanner_parameters(&self, xml: &str) {
    debug!("Handler: Scanner Parameters received ({} bytes)", xml.len());
    // Usually handled by a one-off request, not stored in subscription state.
  }

  fn scanner_data(&self, req_id: i32, rank: i32, contract_details: &ContractDetails, _distance: &str,
                  _benchmark: &str, _projection: &str, _legs_str: Option<&str>) {
    trace!("Handler: Scanner Data Row: ID={}, Rank={}, Symbol={}", req_id, rank, contract_details.contract.symbol);
    // Needs state management if scanner results are tracked.
  }

  fn scanner_data_end(&self, req_id: i32) {
    debug!("Handler: Scanner Data End: ID={}", req_id);
    // Signal completion if scanner request state is managed.
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
