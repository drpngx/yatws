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
  relative_timestamp_ms: f64,
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
  fn _pump_single_recv_message(state: &mut MockConnectionState) -> Result<bool, IBKRError> {
    if state.handler.is_none() {
      log::trace!("Mock (AutoPump): Cannot pump, no handler set.");
      return Ok(false);
    }

    if state.message_iter_index >= state.logged_messages.len() {
      log::trace!("Mock (AutoPump): End of messages.");
      return Ok(false);
    }

    let next_message_index = state.message_iter_index;
    let next_message = &state.logged_messages[next_message_index];

    match next_message.direction {
      LogDirection::Recv => {
        let is_connected = state.connected;
        let payload_to_process = next_message.payload_bytes.clone();
        let message_type_name_for_log = next_message.message_type_name.clone();

        log::debug!("Mock (AutoPump): Processing RECV message #{}: Type={:?}",
                    next_message_index + 1,
                    message_type_name_for_log.as_deref().unwrap_or("UNKNOWN"));

        let handler = state.handler.as_mut().unwrap();

        if !is_connected {
          log::warn!("Mock (AutoPump): Processing RECV message but state not 'connected'.");
        }

        let process_result = process_message(handler, &payload_to_process);
        state.message_iter_index += 1;

        match process_result {
          Ok(_) => {
            log::trace!("Mock (AutoPump): Successfully processed RECV message #{}.", next_message_index + 1);
            Ok(true)
          }
          Err(e) => {
            log::error!("Mock (AutoPump): Error processing RECV message #{}: {:?}", next_message_index + 1, e);
            Err(IBKRError::ParseError(format!("Handler failed processing message: {}", e)))
          }
        }
      }
      LogDirection::Send => {
        log::trace!("Mock (AutoPump): Found SEND message #{}. Stopping pump.", next_message_index + 1);
        Ok(false)
      }
    }
  }

  /// Internal helper to keep pumping RECV messages until a SEND or the end is hit.
  fn _pump_recv_messages_until_send_or_end(state: &mut MockConnectionState) -> Result<(), IBKRError> {
    log::debug!("Mock (AutoPump): Starting batch pump from index {}...", state.message_iter_index + 1);
    loop {
      match Self::_pump_single_recv_message(state) {
        Ok(true) => continue,
        Ok(false) => {
          log::debug!("Mock (AutoPump): Batch pump finished (hit SEND or end). Current index: {}", state.message_iter_index + 1);
          return Ok(());
        },
        Err(e) => {
          log::error!("Mock (AutoPump): Batch pump failed due to handler error.");
          return Err(e);
        },
      }
    }
  }
}

