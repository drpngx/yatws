// yatws/src/parser_client.rs

use crate::handler::ClientHandler;

use crate::base::IBKRError;
use crate::protocol_dec_parser::FieldParser;

/// Process an error message
pub fn process_error_message(handler: &mut Box<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _msg_type = parser.read_int()?; // Skip message type
  let version = parser.read_int()?;

  if version < 2 {
    let message = parser.read_string()?;
    log::error!("TWS Error: {}", message);
  } else {
    let id = parser.read_int()?;
    let error_code = parser.read_int()?;
    let error_msg = parser.read_string()?;

    log::error!("TWS Error {}: {} ({})", id, error_msg, error_code);
  }

  Ok(())
}

/// Process current time message
pub fn process_current_time(handler: &mut Box<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _msg_type = parser.read_int()?; // Skip message type
  let _version = parser.read_int()?;
  let time = parser.read_int()?;

  log::debug!("Current Time: {}", time);

  Ok(())
}

/// Process verify message API message
pub fn process_verify_message_api(handler: &mut Box<dyn ClientHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse verification message
  Ok(())
}

/// Process verify completed message
pub fn process_verify_completed(handler: &mut Box<dyn ClientHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse verify completed message
  Ok(())
}

/// Process verify and auth message API message
pub fn process_verify_and_auth_message_api(handler: &mut Box<dyn ClientHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse verify and auth message
  Ok(())
}

/// Process verify and auth completed message
pub fn process_verify_and_auth_completed(handler: &mut Box<dyn ClientHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse verify and auth completed message
  Ok(())
}

/// Process head timestamp message
pub fn process_head_timestamp(handler: &mut Box<dyn ClientHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse head timestamp message
  Ok(())
}

/// Process user info message
pub fn process_user_info(handler: &mut Box<dyn ClientHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse user info message
  Ok(())
}

/// Process display group list message
pub fn process_display_group_list(handler: &mut Box<dyn ClientHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse display group list
  Ok(())
}

/// Process display group updated message
pub fn process_display_group_updated(handler: &mut Box<dyn ClientHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse display group update
  Ok(())
}
