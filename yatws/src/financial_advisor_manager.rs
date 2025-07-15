
//! Manages Financial Advisor (FA) configurations, including groups, profiles, and aliases.
//!
//! The `FinancialAdvisorManager` allows requesting current FA configurations from TWS
//! and replacing them with new configurations.
use crate::base::IBKRError;
use crate::conn::MessageBroker;
use crate::financial_advisor::{FADataType, FAAlias, FAGroup, FAProfile, FinancialAdvisorConfig, FAProfileAllocation}; // Added FAProfileAllocation
use crate::handler::FinancialAdvisorHandler;
use crate::protocol_decoder::ClientErrorCode;
use crate::protocol_encoder::Encoder;

use chrono::Utc;
use log::{debug, error, info, warn, trace};
use parking_lot::{Condvar, Mutex, RwLock};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

impl From<quick_xml::encoding::EncodingError> for IBKRError {
  fn from(err: quick_xml::encoding::EncodingError) -> Self {
    IBKRError::ParseError(err.to_string())
  }
}

#[derive(Debug, Default)]
struct FARequestState {
  waiting_for_data: bool,
  // received_xml: Option<String>, // XML is processed directly by receive_fa
  data_type_expected: Option<FADataType>, // To verify response matches request
  waiting_for_replace_end: bool,
  replace_end_text: Option<String>,
  error_occurred: Option<IBKRError>, // Store error if one occurs during wait
}

/// Manages Financial Advisor (FA) configurations like groups, profiles, and aliases.
///
/// Accessed via [`IBKRClient::financial_advisor()`](crate::IBKRClient::financial_advisor()).
pub struct FinancialAdvisorManager {
  message_broker: Arc<MessageBroker>,
  config: RwLock<FinancialAdvisorConfig>,
  request_state: Mutex<FARequestState>,
  request_cond: Condvar,
}

impl FinancialAdvisorManager {
  /// Creates a new `FinancialAdvisorManager`.
  ///
  /// This is typically called internally when an `IBKRClient` is created.
  pub(crate) fn new(message_broker: Arc<MessageBroker>) -> Arc<Self> {
    Arc::new(FinancialAdvisorManager {
      message_broker,
      config: RwLock::new(FinancialAdvisorConfig::default()),
      request_state: Mutex::new(FARequestState::default()),
      request_cond: Condvar::new(),
    })
  }

  /// Requests Financial Advisor configuration data from TWS.
  ///
  /// This is a blocking call that waits for TWS to send the FA data.
  /// The internal FA configuration is updated upon successful retrieval.
  ///
  /// # Arguments
  /// * `fa_data_type` - The type of FA data to request (`Groups`, `Profiles`, or `Aliases`).
  ///
  /// # Returns
  /// `Ok(())` if the data was successfully requested and parsed.
  /// The updated configuration can then be retrieved using getter methods like `get_config()`.
  ///
  /// # Errors
  /// Returns `IBKRError` if the request times out, parsing fails, or other communication issues occur.
  pub fn request_fa_data(&self, fa_data_type: FADataType) -> Result<(), IBKRError> {
    info!("Requesting FA data of type: {:?}", fa_data_type);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_request_fa_data(fa_data_type as i32)?;

    let mut req_state_guard = self.request_state.lock();
    if req_state_guard.waiting_for_data || req_state_guard.waiting_for_replace_end {
      return Err(IBKRError::AlreadyRunning(
        "FA data request or replace operation already in progress".to_string(),
      ));
    }
    req_state_guard.waiting_for_data = true;
    // req_state_guard.received_xml = None; // Not needed, processed directly
    req_state_guard.data_type_expected = Some(fa_data_type);
    req_state_guard.error_occurred = None;
    drop(req_state_guard);

    self.message_broker.send_message(&request_msg)?;

    let wait_timeout = Duration::from_secs(15);
    let start_time = std::time::Instant::now();
    let mut req_state_guard = self.request_state.lock();

    while req_state_guard.waiting_for_data && req_state_guard.error_occurred.is_none() && start_time.elapsed() < wait_timeout {
      let remaining_timeout = wait_timeout.checked_sub(start_time.elapsed()).unwrap_or(Duration::from_millis(1));
      let timeout_result = self.request_cond.wait_for(&mut req_state_guard, remaining_timeout);
      if timeout_result.timed_out() {
        if req_state_guard.waiting_for_data && req_state_guard.error_occurred.is_none() {
          warn!("FA data request timed out waiting for receiveFA for type {:?}", fa_data_type);
          req_state_guard.waiting_for_data = false;
          req_state_guard.data_type_expected = None;
          return Err(IBKRError::Timeout(format!("FA data request timed out for {:?}", fa_data_type)));
        }
      }
    }

    if let Some(err) = req_state_guard.error_occurred.take() {
      req_state_guard.waiting_for_data = false;
      req_state_guard.data_type_expected = None;
      return Err(err);
    }

    if req_state_guard.waiting_for_data {
      req_state_guard.waiting_for_data = false;
      req_state_guard.data_type_expected = None;
      error!("FA data request loop ended unexpectedly while still waiting for data for type {:?}", fa_data_type);
      return Err(IBKRError::InternalError(format!("FA data request state inconsistency for {:?}", fa_data_type)));
    }

    Ok(())
  }

