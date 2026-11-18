// yatws/src/conn_mock.rs

use crate::base::IBKRError;
use crate::conn::Connection;
use crate::conn_log::{select_log_format, LogDirection, SelectedLogFormat};
use crate::handler::MessageHandler;

use std::path::Path;
use std::str::FromStr;

#[cfg(any(feature = "log-sql", feature = "log-txt"))]
use crate::conn_log::bytes_to_center_dot_string;
#[cfg(any(feature = "log-sql", feature = "log-txt"))]
use crate::message_parser::process_message;
#[cfg(any(feature = "log-sql", feature = "log-txt"))]
use parking_lot::Mutex;
#[cfg(any(feature = "log-sql", feature = "log-txt"))]
use std::sync::Arc;

#[cfg(feature = "log-sql")]
use crate::conn_log::CENTER_DOT;
#[cfg(feature = "log-sql")]
use rusqlite::{params, Connection as DbConnection, OpenFlags};

#[cfg(feature = "log-txt")]
use crate::conn_log::{decode_txt_field, decode_txt_payload, encode_txt_field};
#[cfg(feature = "log-txt")]
use std::fs::File;
#[cfg(feature = "log-txt")]
use std::io::{BufRead, BufReader};

// --- Helper Functions ---

#[cfg(feature = "log-sql")]
fn center_dot_string_to_bytes(s: &str) -> Vec<u8> {
  let mut bytes = Vec::with_capacity(s.len());
  let mut dot_buf = [0u8; 4];
  let center_dot_str = CENTER_DOT.encode_utf8(&mut dot_buf);
  let mut current_pos = 0;

  while let Some(found_pos) = s[current_pos..].find(&*center_dot_str) {
    let actual_pos = current_pos + found_pos;
    bytes.extend_from_slice(s[current_pos..actual_pos].as_bytes());
    bytes.push(0);
    current_pos = actual_pos + center_dot_str.len();
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

// --- Data Structures (`log-sql` or `log-txt` enabled) ---

#[cfg(any(feature = "log-sql", feature = "log-txt"))]
#[derive(Debug, Clone)]
struct LoggedMessage {
  direction: LogDirection,
  message_type_id: Option<i32>,
  message_type_name: Option<String>,
  payload_bytes: Vec<u8>,
}

#[cfg(any(feature = "log-sql", feature = "log-txt"))]
struct MockConnectionState {
  server_version: i32,
  logged_messages: Vec<LoggedMessage>,
  message_iter_index: usize,
  handler: Option<MessageHandler>,
  connected: bool,
}

#[derive(Clone)]
pub struct MockConnection {
  #[cfg(any(feature = "log-sql", feature = "log-txt"))]
  inner: Arc<Mutex<MockConnectionState>>,
}

#[cfg(any(feature = "log-sql", feature = "log-txt"))]
impl MockConnection {
  /// Internal helper to process the next RECV message if available.
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

    let next_message_index = state.message_iter_index;
    let next_message = &state.logged_messages[next_message_index];

    match next_message.direction {
      LogDirection::Recv => {
        let is_connected = state.connected;
        let payload_to_process = next_message.payload_bytes.clone();
        let message_type_name_for_log = next_message.message_type_name.clone();

        log::debug!(
          "Mock (AutoPump): Processing RECV message #{}: Type={:?}",
          next_message_index + 1,
          message_type_name_for_log.as_deref().unwrap_or("UNKNOWN")
        );

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
            log::error!(
              "Mock (AutoPump): Error processing RECV message #{}: {:?}",
              next_message_index + 1,
              e
            );
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
          log::debug!(
            "Mock (AutoPump): Batch pump finished (hit SEND or end). Current index: {}",
            state.message_iter_index + 1
          );
          return Ok(());
        }
        Err(e) => {
          log::error!("Mock (AutoPump): Batch pump failed due to handler error.");
          return Err(e);
        }
      }
    }
  }

  #[cfg(feature = "log-sql")]
  fn load_session_from_sql(db_path: &Path, session_name: &str) -> Result<(i32, Vec<LoggedMessage>), IBKRError> {
    log::info!(
      "Creating MockConnection for session '{}' from SQLite DB: {:?} (Strict SEND Check, AutoPump)",
      session_name,
      db_path
    );

    let db = DbConnection::open_with_flags(
      db_path,
      OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| IBKRError::ConfigurationError(format!("Mock: Failed to open logger DB: {}", e)))?;

    let (session_id, server_version_opt): (i64, Option<i32>) = db
      .query_row(
        "SELECT session_id, server_version FROM sessions WHERE session_name = ?1",
        params![session_name],
        |row| Ok((row.get(0)?, row.get(1)?)),
      )
      .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => IBKRError::ConfigurationError(format!(
          "Mock: Log session '{}' not found in database.",
          session_name
        )),
        _ => IBKRError::LoggingError(format!("Mock: Failed to query session '{}': {}", session_name, e)),
      })?;

    let server_version = server_version_opt.ok_or_else(|| {
      IBKRError::ConfigurationError(format!(
        "Mock: Log session '{}' found, but server_version is missing (NULL) in the database.",
        session_name
      ))
    })?;

    log::debug!("Mock: Found session_id={}, server_version={}", session_id, server_version);

    let logged_messages: Vec<LoggedMessage> = {
      let mut stmt = db
        .prepare(
          "SELECT direction, message_type_id, message_type_name, payload_text
                 FROM messages
                 WHERE session_id = ?1
                 ORDER BY message_id ASC",
        )
        .map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to prepare message query: {}", e)))?;

      let messages_iter = stmt
        .query_map(params![session_id], |row| {
          let direction_str: String = row.get(0)?;
          let direction = LogDirection::from_str(&direction_str)
            .map_err(|_| rusqlite::Error::InvalidColumnType(0, "LogDirection".into(), rusqlite::types::Type::Text))?;
          let payload_text: String = row.get(3)?;
          let payload_bytes = center_dot_string_to_bytes(&payload_text);

          Ok(LoggedMessage {
            direction,
            message_type_id: row.get(1)?,
            message_type_name: row.get(2)?,
            payload_bytes,
          })
        })
        .map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to query messages: {}", e)))?;

      messages_iter
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| IBKRError::LoggingError(format!("Mock: Failed to map messages: {}", e)))?
    };

    if let Err(e) = db.close() {
      log::warn!("Mock: Error closing SQLite connection after loading session: {:?}", e);
    }

    log::info!("Mock: Loaded {} messages for session '{}'", logged_messages.len(), session_name);
    Ok((server_version, logged_messages))
  }

  #[cfg(feature = "log-txt")]
  fn split_exact_tabs<'a, const N: usize>(line: &'a str, line_no: usize, kind: &str) -> Result<[&'a str; N], IBKRError> {
    let mut iter = line.split('\t');
    let mut out = [""; N];
    for slot in &mut out {
      *slot = iter.next().ok_or_else(|| {
        IBKRError::ParseError(format!("Mock: Malformed {} line (too few columns) at line {}", kind, line_no))
      })?;
    }
    if iter.next().is_some() {
      return Err(IBKRError::ParseError(format!(
        "Mock: Malformed {} line (unescaped extra tab) at line {}",
        kind, line_no
      )));
    }
    Ok(out)
  }

  #[cfg(feature = "log-txt")]
  fn load_session_from_txt(txt_path: &Path, session_name: &str) -> Result<(i32, Vec<LoggedMessage>), IBKRError> {
    log::info!(
      "Creating MockConnection for session '{}' from text log: {:?} (Strict SEND Check, AutoPump)",
      session_name,
      txt_path
    );

    let file = File::open(txt_path)
      .map_err(|e| IBKRError::ConfigurationError(format!("Mock: Failed to open text log file: {}", e)))?;
    let mut reader = BufReader::new(file);

    let encoded_target = encode_txt_field(session_name);
    let mut session_found = false;
    let mut server_version_opt: Option<i32> = None;
    let mut logged_messages = Vec::new();
    let mut raw_line = String::new();
    let mut line_no = 0usize;

    loop {
      raw_line.clear();
      let n = reader
        .read_line(&mut raw_line)
        .map_err(|e| IBKRError::LoggingError(format!("Mock: Failed reading line {} of text log: {}", line_no + 1, e)))?;
      if n == 0 {
        break;
      }
      line_no += 1;

      let trimmed = raw_line.trim_end_matches(&['\r', '\n'][..]);
      if trimmed.is_empty() || trimmed.starts_with('#') {
        continue;
      }

      if trimmed.starts_with("SESSION\t") {
        let cols = Self::split_exact_tabs::<6>(trimmed, line_no, "SESSION")?;
        if cols[1].contains('\\') {
          let _ = decode_txt_field(cols[1])?;
        }
        if cols[1] == encoded_target {
          session_found = true;
          server_version_opt = None;
          logged_messages.clear();
        }
      } else if trimmed.starts_with("SERVER_VERSION\t") {
        let cols = Self::split_exact_tabs::<3>(trimmed, line_no, "SERVER_VERSION")?;
        if cols[1].contains('\\') {
          let _ = decode_txt_field(cols[1])?;
        }
        if cols[1] == encoded_target {
          let ver = cols[2].parse::<i32>().map_err(|e| {
            IBKRError::ParseError(format!(
              "Mock: Invalid server_version '{}' at line {}: {}",
              cols[2], line_no, e
            ))
          })?;
          server_version_opt = Some(ver);
        }
      } else if trimmed.starts_with("MSG\t") {
        let cols = Self::split_exact_tabs::<7>(trimmed, line_no, "MSG")?;
        if cols[1].contains('\\') {
          let _ = decode_txt_field(cols[1])?;
        }
        if cols[1] == encoded_target {
          let direction = LogDirection::from_str(cols[2])?;
          let message_type_id = if cols[4] == "-" || cols[4].is_empty() {
            None
          } else {
            Some(cols[4].parse::<i32>().map_err(|e| {
              IBKRError::ParseError(format!(
                "Mock: Invalid message_type_id '{}' at line {}: {}",
                cols[4], line_no, e
              ))
            })?)
          };
          let message_type_name = if cols[5] == "-" || cols[5].is_empty() {
            None
          } else {
            Some(cols[5].to_string())
          };
          let payload_bytes = decode_txt_payload(cols[6])?;
          logged_messages.push(LoggedMessage {
            direction,
            message_type_id,
            message_type_name,
            payload_bytes,
          });
        }
      } else {
        return Err(IBKRError::ParseError(format!(
          "Mock: Unrecognized record type at line {}",
          line_no
        )));
      }
    }

    if !session_found {
      return Err(IBKRError::ConfigurationError(format!(
        "Mock: Log session '{}' not found in text log.",
        session_name
      )));
    }

    let server_version = server_version_opt.ok_or_else(|| {
      IBKRError::ConfigurationError(format!(
        "Mock: Log session '{}' found, but server_version is missing in the text log.",
        session_name
      ))
    })?;

    log::info!(
      "Mock: Loaded {} messages for session '{}' from text log",
      logged_messages.len(),
      session_name
    );
    Ok((server_version, logged_messages))
  }

  fn log_payload_mismatch(sent_data: &[u8], expected_data: &[u8]) {
    let max_len = 150;
    let sent_str = bytes_to_center_dot_string(&sent_data[..max_len.min(sent_data.len())]);
    let expected_str = bytes_to_center_dot_string(&expected_data[..max_len.min(expected_data.len())]);
    log::error!("Mock Mismatch Detail:");
    log::error!(
      " > Sent    ({} bytes): '{}'{}",
      sent_data.len(),
      sent_str,
      if sent_data.len() > max_len { "..." } else { "" }
    );
    log::error!(
      " > Expected({} bytes): '{}'{}",
      expected_data.len(),
      expected_str,
      if expected_data.len() > max_len { "..." } else { "" }
    );
  }
}

