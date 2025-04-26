// yatws/src/data_ref_manager.rs

use crate::base::IBKRError;
use crate::conn::MessageBroker;
use crate::contract::{
  Bar, Contract, ContractDetails, SecType, SoftDollarTier, FamilyCode, ContractDescription,
  DepthMktDataDescription, SmartComponent, MarketRule, PriceIncrement, HistoricalSession,
};
use crate::data::{
    MarketDataSubscription, RealTimeBarSubscription, TickByTickSubscription, MarketDepthSubscription,
    HistoricalDataRequestState, TickAttrib, TickAttribLast, TickAttribBidAsk, TickOptionComputationData,
    MarketDataTypeEnum, MarketDepthRow, TickByTickData,
};
use crate::handler::{ReferenceDataHandler, MarketDataHandler};
use crate::protocol_encoder::Encoder;
use parking_lot::{Condvar, Mutex, RwLock};
use chrono::{Utc, TimeZone};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::time::Duration;
use log::{debug, error, info, trace, warn};


// --- State for Pending Requests ---

#[derive(Debug, Default)]
struct DataRefRequestState {
  // Fields for ContractDetails / BondContractDetails
  contract_details_list: Vec<ContractDetails>,
  contract_details_end_received: bool,

  // Fields for SecDefOptParams
  sec_def_params: Option<SecDefOptParamsResult>,
  sec_def_params_end_received: bool,

  // Fields for SoftDollarTiers
  soft_dollar_tiers: Option<Vec<SoftDollarTier>>,
  // No explicit end message for SoftDollarTiers, assume completion after receiving

  // Fields for FamilyCodes
  family_codes: Option<Vec<FamilyCode>>,
  // No explicit end message for FamilyCodes

  // Fields for MatchingSymbols
  symbol_samples: Option<Vec<ContractDescription>>,
  // No explicit end message for MatchingSymbols

  // Fields for MktDepthExchanges
  mkt_depth_exchanges: Option<Vec<DepthMktDataDescription>>,
  // No explicit end message for MktDepthExchanges

  // Fields for SmartComponents
  smart_components: Option<HashMap<i32, (String, char)>>,
  // No explicit end message for SmartComponents

  // Fields for MarketRule
  market_rule: Option<MarketRule>,
  // No explicit end message for MarketRule

  // Fields for HistoricalSchedule
  historical_schedule: Option<HistoricalScheduleResult>,
  // No explicit end message for HistoricalSchedule


  // General error fields
  error_code: Option<i32>,
  error_message: Option<String>,
}

#[derive(Debug, Clone)]
struct SecDefOptParamsResult {
  exchange: String,
  underlying_con_id: i32,
  trading_class: String,
  multiplier: String,
  expirations: Vec<String>,
  strikes: Vec<f64>,
}

#[derive(Debug, Clone)]
struct HistoricalScheduleResult {
  start_date_time: String,
  end_date_time: String,
  time_zone: String,
  sessions: Vec<HistoricalSession>,
}


// --- DataRefManager ---

pub struct DataRefManager {
  message_broker: Arc<MessageBroker>,
  request_states: Mutex<HashMap<i32, DataRefRequestState>>,
  request_cond: Condvar,
  // Note: We don't store permanent reference data here, only results for pending requests.
  // A more advanced implementation might cache results.
}

impl DataRefManager {
  pub fn new(message_broker: Arc<MessageBroker>) -> Arc<Self> {
    Arc::new(DataRefManager {
      message_broker,
      request_states: Mutex::new(HashMap::new()),
      request_cond: Condvar::new(),
    })
  }

