// yatws/src/data_fin_manager.rs

//! Manages requests for financial fundamental data and Wall Street Horizon (WSH) event data.
//!
//! The `DataFundamentalsManager` provides methods to:
//! -   Fetch company fundamental data (e.g., financial statements, analyst estimates, reports)
//!     via `get_fundamental_data()`. This is a blocking call.
//! -   Fetch Wall Street Horizon (WSH) corporate event metadata and specific event data.
//!     -   `request_wsh_meta_data()`: Non-blocking request for WSH metadata.
//!     -   `request_wsh_event_data()`: Non-blocking request for WSH event data stream.
//!     -   `get_wsh_events()`: Blocking call to fetch specific WSH events, ensuring metadata is available.
//!
//! # Fundamental Data Reports
//!
//! The `get_fundamental_data()` method accepts a `report_type` string. Common types include:
//! -   `ReportsFinSummary`: Financial summary.
//! -   `ReportSnapshot`: Company overview, ratios, estimates.
//! -   `ReportsFinStatements`: Detailed financial statements (Income, Balance Sheet, Cash Flow).
//! -   `RESC`: Analyst estimates.
//! -   `CalendarReport`: Corporate calendar events (deprecated in favor of WSH).
//!
//! The data is returned as an XML string, which then needs to be parsed (e.g., using `parse_fundamental_xml` from the library).
//!
//! # Wall Street Horizon (WSH) Data
//!
//! WSH provides detailed corporate event data. Accessing it typically involves:
//! 1.  Fetching metadata (describes available event types, filters, etc.). This is often done once
//!     or periodically. `get_wsh_events()` handles this automatically if metadata isn't cached.
//! 2.  Requesting specific event data using filters (e.g., by conId, event type, date range).
//!
//! # Example: Getting Financial Summary
//!
//! ```no_run
//! use yatws::{IBKRClient, IBKRError, contract::Contract, data::{FundamentalReportType, ParsedFundamentalData}, parse_fundamental_xml};
//!
//! fn main() -> Result<(), IBKRError> {
//!     let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
//!     let fin_data_mgr = client.data_financials();
//!
//!     let contract = Contract::stock("AAPL");
//!
//!     match fin_data_mgr.get_fundamental_data(&contract, FundamentalReportType::ReportsFinSummary, &[]) {
//!         Ok(xml_data) => {
//!             println!("Received 'ReportsFinSummary' XML for AAPL (length {}).", xml_data.len());
//!             // Example of parsing (add error handling for production)
//!             match parse_fundamental_xml(&xml_data, FundamentalReportType::ReportsFinSummary) { // Ensure parser also uses the enum correctly
//!                 Ok(ParsedFundamentalData::FinancialSummary(summary)) => {
//!                     println!("Parsed Summary: {} EPS records.", summary.eps_records.len());
//!                 }
//!                 Ok(_) => eprintln!("Parsed XML but got unexpected data type."),
//!                 Err(e) => eprintln!("Failed to parse XML: {:?}", e),
//!             }
//!         }
//!         Err(e) => eprintln!("Error getting financial summary: {:?}", e),
//!     }
//!     Ok(())
//! }
//! ```

use crate::base::IBKRError;
use crate::conn::MessageBroker;
use crate::protocol_decoder::ClientErrorCode; // Added import
use crate::contract::Contract;
use crate::data_wsh::WshEventDataRequest;
use crate::data::FundamentalReportType;
use crate::handler::{FinancialDataHandler};
use crate::protocol_encoder::Encoder;
use parking_lot::{Condvar, Mutex};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use log::{debug, info, trace, error, warn};
use serde_json; // Added for WSH event parsing

// --- DataFundamentalsManager ---

/// Enum identifying the type of a pending request within `DataFundamentalsManager`.
#[derive(Debug, Clone, Copy, Default)]
enum RequestType {
  /// Request for standard fundamental data (XML reports).
  #[default]
  Fundamental,
  /// Request for Wall Street Horizon (WSH) metadata.
  WshMetaData,
  /// Request for Wall Street Horizon (WSH) event data.
  WshEventData,
}

/// Internal state for tracking pending fundamental or WSH data requests.
#[derive(Debug, Default)]
struct RequestState {
  /// The type of this request.
  request_type: RequestType,

  /// Stores the XML/text report for `Fundamental` requests.
  fundamental_data: Option<String>,

  /// Stores the JSON string for `WshMetaData` requests.
  wsh_meta_data_json: Option<String>,