// --- Public API ---
impl MockConnection {
  pub fn new<P: AsRef<Path>>(log_path: P, session_name: &str) -> Result<Self, IBKRError> {
    let path = log_path.as_ref();
    match select_log_format(path)? {
      #[cfg(feature = "log-sql")]
      SelectedLogFormat::Sql => {
        let (server_version, logged_messages) = Self::load_session_from_sql(path, session_name)?;
        Ok(Self {
          inner: Arc::new(Mutex::new(MockConnectionState {
            server_version,
            logged_messages,
            message_iter_index: 0,
            handler: None,
            connected: false,
          })),
        })
      }
      #[cfg(feature = "log-txt")]
      SelectedLogFormat::Txt => {
        let (server_version, logged_messages) = Self::load_session_from_txt(path, session_name)?;
        Ok(Self {
          inner: Arc::new(Mutex::new(MockConnectionState {
            server_version,
            logged_messages,
            message_iter_index: 0,
            handler: None,
            connected: false,
          })),
        })
      }
      #[cfg(not(any(feature = "log-sql", feature = "log-txt")))]
      SelectedLogFormat::None => {
        let _ = session_name;
        Err(IBKRError::Unsupported(
          "Session replay requires yatws to be compiled with the 'log-sql' or 'log-txt' feature.".to_string(),
        ))
      }
    }
  }
}