  // --- Helper to wait for completion ---
  fn wait_for_completion<F, R>(
    &self,
    req_id: i32,
    timeout: Duration,
    is_complete_check: F,
  ) -> Result<R, IBKRError>
  where
    F: Fn(&DataRefRequestState) -> Option<Result<R, IBKRError>>, // Returns Some(result) when complete
  {
    let start_time = std::time::Instant::now();
    let mut guard = self.request_states.lock();

    loop {
      // 1. Check if complete *before* waiting
      if let Some(state) = guard.get(&req_id) {
        match is_complete_check(state) {
          Some(Ok(result)) => {
            guard.remove(&req_id); // Clean up state
            return Ok(result);
          },
          Some(Err(e)) => {
            guard.remove(&req_id); // Clean up state
            return Err(e);
          },
          None => { /* Not complete yet */ }
        }
        // Check for API error stored in the state
        if let (Some(code), Some(msg)) = (state.error_code, state.error_message.as_ref()) {
          let err = IBKRError::ApiError(code, msg.clone());
          guard.remove(&req_id); // Clean up state
          return Err(err);
        }
      } else {
        // State missing, shouldn't happen if initialized correctly
        return Err(IBKRError::InternalError(format!("Request state for {} unexpectedly missing during wait", req_id)));
      }

      // 2. Calculate remaining timeout
      let elapsed = start_time.elapsed();
      if elapsed >= timeout {
        guard.remove(&req_id); // Clean up state on timeout
        return Err(IBKRError::Timeout(format!("Reference data request {} timed out after {:?}", req_id, timeout)));
      }
      let remaining_timeout = timeout - elapsed;

      // 3. Wait
      let wait_result = self.request_cond.wait_for(&mut guard, remaining_timeout);

      // 4. Handle timeout after wait
      if wait_result.timed_out() {
        // Re-check completion status one last time after timeout
        if let Some(state) = guard.get(&req_id) {
          match is_complete_check(state) {
            Some(Ok(result)) => {
              guard.remove(&req_id); return Ok(result);
            },
            Some(Err(e)) => {
              guard.remove(&req_id); return Err(e);
            },
            None => {} // Still not complete
          }
          if let (Some(code), Some(msg)) = (state.error_code, state.error_message.as_ref()) {
            let err = IBKRError::ApiError(code, msg.clone());
            guard.remove(&req_id); // Clean up state
            return Err(err);
          }
        }
        // If still not complete after timeout + final check
        guard.remove(&req_id); // Clean up state
        return Err(IBKRError::Timeout(format!("Reference data request {} timed out after wait", req_id)));
      }
      // If not timed out, loop continues to re-check state
    }
  }


  // --- Public API Methods ---

  /// Requests and returns contract details for a given contract specification.
  /// Can return multiple matches (e.g., for futures chains).
  /// Blocks until the `contractDetailsEnd` message is received or timeout.
  pub fn get_contract_details(&self, contract: &Contract) -> Result<Vec<ContractDetails>, IBKRError> {
    info!("Requesting contract details for: {:?}", contract);
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_request_contract_data(req_id, contract)?;

    // Initialize state
    {
      let mut states = self.request_states.lock();
      if states.contains_key(&req_id) {
        return Err(IBKRError::DuplicateRequestId(req_id));
      }
      states.insert(req_id, DataRefRequestState::default());
    }

    self.message_broker.send_message(&request_msg)?;

    // Wait for completion
    let timeout = Duration::from_secs(20); // Adjust as needed
    self.wait_for_completion(req_id, timeout, |state| {
      if state.contract_details_end_received {
        Some(Ok(state.contract_details_list.clone()))
      } else {
        None // Not complete yet
      }
    })
  }

  /// Requests and returns option chain parameters (expirations, strikes) for an underlying.
  /// Blocks until the `securityDefinitionOptionParameterEnd` message is received or timeout.
  pub fn get_option_chain_params(
    &self,
    underlying_symbol: &str,
    fut_fop_exchange: &str, // Typically "" for options
    underlying_sec_type: SecType, // Typically STK
    underlying_con_id: i32,
  ) -> Result<Vec<SecDefOptParamsResult>, IBKRError> { // Return Vec in case multiple exchanges respond
    info!("Requesting option chain params for: Symbol={}, ConID={}", underlying_symbol, underlying_con_id);
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_request_sec_def_opt_params(
      req_id, underlying_symbol, fut_fop_exchange, underlying_sec_type, underlying_con_id
    )?;

    // Initialize state
    {
      let mut states = self.request_states.lock();
      if states.contains_key(&req_id) { return Err(IBKRError::DuplicateRequestId(req_id)); }
      states.insert(req_id, DataRefRequestState::default());
    }

    self.message_broker.send_message(&request_msg)?;

    // Wait for completion
    let timeout = Duration::from_secs(30); // Option chains can take longer
    // The result needs to aggregate potentially multiple calls to the handler
    // We'll collect them in the RequestState.sec_def_params_list (need to change RequestState)
    // For simplicity *now*, let's assume only one result comes back before the end.

    // --- Adjust RequestState and wait logic ---
    // Add `sec_def_params_list: Vec<SecDefOptParamsResult>` to RequestState
    // Modify the wait closure:
    self.wait_for_completion(req_id, timeout, |state| {
      if state.sec_def_params_end_received {
        // For now, return the single stored Option. A real implementation
        // might aggregate multiple results if the handler stores them in a Vec.
        match &state.sec_def_params {
          Some(params) => Some(Ok(vec![params.clone()])), // Wrap single result in Vec
          None => Some(Ok(Vec::new())), // End received, but no params data
        }
      } else {
        None // Not complete yet
      }
    })
  }