// --- Public API and Connection Trait Impl ---
impl MockConnection {
  /// Create MockConnection from a single session (original behavior)
  pub fn new<P: AsRef<Path>>(
    db_path: P,
    session_name: &str,
  ) -> Result<Self, IBKRError> {
    log::info!("Creating MockConnection for session '{}' from DB: {:?}",
               session_name, db_path.as_ref());

    let db = DbConnection::open(db_path)
      .map_err(|e| IBKRError::ConfigurationError(format!("Mock: Failed to open logger DB: {}", e)))?;

    // Query using new schema - try both old and new table names for compatibility
    let (session_id, server_version_opt): (i64, Option<i32>) = db.query_row(
      "SELECT id, server_version FROM sessions WHERE session_name = ?1 ORDER BY created_at DESC LIMIT 1",
      params![session_name],
      |row| Ok((row.get(0)?, row.get(1)?))
    ).or_else(|_| {
      // Fallback to old schema if new doesn't work
      db.query_row(
        "SELECT session_id, server_version FROM sessions WHERE session_name = ?1",
        params![session_name],
        |row| Ok((row.get(0)?, row.get(1)?))
      )
    }).map_err(|e| match e {
      rusqlite::Error::QueryReturnedNoRows => IBKRError::ConfigurationError(format!("Mock: Log session '{}' not found in database.", session_name)),
      _ => IBKRError::LoggingError(format!("Mock: Failed to query session '{}': {}", session_name, e)),
    })?;

    let server_version = server_version_opt.ok_or_else(||
                                                       IBKRError::ConfigurationError(format!("Mock: Log session '{}' found, but server_version is missing (NULL) in the database.", session_name))
    )?;

    log::debug!("Mock: Found session_id={}, server_version={}", session_id, server_version);

    // Load messages in a properly scoped block to ensure borrowing is released
    let logged_messages = {
      let mut stmt = db.prepare(
        "SELECT session_id, direction, message_type_id, message_type_name, payload_text, relative_timestamp_ms
         FROM session_messages
         WHERE session_id = ?1
         ORDER BY id ASC"
      ).or_else(|_| {
        // Fallback to old schema
        db.prepare(
          "SELECT session_id, direction, message_type_id, message_type_name, payload_text,
                  COALESCE(relative_timestamp_ms, 0.0) as relative_timestamp_ms
           FROM messages
           WHERE session_id = ?1
           ORDER BY message_id ASC"
        )
      }).map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to prepare message query: {}", e)))?;

      let messages_iter = stmt.query_map(params![session_id], |row| {
        let direction_str: String = row.get(1)?;
        let direction = LogDirection::from_str(&direction_str)
          .map_err(|_| rusqlite::Error::InvalidColumnType(1, "LogDirection".into(), rusqlite::types::Type::Text))?;
        let payload_text: String = row.get(4)?;
        let payload_bytes = center_dot_string_to_bytes(&payload_text);
        let relative_timestamp_ms: f64 = row.get(5).unwrap_or(0.0);

        Ok(LoggedMessage {
          session_id: row.get(0)?,
          direction,
          message_type_id: row.get(2)?,
          message_type_name: row.get(3)?,
          payload_bytes,
          relative_timestamp_ms,
        })
      }).map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to query messages for session {}: {}", session_id, e)))?;

      messages_iter.collect::<Result<Vec<_>, _>>()
        .map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to map messages for session {}: {}", session_id, e)))?
    }; // stmt is dropped here, releasing borrow of db

    log::info!("Mock: Loaded {} messages for session '{}'", logged_messages.len(), session_name);

    let inner_state = MockConnectionState {
      db_conn: Some(db), // Now we can safely move db
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

  /// Create MockConnection from parent session + specific child session
  pub fn from_parent_and_child<P: AsRef<Path>>(
    db_path: P,
    run_id: &str,
    test_name: &str,
  ) -> Result<Self, IBKRError> {
    log::info!("Creating MockConnection for run '{}' + test '{}' from DB: {:?}",
               run_id, test_name, db_path.as_ref());

    let db = DbConnection::open(db_path)
      .map_err(|e| IBKRError::ConfigurationError(format!("Mock: Failed to open logger DB: {}", e)))?;

    // Find parent session
    let (parent_id, server_version_opt): (i64, Option<i32>) = db.query_row(
      "SELECT id, server_version FROM sessions
       WHERE session_name = ?1 AND parent_id IS NULL
       ORDER BY created_at DESC LIMIT 1",
      params![run_id],
      |row| Ok((row.get(0)?, row.get(1)?))
    ).map_err(|e| match e {
      rusqlite::Error::QueryReturnedNoRows => IBKRError::ConfigurationError(format!("Mock: Parent session '{}' not found.", run_id)),
      _ => IBKRError::LoggingError(format!("Mock: Failed to query parent session '{}': {}", run_id, e)),
    })?;

    // Find child session
    let child_id: i64 = db.query_row(
      "SELECT id FROM sessions
       WHERE session_name = ?1 AND parent_id = ?2",
      params![test_name, parent_id],
      |row| row.get(0)
    ).map_err(|e| match e {
      rusqlite::Error::QueryReturnedNoRows => IBKRError::ConfigurationError(format!("Mock: Child session '{}' not found under parent '{}'.", test_name, run_id)),
      _ => IBKRError::LoggingError(format!("Mock: Failed to query child session '{}': {}", test_name, e)),
    })?;

    let server_version = server_version_opt.ok_or_else(||
                                                       IBKRError::ConfigurationError(format!("Mock: Parent session '{}' found, but server_version is missing.", run_id))
    )?;

    log::debug!("Mock: Found parent_id={}, child_id={}, server_version={}", parent_id, child_id, server_version);

    // Load and combine messages with proper scoping to avoid borrowing issues
    let all_messages = {
      let mut messages = Vec::new();

      // Load parent messages first - ensure stmt is dropped before moving db
      {
        let mut stmt = db.prepare(
          "SELECT session_id, direction, message_type_id, message_type_name, payload_text, relative_timestamp_ms
           FROM session_messages
           WHERE session_id = ?1
           ORDER BY id ASC"
        ).or_else(|_| {
          db.prepare(
            "SELECT session_id, direction, message_type_id, message_type_name, payload_text,
                    COALESCE(relative_timestamp_ms, 0.0) as relative_timestamp_ms
             FROM messages
             WHERE session_id = ?1
             ORDER BY message_id ASC"
          )
        }).map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to prepare parent message query: {}", e)))?;

        let parent_messages: Result<Vec<_>, _> = stmt.query_map(params![parent_id], |row| {
          let direction_str: String = row.get(1)?;
          let direction = LogDirection::from_str(&direction_str)
            .map_err(|_| rusqlite::Error::InvalidColumnType(1, "LogDirection".into(), rusqlite::types::Type::Text))?;
          let payload_text: String = row.get(4)?;
          let payload_bytes = center_dot_string_to_bytes(&payload_text);
          let relative_timestamp_ms: f64 = row.get(5).unwrap_or(0.0);

          Ok(LoggedMessage {
            session_id: row.get(0)?,
            direction,
            message_type_id: row.get(2)?,
            message_type_name: row.get(3)?,
            payload_bytes,
            relative_timestamp_ms,
          })
        }).map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to query parent messages: {}", e)))?
          .collect();

        messages.extend(parent_messages.map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to collect parent messages: {}", e)))?);
      } // parent stmt is dropped here

      // Load child messages - ensure stmt is dropped before moving db
      {
        let mut stmt = db.prepare(
          "SELECT session_id, direction, message_type_id, message_type_name, payload_text, relative_timestamp_ms
           FROM session_messages
           WHERE session_id = ?1
           ORDER BY id ASC"
        ).or_else(|_| {
          db.prepare(
            "SELECT session_id, direction, message_type_id, message_type_name, payload_text,
                    COALESCE(relative_timestamp_ms, 0.0) as relative_timestamp_ms
             FROM messages
             WHERE session_id = ?1
             ORDER BY message_id ASC"
          )
        }).map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to prepare child message query: {}", e)))?;

        let child_messages: Result<Vec<_>, _> = stmt.query_map(params![child_id], |row| {
          let direction_str: String = row.get(1)?;
          let direction = LogDirection::from_str(&direction_str)
            .map_err(|_| rusqlite::Error::InvalidColumnType(1, "LogDirection".into(), rusqlite::types::Type::Text))?;
          let payload_text: String = row.get(4)?;
          let payload_bytes = center_dot_string_to_bytes(&payload_text);
          let relative_timestamp_ms: f64 = row.get(5).unwrap_or(0.0);

          Ok(LoggedMessage {
            session_id: row.get(0)?,
            direction,
            message_type_id: row.get(2)?,
            message_type_name: row.get(3)?,
            payload_bytes,
            relative_timestamp_ms,
          })
        }).map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to query child messages: {}", e)))?
          .collect();

        messages.extend(child_messages.map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to collect child messages: {}", e)))?);
      } // child stmt is dropped here

      // Sort by relative timestamp to maintain proper order
      messages.sort_by(|a, b| a.relative_timestamp_ms.partial_cmp(&b.relative_timestamp_ms).unwrap_or(std::cmp::Ordering::Equal));
      messages
    }; // All stmt borrows are released here

    log::info!("Mock: Loaded {} total messages for run '{}' + test '{}'",
               all_messages.len(), run_id, test_name);

    let inner_state = MockConnectionState {
      db_conn: Some(db), // Now we can safely move db
      server_version,
      logged_messages: all_messages,
      message_iter_index: 0,
      handler: None,
      connected: false,
    };

    Ok(Self {
      inner: Arc::new(Mutex::new(inner_state)),
    })
  }

  /// Create MockConnection from parent session + all completed child sessions
  pub fn from_run_with_all_tests<P: AsRef<Path>>(
    db_path: P,
    run_id: &str,
  ) -> Result<Self, IBKRError> {
    log::info!("Creating MockConnection for full run '{}' from DB: {:?}",
               run_id, db_path.as_ref());

    let db = DbConnection::open(db_path)
      .map_err(|e| IBKRError::ConfigurationError(format!("Mock: Failed to open logger DB: {}", e)))?;

    // Find parent session
    let (parent_id, server_version_opt): (i64, Option<i32>) = db.query_row(
      "SELECT id, server_version FROM sessions
       WHERE session_name = ?1 AND parent_id IS NULL
       ORDER BY created_at DESC LIMIT 1",
      params![run_id],
      |row| Ok((row.get(0)?, row.get(1)?))
    ).map_err(|e| match e {
      rusqlite::Error::QueryReturnedNoRows => IBKRError::ConfigurationError(format!("Mock: Parent session '{}' not found.", run_id)),
      _ => IBKRError::LoggingError(format!("Mock: Failed to query parent session '{}': {}", run_id, e)),
    })?;

    let server_version = server_version_opt.ok_or_else(||
                                                       IBKRError::ConfigurationError(format!("Mock: Parent session '{}' found, but server_version is missing.", run_id))
    )?;

    // Find all completed child sessions - ensure stmt is dropped before moving db
    let child_ids = {
      let mut stmt = db.prepare(
        "SELECT id FROM sessions
         WHERE parent_id = ?1 AND status = 'completed'
         ORDER BY created_at ASC"
      ).map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to prepare child session query: {}", e)))?;

      let child_ids_result: Result<Vec<i64>, _> = stmt.query_map(params![parent_id], |row| {
        Ok(row.get::<_, i64>(0)?)
      }).map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to query child sessions: {}", e)))?
        .collect();

      child_ids_result.map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to collect child session IDs: {}", e)))?
    }; // stmt is dropped here

    log::debug!("Mock: Found parent_id={}, {} completed child sessions, server_version={}",
                parent_id, child_ids.len(), server_version);

    // Load and combine messages with proper scoping to avoid borrowing issues
    let all_messages = {
      let mut messages = Vec::new();

      // Load parent messages first - ensure stmt is dropped before moving db
      {
        let mut stmt = db.prepare(
          "SELECT session_id, direction, message_type_id, message_type_name, payload_text, relative_timestamp_ms
           FROM session_messages
           WHERE session_id = ?1
           ORDER BY id ASC"
        ).or_else(|_| {
          db.prepare(
            "SELECT session_id, direction, message_type_id, message_type_name, payload_text,
                    COALESCE(relative_timestamp_ms, 0.0) as relative_timestamp_ms
             FROM messages
             WHERE session_id = ?1
             ORDER BY message_id ASC"
          )
        }).map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to prepare parent message query: {}", e)))?;

        let parent_messages: Result<Vec<_>, _> = stmt.query_map(params![parent_id], |row| {
          let direction_str: String = row.get(1)?;
          let direction = LogDirection::from_str(&direction_str)
            .map_err(|_| rusqlite::Error::InvalidColumnType(1, "LogDirection".into(), rusqlite::types::Type::Text))?;
          let payload_text: String = row.get(4)?;
          let payload_bytes = center_dot_string_to_bytes(&payload_text);
          let relative_timestamp_ms: f64 = row.get(5).unwrap_or(0.0);

          Ok(LoggedMessage {
            session_id: row.get(0)?,
            direction,
            message_type_id: row.get(2)?,
            message_type_name: row.get(3)?,
            payload_bytes,
            relative_timestamp_ms,
          })
        }).map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to query parent messages: {}", e)))?
          .collect();

        messages.extend(parent_messages.map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to collect parent messages: {}", e)))?);
      } // parent stmt is dropped here

      // Load each child's messages - ensure each stmt is dropped before moving db
      for child_id in child_ids {
        let mut stmt = db.prepare(
          "SELECT session_id, direction, message_type_id, message_type_name, payload_text, relative_timestamp_ms
           FROM session_messages
           WHERE session_id = ?1
           ORDER BY id ASC"
        ).or_else(|_| {
          db.prepare(
            "SELECT session_id, direction, message_type_id, message_type_name, payload_text,
                    COALESCE(relative_timestamp_ms, 0.0) as relative_timestamp_ms
             FROM messages
             WHERE session_id = ?1
             ORDER BY message_id ASC"
          )
        }).map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to prepare child message query: {}", e)))?;

        let child_messages: Result<Vec<_>, _> = stmt.query_map(params![child_id], |row| {
          let direction_str: String = row.get(1)?;
          let direction = LogDirection::from_str(&direction_str)
            .map_err(|_| rusqlite::Error::InvalidColumnType(1, "LogDirection".into(), rusqlite::types::Type::Text))?;
          let payload_text: String = row.get(4)?;
          let payload_bytes = center_dot_string_to_bytes(&payload_text);
          let relative_timestamp_ms: f64 = row.get(5).unwrap_or(0.0);

          Ok(LoggedMessage {
            session_id: row.get(0)?,
            direction,
            message_type_id: row.get(2)?,
            message_type_name: row.get(3)?,
            payload_bytes,
            relative_timestamp_ms,
          })
        }).map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to query child {} messages: {}", child_id, e)))?
          .collect();

        messages.extend(child_messages.map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to collect child {} messages: {}", child_id, e)))?);
        // stmt is dropped at end of loop iteration
      }

      // Sort by relative timestamp to maintain proper order
      messages.sort_by(|a, b| a.relative_timestamp_ms.partial_cmp(&b.relative_timestamp_ms).unwrap_or(std::cmp::Ordering::Equal));
      messages
    }; // All stmt borrows are released here

    log::info!("Mock: Loaded {} total messages for full run '{}'", all_messages.len(), run_id);

    let inner_state = MockConnectionState {
      db_conn: Some(db), // Now we can safely move db
      server_version,
      logged_messages: all_messages,
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
          guard.message_iter_index += 1;

          if guard.handler.is_some() {
            Self::_pump_recv_messages_until_send_or_end(&mut guard)?;
          } else {
            log::warn!("Mock: send matched SEND #{}, but no handler set to pump subsequent RECVs.", expected_message_index + 1);
          }
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
  }

  /// Sets handler, marks connected, simulates implicit StartAPI verification, and triggers initial auto-pump.
  fn set_message_handler(&mut self, handler: MessageHandler) {
    let mut guard = self.inner.lock();

    log::info!("Mock: Setting message handler.");
    if guard.handler.is_some() {
      log::warn!("Mock: Replacing existing message handler.");
    }
    guard.handler = Some(handler);
    guard.connected = true;
    log::info!("Mock: Connection marked as 'connected'.");

    // Simulate implicit StartAPI verification
    if guard.message_iter_index == 0 && !guard.logged_messages.is_empty() {
      let first_message = &guard.logged_messages[0];
      if first_message.direction == LogDirection::Send && first_message.message_type_id == Some(71) {
        log::debug!("Mock: Implicitly verifying logged SEND StartAPI (message #1) during set_message_handler.");
        guard.message_iter_index += 1;
        log::debug!("Mock: Advanced iterator past implicit StartAPI (now at index {}).", guard.message_iter_index);
      } else {
        log::warn!("Mock: First logged message (#1) was not the expected SEND StartAPI (Direction={:?}, TypeID={:?}). Initial state might be incorrect.",
                   first_message.direction, first_message.message_type_id);
      }
    }

    // Trigger initial auto-pump
    match Self::_pump_recv_messages_until_send_or_end(&mut guard) {
      Ok(_) => {
        log::info!("Mock: Initial auto-pump completed after setting handler (current index: {}).", guard.message_iter_index);
      }
      Err(e) => {
        log::error!("Mock: Error during initial auto-pump after setting handler: {:?}. Connection may be in unexpected state.", e);
      }
    }
  }

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