#[cfg(any(feature = "log-sql", feature = "log-txt"))]
impl Connection for MockConnection {
  fn is_connected(&self) -> bool {
    self.inner.lock().connected
  }

  fn disconnect(&mut self) -> Result<(), IBKRError> {
    log::info!("Mock: disconnect() called.");
    let mut guard = self.inner.lock();
    guard.connected = false;
    guard.handler = None;
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
        log::debug!(
          "Mock: Verifying send_message_body call against expected SEND message #{}: Type={:?}",
          expected_message_index + 1,
          expected_message.message_type_name.as_deref().unwrap_or("UNKNOWN")
        );

        if data == expected_message.payload_bytes.as_slice() {
          log::debug!(
            "Mock: Sent message matches expected SEND message #{}. Advancing iterator.",
            expected_message_index + 1
          );
          guard.message_iter_index += 1;

          if guard.handler.is_some() {
            Self::_pump_recv_messages_until_send_or_end(&mut guard)?;
          } else {
            log::warn!(
              "Mock: send matched SEND #{}, but no handler set to pump subsequent RECVs.",
              expected_message_index + 1
            );
          }
          Ok(())
        } else {
          log::error!(
            "Mock: Mismatch! Sent message body does not match expected logged SEND message #{}.",
            expected_message_index + 1
          );
          MockConnection::log_payload_mismatch(data, &expected_message.payload_bytes);
          Err(IBKRError::LoggingError(format!(
            "Sent message mismatch at logged message index {}. Expected Type: {:?}",
            expected_message_index + 1,
            expected_message.message_type_name.as_deref().unwrap_or("UNKNOWN")
          )))
        }
      }
      LogDirection::Recv => {
        log::error!(
          "Mock: send_message_body called, but the next expected logged message #{} is RECV, not SEND.",
          expected_message_index + 1
        );
        Err(IBKRError::LoggingError(format!(
          "Unexpected send: Expected next logged message #{} to be SEND, but it was RECV.",
          expected_message_index + 1
        )))
      }
    }
  }

  fn set_message_handler(&mut self, handler: MessageHandler) {
    let mut guard = self.inner.lock();

    log::info!("Mock: Setting message handler.");
    if guard.handler.is_some() {
      log::warn!("Mock: Replacing existing message handler.");
    }
    guard.handler = Some(handler);
    guard.connected = true;
    log::info!("Mock: Connection marked as 'connected'.");

    if guard.message_iter_index == 0 && !guard.logged_messages.is_empty() {
      let first_message = &guard.logged_messages[0];
      if first_message.direction == LogDirection::Send && first_message.message_type_id == Some(71) {
        log::debug!("Mock: Implicitly verifying logged SEND StartAPI (message #1) during set_message_handler.");
        guard.message_iter_index += 1;
        log::debug!("Mock: Advanced iterator past implicit StartAPI (now at index {}).", guard.message_iter_index);
      } else {
        log::warn!(
          "Mock: First logged message (#1) was not the expected SEND StartAPI (Direction={:?}, TypeID={:?}). Initial state might be incorrect.",
          first_message.direction,
          first_message.message_type_id
        );
      }
    }

    match Self::_pump_recv_messages_until_send_or_end(&mut guard) {
      Ok(_) => {
        log::info!(
          "Mock: Initial auto-pump completed after setting handler (current index: {}).",
          guard.message_iter_index
        );
      }
      Err(e) => {
        log::error!(
          "Mock: Error during initial auto-pump after setting handler: {:?}. Connection may be in unexpected state.",
          e
        );
      }
    }
  }

  fn get_server_version(&self) -> i32 {
    self.inner.lock().server_version
  }
}