  /// Replaces Financial Advisor configuration data on TWS.
  ///
  /// This is a blocking call that waits for TWS to acknowledge the replacement.
  ///
  /// # Arguments
  /// * `fa_data_type` - The type of FA data to replace (`Groups`, `Profiles`, or `Aliases`).
  /// * `xml_data` - An XML string containing the new FA configuration.
  ///
  /// # Returns
  /// `Ok(())` if the replacement was successfully acknowledged by TWS.
  ///
  /// # Errors
  /// Returns `IBKRError` if the request times out or other communication issues occur.
  pub fn replace_fa_data(&self, fa_data_type: FADataType, xml_data: &str) -> Result<(), IBKRError> {
    info!("Replacing FA data of type: {:?}, XML length: {}", fa_data_type, xml_data.len());
    trace!("Replacing FA data XML: {}", xml_data);
    let server_version = self.message_broker.get_server_version()?;
    let encoder = Encoder::new(server_version);
    let request_msg = encoder.encode_replace_fa_data(fa_data_type as i32, xml_data)?;

    let mut req_state_guard = self.request_state.lock();
    if req_state_guard.waiting_for_data || req_state_guard.waiting_for_replace_end {
      return Err(IBKRError::AlreadyRunning(
        "FA data request or replace operation already in progress".to_string(),
      ));
    }
    req_state_guard.waiting_for_replace_end = true;
    req_state_guard.replace_end_text = None;
    req_state_guard.error_occurred = None;
    drop(req_state_guard);

    self.message_broker.send_message(&request_msg)?;

    let wait_timeout = Duration::from_secs(15);
    let start_time = std::time::Instant::now();
    let mut req_state_guard = self.request_state.lock();

    while req_state_guard.waiting_for_replace_end && req_state_guard.error_occurred.is_none() && start_time.elapsed() < wait_timeout {
      let remaining_timeout = wait_timeout.checked_sub(start_time.elapsed()).unwrap_or(Duration::from_millis(1));
      let timeout_result = self.request_cond.wait_for(&mut req_state_guard, remaining_timeout);
      if timeout_result.timed_out() {
        if req_state_guard.waiting_for_replace_end && req_state_guard.error_occurred.is_none() {
          warn!("FA data replacement timed out waiting for replaceFAEnd for type {:?}", fa_data_type);
          req_state_guard.waiting_for_replace_end = false;
          return Err(IBKRError::Timeout(format!("FA data replacement timed out for {:?}", fa_data_type)));
        }
      }
    }

    if let Some(err) = req_state_guard.error_occurred.take() {
      req_state_guard.waiting_for_replace_end = false;
      return Err(err);
    }

    if req_state_guard.waiting_for_replace_end {
      req_state_guard.waiting_for_replace_end = false;
      error!("FA data replacement loop ended unexpectedly while still waiting for ack for type {:?}", fa_data_type);
      return Err(IBKRError::InternalError(format!("FA data replacement state inconsistency for {:?}", fa_data_type)));
    }

    info!("FA data replacement for type {:?} acknowledged by TWS. Text: {:?}", fa_data_type, req_state_guard.replace_end_text);
    Ok(())
  }

