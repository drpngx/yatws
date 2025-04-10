// yatws/src/parser_client.rs

use std::sync::Arc;
use crate::handler::ClientHandler;

use crate::base::IBKRError;
use crate::protocol_dec_parser::FieldParser;
use crate::protocol_decoder::IncomingMessageType; // Import message type enum
use log::warn;

/// Process an error message (Type 4)
pub fn process_error_message(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let version = parser.read_int()?;

  let mut id: i32 = -1; // Default for version < 2 or errors not associated with a request
  let error_code: i32;
  let error_msg: String;

  if version < 2 {
    // Very old format, just a single message string
    error_msg = parser.read_string()?;
    error_code = 0; // No code provided in this format
  } else {
    id = parser.read_int()?;
    error_code = parser.read_int()?;
    error_msg = parser.read_string()?;
    // Version >= 3 adds an optional 'advanced order reject' JSON string
    // Check remaining fields if needed for that version
  }

  // --- Call the handler ---
  handler.error(id, error_code, &error_msg);
  // ---

  Ok(())
}

/// Process current time message (Type 49)
pub fn process_current_time(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Version is unused for this message type according to docs
  let _version = parser.read_int()?;
  let time_unix = parser.read_int()? as i64; // TWS sends Unix timestamp

  // --- Call the handler ---
  handler.current_time(time_unix);
  // ---

  Ok(())
}

/// Process verify message API message (Type 65)
pub fn process_verify_message_api(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let api_data = parser.read_string()?;

  // --- Call the handler ---
  handler.verify_message_api(&api_data);
  // ---

  warn!("Processing logic for VerifyMessageAPI not fully implemented.");
  Ok(())
}

/// Process verify completed message (Type 66)
pub fn process_verify_completed(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let is_successful_str = parser.read_string()?; // "true" or "false"
  let error_text = parser.read_string()?;

  let is_successful = is_successful_str.eq_ignore_ascii_case("true");

  // --- Call the handler ---
  handler.verify_completed(is_successful, &error_text);
  // ---

  Ok(())
}

/// Process verify and auth message API message (Type 69)
pub fn process_verify_and_auth_message_api(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let api_data = parser.read_string()?;
  let xyz_challenge = parser.read_string()?;

  // --- Call the handler ---
  handler.verify_and_auth_message_api(&api_data, &xyz_challenge);
  // ---

  warn!("Processing logic for VerifyAndAuthMessageAPI not fully implemented (challenge response needed).");
  Ok(())
}

/// Process verify and auth completed message (Type 70)
pub fn process_verify_and_auth_completed(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let is_successful_str = parser.read_string()?; // "true" or "false"
  let error_text = parser.read_string()?;

  let is_successful = is_successful_str.eq_ignore_ascii_case("true");

  // --- Call the handler ---
  handler.verify_and_auth_completed(is_successful, &error_text);
  // ---

  Ok(())
}

/// Process head timestamp message (Type 88)
pub fn process_head_timestamp(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Version is unused according to docs
  // let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let timestamp_str = parser.read_string()?; // Can be Unix timestamp or "yyyyMMdd HH:mm:ss"

  // --- Call the handler ---
  handler.head_timestamp(req_id, &timestamp_str);
  // ---

  // Parsing the timestamp string could happen here or in the handler
  Ok(())
}

/// Process user info message (Type 107)
pub fn process_user_info(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Version is unused according to docs
  // let _version = parser.read_int()?;
  let req_id = parser.read_int()?; // Will always be 0 from TWS
  let white_branding_id = parser.read_string()?;

  // --- Call the handler ---
  handler.user_info(req_id, &white_branding_id);
  // ---

  Ok(())
}

/// Process display group list message (Type 67)
pub fn process_display_group_list(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let groups = parser.read_string()?; // Comma-separated list of group IDs

  // --- Call the handler ---
  handler.display_group_list(req_id, &groups);
  // ---

  Ok(())
}

/// Process display group updated message (Type 68)
pub fn process_display_group_updated(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let contract_info = parser.read_string()?; // Encoded contract string (needs parsing if used)

  // --- Call the handler ---
  handler.display_group_updated(req_id, &contract_info);
  // ---

  warn!("Parsing of contract_info in DisplayGroupUpdated not implemented.");
  Ok(())
}
