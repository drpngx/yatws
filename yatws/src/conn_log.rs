// yatws/src/conn_log.rs

use crate::base::IBKRError;
use crate::protocol_decoder::IncomingMessageType;
use crate::protocol_encoder;

use rusqlite::{params, Connection as DbConnection};
use std::fmt;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use chrono::{DateTime, TimeZone, Utc};

// --- Constants ---
pub(crate) const CENTER_DOT: char = '·'; // Unicode U+00B7

// --- Static Map for Incoming Message Types ---
static INCOMING_TYPE_MAP: Lazy<HashMap<i32, &'static str>> = Lazy::new(|| {
  let mut m = HashMap::new();
  m.insert(IncomingMessageType::TickPrice as i32, "TICK_PRICE");
  m.insert(IncomingMessageType::TickSize as i32, "TICK_SIZE");
  m.insert(IncomingMessageType::OrderStatus as i32, "ORDER_STATUS");
  m.insert(IncomingMessageType::ErrorMessage as i32, "ERROR_MSG");
  m.insert(IncomingMessageType::OpenOrder as i32, "OPEN_ORDER");
  m.insert(IncomingMessageType::AccountValue as i32, "ACCT_VALUE");
  m.insert(IncomingMessageType::PortfolioValue as i32, "PORTFOLIO_VALUE");
  m.insert(IncomingMessageType::AccountUpdateTime as i32, "ACCT_UPDATE_TIME");
  m.insert(IncomingMessageType::NextValidId as i32, "NEXT_VALID_ID");
  m.insert(IncomingMessageType::ContractData as i32, "CONTRACT_DATA");
  m.insert(IncomingMessageType::ExecutionData as i32, "EXEC_DETAILS");
  m.insert(IncomingMessageType::MarketDepth as i32, "MARKET_DEPTH");
  m.insert(IncomingMessageType::MarketDepthL2 as i32, "MARKET_DEPTH_L2");
  m.insert(IncomingMessageType::NewsBulletins as i32, "NEWS_BULLETINS");
  m.insert(IncomingMessageType::ManagedAccounts as i32, "MANAGED_ACCTS");
  m.insert(IncomingMessageType::ReceiveFA as i32, "RECEIVE_FA");
  m.insert(IncomingMessageType::HistoricalData as i32, "HISTORICAL_DATA");
  m.insert(IncomingMessageType::BondContractData as i32, "BOND_CONTRACT_DATA");
  m.insert(IncomingMessageType::ScannerParameters as i32, "SCANNER_PARAMETERS");
  m.insert(IncomingMessageType::ScannerData as i32, "SCANNER_DATA");
  m.insert(IncomingMessageType::TickOptionComputation as i32, "TICK_OPTION_COMPUTATION");
  m.insert(IncomingMessageType::TickGeneric as i32, "TICK_GENERIC");
  m.insert(IncomingMessageType::TickString as i32, "TICK_STRING");
  m.insert(IncomingMessageType::TickEFP as i32, "TICK_EFP");
  m.insert(IncomingMessageType::CurrentTime as i32, "CURRENT_TIME");
  m.insert(IncomingMessageType::RealTimeBars as i32, "REAL_TIME_BARS");
  m.insert(IncomingMessageType::FundamentalData as i32, "FUNDAMENTAL_DATA");
  m.insert(IncomingMessageType::ContractDataEnd as i32, "CONTRACT_DATA_END");
  m.insert(IncomingMessageType::OpenOrderEnd as i32, "OPEN_ORDER_END");
  m.insert(IncomingMessageType::AccountDownloadEnd as i32, "ACCT_DOWNLOAD_END");
  m.insert(IncomingMessageType::ExecutionDataEnd as i32, "EXEC_DETAILS_END");
  m.insert(IncomingMessageType::DeltaNeutralValidation as i32, "DELTA_NEUTRAL_VALIDATION");
  m.insert(IncomingMessageType::TickSnapshotEnd as i32, "TICK_SNAPSHOT_END");
  m.insert(IncomingMessageType::MarketDataType as i32, "MARKET_DATA_TYPE");
  m.insert(IncomingMessageType::CommissionReport as i32, "COMMISSION_REPORT");
  m.insert(IncomingMessageType::Position as i32, "POSITION_DATA");
  m.insert(IncomingMessageType::PositionEnd as i32, "POSITION_END");
  m.insert(IncomingMessageType::AccountSummary as i32, "ACCOUNT_SUMMARY");
  m.insert(IncomingMessageType::AccountSummaryEnd as i32, "ACCOUNT_SUMMARY_END");
  m.insert(IncomingMessageType::VerifyMessageAPI as i32, "VERIFY_MESSAGE_API");
  m.insert(IncomingMessageType::VerifyCompleted as i32, "VERIFY_COMPLETED");
  m.insert(IncomingMessageType::DisplayGroupList as i32, "DISPLAY_GROUP_LIST");
  m.insert(IncomingMessageType::DisplayGroupUpdated as i32, "DISPLAY_GROUP_UPDATED");
  m.insert(IncomingMessageType::VerifyAndAuthMessageAPI as i32, "VERIFY_AND_AUTH_MESSAGE_API");
  m.insert(IncomingMessageType::VerifyAndAuthCompleted as i32, "VERIFY_AND_AUTH_COMPLETED");
  m.insert(IncomingMessageType::PositionMulti as i32, "POSITION_MULTI");
  m.insert(IncomingMessageType::PositionMultiEnd as i32, "POSITION_MULTI_END");
  m.insert(IncomingMessageType::AccountUpdateMulti as i32, "ACCOUNT_UPDATE_MULTI");
  m.insert(IncomingMessageType::AccountUpdateMultiEnd as i32, "ACCOUNT_UPDATE_MULTI_END");
  m.insert(IncomingMessageType::SecurityDefinitionOptionParameter as i32, "SECURITY_DEFINITION_OPTION_PARAMETER");
  m.insert(IncomingMessageType::SecurityDefinitionOptionParameterEnd as i32, "SECURITY_DEFINITION_OPTION_PARAMETER_END");
  m.insert(IncomingMessageType::SoftDollarTiers as i32, "SOFT_DOLLAR_TIERS");
  m.insert(IncomingMessageType::FamilyCodes as i32, "FAMILY_CODES");
  m.insert(IncomingMessageType::SymbolSamples as i32, "SYMBOL_SAMPLES");
  m.insert(IncomingMessageType::MktDepthExchanges as i32, "MKT_DEPTH_EXCHANGES");
  m.insert(IncomingMessageType::TickReqParams as i32, "TICK_REQ_PARAMS");
  m.insert(IncomingMessageType::SmartComponents as i32, "SMART_COMPONENTS");
  m.insert(IncomingMessageType::NewsArticle as i32, "NEWS_ARTICLE");
  m.insert(IncomingMessageType::TickNews as i32, "TICK_NEWS");
  m.insert(IncomingMessageType::NewsProviders as i32, "NEWS_PROVIDERS");
  m.insert(IncomingMessageType::HistoricalNews as i32, "HISTORICAL_NEWS");
  m.insert(IncomingMessageType::HistoricalNewsEnd as i32, "HISTORICAL_NEWS_END");
  m.insert(IncomingMessageType::HeadTimestamp as i32, "HEAD_TIMESTAMP");
  m.insert(IncomingMessageType::HistogramData as i32, "HISTOGRAM_DATA");
  m.insert(IncomingMessageType::HistoricalDataUpdate as i32, "HISTORICAL_DATA_UPDATE");
  m.insert(IncomingMessageType::RerouteMktDataReq as i32, "REROUTE_MKT_DATA_REQ");
  m.insert(IncomingMessageType::RerouteMktDepthReq as i32, "REROUTE_MKT_DEPTH_REQ");
  m.insert(IncomingMessageType::MarketRule as i32, "MARKET_RULE");
  m.insert(IncomingMessageType::PnL as i32, "PNL");
  m.insert(IncomingMessageType::PnLSingle as i32, "PNL_SINGLE");
  m.insert(IncomingMessageType::HistoricalTicks as i32, "HISTORICAL_TICKS");
  m.insert(IncomingMessageType::HistoricalTicksBidAsk as i32, "HISTORICAL_TICKS_BID_ASK");
  m.insert(IncomingMessageType::HistoricalTicksLast as i32, "HISTORICAL_TICKS_LAST");
  m.insert(IncomingMessageType::TickByTick as i32, "TICK_BY_TICK");
  m.insert(IncomingMessageType::OrderBound as i32, "ORDER_BOUND");
  m.insert(IncomingMessageType::CompletedOrder as i32, "COMPLETED_ORDER");
  m.insert(IncomingMessageType::CompletedOrdersEnd as i32, "COMPLETED_ORDERS_END");
  m.insert(IncomingMessageType::ReplaceFAEnd as i32, "REPLACE_FA_END");
  m.insert(IncomingMessageType::WshMetaData as i32, "WSH_META_DATA");
  m.insert(IncomingMessageType::WshEventData as i32, "WSH_EVENT_DATA");
  m.insert(IncomingMessageType::HistoricalSchedule as i32, "HISTORICAL_SCHEDULE");
  m.insert(IncomingMessageType::UserInfo as i32, "USER_INFO");
  m
});

