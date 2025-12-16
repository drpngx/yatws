// yatws/src/parser_fin.rs
use std::sync::Arc;
use crate::handler::FinancialDataHandler;
use crate::min_server_ver::min_server_ver;

use crate::base::IBKRError;
use crate::protocol_dec_parser::FieldParser;

use log::debug;

/// Process WSH meta data message
pub fn process_wsh_meta_data(handler: &Arc<dyn FinancialDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int(false)?;
  let data_json = parser.read_str()?; // Assuming JSON data
  debug!("WSH Meta Data: ReqID={}, Data length={}", req_id, data_json.len());
  if data_json.trim().is_empty() {
      debug!("Received empty WSH Meta Data for ReqID={}", req_id);
  }
  handler.wsh_meta_data(req_id, data_json);
  Ok(())
}

/// Process WSH event data message
pub fn process_wsh_event_data(handler: &Arc<dyn FinancialDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int(false)?;
  let data_json = parser.read_str()?;
  debug!("WSH Event Data: ReqID={}, Data length={}", req_id, data_json.len());
  if data_json.trim().is_empty() {
      debug!("Received empty WSH Event Data for ReqID={}", req_id);
  }
  handler.wsh_event_data(req_id, data_json);
  Ok(())
}

/// Process fundamental data message
pub fn process_fundamental_data(handler: &Arc<dyn FinancialDataHandler>, parser: &mut FieldParser, server_version: i32) -> Result<(), IBKRError> {
  // Version field is implicit based on server version, Java client reads version but ignores it.
  if server_version >= min_server_ver::FUNDAMENTAL_DATA {
    let _version = parser.read_int(false)?; // Read version but ignore (like Java client)
    let req_id = parser.read_int(false)?;
    let data = parser.read_str()?; // The XML/text data
    debug!("Fundamental Data: ReqID={}, Data length={}", req_id, data.len());
    handler.fundamental_data(req_id, data);
  } else {
    // Should not happen if request checked version, but handle defensively
    return Err(IBKRError::ParseError("Received FundamentalData message but server version is too low".to_string()));
  }
  Ok(())
}