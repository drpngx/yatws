// yatws/src/conn_log.rs

use crate::base::IBKRError;
use crate::protocol_decoder::IncomingMessageType;
use crate::protocol_encoder::{self, OutgoingMessageType}; // Use the encoder's identify function

use rusqlite::{params, Connection as DbConnection, OptionalExtension, Result as SqlResult};
use std::fmt;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use once_cell::sync::Lazy; // Using once_cell for the map
use std::collections::HashMap;

// --- Constants ---
pub(crate) const CENTER_DOT: char = '·'; // Unicode U+00B7

// --- Static Map for Incoming Message Types ---
// (Keep INCOMING_TYPE_MAP as is)
static INCOMING_TYPE_MAP: Lazy<HashMap<i32, &'static str>> = Lazy::new(|| {
  let mut m = HashMap::new();
  // ... (all entries from original file) ...
  m.insert(IncomingMessageType::TickPrice as i32, "TICK_PRICE");
  m.insert(IncomingMessageType::TickSize as i32, "TICK_SIZE");
  m.insert(IncomingMessageType::OrderStatus as i32, "ORDER_STATUS");
  m.insert(IncomingMessageType::ErrorMessage as i32, "ERROR_MSG"); // Renamed for clarity
  m.insert(IncomingMessageType::OpenOrder as i32, "OPEN_ORDER");
  m.insert(IncomingMessageType::AccountValue as i32, "ACCT_VALUE"); // Renamed
  m.insert(IncomingMessageType::PortfolioValue as i32, "PORTFOLIO_VALUE");
  m.insert(IncomingMessageType::AccountUpdateTime as i32, "ACCT_UPDATE_TIME"); // Renamed
  m.insert(IncomingMessageType::NextValidId as i32, "NEXT_VALID_ID");
  m.insert(IncomingMessageType::ContractData as i32, "CONTRACT_DATA");
  m.insert(IncomingMessageType::ExecutionData as i32, "EXEC_DETAILS"); // Renamed
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
  m.insert(IncomingMessageType::AccountDownloadEnd as i32, "ACCT_DOWNLOAD_END"); // Renamed
  m.insert(IncomingMessageType::ExecutionDataEnd as i32, "EXEC_DETAILS_END"); // Renamed
  m.insert(IncomingMessageType::DeltaNeutralValidation as i32, "DELTA_NEUTRAL_VALIDATION");
  m.insert(IncomingMessageType::TickSnapshotEnd as i32, "TICK_SNAPSHOT_END");
  m.insert(IncomingMessageType::MarketDataType as i32, "MARKET_DATA_TYPE");
  m.insert(IncomingMessageType::CommissionReport as i32, "COMMISSION_REPORT");
  m.insert(IncomingMessageType::Position as i32, "POSITION_DATA"); // Renamed
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