// --- Public Types for Session Management ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
  Completed,
  Failed,
}

impl fmt::Display for SessionStatus {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      SessionStatus::Completed => write!(f, "completed"),
      SessionStatus::Failed => write!(f, "failed"),
    }
  }
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
  pub id: i64,
  pub session_name: String,
  pub parent_id: Option<i64>,
  pub created_at: DateTime<Utc>,
  pub completed_at: Option<DateTime<Utc>>,
  pub status: String,
  pub host: String,
  pub port: u16,
  pub client_id: i32,
  pub server_version: Option<i32>,
}

// --- Helper Functions ---
pub(crate) fn bytes_to_center_dot_string(bytes: &[u8]) -> String {
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

fn parse_message_type_id(payload: &[u8]) -> Option<i32> {
  payload
    .iter()
    .position(|&b| b == 0)
    .and_then(|end_pos| {
      if end_pos == 0 { return None; }
      std::str::from_utf8(&payload[0..end_pos]).ok()
    })
    .and_then(|id_str| id_str.parse::<i32>().ok())
}

// --- Logger Direction and Internal State ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogDirection {
  Send,
  Recv,
}

impl fmt::Display for LogDirection {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      LogDirection::Send => write!(f, "SEND"),
      LogDirection::Recv => write!(f, "RECV"),
    }
  }
}

