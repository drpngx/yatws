// yatws/src/conn_mock.rs

use crate::base::IBKRError;
use crate::conn::Connection;
use crate::handler::MessageHandler;
use crate::message_parser::process_message;
use crate::conn_log::LogDirection;

use parking_lot::Mutex;
use rusqlite::{params, Connection as DbConnection};
use std::path::Path;
use std::sync::Arc;
use std::str::FromStr;

// --- Constants ---
const CENTER_DOT: char = '·';

// --- Helper Functions ---
// (center_dot_string_to_bytes and FromStr for LogDirection remain the same)
fn center_dot_string_to_bytes(s: &str) -> Vec<u8> {
  let mut bytes = Vec::with_capacity(s.len());
  let center_dot_bytes = CENTER_DOT.to_string();
  let mut current_pos = 0;

  while let Some(found_pos) = s[current_pos..].find(&center_dot_bytes) {
    let actual_pos = current_pos + found_pos;
    bytes.extend_from_slice(s[current_pos..actual_pos].as_bytes());
    bytes.push(0);
    current_pos = actual_pos + center_dot_bytes.len();
  }
  bytes.extend_from_slice(s[current_pos..].as_bytes());
  bytes
}

impl FromStr for LogDirection {
  type Err = IBKRError;
  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "SEND" => Ok(LogDirection::Send),
      "RECV" => Ok(LogDirection::Recv),
      _ => Err(IBKRError::ParseError(format!("Invalid LogDirection string in log: {}", s))),
    }
  }
}


// --- Data Structures ---
#[derive(Debug, Clone)]
struct LoggedMessage {
  #[allow(dead_code)]
  session_id: i64,
  direction: LogDirection,
  message_type_id: Option<i32>,
  message_type_name: Option<String>,
  payload_bytes: Vec<u8>,
}

// --- Mock Connection Implementation ---
struct MockConnectionState {
  db_conn: Option<DbConnection>,
  server_version: i32,
  logged_messages: Vec<LoggedMessage>,
  message_iter_index: usize,
  handler: Option<MessageHandler>,
  connected: bool,
}

#[derive(Clone)]
pub struct MockConnection {
  inner: Arc<Mutex<MockConnectionState>>,
}

// --- Internal Auto-Pumping Logic ---
impl MockConnection {
  /// Internal helper to process the next RECV message if available.
  /// Needs mutable access to the state.
  /// Returns Ok(true) if a message was processed, Ok(false) if stopped (SEND/End/No Handler), Err on handler failure.
  fn _pump_single_recv_message(state: &mut MockConnectionState) -> Result<bool, IBKRError> {
    if state.handler.is_none() {
      log::trace!("Mock (AutoPump): Cannot pump, no handler set.");
      return Ok(false);
    }

    if state.message_iter_index >= state.logged_messages.len() {
      log::trace!("Mock (AutoPump): End of messages.");
      return Ok(false);
    }

    // Peek at the next message
    let next_message_index = state.message_iter_index;
    let next_message = &state.logged_messages[next_message_index];

    match next_message.direction {
      LogDirection::Recv => {
        // Read immutable data before mutable borrow
        let is_connected = state.connected;
        let payload_to_process = next_message.payload_bytes.clone();
        let message_type_name_for_log = next_message.message_type_name.clone();

        log::debug!("Mock (AutoPump): Processing RECV message #{}: Type={:?}",
                    next_message_index + 1,
                    message_type_name_for_log.as_deref().unwrap_or("UNKNOWN"));

        // Get mutable handler ref (we know it's Some)
        let handler = state.handler.as_mut().unwrap();

        if !is_connected {
          log::warn!("Mock (AutoPump): Processing RECV message but state not 'connected'.");
        }

        // Process (mutable borrow used here)
        let process_result = process_message(handler, &payload_to_process);

        // Advance index ONLY after processing attempt
        state.message_iter_index += 1;

        match process_result {
          Ok(_) => {
            log::trace!("Mock (AutoPump): Successfully processed RECV message #{}.", next_message_index + 1);
            Ok(true) // Indicate success and potential for more
          }
          Err(e) => {
            log::error!("Mock (AutoPump): Error processing RECV message #{}: {:?}", next_message_index + 1, e);
            // Use a specific error type if available, otherwise wrap it
            Err(IBKRError::ParseError(format!("Handler failed processing message: {}", e)))
          }
        }
      }
      LogDirection::Send => {
        log::trace!("Mock (AutoPump): Found SEND message #{}. Stopping pump.", next_message_index + 1);
        Ok(false) // Stop pumping, wait for client send
      }
    }
  }