// --- Helper Functions ---
// (Keep bytes_to_center_dot_string and parse_message_type_id as is)
fn bytes_to_center_dot_string(bytes: &[u8]) -> String {
  let mut modified_bytes = Vec::with_capacity(bytes.len());
  for &byte in bytes {
    if byte == 0 {
      // Append the UTF-8 bytes for CENTER_DOT
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
    .position(|&b| b == 0) // Find the first null terminator
    .and_then(|end_pos| {
      if end_pos == 0 { return None; } // Empty ID field
      std::str::from_utf8(&payload[0..end_pos]).ok() // Convert ID part to string
    })
    .and_then(|id_str| id_str.parse::<i32>().ok()) // Parse string to i32
}


// --- Logger Struct ---

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

struct ConnectionLoggerInner {
  db: DbConnection,
  session_id: i64,
  start_time_instant: Instant,
}

#[derive(Clone)]
pub struct ConnectionLogger {
  inner: Arc<Mutex<ConnectionLoggerInner>>,
}

impl ConnectionLogger {
  pub fn new<P: AsRef<Path>>(
    db_path: P,
    session_name: &str,
    host: &str,
    port: u16,
    client_id: i32,
  ) -> Result<Self, IBKRError> {
    log::info!(
      "Initializing connection logger at path: {:?}, Session Name: '{}'",
      db_path.as_ref(), session_name
    );
    let mut db = DbConnection::open(db_path)
      .map_err(|e| IBKRError::ConfigurationError(format!("Failed to open logger database: {}", e)))?;

    db.pragma_update(None, "journal_mode", "WAL")
      .map_err(|e| IBKRError::ConfigurationError(format!("Failed to set WAL mode: {}", e)))?;
    db.execute("PRAGMA foreign_keys = ON;", [])
      .map_err(|e| IBKRError::ConfigurationError(format!("Failed to enable foreign keys: {}", e)))?;

    Self::create_tables(&db)?;

    let start_time_instant = Instant::now();
    let start_time_system = SystemTime::now();
    let start_time_unix_ms = start_time_system
      .duration_since(UNIX_EPOCH)
      .map_err(|e| IBKRError::InternalError(format!("System time error: {}", e)))?
      .as_millis() as i64;

    let session_id = Self::delete_and_insert_session(
      &mut db,
      session_name,
      start_time_unix_ms,
      host,
      port,
      client_id,
    )?;

    log::info!("Started logger session ID: {} for name '{}'", session_id, session_name);

    let inner_state = ConnectionLoggerInner {
      db,
      session_id,
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
                 session_id          INTEGER PRIMARY KEY AUTOINCREMENT,
                 session_name        TEXT NOT NULL UNIQUE,
                 start_time_unix_ms  INTEGER NOT NULL,
                 host                TEXT NOT NULL,
                 port                INTEGER NOT NULL,
                 client_id           INTEGER NOT NULL,
                 server_version      INTEGER NULL -- <<< ADDED: Make nullable initially
             );
             CREATE TABLE IF NOT EXISTS messages (
                 message_id            INTEGER PRIMARY KEY AUTOINCREMENT,
                 session_id            INTEGER NOT NULL,
                 direction             TEXT NOT NULL CHECK(direction IN ('SEND', 'RECV')),
                 relative_timestamp_ms REAL NOT NULL,
                 message_type_id       INTEGER NULL,
                 message_type_name     TEXT NULL,
                 payload_text          TEXT NOT NULL,
                 FOREIGN KEY (session_id) REFERENCES sessions(session_id) ON DELETE CASCADE
             );
             -- Indices
             CREATE INDEX IF NOT EXISTS idx_sessions_name ON sessions (session_name);
             CREATE INDEX IF NOT EXISTS idx_messages_session_time ON messages (session_id, relative_timestamp_ms);
             CREATE INDEX IF NOT EXISTS idx_messages_session_type ON messages (session_id, message_type_name);
             CREATE INDEX IF NOT EXISTS idx_messages_session_direction ON messages (session_id, direction);
             COMMIT;"
    ).map_err(|e| IBKRError::ConfigurationError(format!("Failed to create logger tables: {}", e)))?;
    Ok(())
  }

  fn delete_and_insert_session(
    db: &mut DbConnection,
    session_name: &str,
    start_time_unix_ms: i64,
    host: &str,
    port: u16,
    client_id: i32,
  ) -> Result<i64, IBKRError> {
    let tx = db.transaction()
      .map_err(|e| IBKRError::LoggingError(format!("Failed to start logger transaction: {}", e)))?;

    let deleted_count = tx.execute("DELETE FROM sessions WHERE session_name = ?1", params![session_name])
      .map_err(|e| IBKRError::LoggingError(format!("Failed to delete previous session '{}': {}", session_name, e)))?;

    if deleted_count > 0 {
      log::warn!("Deleted {} previous log session(s) named '{}'", deleted_count, session_name);
    }

    // Insert WITHOUT server_version initially
    tx.execute(
      "INSERT INTO sessions (session_name, start_time_unix_ms, host, port, client_id) VALUES (?1, ?2, ?3, ?4, ?5)",
      params![session_name, start_time_unix_ms, host, port, client_id],
    ).map_err(|e| IBKRError::LoggingError(format!("Failed to insert new session '{}': {}", session_name, e)))?;

    let new_session_id = tx.last_insert_rowid();

    tx.commit()
      .map_err(|e| IBKRError::LoggingError(format!("Failed to commit logger transaction: {}", e)))?;

    Ok(new_session_id)
  }

  /// Sets the server version for the current session in the database.
  /// Should be called after the connection handshake is complete.
  pub fn set_session_server_version(&self, server_version: i32) {
    match self.inner.lock() {
      Ok(guard) => {
        match guard.db.execute(
          "UPDATE sessions SET server_version = ?1 WHERE session_id = ?2",
          params![server_version, guard.session_id],
        ) {
          Ok(updated_rows) => {
            if updated_rows == 1 {
              log::info!("Logged server_version={} for session_id={}", server_version, guard.session_id);
            } else {
              log::warn!("Failed to log server_version={} for session_id={}: Session not found or already set?", server_version, guard.session_id);
            }
          }
          Err(e) => {
            log::error!("Failed to update server_version in logger database for session_id={}: {}", guard.session_id, e);
          }
        }
      }
      Err(poisoned) => {
        log::error!("ConnectionLogger mutex poisoned while setting server version: {}", poisoned);
      }
    }
  }


  /// Logs a single message (sent or received).
  pub fn log_message(&self, direction: LogDirection, payload: &[u8]) {
    // (Parsing logic remains the same)
    let message_type_id = parse_message_type_id(payload);
    let message_type_name = match direction {
      LogDirection::Send => message_type_id.and_then(|id| {
        protocol_encoder::identify_outgoing_type(payload)
      }),
      LogDirection::Recv => message_type_id.and_then(|id| {
        INCOMING_TYPE_MAP.get(&id).copied()
      }),
    };
    let payload_text = bytes_to_center_dot_string(payload);

    // (Locking and insertion logic remains mostly the same)
    match self.inner.lock() {
      Ok(mut guard) => {
        let now_instant = Instant::now();
        let relative_timestamp_s = now_instant
          .duration_since(guard.start_time_instant)
          .as_secs_f64();

        match guard.db.execute(
          "INSERT INTO messages (session_id, direction, relative_timestamp_ms, message_type_id, message_type_name, payload_text)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
          params![
            guard.session_id,
            direction.to_string(),
            relative_timestamp_s * 1000.0, // Store as milliseconds
            message_type_id,
            message_type_name,
            payload_text
          ],
        ) {
          Ok(_) => {
            log::trace!(
              "Logged {} message: Type={:?}, RelTime={:.3}ms, Size={}",
              direction,
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