  /// Retrieves a clone of the current Financial Advisor configuration.
  pub fn get_config(&self) -> FinancialAdvisorConfig {
    self.config.read().clone()
  }

  // --- XML Parsing Helpers ---
  fn parse_fa_groups(xml_data: &str) -> Result<HashMap<String, FAGroup>, IBKRError> {
    let mut reader = Reader::from_str(xml_data);
    reader.config_mut().trim_text(true);
    let mut groups = HashMap::new();
    let mut current_group: Option<FAGroup> = None;
    let mut in_accounts_list = false;
    let mut buf = Vec::new();

    loop {
      match reader.read_event_into(&mut buf) {
        Ok(Event::Start(e)) => {
          match e.name().as_ref() {
            b"Group" => current_group = Some(FAGroup::default()),
            b"name" if current_group.is_some() => {
              if let Ok(Event::Text(t)) = reader.read_event_into(&mut buf) {
                current_group.as_mut().unwrap().name = t.decode()?.into_owned();
              }
            }
            b"ListOfAccts" if current_group.is_some() => in_accounts_list = true,
            b"String" if current_group.is_some() && in_accounts_list => {
              if let Ok(Event::Text(t)) = reader.read_event_into(&mut buf) {
                current_group.as_mut().unwrap().accounts.push(t.decode()?.into_owned());
              }
            }
            b"DefaultMethod" if current_group.is_some() => {
              if let Ok(Event::Text(t)) = reader.read_event_into(&mut buf) {
                current_group.as_mut().unwrap().default_method = t.decode()?.into_owned();
              }
            }
            _ => (),
          }
        }
        Ok(Event::End(e)) => {
          match e.name().as_ref() {
            b"Group" => {
              if let Some(group) = current_group.take() {
                groups.insert(group.name.clone(), group);
              }
            }
            b"ListOfAccts" => in_accounts_list = false,
            _ => (),
          }
        }
        Ok(Event::Eof) => break,
        Err(e) => return Err(IBKRError::ParseError(format!("XML parsing error for FA Groups: {}", e))),
        _ => (),
      }
      buf.clear();
    }
    Ok(groups)
  }

  fn parse_fa_profiles(xml_data: &str) -> Result<HashMap<String, FAProfile>, IBKRError> {
    let mut reader = Reader::from_str(xml_data);
    reader.config_mut().trim_text(true);
    let mut profiles = HashMap::new();
    let mut current_profile: Option<FAProfile> = None;
    let mut current_allocation: Option<FAProfileAllocation> = None;
    let mut in_allocations_list = false;
    let mut buf = Vec::new();

    loop {
      match reader.read_event_into(&mut buf) {
        Ok(Event::Start(e)) => {
          match e.name().as_ref() {
            b"Profile" => current_profile = Some(FAProfile::default()),
            b"name" if current_profile.is_some() && !in_allocations_list => {
              if let Ok(Event::Text(t)) = reader.read_event_into(&mut buf) {
                current_profile.as_mut().unwrap().name = t.decode()?.into_owned();
              }
            }
            b"type" if current_profile.is_some() => {
              if let Ok(Event::Text(t)) = reader.read_event_into(&mut buf) {
                current_profile.as_mut().unwrap().profile_type = t.decode()?.into_owned();
              }
            }
            b"ListOfAllocations" if current_profile.is_some() => in_allocations_list = true,
            b"Allocation" if current_profile.is_some() && in_allocations_list => {
              current_allocation = Some(FAProfileAllocation::default());
            }
            b"acct" if current_allocation.is_some() => {
              if let Ok(Event::Text(t)) = reader.read_event_into(&mut buf) {
                current_allocation.as_mut().unwrap().account_id = t.decode()?.into_owned();
              }
            }
            b"amount" if current_allocation.is_some() => {
              if let Ok(Event::Text(t)) = reader.read_event_into(&mut buf) {
                current_allocation.as_mut().unwrap().amount = t.decode()?.into_owned();
              }
            }
            _ => (),
          }
        }
        Ok(Event::End(e)) => {
          match e.name().as_ref() {
            b"Profile" => {
              if let Some(profile) = current_profile.take() {
                profiles.insert(profile.name.clone(), profile);
              }
            }
            b"ListOfAllocations" => in_allocations_list = false,
            b"Allocation" => {
              if let Some(alloc) = current_allocation.take() {
                if let Some(profile) = current_profile.as_mut() {
                  profile.allocations.push(alloc);
                }
              }
            }
            _ => (),
          }
        }
        Ok(Event::Eof) => break,
        Err(e) => return Err(IBKRError::ParseError(format!("XML parsing error for FA Profiles: {}", e))),
        _ => (),
      }
      buf.clear();
    }
    Ok(profiles)
  }