  /// Requests and returns soft dollar tiers.
  pub fn get_soft_dollar_tiers(&self) -> Result<Vec<SoftDollarTier>, IBKRError> {
    info!("Requesting soft dollar tiers");
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_request_soft_dollar_tiers(req_id)?;

    {
      let mut states = self.request_states.lock();
      if states.contains_key(&req_id) { return Err(IBKRError::DuplicateRequestId(req_id)); }
      states.insert(req_id, DataRefRequestState::default());
    }

    self.message_broker.send_message(&request_msg)?;

    let timeout = Duration::from_secs(10);
    self.wait_for_completion(req_id, timeout, |state| {
      // No end message, completion is based on receiving the data
      state.soft_dollar_tiers.as_ref().map(|tiers| Ok(tiers.clone()))
    })
  }

  /// Requests and returns family codes.
  pub fn get_family_codes(&self) -> Result<Vec<FamilyCode>, IBKRError> {
    info!("Requesting family codes");
    // req_id is not used for this request/response pair
    let req_id = self.message_broker.next_request_id(); // Get one anyway for state tracking
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_request_family_codes()?;

    { // Use req_id=0 or a special marker? Let's use the generated req_id for state map key.
      let mut states = self.request_states.lock();
      if states.contains_key(&req_id) { return Err(IBKRError::DuplicateRequestId(req_id)); }
      states.insert(req_id, DataRefRequestState::default());
    }

    self.message_broker.send_message(&request_msg)?;

    let timeout = Duration::from_secs(10);
    // Wait for the data to arrive, identified by the placeholder req_id
    self.wait_for_completion(req_id, timeout, |state| {
      state.family_codes.as_ref().map(|codes| Ok(codes.clone()))
    })
  }

  /// Requests and returns contracts matching a pattern.
  pub fn get_matching_symbols(&self, pattern: &str) -> Result<Vec<ContractDescription>, IBKRError> {
    info!("Requesting matching symbols for pattern: {}", pattern);
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_request_matching_symbols(req_id, pattern)?;

    {
      let mut states = self.request_states.lock();
      if states.contains_key(&req_id) { return Err(IBKRError::DuplicateRequestId(req_id)); }
      states.insert(req_id, DataRefRequestState::default());
    }

    self.message_broker.send_message(&request_msg)?;

    let timeout = Duration::from_secs(20);
    self.wait_for_completion(req_id, timeout, |state| {
      state.symbol_samples.as_ref().map(|samples| Ok(samples.clone()))
    })
  }

  /// Requests and returns exchanges offering market depth.
  pub fn get_mkt_depth_exchanges(&self) -> Result<Vec<DepthMktDataDescription>, IBKRError> {
    info!("Requesting market depth exchanges");
    // req_id is not used for this request/response pair
    let req_id = self.message_broker.next_request_id(); // Get one for state tracking
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_request_mkt_depth_exchanges()?;

    {
      let mut states = self.request_states.lock();
      if states.contains_key(&req_id) { return Err(IBKRError::DuplicateRequestId(req_id)); }
      states.insert(req_id, DataRefRequestState::default());
    }

    self.message_broker.send_message(&request_msg)?;

    let timeout = Duration::from_secs(10);
    self.wait_for_completion(req_id, timeout, |state| {
      state.mkt_depth_exchanges.as_ref().map(|exchanges| Ok(exchanges.clone()))
    })
  }