#[derive(Debug)]
struct LoggerState {
  parent_session_id: i64,
  current_session_id: i64, // Points to parent or active child
  active_child_name: Option<String>,
}

struct ConnectionLoggerInner {
  db: DbConnection,
  logger_state: LoggerState,
  start_time_instant: Instant,
}

#[derive(Clone)]
pub struct ConnectionLogger {
  inner: Arc<Mutex<ConnectionLoggerInner>>,
}

impl ConnectionLogger {
  /// Creates a logger for a run (parent session)
  pub fn new<P: AsRef<Path>>(
    db_path: P,
    run_id: &str,
    host: &str,
    port: u16,
    client_id: i32,
  ) -> Result<Self, IBKRError> {
    log::info!(
      "Initializing connection logger at path: {:?}, Run ID: '{}'",
      db_path.as_ref(), run_id
    );

    let mut db = DbConnection::open(db_path)
      .map_err(|e| IBKRError::ConfigurationError(format!("Failed to open logger database: {}", e)))?;

    db.pragma_update(None, "journal_mode", "WAL")
      .map_err(|e| IBKRError::ConfigurationError(format!("Failed to set WAL mode: {}", e)))?;
    db.execute("PRAGMA foreign_keys = ON;", [])
      .map_err(|e| IBKRError::ConfigurationError(format!("Failed to enable foreign keys: {}", e)))?;

    Self::create_tables(&db)?;

    // Clear any existing sessions with this run_id
    Self::clear_run_sessions_db(&mut db, run_id)?;

    let start_time_instant = Instant::now();
    let start_time_system = SystemTime::now();
    let start_time_unix_ms = start_time_system
      .duration_since(UNIX_EPOCH)
      .map_err(|e| IBKRError::InternalError(format!("System time error: {}", e)))?
      .as_millis() as i64;

    // Create parent session
    let parent_session_id = Self::insert_parent_session(
      &mut db,
      run_id,
      start_time_unix_ms,
      host,
      port,
      client_id,
    )?;

    log::info!("Started parent session ID: {} for run '{}'", parent_session_id, run_id);

    let logger_state = LoggerState {
      parent_session_id,
      current_session_id: parent_session_id, // Start logging to parent
      active_child_name: None,
    };

    let inner_state = ConnectionLoggerInner {
      db,
      logger_state,
      start_time_instant,
    };

    Ok(Self {
      inner: Arc::new(Mutex::new(inner_state)),
    })
  }