  /// Internal helper to keep pumping RECV messages until a SEND or the end is hit.
  /// Returns Ok(()) if pumping finished normally, Err if a handler failed.
  fn _pump_recv_messages_until_send_or_end(state: &mut MockConnectionState) -> Result<(), IBKRError> {
    log::debug!("Mock (AutoPump): Starting batch pump from index {}...", state.message_iter_index + 1);
    loop {
      match Self::_pump_single_recv_message(state) {
        Ok(true) => continue, // Message processed, loop again
        Ok(false) => {
          log::debug!("Mock (AutoPump): Batch pump finished (hit SEND or end). Current index: {}", state.message_iter_index + 1);
          return Ok(()); // Stopped normally
        },
        Err(e) => {
          log::error!("Mock (AutoPump): Batch pump failed due to handler error.");
          return Err(e); // Propagate handler error
        },
      }
    }
  }
}


// --- Public API and Connection Trait Impl ---
impl MockConnection {
  pub fn new<P: AsRef<Path>>(
    db_path: P,
    session_name: &str,
  ) -> Result<Self, IBKRError> {
    log::info!("Creating MockConnection for session '{}' from DB: {:?} (Strict SEND Check, AutoPump)",
               session_name, db_path.as_ref());

    let db = DbConnection::open(db_path)
      .map_err(|e| IBKRError::ConfigurationError(format!("Mock: Failed to open logger DB: {}", e)))?;

    let (session_id, server_version_opt): (i64, Option<i32>) = db.query_row(
      "SELECT session_id, server_version FROM sessions WHERE session_name = ?1",
      params![session_name],
      |row| Ok((row.get(0)?, row.get(1)?))
    ).map_err(|e| match e {
      rusqlite::Error::QueryReturnedNoRows => IBKRError::ConfigurationError(format!("Mock: Log session '{}' not found in database.", session_name)),
      _ => IBKRError::LoggingError(format!("Mock: Failed to query session '{}': {}", session_name, e)),
    })?;

    let server_version = server_version_opt.ok_or_else(||
                                                       IBKRError::ConfigurationError(format!("Mock: Log session '{}' found, but server_version is missing (NULL) in the database.", session_name))
    )?;

    log::debug!("Mock: Found session_id={}, server_version={}", session_id, server_version);

    let logged_messages: Vec<LoggedMessage> = {
      let mut stmt = db.prepare(
        "SELECT session_id, direction, message_type_id, message_type_name, payload_text
                 FROM messages
                 WHERE session_id = ?1
                 ORDER BY message_id ASC"
      ).map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to prepare message query: {}", e)))?;

      let messages_iter = stmt.query_map(params![session_id], |row| {
        let direction_str: String = row.get(1)?;
        let direction = LogDirection::from_str(&direction_str)
          .map_err(|_| rusqlite::Error::InvalidColumnType(1, "LogDirection".into(), rusqlite::types::Type::Text))?;
        let payload_text: String = row.get(4)?;
        let payload_bytes = center_dot_string_to_bytes(&payload_text);

        Ok(LoggedMessage {
          session_id: row.get(0)?,
          direction,
          message_type_id: row.get(2)?,
          message_type_name: row.get(3)?,
          payload_bytes,
        })
      }).map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to query messages: {}", e)))?;

      messages_iter.collect::<Result<Vec<_>, _>>()
        .map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to map messages: {}", e)))?
    };

    log::info!("Mock: Loaded {} messages for session '{}'", logged_messages.len(), session_name);

    let inner_state = MockConnectionState {
      db_conn: Some(db),
      server_version,
      logged_messages,
      message_iter_index: 0,
      handler: None,
      connected: false,
    };

    Ok(Self {
      inner: Arc::new(Mutex::new(inner_state)),
    })
  }
}

// Implement the Connection trait
impl Connection for MockConnection {
  fn is_connected(&self) -> bool {
    self.inner.lock().connected
  }

  fn disconnect(&mut self) -> Result<(), IBKRError> {
    log::info!("Mock: disconnect() called.");
    let mut guard = self.inner.lock();
    guard.connected = false;
    guard.handler = None;
    guard.db_conn = None;
    Ok(())
  }