  /// Requests and returns SMART routing components for a BBO exchange.
  pub fn get_smart_components(&self, bbo_exchange: &str) -> Result<HashMap<i32, (String, char)>, IBKRError> {
    info!("Requesting SMART components for BBO exchange: {}", bbo_exchange);
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_request_smart_components(req_id, bbo_exchange)?;

    {
      let mut states = self.request_states.lock();
      if states.contains_key(&req_id) { return Err(IBKRError::DuplicateRequestId(req_id)); }
      states.insert(req_id, DataRefRequestState::default());
    }

    self.message_broker.send_message(&request_msg)?;

    let timeout = Duration::from_secs(10);
    self.wait_for_completion(req_id, timeout, |state| {
      state.smart_components.as_ref().map(|components| Ok(components.clone()))
    })
  }

  /// Requests and returns details for a specific market rule ID.
  pub fn get_market_rule(&self, market_rule_id: i32) -> Result<MarketRule, IBKRError> {
    info!("Requesting market rule: {}", market_rule_id);
    // req_id is not used for this request/response pair
    let req_id = self.message_broker.next_request_id(); // Get one for state tracking
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_request_market_rule(market_rule_id)?; // Pass the rule ID

    {
      let mut states = self.request_states.lock();
      if states.contains_key(&req_id) { return Err(IBKRError::DuplicateRequestId(req_id)); }
      // Store the requested rule ID so the handler can match it? No, the handler receives the rule ID.
      states.insert(req_id, DataRefRequestState::default());
    }

    self.message_broker.send_message(&request_msg)?;

    let timeout = Duration::from_secs(10);
    self.wait_for_completion(req_id, timeout, |state| {
      // Check if the stored rule's ID matches the requested one
      state.market_rule.as_ref()
        .filter(|rule| rule.market_rule_id == market_rule_id) // Match ID
        .map(|rule| Ok(rule.clone()))
    })
  }

  // --- Add get_historical_schedule if needed ---


  // --- Internal error handling ---
  pub(crate) fn handle_error(&self, req_id: i32, error_code: i32, error_msg: String) {
    if req_id <= 0 { return; } // Ignore general errors

    let mut states = self.request_states.lock();
    if let Some(state) = states.get_mut(&req_id) {
      warn!("API Error received for reference data request {}: Code={}, Msg={}", req_id, error_code, error_msg);
      state.error_code = Some(error_code);
      state.error_message = Some(error_msg);
      // Signal potentially waiting thread
      self.request_cond.notify_all();
    }
    // If req_id not found, it might have already completed or timed out.
  }


}

// --- Implement ReferenceDataHandler Trait ---

impl ReferenceDataHandler for DataRefManager {
  fn contract_details(&self, req_id: i32, contract_details: &ContractDetails) {
    debug!("Handler: Contract Details: ReqID={}, ConID={}", req_id, contract_details.contract.con_id);
    let mut states = self.request_states.lock();
    if let Some(state) = states.get_mut(&req_id) {
      state.contract_details_list.push(contract_details.clone());
      // Don't notify yet, wait for end message
    } else {
      warn!("Received contract details for unknown or completed request ID: {}", req_id);
    }
  }

  fn bond_contract_details(&self, req_id: i32, contract_details: &ContractDetails) {
    debug!("Handler: Bond Contract Details: ReqID={}, ConID={}", req_id, contract_details.contract.con_id);
    let mut states = self.request_states.lock();
    if let Some(state) = states.get_mut(&req_id) {
      // Bond details are also contract details, store in the same list
      state.contract_details_list.push(contract_details.clone());
      // Don't notify yet, wait for end message
    } else {
      warn!("Received bond contract details for unknown or completed request ID: {}", req_id);
    }
  }

  fn contract_details_end(&self, req_id: i32) {
    debug!("Handler: Contract Details End: ReqID={}", req_id);
    let mut states = self.request_states.lock();
    if let Some(state) = states.get_mut(&req_id) {
      state.contract_details_end_received = true;
      info!("Contract details end received for request {}. Notifying waiter.", req_id);
      self.request_cond.notify_all();
    } else {
      warn!("Received contract details end for unknown or completed request ID: {}", req_id);
    }
  }