  fn create_tables(db: &DbConnection) -> Result<(), IBKRError> {
    db.execute_batch(
      "BEGIN;
       CREATE TABLE IF NOT EXISTS sessions (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           session_name TEXT NOT NULL,
           host TEXT NOT NULL,
           port INTEGER NOT NULL,
           client_id INTEGER NOT NULL,
           server_version INTEGER,
           connection_time TEXT,
           parent_id INTEGER REFERENCES sessions(id) ON DELETE CASCADE,
           created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
           completed_at DATETIME,
           status TEXT DEFAULT 'active'
       );

       CREATE TABLE IF NOT EXISTS session_messages (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           session_id INTEGER NOT NULL,
           direction TEXT NOT NULL CHECK(direction IN ('SEND', 'RECV')),
           relative_timestamp_ms REAL NOT NULL,
           message_type_id INTEGER NULL,
           message_type_name TEXT NULL,
           payload_text TEXT NOT NULL,
           FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
       );

       -- Indexes for performance
       CREATE INDEX IF NOT EXISTS idx_sessions_parent ON sessions(parent_id);
       CREATE INDEX IF NOT EXISTS idx_session_messages_session ON session_messages(session_id);

       -- Ensure parent sessions (runs) have unique names using partial unique index
       CREATE UNIQUE INDEX IF NOT EXISTS idx_unique_parent_session_name
       ON sessions(session_name) WHERE parent_id IS NULL;

       -- Auto-cleanup when last child is deleted
       CREATE TRIGGER IF NOT EXISTS cleanup_empty_parents
       AFTER DELETE ON sessions
       WHEN OLD.parent_id IS NOT NULL
         AND (SELECT COUNT(*) FROM sessions WHERE parent_id = OLD.parent_id) = 0
       BEGIN
           DELETE FROM sessions WHERE id = OLD.parent_id;
       END;

       COMMIT;"
    ).map_err(|e| IBKRError::ConfigurationError(format!("Failed to create logger tables: {}", e)))?;
    Ok(())
  }

  fn clear_run_sessions_db(db: &mut DbConnection, run_id: &str) -> Result<(), IBKRError> {
    let deleted_count = db.execute(
      "DELETE FROM sessions WHERE session_name = ?1 AND parent_id IS NULL",
      params![run_id],
    ).map_err(|e| IBKRError::LoggingError(format!("Failed to clear run '{}': {}", run_id, e)))?;

    if deleted_count > 0 {
      log::warn!("Cleared {} previous run session(s) named '{}'", deleted_count, run_id);
    }

    Ok(())
  }