  fn parse_fa_aliases(xml_data: &str) -> Result<HashMap<String, FAAlias>, IBKRError> {
    let mut reader = Reader::from_str(xml_data);
    reader.config_mut().trim_text(true);
    let mut aliases = HashMap::new();
    let mut current_alias: Option<FAAlias> = None;
    let mut buf = Vec::new();

    loop {
      match reader.read_event_into(&mut buf) {
        Ok(Event::Start(e)) => {
          match e.name().as_ref() {
            b"Alias" => current_alias = Some(FAAlias::default()),
            b"alias" if current_alias.is_some() => {
              if let Ok(Event::Text(t)) = reader.read_event_into(&mut buf) {
                current_alias.as_mut().unwrap().alias = t.decode()?.into_owned();
              }
            }
            b"acct" if current_alias.is_some() => {
              if let Ok(Event::Text(t)) = reader.read_event_into(&mut buf) {
                current_alias.as_mut().unwrap().account_id = t.decode()?.into_owned();
              }
            }
            _ => (),
          }
        }
        Ok(Event::End(e)) => {
          match e.name().as_ref() {
            b"Alias" => {
              if let Some(alias_obj) = current_alias.take() {
                aliases.insert(alias_obj.alias.clone(), alias_obj);
              }
            }
            _ => (),
          }
        }
        Ok(Event::Eof) => break,
        Err(e) => return Err(IBKRError::ParseError(format!("XML parsing error for FA Aliases: {}", e))),
        _ => (),
      }
      buf.clear();
    }
    Ok(aliases)
  }

  /// Cancels all pending Financial Advisor requests and cleans up internal state.
  ///
  /// This method is typically called during client shutdown to ensure that any
  /// threads waiting for FA data requests or replacements are properly notified
  /// and released.
  ///
  /// # Returns
  /// `Ok(())` if cleanup completed successfully.
  ///
  /// # Example
  /// ```no_run
  /// // This is typically called internally during IBKRClient::disconnect()
  /// fa_manager.cleanup_requests()?;
  /// ```
  pub(crate) fn cleanup_requests(&self) -> Result<(), IBKRError> {
    let mut req_state_guard = self.request_state.lock();

    let had_pending_requests = req_state_guard.waiting_for_data || req_state_guard.waiting_for_replace_end;

    if had_pending_requests {
      info!("Cleaning up pending FA requests during shutdown");

      // Set error state for any waiting operations
      req_state_guard.error_occurred = Some(IBKRError::ConnectionFailed(
        "Connection closing, canceling pending FA requests".to_string()
      ));

      // Clear all waiting flags
      req_state_guard.waiting_for_data = false;
      req_state_guard.waiting_for_replace_end = false;
      req_state_guard.data_type_expected = None;
      req_state_guard.replace_end_text = None;

      // Notify any waiting threads
      self.request_cond.notify_all();

      info!("Successfully cleaned up {} pending FA request(s)",
            if had_pending_requests { 1 } else { 0 });
    } else {
      debug!("No pending FA requests to clean up");
    }

    Ok(())
  }

}