  fn security_definition_option_parameter(
    &self, req_id: i32, exchange: &str, underlying_con_id: i32,
    trading_class: &str, multiplier: &str, expirations: &[String], strikes: &[f64]
  ) {
    debug!("Handler: SecDefOptParams: ReqID={}, Exchange={}, UnderConID={}", req_id, exchange, underlying_con_id);
    let result = SecDefOptParamsResult {
      exchange: exchange.to_string(),
      underlying_con_id,
      trading_class: trading_class.to_string(),
      multiplier: multiplier.to_string(),
      expirations: expirations.to_vec(),
      strikes: strikes.to_vec(),
    };

    let mut states = self.request_states.lock();
    if let Some(state) = states.get_mut(&req_id) {
      // Store the result. If multiple come, this overwrites.
      // A real implementation might store a Vec<SecDefOptParamsResult>.
      state.sec_def_params = Some(result);
      // Don't notify yet, wait for end message
    } else {
      warn!("Received SecDefOptParams for unknown or completed request ID: {}", req_id);
    }
  }

  fn security_definition_option_parameter_end(&self, req_id: i32) {
    debug!("Handler: SecDefOptParams End: ReqID={}", req_id);
    let mut states = self.request_states.lock();
    if let Some(state) = states.get_mut(&req_id) {
      state.sec_def_params_end_received = true;
      info!("SecDefOptParams end received for request {}. Notifying waiter.", req_id);
      self.request_cond.notify_all();
    } else {
      warn!("Received SecDefOptParams end for unknown or completed request ID: {}", req_id);
    }
  }

  fn soft_dollar_tiers(&self, req_id: i32, tiers: &[SoftDollarTier]) {
    debug!("Handler: Soft Dollar Tiers: ReqID={}, Count={}", req_id, tiers.len());
    let mut states = self.request_states.lock();
    if let Some(state) = states.get_mut(&req_id) {
      state.soft_dollar_tiers = Some(tiers.to_vec());
      // No end message, so notify immediately
      info!("Soft dollar tiers received for request {}. Notifying waiter.", req_id);
      self.request_cond.notify_all();
    } else {
      warn!("Received soft dollar tiers for unknown or completed request ID: {}", req_id);
    }
  }

  fn family_codes(&self, family_codes: &[FamilyCode]) {
    debug!("Handler: Family Codes: Count={}", family_codes.len());
    // This message doesn't have a req_id from TWS.
    // Find the *first* pending request waiting for family codes (if any).
    let mut states = self.request_states.lock();
    let mut found_req_id = None;

    for (id, state) in states.iter_mut() {
      // How to identify the waiting request? Add a flag to RequestState?
      // Or just assume the *latest* request without data is the one? Risky.
      // Let's use the req_id generated by the client for tracking.
      if state.family_codes.is_none() && state.error_code.is_none() { // Check if not already filled
        state.family_codes = Some(family_codes.to_vec());
        found_req_id = Some(*id);
        break; // Assume only one request is active at a time
      }
    }

    if let Some(req_id) = found_req_id {
      info!("Family codes received, matching to request {}. Notifying waiter.", req_id);
      self.request_cond.notify_all();
    } else {
      warn!("Received family codes but no matching pending request found.");
    }
  }

  fn symbol_samples(&self, req_id: i32, contract_descriptions: &[ContractDescription]) {
    debug!("Handler: Symbol Samples: ReqID={}, Count={}", req_id, contract_descriptions.len());
    let mut states = self.request_states.lock();
    if let Some(state) = states.get_mut(&req_id) {
      state.symbol_samples = Some(contract_descriptions.to_vec());
      // No end message, notify immediately
      info!("Symbol samples received for request {}. Notifying waiter.", req_id);
      self.request_cond.notify_all();
    } else {
      warn!("Received symbol samples for unknown or completed request ID: {}", req_id);
    }
  }

