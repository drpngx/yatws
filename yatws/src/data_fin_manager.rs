// yatws/src/data_fin_manager.rs
use crate::base::IBKRError;
use crate::conn::MessageBroker;
use crate::protocol_decoder::ClientErrorCode; // Added import
use crate::contract::Contract;
use crate::data_wsh::WshEventDataRequest;
use crate::handler::{FinancialDataHandler};
use crate::protocol_encoder::Encoder;
use parking_lot::{Condvar, Mutex};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use log::{debug, info, trace, error, warn};
use serde_json; // Added for WSH event parsing

// --- DataFundamentalsManager ---

#[derive(Debug, Clone, Copy, Default)]
enum RequestType {
  #[default]
  Fundamental,
  WshMetaData,
  WshEventData,
}

#[derive(Debug, Default)]
struct RequestState {
  request_type: RequestType,

  // Field for FundamentalData
  fundamental_data: Option<String>, // Stores the XML/text report

  // Fields for WSH Meta Data
  wsh_meta_data_json: Option<String>, // Stores JSON for WSH metadata

  // Fields for WSH Event Data
  wsh_event_data_json_list: Vec<String>, // Stores JSON strings for WSH events
  wsh_event_expected_count: Option<usize>, // Expected number of events for WshEventData

  // Common fields
  request_complete: bool, // Flagged when data is received or expected count met
  error_code: Option<i32>,
  error_message: Option<String>,
}

pub struct DataFundamentalsManager {
  message_broker: Arc<MessageBroker>,
  request_states: Mutex<HashMap<i32, RequestState>>,
  request_cond: Condvar,
  wsh_global_lock: Mutex<()>, // Ensures one WSH operation (meta or event) at a time for get_wsh_events
  wsh_meta_data_cache: Mutex<Option<String>>, // Cache for successfully fetched WSH metadata
}

