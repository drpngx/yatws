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
use log::{debug, info, trace, warn};


// --- DataFundamentalsManager ---

#[derive(Debug, Default)]
struct FundamentalsRequestState {
  // Field for FundamentalData
  fundamental_data: Option<String>, // Stores the XML/text report
  request_complete: bool, // Flagged when data is received

  // General error fields
  error_code: Option<i32>,
  error_message: Option<String>,
}

pub struct DataFundamentalsManager {
  message_broker: Arc<MessageBroker>,
  request_states: Mutex<HashMap<i32, FundamentalsRequestState>>,
  request_cond: Condvar,
}

impl DataFundamentalsManager {
  pub fn new(message_broker: Arc<MessageBroker>) -> Arc<Self> {
    Arc::new(DataFundamentalsManager {
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
    F: Fn(&FundamentalsRequestState) -> Option<Result<R, IBKRError>>, // Returns Some(result) when complete
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
      states.insert(req_id, FundamentalsRequestState::default());
      debug!("Fundamental data request state initialized for ReqID: {}", req_id);
    }

    self.message_broker.send_message(&request_msg)?;

    // Wait for completion (data arrival or error)
    let timeout = Duration::from_secs(60); // Fundamental data can take time
    self.wait_for_completion(req_id, timeout, |state| {
      if state.request_complete {
        match &state.fundamental_data {
          Some(data) => Some(Ok(data.clone())),
          None => Some(Err(IBKRError::InternalError(format!(
            "Fundamental data request {} completed but no data found", req_id
          )))),
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

  // --- WSH Public API Methods (Non-Blocking Streaming) ---

  /// Requests Wall Street Horizon metadata. Non-blocking. Returns req_id.
  /// The metadata is delivered via the `wsh_meta_data` handler method.
  pub fn request_wsh_meta_data(&self) -> Result<i32, IBKRError> {
    info!("Requesting WSH metadata");
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
  pub fn request_wsh_event_data(&self, wsh_event_data: &WshEventDataRequest) -> Result<i32, IBKRError> {
    info!("Requesting WSH event data: Filters={:?}", wsh_event_data);
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
    self.message_broker.send_message(&request_msg)?; // Added semicolon
    // No state to clean up in the manager
    Ok(()) // Added Ok(())
  }


  // --- Internal error handling (called by the trait method) ---
  fn _internal_handle_error(&self, req_id: i32, code: ClientErrorCode, msg: &str) {
    if req_id <= 0 { return; } // Ignore general errors not tied to a request

    let mut states = self.request_states.lock();
    if let Some(state) = states.get_mut(&req_id) {
      warn!("API Error received for fundamental data request {}: Code={:?}, Msg={}", req_id, code, msg);
      state.error_code = Some(code as i32); // Store integer code
      state.error_message = Some(msg.to_string()); // Store owned string
      state.request_complete = true; // Mark as complete on error
      // Signal potentially waiting thread
      self.request_cond.notify_all();
    } else {
      // Error might be for a request that already completed, cancelled, or timed out.
      trace!("Received error for unknown or completed fundamental data request ID: {}", req_id);
    }
  }
}

// --- Implement FinancialDataHandler Trait ---

impl FinancialDataHandler for DataFundamentalsManager {
  fn fundamental_data(&self, req_id: i32, data: &str) {
    debug!("Handler: Fundamental Data Received: ReqID={}, Data length={}", req_id, data.len());
    let mut states = self.request_states.lock();
    if let Some(state) = states.get_mut(&req_id) {
      state.fundamental_data = Some(data.to_string());
      state.request_complete = true; // Mark as complete now that data arrived
      info!("Fundamental data received for request {}. Notifying waiter.", req_id);
      self.request_cond.notify_all(); // Signal waiting thread
    } else {
      warn!("Received fundamental data for unknown or completed request ID: {}", req_id);
    }
  }

  // --- WSH Handler Methods ---
  fn wsh_meta_data(&self, req_id: i32, data_json: &str) {
    // use data_wsh.
    info!("Handler: Received WSH Meta Data: ReqID={}, Len={}. Handler implementation should parse JSON.", req_id, data_json.len());
  }

  fn wsh_event_data(&self, req_id: i32, data_json: &str) {
    info!("Handler: Received WSH Event Data: ReqID={}, Len={}. Handler implementation should parse JSON.", req_id, data_json.len());
  }

  /// Handles errors related to financial data requests.
  /// This is the implementation of the trait method.
  fn handle_error(&self, req_id: i32, code: ClientErrorCode, msg: &str) {
    // Delegate to the internal helper
    self._internal_handle_error(req_id, code, msg);
  }
}