#[cfg(not(any(feature = "log-sql", feature = "log-txt")))]
impl Connection for MockConnection {
  fn is_connected(&self) -> bool {
    false
  }

  fn disconnect(&mut self) -> Result<(), IBKRError> {
    Ok(())
  }

  fn send_message_body(&mut self, _data: &[u8]) -> Result<(), IBKRError> {
    Err(IBKRError::NotConnected)
  }

  fn set_message_handler(&mut self, _handler: MessageHandler) {}

  fn get_server_version(&self) -> i32 {
    0
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::conn_log::ConnectionLogger;
  use crate::IBKRClient;

  #[cfg(feature = "log-txt")]
  #[test]
  fn test_txt_logger_multisession_overwrite_and_client_replay() {
    let dir = tempfile::tempdir().expect("tempdir");
    let log_path = dir.path().join("sessions.txt");
    let log_path_str = log_path.to_str().unwrap();

    // 1. Log an initial "time_session" that we will overwrite, plus a sibling "keep_session"
    {
      let old = ConnectionLogger::new(&log_path, "time_session", "127.0.0.1", 4002, 1).unwrap();
      old.set_session_server_version(170);
      old.log_message(LogDirection::Send, b"71\02\01\0\0");
      old.log_message(LogDirection::Recv, b"9\01\0100\0");
    }
    {
      let keep = ConnectionLogger::new(&log_path, "keep_session", "127.0.0.1", 4002, 2).unwrap();
      keep.set_session_server_version(176);
      keep.log_message(LogDirection::Send, b"71\02\02\0\0");
      keep.log_message(LogDirection::Recv, b"15\01\0DU99999\0");
      keep.log_message(LogDirection::Recv, b"9\01\0500\0");
    }

    // 2. Re-record "time_session" with server_version=178 and a `request_current_time` ("49\01\0") -> "49\01\01700000000\0"
    {
      let logger = ConnectionLogger::new(&log_path, "time_session", "127.0.0.1", 4002, 1).unwrap();
      logger.set_session_server_version(178);
      logger.log_message(LogDirection::Send, b"71\02\01\0\0"); // StartAPI
      logger.log_message(LogDirection::Recv, b"15\01\0DU12345\0"); // ManagedAccounts
      logger.log_message(LogDirection::Recv, b"9\01\01000\0"); // NextValidId (required by order_init)
      logger.log_message(LogDirection::Send, b"49\01\0"); // ReqCurrentTime
      logger.log_message(LogDirection::Recv, b"49\01\01700000000\0"); // CurrentTime
    }

    // 3. Replay "time_session" via `IBKRClient::from_db` and verify `request_current_time`!
    let client = IBKRClient::from_db(log_path_str, "time_session").expect("txt replay should succeed");
    let ts = client.client().request_current_time().expect("request_current_time replay");
    assert_eq!(ts.timestamp(), 1700000000);

    // 4. Verify sibling "keep_session" was preserved intact!
    let keep_client = IBKRClient::from_db(log_path_str, "keep_session").expect("sibling txt session preserved");
    assert_eq!(keep_client.orders().peek_order_id().unwrap(), "500");

    // 5. Verify extra unescaped tabs in `MSG` lines are rejected with `ParseError` (`#4`)
    let bad_path = dir.path().join("bad_tabs.txt");
    std::fs::write(
      &bad_path,
      "# yatws-txt-log v1\nSESSION\ts1\t1\t127.0.0.1\t4002\t1\nSERVER_VERSION\ts1\t178\nMSG\ts1\tSEND\t0.1\t71\tSTART_API\t71·2·1·\textra\n",
    )
    .unwrap();
    assert!(matches!(
      MockConnection::new(&bad_path, "s1"),
      Err(IBKRError::ParseError(_))
    ));
  }

  #[cfg(all(feature = "log-txt", not(feature = "log-sql")))]
  #[test]
  fn test_txt_only_mode_rejects_sqlite_magic_with_unsupported() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fake_sqlite = dir.path().join("yatws_golden.db");
    std::fs::write(&fake_sqlite, b"SQLite format 3\0dummy header bytes").unwrap();
    assert!(matches!(
      MockConnection::new(&fake_sqlite, "any"),
      Err(IBKRError::Unsupported(_))
    ));
    assert!(matches!(
      ConnectionLogger::new(&fake_sqlite, "any", "127.0.0.1", 4002, 1),
      Err(IBKRError::Unsupported(_))
    ));
  }