  fn insert_parent_session(
    db: &mut DbConnection,
    session_name: &str,
    start_time_unix_ms: i64,
    host: &str,
    port: u16,
    client_id: i32,
  ) -> Result<i64, IBKRError> {
    db.execute(
      "INSERT INTO sessions (session_name, host, port, client_id, created_at, parent_id)
       VALUES (?1, ?2, ?3, ?4, datetime(?5/1000, 'unixepoch'), NULL)",
      params![session_name, host, port, client_id, start_time_unix_ms],
    ).map_err(|e| IBKRError::LoggingError(format!("Failed to insert parent session '{}': {}", session_name, e)))?;

    Ok(db.last_insert_rowid())
  }

  /// Sets the server version for the current parent session
  pub fn set_session_server_version(&self, server_version: i32) {
    match self.inner.lock() {
      Ok(guard) => {
        let parent_id = guard.logger_state.parent_session_id;
        match guard.db.execute(
          "UPDATE sessions SET server_version = ?1 WHERE id = ?2",
          params![server_version, parent_id],
        ) {
          Ok(updated_rows) => {
            if updated_rows == 1 {
              log::info!("Logged server_version={} for parent session_id={}", server_version, parent_id);
            } else {
              log::warn!("Failed to log server_version={} for parent session_id={}: Session not found", server_version, parent_id);
            }
          }
          Err(e) => {
            log::error!("Failed to update server_version in logger database for parent session_id={}: {}", parent_id, e);
          }
        }
      }
      Err(poisoned) => {
        log::error!("ConnectionLogger mutex poisoned while setting server version: {}", poisoned);
      }
    }
  }

  /// Starts a new child session for a test
  pub fn start_session(&self, test_name: &str) -> Result<(), IBKRError> {
    let mut guard = self.inner.lock()
      .map_err(|e| IBKRError::InternalError(format!("Logger mutex poisoned: {}", e)))?;

    if guard.logger_state.active_child_name.is_some() {
      return Err(IBKRError::AlreadyRunning(format!(
        "Child session '{}' already active, cannot start '{}'",
        guard.logger_state.active_child_name.as_ref().unwrap(),
        test_name
      )));
    }

    let start_time_system = SystemTime::now();
    let start_time_unix_ms = start_time_system
      .duration_since(UNIX_EPOCH)
      .map_err(|e| IBKRError::InternalError(format!("System time error: {}", e)))?
      .as_millis() as i64;

    // Create child session
    guard.db.execute(
      "INSERT INTO sessions (session_name, host, port, client_id, created_at, parent_id)
       SELECT ?1, host, port, client_id, datetime(?2/1000, 'unixepoch'), id
       FROM sessions WHERE id = ?3",
      params![test_name, start_time_unix_ms, guard.logger_state.parent_session_id],
    ).map_err(|e| IBKRError::LoggingError(format!("Failed to create child session '{}': {}", test_name, e)))?;

    let child_session_id = guard.db.last_insert_rowid();

    // Switch logging target to child
    guard.logger_state.current_session_id = child_session_id;
    guard.logger_state.active_child_name = Some(test_name.to_string());

    log::info!("Started child session '{}' (ID: {}) under parent ID: {}",
               test_name, child_session_id, guard.logger_state.parent_session_id);

    Ok(())
  }