impl DataFundamentalsManager {
  pub(crate) fn new(message_broker: Arc<MessageBroker>) -> Arc<Self> {
    Arc::new(DataFundamentalsManager {
      message_broker,
      request_states: Mutex::new(HashMap::new()),
      request_cond: Condvar::new(),
      wsh_global_lock: Mutex::new(()),
      wsh_meta_data_cache: Mutex::new(None),
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
    F: Fn(&RequestState) -> Option<Result<R, IBKRError>>, // Returns Some(result) when complete
  {
    let start_time = std::time::Instant::now();
    let mut guard = self.request_states.lock();

    loop {
      // 1. Check if complete *before* waiting
      if let Some(state) = guard.get(&req_id) {
        // Prioritize checking for an API error first
        if let (Some(code), Some(msg)) = (state.error_code, state.error_message.as_ref()) {
          let err = IBKRError::ApiError(code, msg.clone());
          guard.remove(&req_id); // Clean up state
          return Err(err);
        }
        // Then check the custom completion logic
        match is_complete_check(state) {
          Some(Ok(result)) => {
            guard.remove(&req_id); // Clean up state
            return Ok(result);
          },
          Some(Err(e)) => { // This allows is_complete_check to return an error directly
            guard.remove(&req_id); // Clean up state
            return Err(e);
          },
          None => { /* Not complete yet */ }
        }
      } else {
        // State missing, could happen if request was never properly initialized or already cleaned up
        return Err(IBKRError::InternalError(format!("Request state for {} missing or already cleaned up during wait", req_id)));
      }

      // 2. Calculate remaining timeout
      let elapsed = start_time.elapsed();
      if elapsed >= timeout {
        guard.remove(&req_id); // Clean up state on timeout
        return Err(IBKRError::Timeout(format!("Fundamental data request {} timed out after {:?}", req_id, timeout)));
      }
      let remaining_timeout = timeout - elapsed;

      // 3. Wait
      let wait_result = self.request_cond.wait_for(&mut guard, remaining_timeout);

      // 4. Handle timeout after wait
      if wait_result.timed_out() {
        // Re-check completion status one last time after timeout
        if let Some(state) = guard.get(&req_id) {
          match is_complete_check(state) {
            Some(Ok(result)) => { guard.remove(&req_id); return Ok(result); },
            Some(Err(e)) => { guard.remove(&req_id); return Err(e); },
            None => {} // Still not complete
          }
          if let (Some(code), Some(msg)) = (state.error_code, state.error_message.as_ref()) {
            let err = IBKRError::ApiError(code, msg.clone());
            guard.remove(&req_id); return Err(err);
          }
        }
        // If still not complete after timeout + final check
        guard.remove(&req_id); // Clean up state
        return Err(IBKRError::Timeout(format!("Fundamental data request {} timed out after wait", req_id)));
      }
      // If not timed out, loop continues to re-check state
    }
  }

  // --- Public API Methods ---

  /// Requests and returns fundamental data (XML/text) for a contract.
  /// Blocks until the data is received or timeout.
  pub fn get_fundamental_data(
    &self,
    contract: &Contract,
    report_type: &str, // e.g., "ReportsFinSummary", "ReportSnapshot", "ReportsFinStatements", "RESC", "CalendarReport"
    fundamental_data_options: &[(String, String)], // Added in API v???, check docs/min_server_ver
  ) -> Result<String, IBKRError> {
    info!("Requesting fundamental data: Contract={}, ReportType={}", contract.symbol, report_type);
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    // Check if options are supported (optional refinement)
    // if server_version < SOME_VERSION_FOR_FUND_OPTIONS && !fundamental_data_options.is_empty() {
    //     warn!("Fundamental data options provided but server version might not support them.");
    // }

    let request_msg = encoder.encode_request_fundamental_data(
      req_id, contract, report_type, fundamental_data_options
    )?;

    // Initialize state
    {
      let mut states = self.request_states.lock();
      if states.contains_key(&req_id) {
        return Err(IBKRError::DuplicateRequestId(req_id));
      }
      states.insert(req_id, RequestState { request_type: RequestType::Fundamental, ..Default::default() });
      debug!("Fundamental data request state initialized for ReqID: {}", req_id);
    }

    self.message_broker.send_message(&request_msg)?;

    // Wait for completion (data arrival or error)
    let timeout = Duration::from_secs(60); // Fundamental data can take time
    self.wait_for_completion(req_id, timeout, |state| {
      // Error case is handled by wait_for_completion's primary error check now
      if state.request_complete {
        match &state.fundamental_data {
          Some(data) => Some(Ok(data.clone())),
          None => {
            // If request_complete is true but no data and no error, it's an issue.
            // However, error_code check in wait_for_completion should catch API errors.
            // This path implies completion without data and without a reported API error.
            Some(Err(IBKRError::InternalError(format!(
              "Fundamental data request {} marked complete but no data found and no API error reported", req_id
            ))))
          }
        }
      } else {
        None // Not complete yet
      }
    })
  }

  /// Cancels an ongoing fundamental data request.
  pub fn cancel_fundamental_data(&self, req_id: i32) -> Result<(), IBKRError> {
    info!("Cancelling fundamental data request: ReqID={}", req_id);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_fundamental_data(req_id)?;

    // Send cancel message first
    self.message_broker.send_message(&request_msg)?;

    // Update state and notify any waiter
    {
      let mut states = self.request_states.lock();
      if let Some(state) = states.get_mut(&req_id) {
        warn!("Fundamental data request {} cancelled by user.", req_id);
        state.error_code = Some(-1); // Custom code for user cancel
        state.error_message = Some("Request cancelled by user".to_string());
        state.request_complete = true; // Mark as complete to unblock waiter
        self.request_cond.notify_all();
      } else {
        warn!("Attempted to cancel fundamental data for unknown or already completed ReqID: {}", req_id);
        // No state to remove or notify if it's already gone
      }
      // The waiter loop will handle removing the state upon seeing completion/error.
    }
    Ok(())
  }

  // --- WSH Public API Methods ---

  /// Requests Wall Street Horizon metadata. Non-blocking. Returns req_id.
  /// The metadata is delivered via the `wsh_meta_data` handler method.
  /// Note: This non-blocking call does not use the `wsh_global_lock`.
  /// Users must ensure sequencing if mixing with `get_wsh_events` or other WSH calls.
  pub fn request_wsh_meta_data(&self) -> Result<i32, IBKRError> {
    info!("Requesting WSH metadata (non-blocking)");
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_request_wsh_meta_data(req_id)?;
    // No state needed in the manager for simple non-blocking request
    self.message_broker.send_message(&request_msg)?;
    Ok(req_id)
  }

  /// Cancels an ongoing WSH metadata request.
  pub fn cancel_wsh_meta_data(&self, req_id: i32) -> Result<(), IBKRError> {
    info!("Cancelling WSH metadata request: ReqID={}", req_id);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_wsh_meta_data(req_id)?;
    self.message_broker.send_message(&request_msg)
    // No state to clean up in the manager
  }

  /// Requests Wall Street Horizon event data streaming. Non-blocking. Returns req_id.
  /// Events are delivered individually via the `wsh_event_data` handler method.
  /// Filters are applied based on the `wsh_event_data` parameter and server version support.
  /// Note: This non-blocking call does not use the `wsh_global_lock`.
  /// Users must ensure sequencing if mixing with `get_wsh_events` or other WSH calls,
  /// and are responsible for ensuring metadata has been fetched if required by TWS.
  pub fn request_wsh_event_data(&self, wsh_event_data: &WshEventDataRequest) -> Result<i32, IBKRError> {
    info!("Requesting WSH event data (non-blocking): Filters={:?}", wsh_event_data);
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    // Encoding function now handles version checks based on the request content
    let request_msg = encoder.encode_request_wsh_event_data(req_id, wsh_event_data)?;

    // No state needed in the manager for simple non-blocking request
    self.message_broker.send_message(&request_msg)?;
    Ok(req_id)
  }

  /// Cancels an ongoing WSH event data request.
  pub fn cancel_wsh_event_data(&self, req_id: i32) -> Result<(), IBKRError> {
    info!("Cancelling WSH event data request: ReqID={}", req_id);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_wsh_event_data(req_id)?;
    self.message_broker.send_message(&request_msg)?;
    // No state to clean up in the manager
    Ok(())
  }

  /// Requests and returns WSH (Wall Street Horizon) event data.
  /// This is a blocking call that first ensures WSH metadata is available (fetching if necessary)
  /// and then requests the specified event data, waiting for the expected number of events or a timeout.
  ///
  /// # Arguments
  /// * `request_details` - Specifies the WSH event data to request, including `con_id` and `total_limit`.
  ///                       `total_limit` must be a positive number.
  /// * `timeout` - Overall timeout for the entire operation (metadata + event data).
  ///
  /// # Errors
  /// Returns `IBKRError` if the request fails, times out, `total_limit` is invalid, or parsing fails.
  pub fn get_wsh_events(
    &self,
    request_details: &WshEventDataRequest,
    timeout: Duration,
  ) -> Result<Vec<crate::data_wsh::WshEventData>, IBKRError> {
    let _lock_guard = self.wsh_global_lock.lock(); // Ensure only one WSH operation at a time via this manager
    let overall_start_time = std::time::Instant::now();

    // 1. Ensure WSH Metadata is available
    let mut meta_cache = self.wsh_meta_data_cache.lock();
    if meta_cache.is_none() {
      info!("WSH metadata not cached. Requesting...");
      let req_id_meta = self.message_broker.next_request_id();
      let server_version = self.message_broker.get_server_version()?;
      let encoder = Encoder::new(server_version);
      let request_msg = encoder.encode_request_wsh_meta_data(req_id_meta)?;

      {
        let mut states = self.request_states.lock();
        if states.contains_key(&req_id_meta) {
          return Err(IBKRError::DuplicateRequestId(req_id_meta));
        }
        states.insert(req_id_meta, RequestState { request_type: RequestType::WshMetaData, ..Default::default() });
      }

      self.message_broker.send_message(&request_msg)?;

      let time_spent = overall_start_time.elapsed();
      if time_spent >= timeout { return Err(IBKRError::Timeout("Timeout before WSH metadata could be fetched".to_string())); }
      let meta_timeout = timeout - time_spent;

      match self.wait_for_completion(req_id_meta, meta_timeout, |state| {
        if state.request_complete { // Error handled by wait_for_completion's main check
          state.wsh_meta_data_json.as_ref().map(|json| Ok(json.clone()))
            .or_else(|| Some(Err(IBKRError::InternalError("WSH metadata request complete but no data".to_string()))))
        } else { None }
      }) {
        Ok(meta_json) => {
          info!("WSH metadata successfully fetched and cached.");
          *meta_cache = Some(meta_json);
        }
        Err(e) => {
          error!("Failed to fetch WSH metadata: {:?}", e);
          return Err(e);
        }
      }
    } else {
      info!("Using cached WSH metadata.");
    }
    // Release meta_cache lock explicitly before event data request
    drop(meta_cache);

    // 2. Request WSH Event Data
    let expected_count = match request_details.total_limit {
        Some(limit) if limit > 0 => limit as usize,
        _ => return Err(IBKRError::InvalidParameter("WshEventDataRequest.total_limit must be positive for get_wsh_events".to_string())),
    };

    info!("Requesting {} WSH events for con_id: {:?}", expected_count, request_details.con_id);
    let req_id_event = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let event_request_msg = encoder.encode_request_wsh_event_data(req_id_event, request_details)?;

    {
      let mut states = self.request_states.lock();
      if states.contains_key(&req_id_event) {
        return Err(IBKRError::DuplicateRequestId(req_id_event));
      }
      states.insert(req_id_event, RequestState {
        request_type: RequestType::WshEventData,
        wsh_event_expected_count: Some(expected_count),
        ..Default::default()
      });
    }

    self.message_broker.send_message(&event_request_msg)?;

    let time_spent = overall_start_time.elapsed();
    if time_spent >= timeout { return Err(IBKRError::Timeout("Timeout before WSH event data could be fully fetched".to_string())); }
    let event_timeout = timeout - time_spent;

    let json_event_list = self.wait_for_completion(req_id_event, event_timeout, |state| {
       // Error handled by wait_for_completion's main check
      if state.request_complete { // True if expected count reached OR error occurred
          // If an error occurred, wait_for_completion would have returned Err already.
          // So, if we are here and request_complete is true, it means count was met.
          Some(Ok(state.wsh_event_data_json_list.clone()))
      } else if state.wsh_event_data_json_list.len() >= state.wsh_event_expected_count.unwrap_or(usize::MAX) {
          // This case handles if the handler didn't set request_complete but count is met.
          // The handler *should* set request_complete. This is a fallback.
          warn!("WSH event count met for req_id {} but request_complete flag not set by handler.", req_id_event);
          Some(Ok(state.wsh_event_data_json_list.clone()))
      }
      else {
          None // Not complete yet
      }
    })?;

    // Release global WSH lock before parsing
    // _lock_guard is dropped automatically here when it goes out of scope.

    // 3. Parse JSON strings into WshEventData structs
    let mut parsed_events = Vec::with_capacity(json_event_list.len());
    for json_str in json_event_list {
      match serde_json::from_str::<crate::data_wsh::WshEventData>(&json_str) {
        Ok(event) => parsed_events.push(event),
        Err(e) => {
          warn!("Failed to parse WSH event JSON: {}. JSON: {}", e, json_str);
          // Return an error if any single event fails to parse.
          // The wsh_global_lock is already released.
          return Err(IBKRError::ParseError(std::format!("{}: {}", e.to_string(), json_str)));
        }
      }
    }
    info!("Successfully fetched and parsed {} WSH events.", parsed_events.len());
    Ok(parsed_events)
  }

  // --- Internal error handling (called by the trait method) ---
  fn _internal_handle_error(&self, req_id: i32, code: ClientErrorCode, msg: &str) {
    if req_id <= 0 { return; } // Ignore general errors not tied to a request

    let mut states = self.request_states.lock();
    if let Some(state) = states.get_mut(&req_id) {
      warn!("API Error received for request {}: Code={:?}, Msg={}", req_id, code, msg);
      state.error_code = Some(code as i32); // Store integer code
      state.error_message = Some(msg.to_string()); // Store owned string
      state.request_complete = true; // Mark as complete on error
      // Signal potentially waiting thread
      self.request_cond.notify_all();
    } else {
      // Error might be for a request that already completed, cancelled, or timed out.
      trace!("Received error for unknown or completed request ID: {}", req_id);
    }
  }
}

// --- Implement FinancialDataHandler Trait ---

impl FinancialDataHandler for DataFundamentalsManager {
  fn fundamental_data(&self, req_id: i32, data: &str) {
    debug!("Handler: Fundamental Data Received: ReqID={}, Data length={}", req_id, data.len());
    let mut states = self.request_states.lock();
    if let Some(state) = states.get_mut(&req_id) {
      if matches!(state.request_type, RequestType::Fundamental) {
        state.fundamental_data = Some(data.to_string());
        state.request_complete = true; // Mark as complete now that data arrived
        info!("Fundamental data received for request {}. Notifying waiter.", req_id);
        self.request_cond.notify_all(); // Signal waiting thread
      } else {
        warn!("Received fundamental_data for ReqID {} with unexpected request_type {:?}", req_id, state.request_type);
      }
    } else {
      warn!("Received fundamental_data for unknown or completed request ID: {}", req_id);
    }
  }

  // --- WSH Handler Methods ---
  fn wsh_meta_data(&self, req_id: i32, data_json: &str) {
    info!("Handler: Received WSH Meta Data: ReqID={}, Len={}", req_id, data_json.len());
    let mut states = self.request_states.lock();
    if let Some(state) = states.get_mut(&req_id) {
      if matches!(state.request_type, RequestType::WshMetaData) {
        state.wsh_meta_data_json = Some(data_json.to_string());
        state.request_complete = true;
        info!("WSH metadata received for request {}. Notifying waiter.", req_id);
        self.request_cond.notify_all();
      } else {
         warn!("Received wsh_meta_data for ReqID {} with unexpected request_type {:?}", req_id, state.request_type);
      }
    } else {
      warn!("Received wsh_meta_data for unknown or completed request ID: {}", req_id);
    }
  }

  fn wsh_event_data(&self, req_id: i32, data_json: &str) {
    info!("Handler: Received WSH Event Data: ReqID={}, Len={}", req_id, data_json.len());
    let mut states = self.request_states.lock();
    if let Some(state) = states.get_mut(&req_id) {
      if matches!(state.request_type, RequestType::WshEventData) {
        state.wsh_event_data_json_list.push(data_json.to_string());
        debug!("WSH event data added for ReqID {}. Current count: {}/{}",
                 req_id, state.wsh_event_data_json_list.len(), state.wsh_event_expected_count.unwrap_or(0));
        if let Some(expected_count) = state.wsh_event_expected_count {
          if state.wsh_event_data_json_list.len() >= expected_count {
            state.request_complete = true;
            info!("All expected WSH events ({}) received for request {}. Notifying waiter.", expected_count, req_id);
            self.request_cond.notify_all();
          }
        }
        // If no expected_count, it's streaming, rely on timeout or explicit cancel.
        // For get_wsh_events, expected_count is always Some(>0).
      } else {
        warn!("Received wsh_event_data for ReqID {} with unexpected request_type {:?}", req_id, state.request_type);
      }
    } else {
      warn!("Received wsh_event_data for unknown or completed request ID: {}", req_id);
    }
  }

  /// Handles errors related to financial data requests.
  /// This is the implementation of the trait method.
  fn handle_error(&self, req_id: i32, code: ClientErrorCode, msg: &str) {
    // Delegate to the internal helper
    self._internal_handle_error(req_id, code, msg);
  }
}