  fn send_message_body(&mut self, data: &[u8]) -> Result<(), IBKRError> {
    // Acquire lock once for the whole operation
    let mut guard = self.inner.lock();

    if !guard.connected {
      log::warn!("Mock: send_message_body called while not connected.");
      return Err(IBKRError::NotConnected);
    }

    if guard.message_iter_index >= guard.logged_messages.len() {
      log::error!("Mock: send_message_body called, but no more logged messages expected.");
      return Err(IBKRError::LoggingError("Unexpected send: End of logged messages reached.".to_string()));
    }

    let expected_message_index = guard.message_iter_index;
    let expected_message = &guard.logged_messages[expected_message_index];

    match expected_message.direction {
      LogDirection::Send => {
        log::debug!("Mock: Verifying send_message_body call against expected SEND message #{}: Type={:?}",
                    expected_message_index + 1,
                    expected_message.message_type_name.as_deref().unwrap_or("UNKNOWN"));

        if data == expected_message.payload_bytes.as_slice() {
          log::debug!("Mock: Sent message matches expected SEND message #{}. Advancing iterator.",
                      expected_message_index + 1);
          // Advance index past the verified SEND
          guard.message_iter_index += 1;

          // --- TRIGGER AUTO-PUMP ---
          if guard.handler.is_some() {
            Self::_pump_recv_messages_until_send_or_end(&mut guard)?;
          } else {
            log::warn!("Mock: send matched SEND #{}, but no handler set to pump subsequent RECVs.", expected_message_index + 1);
          }
          // --- END AUTO-PUMP ---
          Ok(())
        } else {
          log::error!("Mock: Mismatch! Sent message body does not match expected logged SEND message #{}.",
                      expected_message_index + 1);
          MockConnection::log_payload_mismatch(data, &expected_message.payload_bytes);
          Err(IBKRError::LoggingError(format!(
            "Sent message mismatch at logged message index {}. Expected Type: {:?}",
            expected_message_index + 1,
            expected_message.message_type_name.as_deref().unwrap_or("UNKNOWN")
          )))
        }
      }
      LogDirection::Recv => {
        log::error!("Mock: send_message_body called, but the next expected logged message #{} is RECV, not SEND.",
                    expected_message_index + 1);
        Err(IBKRError::LoggingError(format!(
          "Unexpected send: Expected next logged message #{} to be SEND, but it was RECV.",
          expected_message_index + 1
        )))
      }
    }
  } // Lock `guard` is dropped here


  /// Sets handler, marks connected, simulates implicit StartAPI verification, and triggers initial auto-pump.
  fn set_message_handler(&mut self, handler: MessageHandler) {
    // Acquire lock once
    let mut guard = self.inner.lock();

    log::info!("Mock: Setting message handler.");
    if guard.handler.is_some() {
      log::warn!("Mock: Replacing existing message handler.");
    }
    guard.handler = Some(handler);
    guard.connected = true;
    log::info!("Mock: Connection marked as 'connected'.");

    // --- ADDED: Simulate implicit StartAPI verification ---
    if guard.message_iter_index == 0 && !guard.logged_messages.is_empty() {
      let first_message = &guard.logged_messages[0];
      // Check direction and type ID (71 for StartAPI)
      if first_message.direction == LogDirection::Send && first_message.message_type_id == Some(71) {
        log::debug!("Mock: Implicitly verifying logged SEND StartAPI (message #1) during set_message_handler.");
        guard.message_iter_index += 1;
        log::debug!("Mock: Advanced iterator past implicit StartAPI (now at index {}).", guard.message_iter_index);
      } else {
        log::warn!("Mock: First logged message (#1) was not the expected SEND StartAPI (Direction={:?}, TypeID={:?}). Initial state might be incorrect.",
                   first_message.direction, first_message.message_type_id);
      }
    }
    // --- END ADDED ---


    // --- TRIGGER INITIAL AUTO-PUMP ---
    match Self::_pump_recv_messages_until_send_or_end(&mut guard) {
      Ok(_) => {
        log::info!("Mock: Initial auto-pump completed after setting handler (current index: {}).", guard.message_iter_index);
      }
      Err(e) => {
        log::error!("Mock: Error during initial auto-pump after setting handler: {:?}. Connection may be in unexpected state.", e);
      }
    }
    // --- END AUTO-PUMP ---

  } // Lock `guard` is dropped here

  fn get_server_version(&self) -> i32 {
    self.inner.lock().server_version
  }
}

// --- Private helpers ---
impl MockConnection {
  fn log_payload_mismatch(sent_data: &[u8], expected_data: &[u8]) {
    let max_len = 150;
    let sent_str = Self::bytes_to_debug_string(&sent_data[..max_len.min(sent_data.len())]);
    let expected_str = Self::bytes_to_debug_string(&expected_data[..max_len.min(expected_data.len())]);
    log::error!("Mock Mismatch Detail:");
    log::error!(" > Sent    ({} bytes): '{}'{}", sent_data.len(), sent_str, if sent_data.len() > max_len {"..."} else {""});
    log::error!(" > Expected({} bytes): '{}'{}", expected_data.len(), expected_str, if expected_data.len() > max_len {"..."} else {""});
  }

  fn bytes_to_debug_string(bytes: &[u8]) -> String {
    let mut modified_bytes = Vec::with_capacity(bytes.len());
    for &byte in bytes {
      if byte == 0 {
        modified_bytes.extend_from_slice(CENTER_DOT.to_string().as_bytes());
      } else {
        modified_bytes.push(byte);
      }
    }
    String::from_utf8_lossy(&modified_bytes).to_string()
  }
}

// --- Drop implementation ---
impl Drop for MockConnectionState {
  fn drop(&mut self) {
    if let Some(conn) = self.db_conn.take() {
      if let Err(e) = conn.close() {
        log::error!("Mock: Error closing logger database connection: {:?}", e);
      } else {
        log::debug!("Mock: Logger database connection closed.");
      }
    }
  }
}
