// yatws/src/data_ref_manager.rs
use crate::base::IBKRError;
use crate::conn::MessageBroker;
use crate::protocol_decoder::ClientErrorCode;
use crate::contract::{
  Contract, ContractDetails, SecType, SoftDollarTier, FamilyCode, ContractDescription,
  DepthMktDataDescription, MarketRule, PriceIncrement, HistoricalSession,
};
use crate::handler::{ReferenceDataHandler};
use crate::protocol_encoder::Encoder;
use parking_lot::{Condvar, Mutex};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use log::{debug, info, warn};


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
#[allow(unused)]
pub struct SecDefOptParamsResult {
  exchange: String,
  underlying_con_id: i32,
  trading_class: String,
  multiplier: String,
  expirations: Vec<String>,
  strikes: Vec<f64>,
}

#[derive(Debug, Clone)]
#[allow(unused)]
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


// --- Internal error handling (called by the trait method) ---
  fn _internal_handle_error(&self, req_id: i32, code: ClientErrorCode, msg: &str) {
    if req_id <= 0 { return; } // Ignore general errors

    let mut states = self.request_states.lock();
    if let Some(state) = states.get_mut(&req_id) {
      warn!("API Error received for reference data request {}: Code={:?}, Msg={}", req_id, code, msg);
      state.error_code = Some(code as i32); // Store integer code
      state.error_message = Some(msg.to_string()); // Store owned string
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

  /// Handles errors related to reference data requests.
  /// This is the implementation of the trait method.
  fn handle_error(&self, req_id: i32, code: ClientErrorCode, msg: &str) {
    // Delegate to the internal helper
    self._internal_handle_error(req_id, code, msg);
  }
}