  fn mkt_depth_exchanges(&self, descriptions: &[DepthMktDataDescription]) {
    debug!("Handler: Mkt Depth Exchanges: Count={}", descriptions.len());
    // Similar to family codes, no req_id from TWS. Find pending request.
    let mut states = self.request_states.lock();
    let mut found_req_id = None;

    for (id, state) in states.iter_mut() {
      if state.mkt_depth_exchanges.is_none() && state.error_code.is_none() {
        state.mkt_depth_exchanges = Some(descriptions.to_vec());
        found_req_id = Some(*id);
        break;
      }
    }

    if let Some(req_id) = found_req_id {
      info!("Mkt depth exchanges received, matching to request {}. Notifying waiter.", req_id);
      self.request_cond.notify_all();
    } else {
      warn!("Received mkt depth exchanges but no matching pending request found.");
    }
  }

  fn smart_components(&self, req_id: i32, components: &HashMap<i32, (String, char)>) {
    debug!("Handler: Smart Components: ReqID={}, Count={}", req_id, components.len());
    let mut states = self.request_states.lock();
    if let Some(state) = states.get_mut(&req_id) {
      state.smart_components = Some(components.clone()); // Clone the map
      info!("Smart components received for request {}. Notifying waiter.", req_id);
      self.request_cond.notify_all();
    } else {
      warn!("Received smart components for unknown or completed request ID: {}", req_id);
    }
  }

  fn market_rule(&self, market_rule_id: i32, price_increments: &[PriceIncrement]) {
    debug!("Handler: Market Rule: RuleID={}, Increments={}", market_rule_id, price_increments.len());
    // No req_id from TWS. Find pending request based on expected rule ID?
    // Or just use the client-generated req_id for the request function call.
    let mut states = self.request_states.lock();
    let mut found_req_id = None;

    for (id, state) in states.iter_mut() {
      // Need a way to match other than req_id, or assume the state knows which rule it asked for.
      // Sticking with the client req_id association for now.
      if state.market_rule.is_none() && state.error_code.is_none() {
        state.market_rule = Some(MarketRule {
          market_rule_id,
          price_increments: price_increments.to_vec(),
        });
        found_req_id = Some(*id);
        break;
      }
    }

    if let Some(req_id) = found_req_id {
      info!("Market rule {} received, matching to request {}. Notifying waiter.", market_rule_id, req_id);
      self.request_cond.notify_all();
    } else {
      warn!("Received market rule {} but no matching pending request found.", market_rule_id);
    }
  }