  /// Stores a list of JSON strings, each representing a WSH event, for `WshEventData` requests.
  wsh_event_data_json_list: Vec<String>,
  /// For `WshEventData` requests made via `get_wsh_events`, this stores the expected number of events.
  wsh_event_expected_count: Option<usize>,

  /// Flag indicating if the request is considered complete (data received, error occurred, or expected count met).
  request_complete: bool,
  /// Stores an error code if an API error occurred for this request.
  error_code: Option<i32>,
  /// Stores an error message if an API error occurred for this request.
  error_message: Option<String>,
}

/// Manages requests for company fundamental data and Wall Street Horizon (WSH) event data.
///
/// Accessed via [`IBKRClient::data_financials()`](crate::IBKRClient::data_financials()).
///
/// See the [module-level documentation](index.html) for more details and examples.
pub struct DataFundamentalsManager {
  message_broker: Arc<MessageBroker>,
  request_states: Mutex<HashMap<i32, RequestState>>,
  request_cond: Condvar,
  wsh_global_lock: Mutex<()>, // Ensures one WSH operation (meta or event) at a time for get_wsh_events
  wsh_meta_data_cache: Mutex<Option<String>>, // Cache for successfully fetched WSH metadata
}

impl DataFundamentalsManager {
  /// Creates a new `DataFundamentalsManager`.
  ///
  /// This is typically called internally when an `IBKRClient` is created.
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

  /// Internal helper for blocking calls that wait for a fundamental or WSH data request to complete.
  ///
  /// It waits until the `is_complete_check` closure indicates completion, an API error occurs,
  /// or the timeout is reached.
  ///
  /// # Arguments
  /// * `req_id` - The request ID.
  /// * `timeout` - Maximum duration to wait.
  /// * `is_complete_check` - A closure `Fn(&RequestState) -> Option<Result<R, IBKRError>>`.
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