  /// Ends the current child session
  pub fn end_session(&self, status: SessionStatus) -> Result<(), IBKRError> {
    let mut guard = self.inner.lock()
      .map_err(|e| IBKRError::InternalError(format!("Logger mutex poisoned: {}", e)))?;

    let child_name = guard.logger_state.active_child_name.take()
      .ok_or_else(|| IBKRError::InternalError("No active child session to end".to_string()))?;

    let current_time_system = SystemTime::now();
    let current_time_unix_ms = current_time_system
      .duration_since(UNIX_EPOCH)
      .map_err(|e| IBKRError::InternalError(format!("System time error: {}", e)))?
      .as_millis() as i64;

    // Mark child session complete
    guard.db.execute(
      "UPDATE sessions SET completed_at = datetime(?1/1000, 'unixepoch'), status = ?2 WHERE id = ?3",
      params![current_time_unix_ms, status.to_string(), guard.logger_state.current_session_id],
    ).map_err(|e| IBKRError::LoggingError(format!("Failed to complete child session '{}': {}", child_name, e)))?;

    // Switch back to parent
    guard.logger_state.current_session_id = guard.logger_state.parent_session_id;

    log::info!("Ended child session '{}' with status: {}", child_name, status);

    Ok(())
  }

  /// Completes the entire run
  pub fn complete_run(&self, status: SessionStatus) -> Result<(), IBKRError> {
    let mut guard = self.inner.lock()
      .map_err(|e| IBKRError::InternalError(format!("Logger mutex poisoned: {}", e)))?;

    if guard.logger_state.active_child_name.is_some() {
      return Err(IBKRError::InternalError(
        "Cannot complete run while child session is active".to_string()
      ));
    }

    let current_time_system = SystemTime::now();
    let current_time_unix_ms = current_time_system
      .duration_since(UNIX_EPOCH)
      .map_err(|e| IBKRError::InternalError(format!("System time error: {}", e)))?
      .as_millis() as i64;

    // Mark parent session complete
    guard.db.execute(
      "UPDATE sessions SET completed_at = datetime(?1/1000, 'unixepoch'), status = ?2 WHERE id = ?3",
      params![current_time_unix_ms, status.to_string(), guard.logger_state.parent_session_id],
    ).map_err(|e| IBKRError::LoggingError(format!("Failed to complete run: {}", e)))?;

    log::info!("Completed run (parent session ID: {}) with status: {}",
               guard.logger_state.parent_session_id, status);

    Ok(())
  }

  /// Clear all sessions for a run_id (parent + all children)
  pub fn clear_run_sessions<P: AsRef<Path>>(db_path: P, run_id: &str) -> Result<(), IBKRError> {
    let mut db = DbConnection::open(db_path)
      .map_err(|e| IBKRError::ConfigurationError(format!("Failed to open logger database: {}", e)))?;

    Self::clear_run_sessions_db(&mut db, run_id)
  }

  /// Find latest run (most recent parent session)
  pub fn find_latest_run<P: AsRef<Path>>(db_path: P, run_id: &str) -> Result<Option<SessionInfo>, IBKRError> {
    let db = DbConnection::open(db_path)
      .map_err(|e| IBKRError::ConfigurationError(format!("Failed to open logger database: {}", e)))?;

    let result = db.query_row(
      "SELECT id, session_name, parent_id, created_at, completed_at, status, host, port, client_id, server_version
       FROM sessions
       WHERE session_name = ?1 AND parent_id IS NULL
       ORDER BY created_at DESC LIMIT 1",
      params![run_id],
      |row| {
        let created_at_str: String = row.get(3)?;
        let completed_at_str: Option<String> = row.get(4)?;

        let created_at = DateTime::parse_from_rfc3339(&format!("{}Z", created_at_str))
          .map_err(|_| rusqlite::Error::InvalidColumnType(3, "DateTime".into(), rusqlite::types::Type::Text))?
          .with_timezone(&Utc);

        let completed_at = completed_at_str
          .map(|s| DateTime::parse_from_rfc3339(&format!("{}Z", s))
               .map(|dt| dt.with_timezone(&Utc)))
          .transpose()
          .map_err(|_| rusqlite::Error::InvalidColumnType(4, "DateTime".into(), rusqlite::types::Type::Text))?;

        Ok(SessionInfo {
          id: row.get(0)?,
          session_name: row.get(1)?,
          parent_id: row.get(2)?,
          created_at,
          completed_at,
          status: row.get(5)?,
          host: row.get(6)?,
          port: row.get(7)?,
          client_id: row.get(8)?,
          server_version: row.get(9)?,
        })
      },
    );

    match result {
      Ok(session_info) => Ok(Some(session_info)),
      Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
      Err(e) => Err(IBKRError::LoggingError(format!("Failed to query latest run '{}': {}", run_id, e))),
    }
  }