  fn historical_schedule(
    &self, req_id: i32, start_date_time: &str, end_date_time: &str, time_zone: &str, sessions: &[HistoricalSession]
  ) {
    debug!("Handler: Historical Schedule: ReqID={}", req_id);
    let result = HistoricalScheduleResult {
      start_date_time: start_date_time.to_string(),
      end_date_time: end_date_time.to_string(),
      time_zone: time_zone.to_string(),
      sessions: sessions.to_vec(),
    };
    let mut states = self.request_states.lock();
    if let Some(state) = states.get_mut(&req_id) {
      state.historical_schedule = Some(result);
      info!("Historical schedule received for request {}. Notifying waiter.", req_id);
      self.request_cond.notify_all();
    } else {
      warn!("Received historical schedule for unknown or completed request ID: {}", req_id);
    }
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
      req_id, contract, generic_tick_list, snapshot, regulatory_snapshot, /* mkt_data_options */
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
        error_code: None,
        error_message: None,
      };
      subs.insert(req_id, MarketSubscription::RealTimeBars(state));
      debug!("Real time bar subscription added for ReqID: {}", req_id);
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
        error_code: None,
        error_message: None,
      };
      subs.insert(req_id, MarketSubscription::TickByTick(state));
      debug!("Tick-by-tick subscription added for ReqID: {}", req_id);
    }

    self.message_broker.send_message(&request_msg)?;
    Ok(req_id)
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
        error_code: None, error_message: None,
      };
      subs.insert(req_id, MarketSubscription::MarketDepth(state));
      debug!("Market depth subscription added for ReqID: {}", req_id);
    }

    self.message_broker.send_message(&request_msg)?;
    Ok(req_id)
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
    chart_options: &[(String, String)], // TagValue list
  ) -> Result<Vec<Bar>, IBKRError> {
    info!("Requesting historical data: Contract={}, Duration={}, BarSize={}, What={}",
          contract.symbol, duration_str, bar_size_setting, what_to_show);
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    let request_msg = encoder.encode_request_historical_data(
      req_id, contract, end_date_time, duration_str, bar_size_setting,
      what_to_show, use_rth, format_date, keep_up_to_date, /* chart_options */
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

  // --- Internal error handling ---
  pub(crate) fn handle_error(&self, req_id: i32, error_code: i32, error_msg: String) {
    if req_id <= 0 { return; } // Ignore general errors not tied to a request

    let mut subs = self.subscriptions.lock();
    if let Some(sub_state) = subs.get_mut(&req_id) {
      warn!("API Error received for market data request {}: Code={}, Msg={}", req_id, error_code, error_msg);
      let (err_code_field, err_msg_field, is_historical) = match sub_state {
        MarketSubscription::TickData(s) => (&mut s.error_code, &mut s.error_message, false),
        MarketSubscription::RealTimeBars(s) => (&mut s.error_code, &mut s.error_message, false),
        MarketSubscription::TickByTick(s) => (&mut s.error_code, &mut s.error_message, false),
        MarketSubscription::MarketDepth(s) => (&mut s.error_code, &mut s.error_message, false),
        MarketSubscription::HistoricalData(s) => (&mut s.error_code, &mut s.error_message, true),
      };

      *err_code_field = Some(error_code);
      *err_msg_field = Some(error_msg);

      // If it's a historical request error, mark end and notify waiter
      if is_historical {
        if let MarketSubscription::HistoricalData(s) = sub_state {
          s.end_received = true;
        }
        self.request_cond.notify_all();
      }
      // For streaming requests, the error is stored, but doesn't stop the subscription state yet.
      // User needs to decide whether to cancel based on the error.
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
        _ => trace!("Unhandled tick_type {} in tick_price", tick_type),
      }
      // TODO: Store attrib if needed
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

  fn tick_efp(&self, req_id: i32, tick_type: i32, basis_points: f64, formatted_basis_points: &str,
              implied_futures_price: f64, hold_days: i32, future_last_trade_date: &str,
              dividend_impact: f64, dividends_to_last_trade_date: f64) {
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
      // If this was a snapshot request, maybe notify a waiter?
      // Or just mark the state.
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
    } else if let Some(MarketSubscription::MarketDepth(state)) = subs.get_mut(&req_id) {
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
        time: Utc.timestamp_opt(time, 0).single().unwrap_or(Utc::now()), // Convert Unix timestamp
        open, high, low, close,
        volume: volume as i64, // Convert f64 back to i64
        wap,
        count,
      };
      state.latest_bar = Some(bar);
      // self.notify_observers(req_id); // Or specific bar observer
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
      state.latest_tick = Some(data);
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
      state.latest_tick = Some(TickByTickData::BidAsk { time, bid_price, ask_price, bid_size, ask_size, tick_attrib_bid_ask });
      // self.notify_observers(req_id);
    } else {
      // warn!("Received tick_by_tick_bid_ask for unknown or non-TBT subscription ID: {}", req_id);
    }
  }

  fn tick_by_tick_mid_point(&self, req_id: i32, time: i64, mid_point: f64) {
    trace!("Handler: TickByTick MidPoint: ID={}, Time={}, MidPt={}", req_id, time, mid_point);
    let mut subs = self.subscriptions.lock();
    if let Some(MarketSubscription::TickByTick(state)) = subs.get_mut(&req_id) {
      state.latest_tick = Some(TickByTickData::MidPoint { time, mid_point });
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

  fn scanner_data(&self, req_id: i32, rank: i32, contract_details: &ContractDetails, distance: &str,
                  benchmark: &str, projection: &str, legs_str: Option<&str>) {
    trace!("Handler: Scanner Data Row: ID={}, Rank={}, Symbol={}", req_id, rank, contract_details.contract.symbol);
    // Needs state management if scanner results are tracked.
  }

  fn scanner_data_end(&self, req_id: i32) {
    debug!("Handler: Scanner Data End: ID={}", req_id);
    // Signal completion if scanner request state is managed.
  }

  // --- Errors ---
  fn market_data_error(&self, req_id: i32, error_code: i32, error_msg: &str) {
    // This is called from the central error processor now
    // We delegate the actual state update to the manager's internal handle_error
    self.handle_error(req_id, error_code, error_msg.to_string());
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