  /// Requests and returns fundamental data for a contract. This is a blocking call.
  ///
  /// The data is typically returned as an XML string by TWS, which can then be parsed
  /// using functions like `yatws::parse_fundamental_xml`.
  ///
  /// # Arguments
  /// * `contract` - The [`Contract`] for which to request fundamental data.
  /// * `report_type` - A [`FundamentalReportType`] enum specifying the type of report.
  /// * `fundamental_data_options` - A list of `(tag, value)` pairs for additional options (rarely used).
  ///
  /// # Returns
  /// An XML `String` containing the fundamental data report.
  ///
  /// # Errors
  /// Returns `IBKRError::Timeout` if the data is not received within the timeout.
  /// Returns other `IBKRError` variants for communication or encoding issues.
  ///
  /// # Example from `gen_goldens.rs`:
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError, contract::Contract};
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// # let fin_data_mgr = client.data_financials();
  /// # use data::FundamentalReportType;
  /// let contract = Contract::stock("AAPL");
  /// let xml_data = fin_data_mgr.get_fundamental_data(
  ///     &contract,
  ///     FundamentalReportType::ReportsFinSummary, // Request Financial Summary
  ///     &[]
  /// )?;
  /// println!("Received Financial Summary XML for AAPL (length {}).", xml_data.len());
  /// // ... (parse xml_data using yatws::parse_fundamental_xml) ...
  /// # Ok(())
  /// # }
  /// ```
  pub fn get_fundamental_data(
    &self,
    contract: &Contract,
    report_type: FundamentalReportType,
    fundamental_data_options: &[(String, String)], // Added in API v???, check docs/min_server_ver
  ) -> Result<String, IBKRError> {
    let report_type_str = report_type.as_tws_str();
    info!("Requesting fundamental data: Contract={}, ReportType={}", contract.symbol, report_type_str);
    let req_id = self.message_broker.next_request_id();
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);

    // Check if options are supported (optional refinement)
    // if server_version < SOME_VERSION_FOR_FUND_OPTIONS && !fundamental_data_options.is_empty() {
    //     warn!("Fundamental data options provided but server version might not support them.");
    // }

    let request_msg = encoder.encode_request_fundamental_data(
      req_id, contract, report_type_str, fundamental_data_options
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
  ///
  /// # Arguments
  /// * `req_id` - The request ID obtained from `get_fundamental_data()`.
  ///
  /// # Errors
  /// Returns `IBKRError` if the cancellation message cannot be encoded or sent.
  /// It logs a warning if the `req_id` is not found (e.g., already completed or cancelled).
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

  /// Requests Wall Street Horizon (WSH) metadata. This is a non-blocking call.
  ///
  /// The WSH metadata (a JSON string describing available event types, filters, etc.)
  /// is delivered asynchronously via the `wsh_meta_data` method of the `FinancialDataHandler` trait.
  ///
  /// This method does not use the internal `wsh_global_lock`, so users must ensure
  /// proper sequencing if mixing this call with `get_wsh_events()` or other WSH operations
  /// that might depend on this metadata. `get_wsh_events()` handles metadata fetching internally.
  ///
  /// # Returns
  /// The request ID (`i32`) assigned to this WSH metadata request.
  ///
  /// # Errors
  /// Returns `IBKRError` if the request cannot be encoded or sent.
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

  /// Cancels an ongoing Wall Street Horizon (WSH) metadata request.
  ///
  /// # Arguments
  /// * `req_id` - The request ID obtained from `request_wsh_meta_data()`.
  ///
  /// # Errors
  /// Returns `IBKRError` if the cancellation message cannot be encoded or sent.
  pub fn cancel_wsh_meta_data(&self, req_id: i32) -> Result<(), IBKRError> {
    info!("Cancelling WSH metadata request: ReqID={}", req_id);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_wsh_meta_data(req_id)?;
    self.message_broker.send_message(&request_msg)
    // No state to clean up in the manager
  }

  /// Requests a stream of Wall Street Horizon (WSH) event data. This is a non-blocking call.
  ///
  /// Individual WSH events (JSON strings) are delivered asynchronously via the `wsh_event_data`
  /// method of the `FinancialDataHandler` trait.
  ///
  /// Filters for the event data (e.g., by conId, event type, date range) are specified
  /// in the `wsh_event_data` argument.
  ///
  /// This method does not use the internal `wsh_global_lock`. Users must ensure
  /// proper sequencing and that WSH metadata (if required by TWS for the specific filters)
  /// has been fetched beforehand if not using `get_wsh_events()`.
  ///
  /// # Arguments
  /// * `wsh_event_data` - A [`WshEventDataRequest`] struct specifying the filters for the event data.
  ///
  /// # Returns
  /// The request ID (`i32`) assigned to this WSH event data request.
  ///
  /// # Errors
  /// Returns `IBKRError` if the request cannot be encoded (e.g., due to unsupported filters
  /// for the TWS version) or if the message fails to send.
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

  /// Cancels an ongoing Wall Street Horizon (WSH) event data streaming request.
  ///
  /// # Arguments
  /// * `req_id` - The request ID obtained from `request_wsh_event_data()`.
  ///
  /// # Errors
  /// Returns `IBKRError` if the cancellation message cannot be encoded or sent.
  pub fn cancel_wsh_event_data(&self, req_id: i32) -> Result<(), IBKRError> {
    info!("Cancelling WSH event data request: ReqID={}", req_id);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_cancel_wsh_event_data(req_id)?;
    self.message_broker.send_message(&request_msg)?;
    // No state to clean up in the manager
    Ok(())
  }

  /// Requests and returns a specific set of Wall Street Horizon (WSH) corporate event data.
  /// This is a blocking call.
  ///
  /// **Behavior:**
  /// 1.  It first checks if WSH metadata is cached. If not, it requests and waits for the metadata.
  /// 2.  Then, it requests the WSH event data specified by `request_details`.
  /// 3.  It waits until the number of events specified by `request_details.total_limit`
  ///     are received, or until the `timeout` occurs.
  /// 4.  Finally, it parses the received JSON event strings into a `Vec<WshEventData>`.
  ///
  /// This method uses an internal lock (`wsh_global_lock`) to ensure that only one
  /// WSH operation (metadata fetch or event data fetch via this method) occurs at a time
  /// through this manager instance, preventing conflicts with metadata caching.
  ///
  /// # Arguments
  /// * `request_details` - A [`WshEventDataRequest`] specifying the filters for the event data.
  ///   Crucially, `request_details.total_limit` must be `Some(positive_number)` as this
  ///   determines how many events to wait for.
  /// * `timeout` - The overall timeout for the entire operation (including potential metadata fetch
  ///   and the event data fetch).
  ///
  /// # Returns
  /// A `Vec<crate::data_wsh::WshEventData>` containing the parsed WSH events.
  ///
  /// # Errors
  /// Returns `IBKRError` if:
  /// -   `request_details.total_limit` is not a positive number.
  /// -   Fetching WSH metadata fails or times out.
  /// -   Fetching WSH event data fails or times out.
  /// -   Parsing any of the received JSON event strings fails.
  /// -   Other communication or encoding issues occur.
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

  /// Cleanup all pending requests and subscriptions.
  ///
  /// This method is typically called during client shutdown to ensure all
  /// outstanding requests are properly cancelled and resources are cleaned up.
  /// It will attempt to send cancellation messages to TWS for all pending requests
  /// and then clear all internal state.
  ///
  /// # Behavior
  /// 1. Collects all pending request IDs and their types
  /// 2. Marks all requests as cancelled and notifies waiting threads
  /// 3. Attempts to send cancel messages to TWS (if connection is available)
  /// 4. Clears all request states and WSH metadata cache
  ///
  /// # Errors
  /// Returns `IBKRError` if there are issues sending cancellation messages,
  /// but continues processing even if individual cancellations fail to ensure
  /// cleanup is as complete as possible.
  pub(crate) fn cleanup_requests(&self) -> Result<(), IBKRError> {
    info!("Cleaning up all pending fundamental data and WSH requests");

    let mut cleanup_errors = Vec::new();
    let server_version_result = self.message_broker.get_server_version();

    // If we can't get server version, we can't send cancel messages but we can still clean up state
    let encoder = match server_version_result {
      Ok(version) => Some(Encoder::new(version)),
      Err(e) => {
        warn!("Could not get server version during cleanup, skipping cancel messages: {:?}", e);
        None
      }
    };

    // Collect all pending request IDs and their types, then clear the states
    let request_ids_to_cancel: Vec<(i32, RequestType)>;
    {
      let mut states = self.request_states.lock();
      request_ids_to_cancel = states.iter()
        .map(|(&req_id, state)| (req_id, state.request_type))
        .collect();

      // Mark all requests as cancelled and notify waiters before clearing
      for (req_id, state) in states.iter_mut() {
        if !state.request_complete {
          debug!("Marking request {} as cancelled during cleanup", req_id);
          state.error_code = Some(-1);
          state.error_message = Some("Request cancelled during cleanup".to_string());
          state.request_complete = true;
        }
      }

      let num_requests = states.len();
      if num_requests > 0 {
        info!("Cancelling {} pending requests", num_requests);
        // Clear the states after marking them complete
        states.clear();
      }
    }

    // Notify any waiting threads that their requests have been cancelled
    self.request_cond.notify_all();

    // Send cancel messages to TWS if we have an encoder and connection
    if let Some(encoder) = encoder {
      for (req_id, request_type) in request_ids_to_cancel {
        let cancel_result = match request_type {
          RequestType::Fundamental => {
            debug!("Sending cancel for fundamental data request: ReqID={}", req_id);
            encoder.encode_cancel_fundamental_data(req_id)
              .and_then(|msg| self.message_broker.send_message(&msg))
          },
          RequestType::WshMetaData => {
            debug!("Sending cancel for WSH metadata request: ReqID={}", req_id);
            encoder.encode_cancel_wsh_meta_data(req_id)
              .and_then(|msg| self.message_broker.send_message(&msg))
          },
          RequestType::WshEventData => {
            debug!("Sending cancel for WSH event data request: ReqID={}", req_id);
            encoder.encode_cancel_wsh_event_data(req_id)
              .and_then(|msg| self.message_broker.send_message(&msg))
          },
        };

        if let Err(e) = cancel_result {
          // Log as debug since connection may already be closed during shutdown
          debug!("Failed to send cancel message for request {} (type {:?}): {:?}", req_id, request_type, e);
          cleanup_errors.push(e);
        }
      }
    }

    // Clear WSH metadata cache
    {
      let mut meta_cache = self.wsh_meta_data_cache.lock();
      if meta_cache.is_some() {
        *meta_cache = None;
        debug!("Cleared WSH metadata cache");
      }
    }

    // Note: We don't try to acquire wsh_global_lock since:
    // 1. We're shutting down and don't want to block
    // 2. Any ongoing get_wsh_events() calls will fail when their requests are cancelled
    // 3. The lock will be released when those calls complete or timeout

    if cleanup_errors.is_empty() {
      info!("Successfully cleaned up all fundamental data and WSH requests");
      Ok(())
    } else {
      // During shutdown, connection errors are expected, so don't treat as fatal
      info!("Cleanup completed with {} non-fatal errors (connection may already be closed)", cleanup_errors.len());
      Ok(()) // Return Ok since cleanup of state was successful
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