  #[cfg(all(feature = "log-sql", not(feature = "log-txt")))]
  #[test]
  fn test_sql_only_mode_rejects_txt_magic_with_unsupported() {
    let dir = tempfile::tempdir().expect("tempdir");
    let txt_file = dir.path().join("sessions.txt");
    std::fs::write(&txt_file, b"# yatws-txt-log v1\nSESSION\ts\t1\th\t4002\t1\n").unwrap();
    assert!(matches!(
      MockConnection::new(&txt_file, "s"),
      Err(IBKRError::Unsupported(_))
    ));
  }

  #[cfg(feature = "log-sql")]
  #[test]
  fn test_sql_logger_and_client_replay() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("sessions.db");
    let db_path_str = db_path.to_str().unwrap();

    // Missing SQLite file must NOT create an empty file on disk when opened read-only (`Nit #4`)
    let missing_path = dir.path().join("does_not_exist.db");
    assert!(MockConnection::new(&missing_path, "none").is_err());
    assert!(!missing_path.exists());

    {
      let logger = ConnectionLogger::new(&db_path, "sql_time_session", "127.0.0.1", 4002, 1).unwrap();
      logger.set_session_server_version(178);
      logger.log_message(LogDirection::Send, b"71\02\01\0\0");
      logger.log_message(LogDirection::Recv, b"15\01\0DU12345\0");
      logger.log_message(LogDirection::Recv, b"9\01\042\0");
      logger.log_message(LogDirection::Send, b"49\01\0");
      logger.log_message(LogDirection::Recv, b"49\01\01700000123\0");
    }

    let client = IBKRClient::from_db(db_path_str, "sql_time_session").expect("sql replay should succeed");
    let ts = client.client().request_current_time().expect("request_current_time sql replay");
    assert_eq!(ts.timestamp(), 1700000123);
  }

  #[cfg(not(any(feature = "log-sql", feature = "log-txt")))]
  #[test]
  fn test_none_logger_noop_and_from_db_returns_unsupported() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ignored_path = dir.path().join("ignored.log");
    let ignored_str = ignored_path.to_str().unwrap();

    let logger = ConnectionLogger::new(&ignored_path, "noop_session", "127.0.0.1", 4002, 1)
      .expect("Noop logger creation should succeed");
    logger.set_session_server_version(178);
    logger.log_message(LogDirection::Send, b"71\02\01\0\0");
    assert!(!ignored_path.exists());

    let res = IBKRClient::from_db(ignored_str, "noop_session");
    assert!(matches!(res, Err(IBKRError::Unsupported(_))));
  }
}