  /// Get completed child sessions for a run
  pub fn get_completed_tests<P: AsRef<Path>>(db_path: P, run_id: &str) -> Result<Vec<String>, IBKRError> {
    let db = DbConnection::open(db_path)
      .map_err(|e| IBKRError::ConfigurationError(format!("Failed to open logger database: {}", e)))?;

    // First find the parent session
    let parent_id: i64 = db.query_row(
      "SELECT id FROM sessions WHERE session_name = ?1 AND parent_id IS NULL ORDER BY created_at DESC LIMIT 1",
      params![run_id],
      |row| row.get(0),
    ).map_err(|e| match e {
      rusqlite::Error::QueryReturnedNoRows => IBKRError::InvalidParameter(format!("Run '{}' not found", run_id)),
      _ => IBKRError::LoggingError(format!("Failed to find parent session for run '{}': {}", run_id, e)),
    })?;

    // Get completed child sessions
    let mut stmt = db.prepare(
      "SELECT session_name FROM sessions
       WHERE parent_id = ?1 AND status = 'completed'
       ORDER BY created_at ASC"
    ).map_err(|e| IBKRError::LoggingError(format!("Failed to prepare query for completed tests: {}", e)))?;

    let test_names: Result<Vec<String>, _> = stmt.query_map(params![parent_id], |row| {
      Ok(row.get::<_, String>(0)?)
    }).map_err(|e| IBKRError::LoggingError(format!("Failed to query completed tests: {}", e)))?
      .collect();

    test_names.map_err(|e| IBKRError::LoggingError(format!("Failed to collect completed tests: {}", e)))
  }

  /// Logs a single message (sent or received) to the current session
  pub fn log_message(&self, direction: LogDirection, payload: &[u8]) {
    let message_type_id = parse_message_type_id(payload);
    let message_type_name = match direction {
      LogDirection::Send => message_type_id.and_then(|_id| {
        protocol_encoder::identify_outgoing_type(payload)
      }),
      LogDirection::Recv => message_type_id.and_then(|id| {
        INCOMING_TYPE_MAP.get(&id).copied()
      }),
    };
    let payload_text = bytes_to_center_dot_string(payload);

    match self.inner.lock() {
      Ok(guard) => {
        let now_instant = Instant::now();
        let relative_timestamp_s = now_instant
          .duration_since(guard.start_time_instant)
          .as_secs_f64();

        match guard.db.execute(
          "INSERT INTO session_messages (session_id, direction, relative_timestamp_ms, message_type_id, message_type_name, payload_text)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
          params![
            guard.logger_state.current_session_id,
            direction.to_string(),
            relative_timestamp_s * 1000.0,
            message_type_id,
            message_type_name,
            payload_text
          ],
        ) {
          Ok(_) => {
            log::trace!(
              "Logged {} message to session {}: Type={:?}, RelTime={:.3}ms, Size={}",
              direction,
              guard.logger_state.current_session_id,
              message_type_name.unwrap_or("UNKNOWN"),
              relative_timestamp_s * 1000.0,
              payload.len()
            );
          }
          Err(e) => {
            log::error!("Failed to log message to database: {}", e);
          }
        }
      }
      Err(poisoned) => {
        log::error!("ConnectionLogger mutex poisoned: {}", poisoned);
      }
    }
  }
}