impl FinancialAdvisorHandler for FinancialAdvisorManager {
  fn receive_fa(&self, fa_data_type_int: i32, xml_data: &str) {
    debug!("Handler: receiveFA: DataType={}, XML Length={}", fa_data_type_int, xml_data.len());
    trace!("Handler: receiveFA XML: {}", xml_data);

    let fa_data_type = match FADataType::from_i32(fa_data_type_int) {
      Some(dt) => dt,
      None => {
        let err_msg = format!("Received unknown FADataType: {}", fa_data_type_int);
        error!("{}", err_msg);
        let mut req_state_guard = self.request_state.lock();
        if req_state_guard.waiting_for_data {
          req_state_guard.error_occurred = Some(IBKRError::ParseError(err_msg));
          req_state_guard.waiting_for_data = false; // Stop waiting
          self.request_cond.notify_all();
        }
        return;
      }
    };

    let mut parse_error: Option<IBKRError> = None;

    match fa_data_type {
      FADataType::Groups => {
        match Self::parse_fa_groups(xml_data) {
          Ok(groups) => {
            let mut config_guard = self.config.write();
            config_guard.groups = groups;
            config_guard.last_updated_groups = Some(Utc::now());
            info!("FA Groups configuration updated with {} groups.", config_guard.groups.len());
          }
          Err(e) => parse_error = Some(e),
        }
      }
      FADataType::Profiles => {
        match Self::parse_fa_profiles(xml_data) {
          Ok(profiles) => {
            let mut config_guard = self.config.write();
            config_guard.profiles = profiles;
            config_guard.last_updated_profiles = Some(Utc::now());
            info!("FA Profiles configuration updated with {} profiles.", config_guard.profiles.len());
          }
          Err(e) => parse_error = Some(e),
        }
      }
      FADataType::Aliases => {
        match Self::parse_fa_aliases(xml_data) {
          Ok(aliases) => {
            let mut config_guard = self.config.write();
            config_guard.aliases = aliases;
            config_guard.last_updated_aliases = Some(Utc::now());
            info!("FA Aliases configuration updated with {} aliases.", config_guard.aliases.len());
          }
          Err(e) => parse_error = Some(e),
        }
      }
    };

    let mut req_state_guard = self.request_state.lock();
    if req_state_guard.waiting_for_data {
      if req_state_guard.data_type_expected == Some(fa_data_type) {
        if let Some(err) = parse_error {
          error!("Failed to parse FA data type {:?}: {:?}", fa_data_type, err);
          req_state_guard.error_occurred = Some(err);
        }
        req_state_guard.waiting_for_data = false;
        req_state_guard.data_type_expected = None;
        self.request_cond.notify_all();
      } else {
        warn!("Received FA data type {:?} but was waiting for {:?}. Processing anyway.",
              fa_data_type, req_state_guard.data_type_expected);
        if let Some(err) = parse_error { // Log error if parsing failed for this unexpected type
          error!("Failed to parse unexpected FA data type {:?}: {:?}", fa_data_type, err);
        }
      }
    } else if let Some(err) = parse_error { // Not waiting, but parsing failed
      error!("Failed to parse unsolicited FA data type {:?}: {:?}", fa_data_type, err);
    }
  }

  fn replace_fa_end(&self, req_id: i32, text: &str) {
    debug!("Handler: replaceFAEnd: ReqID={}, Text='{}'", req_id, text);
    let mut req_state_guard = self.request_state.lock();
    if req_state_guard.waiting_for_replace_end {
      req_state_guard.waiting_for_replace_end = false;
      req_state_guard.replace_end_text = Some(text.to_string());
      self.request_cond.notify_all();
    } else {
      warn!("Received replaceFAEnd but not waiting for it. ReqID={}, Text='{}'", req_id, text);
    }
  }

  fn handle_error(&self, req_id: i32, code: ClientErrorCode, msg: &str) {
    let err_msg = format!("FinancialAdvisorManager received error: ReqID={}, Code={:?}, Msg={}", req_id, code, msg);
    error!("{}", err_msg);

    let mut req_state_guard = self.request_state.lock();
    // For FA, req_id is not consistently used by TWS for REQ_FA/REPLACE_FA errors.
    // We assume any error during an active wait is relevant.
    if req_state_guard.waiting_for_data || req_state_guard.waiting_for_replace_end {
      req_state_guard.error_occurred = Some(IBKRError::ApiError(code as i32, msg.to_string()));
      // Clear flags to stop waiting
      req_state_guard.waiting_for_data = false;
      req_state_guard.waiting_for_replace_end = false;
      self.request_cond.notify_all();
      warn!("Error occurred during an active FA request/replace operation. Operation aborted.");
    }
  }
}
