// yatws/src/conn_log.rs

use crate::base::IBKRError;
use std::fmt;
use std::path::Path;

#[cfg(any(feature = "log-sql", feature = "log-txt"))]
use crate::protocol_decoder::IncomingMessageType;
#[cfg(any(feature = "log-sql", feature = "log-txt"))]
use crate::protocol_encoder;
#[cfg(any(feature = "log-sql", feature = "log-txt"))]
use once_cell::sync::Lazy;
#[cfg(any(feature = "log-sql", feature = "log-txt"))]
use parking_lot::Mutex;
#[cfg(any(feature = "log-sql", feature = "log-txt"))]
use std::collections::HashMap;
#[cfg(any(feature = "log-sql", feature = "log-txt"))]
use std::io::Read;
#[cfg(any(feature = "log-sql", feature = "log-txt"))]
use std::sync::Arc;
#[cfg(any(feature = "log-sql", feature = "log-txt"))]
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[cfg(feature = "log-sql")]
use rusqlite::{params, Connection as DbConnection};

#[cfg(feature = "log-txt")]
use std::fs::{File, OpenOptions};
#[cfg(feature = "log-txt")]
use std::io::{BufRead, BufReader, BufWriter, Write};
#[cfg(feature = "log-txt")]
use std::path::PathBuf;
#[cfg(feature = "log-txt")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "log-txt")]
use std::sync::Weak;

// --- Constants ---
pub(crate) const CENTER_DOT: char = '·'; // Unicode U+00B7

#[cfg(feature = "log-txt")]
const TXT_LOG_HEADER: &str = "# yatws-txt-log v1\n";

#[cfg(feature = "log-txt")]
static TXT_TMP_COUNTER: AtomicU64 = AtomicU64::new(1);

// --- Static Map for Incoming Message Types ---
#[cfg(any(feature = "log-sql", feature = "log-txt"))]
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

// --- Helper Functions ---

pub(crate) fn bytes_to_center_dot_string(bytes: &[u8]) -> String {
  let mut modified_bytes = Vec::with_capacity(bytes.len());
  let mut dot_buf = [0u8; 4];
  let dot_bytes = CENTER_DOT.encode_utf8(&mut dot_buf).as_bytes();
  for &byte in bytes {
    if byte == 0 {
      modified_bytes.extend_from_slice(dot_bytes);
    } else {
      modified_bytes.push(byte);
    }
  }
  String::from_utf8_lossy(&modified_bytes).to_string()
}

#[cfg(any(feature = "log-sql", feature = "log-txt"))]
fn parse_message_type_id(payload: &[u8]) -> Option<i32> {
  payload
    .iter()
    .position(|&b| b == 0)
    .and_then(|end_pos| {
      if end_pos == 0 {
        return None;
      }
      std::str::from_utf8(&payload[0..end_pos]).ok()
    })
    .and_then(|id_str| id_str.parse::<i32>().ok())
}

#[cfg(any(feature = "log-sql", feature = "log-txt"))]
fn classify_message(direction: LogDirection, payload: &[u8]) -> (Option<i32>, Option<&'static str>) {
  let message_type_id = parse_message_type_id(payload);
  let message_type_name = match direction {
    LogDirection::Send => message_type_id.and_then(|_id| protocol_encoder::identify_outgoing_type(payload)),
    LogDirection::Recv => message_type_id.and_then(|id| INCOMING_TYPE_MAP.get(&id).copied()),
  };
  (message_type_id, message_type_name)
}

#[cfg(any(feature = "log-sql", feature = "log-txt"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExistingFileFormat {
  Sql,
  Txt,
}

/// Peeks at the first 16 bytes of `path` (if it already exists and is non-empty) to detect
/// whether it is an existing SQLite database (`"SQLite format 3\0"`) or a plain-text log.
#[cfg(any(feature = "log-sql", feature = "log-txt"))]
fn detect_existing_file_format(path: &Path) -> Option<ExistingFileFormat> {
  let mut f = std::fs::File::open(path).ok()?;
  let mut magic = [0u8; 16];
  let n = f.read(&mut magic).ok()?;
  if n == 0 {
    return None;
  }
  if n >= 16 && &magic == b"SQLite format 3\0" {
    return Some(ExistingFileFormat::Sql);
  }
  let prefix = &magic[..n];
  if prefix.starts_with(b"# yatws-txt-log")
    || prefix.starts_with(b"SESSION\t")
    || prefix.starts_with(b"SERVER_VERSION\t")
    || prefix.starts_with(b"MSG\t")
  {
    return Some(ExistingFileFormat::Txt);
  }
  None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectedLogFormat {
  #[cfg(feature = "log-sql")]
  Sql,
  #[cfg(feature = "log-txt")]
  Txt,
  #[cfg(not(any(feature = "log-sql", feature = "log-txt")))]
  None,
}

pub(crate) fn select_log_format(path: &Path) -> Result<SelectedLogFormat, IBKRError> {
  #[cfg(not(any(feature = "log-sql", feature = "log-txt")))]
  {
    let _ = path;
    let _explicit_log_none = cfg!(feature = "log-none");
    return Ok(SelectedLogFormat::None);
  }

  #[cfg(any(feature = "log-sql", feature = "log-txt"))]
  {
    match detect_existing_file_format(path) {
      Some(ExistingFileFormat::Sql) => {
        #[cfg(feature = "log-sql")]
        {
          return Ok(SelectedLogFormat::Sql);
        }
        #[cfg(not(feature = "log-sql"))]
        {
          return Err(IBKRError::Unsupported(format!(
            "Session file {:?} is a SQLite database, which requires the 'log-sql' feature.",
            path
          )));
        }
      }
      Some(ExistingFileFormat::Txt) => {
        #[cfg(feature = "log-txt")]
        {
          return Ok(SelectedLogFormat::Txt);
        }
        #[cfg(not(feature = "log-txt"))]
        {
          return Err(IBKRError::Unsupported(format!(
            "Session file {:?} is a plain-text log, which requires the 'log-txt' feature.",
            path
          )));
        }
      }
      None => {}
    }

    #[cfg(all(feature = "log-sql", feature = "log-txt"))]
    {
      if path.extension().and_then(|e| e.to_str()).is_some_and(|ext| {
        ext.eq_ignore_ascii_case("txt") || ext.eq_ignore_ascii_case("log")
      }) {
        return Ok(SelectedLogFormat::Txt);
      }
      return Ok(SelectedLogFormat::Sql);
    }
    #[cfg(all(feature = "log-sql", not(feature = "log-txt")))]
    {
      return Ok(SelectedLogFormat::Sql);
    }
    #[cfg(all(feature = "log-txt", not(feature = "log-sql")))]
    {
      return Ok(SelectedLogFormat::Txt);
    }
  }
}

// --- Plain-Text Bijective Field & Payload Encoding (`log-txt`) ---

#[cfg(feature = "log-txt")]
pub(crate) fn encode_txt_field(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  for ch in s.chars() {
    match ch {
      '\\' => out.push_str("\\\\"),
      '\n' => out.push_str("\\n"),
      '\r' => out.push_str("\\r"),
      '\t' => out.push_str("\\t"),
      _ => out.push(ch),
    }
  }
  out
}

#[cfg(feature = "log-txt")]
pub(crate) fn decode_txt_field(s: &str) -> Result<String, IBKRError> {
  let mut out = String::with_capacity(s.len());
  let mut chars = s.chars();
  while let Some(ch) = chars.next() {
    if ch == '\\' {
      match chars.next() {
        Some('\\') => out.push('\\'),
        Some('n') => out.push('\n'),
        Some('r') => out.push('\r'),
        Some('t') => out.push('\t'),
        other => {
          return Err(IBKRError::ParseError(format!(
            "Invalid escape sequence '\\{:?}' in text log field",
            other
          )))
        }
      }
    } else {
      out.push(ch);
    }
  }
  Ok(out)
}

/// Encodes raw TWS wire bytes onto a single human-readable text line:
/// `\0` becomes `CENTER_DOT` (`·`), while `\`, `\n`, `\r`, `\t`, literal `·`, and non-UTF-8 bytes
/// are backslash-escaped so arbitrary payloads round-trip byte-for-byte.
#[cfg(feature = "log-txt")]
pub(crate) fn encode_txt_payload(bytes: &[u8]) -> String {
  use std::fmt::Write as _;
  let mut out = String::with_capacity(bytes.len() + 16);
  for chunk in bytes.utf8_chunks() {
    for ch in chunk.valid().chars() {
      match ch {
        '\0' => out.push(CENTER_DOT),
        '\\' => out.push_str("\\\\"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        CENTER_DOT => out.push_str("\\d"),
        _ => out.push(ch),
      }
    }
    for &b in chunk.invalid() {
      let _ = write!(out, "\\x{:02x}", b);
    }
  }
  out
}

#[cfg(feature = "log-txt")]
pub(crate) fn decode_txt_payload(s: &str) -> Result<Vec<u8>, IBKRError> {
  let mut out = Vec::with_capacity(s.len());
  let mut chars = s.chars();
  let mut utf8_buf = [0u8; 4];
  let mut dot_buf = [0u8; 4];
  let dot_bytes = CENTER_DOT.encode_utf8(&mut dot_buf).as_bytes();

  while let Some(ch) = chars.next() {
    if ch == CENTER_DOT {
      out.push(0);
    } else if ch == '\\' {
      match chars.next() {
        Some('\\') => out.push(b'\\'),
        Some('n') => out.push(b'\n'),
        Some('r') => out.push(b'\r'),
        Some('t') => out.push(b'\t'),
        Some('d') => out.extend_from_slice(dot_bytes),
        Some('x') => {
          let h1 = chars
            .next()
            .and_then(|c| c.to_digit(16))
            .ok_or_else(|| IBKRError::ParseError("Invalid \\x escape in text log payload".to_string()))?;
          let h2 = chars
            .next()
            .and_then(|c| c.to_digit(16))
            .ok_or_else(|| IBKRError::ParseError("Invalid \\x escape in text log payload".to_string()))?;
          out.push(((h1 << 4) | h2) as u8);
        }
        other => {
          return Err(IBKRError::ParseError(format!(
            "Invalid escape sequence '\\{:?}' in text log payload",
            other
          )))
        }
      }
    } else {
      out.extend_from_slice(ch.encode_utf8(&mut utf8_buf).as_bytes());
    }
  }
  Ok(out)
}

// --- Per-File Shared Handle for `log-txt` (prevents multi-writer interleaving & stale-inode writes) ---

#[cfg(feature = "log-txt")]
struct TxtSharedFile {
  path: PathBuf,
  file: Option<File>,
}

#[cfg(feature = "log-txt")]
static TXT_FILE_REGISTRY: Lazy<Mutex<HashMap<PathBuf, Weak<Mutex<TxtSharedFile>>>>> =
  Lazy::new(|| Mutex::new(HashMap::new()));

#[cfg(feature = "log-txt")]
fn normalize_txt_path_key(path: &Path) -> PathBuf {
  if let Ok(canon) = std::fs::canonicalize(path) {
    return canon;
  }
  let abs = if path.is_absolute() {
    path.to_path_buf()
  } else {
    std::env::current_dir()
      .map(|cwd| cwd.join(path))
      .unwrap_or_else(|_| path.to_path_buf())
  };
  if let (Some(parent), Some(name)) = (abs.parent(), abs.file_name()) {
    if let Ok(canon_parent) = std::fs::canonicalize(parent) {
      return canon_parent.join(name);
    }
  }
  abs
}

#[cfg(feature = "log-txt")]
fn get_or_create_shared_txt_file(path: &Path) -> Arc<Mutex<TxtSharedFile>> {
  let key = normalize_txt_path_key(path);
  let mut reg = TXT_FILE_REGISTRY.lock();
  reg.retain(|_, weak| weak.strong_count() > 0);
  if let Some(existing) = reg.get(&key).and_then(|w| w.upgrade()) {
    return existing;
  }
  let created = Arc::new(Mutex::new(TxtSharedFile {
    path: key.clone(),
    file: None,
  }));
  reg.insert(key, Arc::downgrade(&created));
  created
}

#[cfg(feature = "log-txt")]
fn open_secure_append_file(path: &Path) -> std::io::Result<File> {
  let mut opts = OpenOptions::new();
  opts.create(true).append(true);
  #[cfg(unix)]
  {
    use std::os::unix::fs::OpenOptionsExt as _;
    opts.mode(0o600);
  }
  opts.open(path)
}

#[cfg(feature = "log-txt")]
fn open_secure_new_temp_file(path: &Path) -> std::io::Result<File> {
  let mut opts = OpenOptions::new();
  opts.write(true).create_new(true);
  #[cfg(unix)]
  {
    use std::os::unix::fs::OpenOptionsExt as _;
    opts.mode(0o600);
  }
  opts.open(path)
}

#[cfg(feature = "log-txt")]
fn line_matches_session(line: &str, s_pref: &str, v_pref: &str, m_pref: &str) -> bool {
  line.starts_with(s_pref) || line.starts_with(v_pref) || line.starts_with(m_pref)
}

/// Streams `txt_path` line-by-line ($O(\text{max line})$ memory) to check if `encoded_session_name`
/// already exists, and if so atomically rewrites `txt_path` via a `0600` sibling temporary file + `sync_all` + `rename`.
#[cfg(feature = "log-txt")]
fn remove_existing_txt_session_streaming(txt_path: &Path, session_name: &str, encoded_session_name: &str) -> Result<(), IBKRError> {
  if !txt_path.exists() {
    return Ok(());
  }

  let s_pref = format!("SESSION\t{}\t", encoded_session_name);
  let v_pref = format!("SERVER_VERSION\t{}\t", encoded_session_name);
  let m_pref = format!("MSG\t{}\t", encoded_session_name);

  // Pass 1: streaming scan with reusable line buffer to avoid rewriting if `session_name` is new
  let mut had_previous = false;
  {
    let src = File::open(txt_path)
      .map_err(|e| IBKRError::ConfigurationError(format!("Failed to read existing text log file: {}", e)))?;
    let mut reader = BufReader::new(src);
    let mut line_buf = String::new();
    loop {
      line_buf.clear();
      let n = reader
        .read_line(&mut line_buf)
        .map_err(|e| IBKRError::ConfigurationError(format!("Failed scanning text log file: {}", e)))?;
      if n == 0 {
        break;
      }
      if line_matches_session(&line_buf, &s_pref, &v_pref, &m_pref) {
        had_previous = true;
        break;
      }
    }
  }

  if !had_previous {
    return Ok(());
  }

  // Pass 2: stream non-matching lines into a sibling temporary file, fsync, and atomically rename
  let parent = txt_path.parent().unwrap_or_else(|| Path::new("."));
  let base_name = txt_path
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or("yatws_log");
  let seq = TXT_TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
  let tmp_path = parent.join(format!(".{}.yatws-tmp-{}-{}", base_name, std::process::id(), seq));

  let rewrite_result = (|| -> Result<(), IBKRError> {
    let src = File::open(txt_path)
      .map_err(|e| IBKRError::LoggingError(format!("Failed opening text log for session replacement: {}", e)))?;
    let mut reader = BufReader::new(src);

    let tmp_file = open_secure_new_temp_file(&tmp_path)
      .map_err(|e| IBKRError::LoggingError(format!("Failed creating temporary text log {:?}: {}", tmp_path, e)))?;
    let mut writer = BufWriter::new(tmp_file);

    let mut line_buf = String::new();
    let mut wrote_header = false;
    let mut first_line = true;

    loop {
      line_buf.clear();
      let n = reader
        .read_line(&mut line_buf)
        .map_err(|e| IBKRError::LoggingError(format!("Failed reading text log during rewrite: {}", e)))?;
      if n == 0 {
        break;
      }
      if first_line {
        first_line = false;
        if !line_buf.starts_with("# yatws-txt-log") {
          writer
            .write_all(TXT_LOG_HEADER.as_bytes())
            .map_err(|e| IBKRError::LoggingError(format!("Failed writing header to temporary text log: {}", e)))?;
        }
        wrote_header = true;
      }
      if line_matches_session(&line_buf, &s_pref, &v_pref, &m_pref) {
        continue;
      }
      writer
        .write_all(line_buf.as_bytes())
        .map_err(|e| IBKRError::LoggingError(format!("Failed writing to temporary text log: {}", e)))?;
      if !line_buf.ends_with('\n') {
        writer
          .write_all(b"\n")
          .map_err(|e| IBKRError::LoggingError(format!("Failed writing newline to temporary text log: {}", e)))?;
      }
    }

    if !wrote_header {
      writer
        .write_all(TXT_LOG_HEADER.as_bytes())
        .map_err(|e| IBKRError::LoggingError(format!("Failed writing header to temporary text log: {}", e)))?;
    }

    writer
      .flush()
      .map_err(|e| IBKRError::LoggingError(format!("Failed flushing temporary text log: {}", e)))?;
    let tmp_file = writer
      .into_inner()
      .map_err(|e| IBKRError::LoggingError(format!("Failed finalizing temporary text log: {}", e.error())))?;
    tmp_file
      .sync_all()
      .map_err(|e| IBKRError::LoggingError(format!("Failed syncing temporary text log: {}", e)))?;
    drop(tmp_file);

    std::fs::rename(&tmp_path, txt_path)
      .map_err(|e| IBKRError::LoggingError(format!("Failed atomically replacing text log {:?}: {}", txt_path, e)))?;
    Ok(())
  })();

  if rewrite_result.is_err() {
    let _ = std::fs::remove_file(&tmp_path);
  } else {
    log::warn!("Deleted previous text log session named '{}'", session_name);
  }
  rewrite_result
}

// --- Logger Types ---

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

#[cfg(feature = "log-sql")]
struct SqlLoggerInner {
  db: DbConnection,
  session_id: i64,
  start_time_instant: Instant,
}

#[cfg(feature = "log-txt")]
struct TxtLoggerInner {
  shared_file: Arc<Mutex<TxtSharedFile>>,
  encoded_session_name: String,
  start_time_instant: Instant,
}

#[derive(Clone)]
enum LoggerBackend {
  #[cfg(feature = "log-sql")]
  Sql(Arc<Mutex<SqlLoggerInner>>),
  #[cfg(feature = "log-txt")]
  Txt(Arc<TxtLoggerInner>),
  #[cfg(not(any(feature = "log-sql", feature = "log-txt")))]
  Noop,
}

#[derive(Clone)]
pub struct ConnectionLogger {
  backend: LoggerBackend,
}

impl ConnectionLogger {
  pub fn new<P: AsRef<Path>>(
    log_path: P,
    session_name: &str,
    host: &str,
    port: u16,
    client_id: i32,
  ) -> Result<Self, IBKRError> {
    let path = log_path.as_ref();
    match select_log_format(path)? {
      #[cfg(feature = "log-sql")]
      SelectedLogFormat::Sql => Self::new_sql(path, session_name, host, port, client_id),
      #[cfg(feature = "log-txt")]
      SelectedLogFormat::Txt => Self::new_txt(path, session_name, host, port, client_id),
      #[cfg(not(any(feature = "log-sql", feature = "log-txt")))]
      SelectedLogFormat::None => {
        let _ = (host, port, client_id);
        log::warn!(
          "Connection logging requested for session '{}' ({:?}), but yatws was compiled without 'log-sql' or 'log-txt' (log-none mode); messages will not be recorded.",
          session_name,
          path
        );
        Ok(Self {
          backend: LoggerBackend::Noop,
        })
      }
    }
  }

  #[cfg(feature = "log-sql")]
  fn new_sql(
    db_path: &Path,
    session_name: &str,
    host: &str,
    port: u16,
    client_id: i32,
  ) -> Result<Self, IBKRError> {
    log::info!(
      "Initializing SQLite connection logger at path: {:?}, Session Name: '{}'",
      db_path,
      session_name
    );
    let mut db = DbConnection::open(db_path)
      .map_err(|e| IBKRError::ConfigurationError(format!("Failed to open logger database: {}", e)))?;

    db.pragma_update(None, "journal_mode", "WAL")
      .map_err(|e| IBKRError::ConfigurationError(format!("Failed to set WAL mode: {}", e)))?;
    db.execute("PRAGMA foreign_keys = ON;", [])
      .map_err(|e| IBKRError::ConfigurationError(format!("Failed to enable foreign keys: {}", e)))?;

    Self::create_sql_tables(&db)?;

    let start_time_instant = Instant::now();
    let start_time_unix_ms = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map_err(|e| IBKRError::InternalError(format!("System time error: {}", e)))?
      .as_millis() as i64;

    let session_id = Self::delete_and_insert_sql_session(
      &mut db,
      session_name,
      start_time_unix_ms,
      host,
      port,
      client_id,
    )?;

    log::info!("Started SQLite logger session ID: {} for name '{}'", session_id, session_name);

    Ok(Self {
      backend: LoggerBackend::Sql(Arc::new(Mutex::new(SqlLoggerInner {
        db,
        session_id,
        start_time_instant,
      }))),
    })
  }

  #[cfg(feature = "log-sql")]
  fn create_sql_tables(db: &DbConnection) -> Result<(), IBKRError> {
    db.execute_batch(
      "BEGIN;
             CREATE TABLE IF NOT EXISTS sessions (
                 session_id          INTEGER PRIMARY KEY AUTOINCREMENT,
                 session_name        TEXT NOT NULL UNIQUE,
                 start_time_unix_ms  INTEGER NOT NULL,
                 host                TEXT NOT NULL,
                 port                INTEGER NOT NULL,
                 client_id           INTEGER NOT NULL,
                 server_version      INTEGER NULL
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
             CREATE INDEX IF NOT EXISTS idx_sessions_name ON sessions (session_name);
             CREATE INDEX IF NOT EXISTS idx_messages_session_time ON messages (session_id, relative_timestamp_ms);
             CREATE INDEX IF NOT EXISTS idx_messages_session_type ON messages (session_id, message_type_name);
             CREATE INDEX IF NOT EXISTS idx_messages_session_direction ON messages (session_id, direction);
             COMMIT;",
    )
    .map_err(|e| IBKRError::ConfigurationError(format!("Failed to create logger tables: {}", e)))?;
    Ok(())
  }

  #[cfg(feature = "log-sql")]
  fn delete_and_insert_sql_session(
    db: &mut DbConnection,
    session_name: &str,
    start_time_unix_ms: i64,
    host: &str,
    port: u16,
    client_id: i32,
  ) -> Result<i64, IBKRError> {
    let tx = db
      .transaction()
      .map_err(|e| IBKRError::LoggingError(format!("Failed to start logger transaction: {}", e)))?;

    let deleted_count = tx
      .execute("DELETE FROM sessions WHERE session_name = ?1", params![session_name])
      .map_err(|e| IBKRError::LoggingError(format!("Failed to delete previous session '{}': {}", session_name, e)))?;

    if deleted_count > 0 {
      log::warn!("Deleted {} previous log session(s) named '{}'", deleted_count, session_name);
    }

    tx.execute(
      "INSERT INTO sessions (session_name, start_time_unix_ms, host, port, client_id) VALUES (?1, ?2, ?3, ?4, ?5)",
      params![session_name, start_time_unix_ms, host, port, client_id],
    )
    .map_err(|e| IBKRError::LoggingError(format!("Failed to insert new session '{}': {}", session_name, e)))?;

    let new_session_id = tx.last_insert_rowid();

    tx.commit()
      .map_err(|e| IBKRError::LoggingError(format!("Failed to commit logger transaction: {}", e)))?;

    Ok(new_session_id)
  }

  #[cfg(feature = "log-txt")]
  fn new_txt(
    txt_path: &Path,
    session_name: &str,
    host: &str,
    port: u16,
    client_id: i32,
  ) -> Result<Self, IBKRError> {
    log::info!(
      "Initializing plain-text connection logger at path: {:?}, Session Name: '{}'",
      txt_path,
      session_name
    );

    let encoded_session_name = encode_txt_field(session_name);
    let encoded_host = encode_txt_field(host);
    let shared_file = get_or_create_shared_txt_file(txt_path);

    let start_time_instant = Instant::now();
    let start_time_unix_ms = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map_err(|e| IBKRError::InternalError(format!("System time error: {}", e)))?
      .as_millis() as i64;

    {
      let mut file_guard = shared_file.lock();
      // Close any existing descriptor before streaming rewrite + atomic rename so all active
      // loggers sharing this path seamlessly re-bind to the new inode below.
      file_guard.file = None;

      remove_existing_txt_session_streaming(txt_path, session_name, &encoded_session_name)?;

      let mut file = open_secure_append_file(txt_path)
        .map_err(|e| IBKRError::ConfigurationError(format!("Failed to open text logger file: {}", e)))?;

      let is_empty = file
        .metadata()
        .map_err(|e| IBKRError::ConfigurationError(format!("Failed to stat text logger file: {}", e)))?
        .len()
        == 0;

      let mut init_buf = String::with_capacity(128);
      if is_empty {
        init_buf.push_str(TXT_LOG_HEADER);
      }
      use std::fmt::Write as _;
      let _ = writeln!(
        init_buf,
        "SESSION\t{}\t{}\t{}\t{}\t{}",
        encoded_session_name, start_time_unix_ms, encoded_host, port, client_id
      );

      file
        .write_all(init_buf.as_bytes())
        .map_err(|e| IBKRError::LoggingError(format!("Failed to write text log session header: {}", e)))?;

      file_guard.file = Some(file);
    }

    log::info!("Started plain-text logger session '{}'", session_name);

    Ok(Self {
      backend: LoggerBackend::Txt(Arc::new(TxtLoggerInner {
        shared_file,
        encoded_session_name,
        start_time_instant,
      })),
    })
  }

  #[cfg(feature = "log-txt")]
  fn write_txt_line(inner: &TxtLoggerInner, line: &str) -> std::io::Result<()> {
    let mut guard = inner.shared_file.lock();
    if guard.file.is_none() {
      let reopened = open_secure_append_file(&guard.path)?;
      guard.file = Some(reopened);
    }
    let file = guard.file.as_mut().unwrap();
    file.write_all(line.as_bytes())
  }

  /// Sets the server version for the current session in the configured log backend.
  /// Should be called after the connection handshake is complete.
  pub fn set_session_server_version(&self, server_version: i32) {
    match &self.backend {
      #[cfg(feature = "log-sql")]
      LoggerBackend::Sql(inner) => {
        let guard = inner.lock();
        match guard.db.execute(
          "UPDATE sessions SET server_version = ?1 WHERE session_id = ?2",
          params![server_version, guard.session_id],
        ) {
          Ok(updated_rows) => {
            if updated_rows == 1 {
              log::info!("Logged server_version={} for session_id={}", server_version, guard.session_id);
            } else {
              log::warn!(
                "Failed to log server_version={} for session_id={}: Session not found or already set?",
                server_version,
                guard.session_id
              );
            }
          }
          Err(e) => {
            log::error!(
              "Failed to update server_version in logger database for session_id={}: {}",
              guard.session_id,
              e
            );
          }
        }
      }
      #[cfg(feature = "log-txt")]
      LoggerBackend::Txt(inner) => {
        let line = format!("SERVER_VERSION\t{}\t{}\n", inner.encoded_session_name, server_version);
        match Self::write_txt_line(inner, &line) {
          Ok(()) => log::info!(
            "Logged server_version={} for text session '{}'",
            server_version,
            inner.encoded_session_name
          ),
          Err(e) => log::error!("Failed to write server_version to text log: {}", e),
        }
      }
      #[cfg(not(any(feature = "log-sql", feature = "log-txt")))]
      LoggerBackend::Noop => {
        let _ = server_version;
      }
    }
  }

  /// Logs a single message (sent or received).
  pub fn log_message(&self, direction: LogDirection, payload: &[u8]) {
    match &self.backend {
      #[cfg(feature = "log-sql")]
      LoggerBackend::Sql(inner) => {
        let (message_type_id, message_type_name) = classify_message(direction, payload);
        let payload_text = bytes_to_center_dot_string(payload);
        let guard = inner.lock();
        let relative_timestamp_ms = guard.start_time_instant.elapsed().as_secs_f64() * 1000.0;

        match guard.db.execute(
          "INSERT INTO messages (session_id, direction, relative_timestamp_ms, message_type_id, message_type_name, payload_text)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
          params![
            guard.session_id,
            direction.to_string(),
            relative_timestamp_ms,
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
              relative_timestamp_ms,
              payload.len()
            );
          }
          Err(e) => {
            log::error!("Failed to log message to database: {}", e);
          }
        }
      }
      #[cfg(feature = "log-txt")]
      LoggerBackend::Txt(inner) => {
        let (message_type_id, message_type_name) = classify_message(direction, payload);
        let payload_encoded = encode_txt_payload(payload);
        let type_name_str = message_type_name.unwrap_or("-");
        let relative_timestamp_ms = inner.start_time_instant.elapsed().as_secs_f64() * 1000.0;

        let mut line = String::with_capacity(payload_encoded.len() + 64);
        use std::fmt::Write as _;
        match message_type_id {
          Some(id) => {
            let _ = writeln!(
              line,
              "MSG\t{}\t{}\t{:.3}\t{}\t{}\t{}",
              inner.encoded_session_name,
              direction,
              relative_timestamp_ms,
              id,
              type_name_str,
              payload_encoded
            );
          }
          None => {
            let _ = writeln!(
              line,
              "MSG\t{}\t{}\t{:.3}\t-\t{}\t{}",
              inner.encoded_session_name,
              direction,
              relative_timestamp_ms,
              type_name_str,
              payload_encoded
            );
          }
        }

        match Self::write_txt_line(inner, &line) {
          Ok(()) => {
            log::trace!(
              "Logged {} message to text file: Type={}, RelTime={:.3}ms, Size={}",
              direction,
              type_name_str,
              relative_timestamp_ms,
              payload.len()
            );
          }
          Err(e) => {
            log::error!("Failed to log message to text file: {}", e);
          }
        }
      }
      #[cfg(not(any(feature = "log-sql", feature = "log-txt")))]
      LoggerBackend::Noop => {
        let _ = (direction, payload);
      }
    }
  }
}

#[cfg(all(test, feature = "log-txt"))]
mod tests {
  use super::*;

  #[test]
  fn test_txt_payload_and_field_bijective_roundtrip() {
    let tricky_payload: Vec<u8> = vec![
      b'4', 0, b'2', 0, b'-', b'1', 0, b'3', b'2', b'2', 0,
      b'L', b'i', b'n', b'e', b'1', b'\n', b'L', b'i', b'n', b'e', b'2', b'\r', b'\t', b'\\',
      0xc2, 0xb7, // literal UTF-8 CENTER_DOT '·'
      0xe2, 0x82, 0xac, // '€'
      0xff, 0xfe, // invalid UTF-8 bytes
      0,
    ];
    let encoded = encode_txt_payload(&tricky_payload);
    assert!(!encoded.contains('\n') && !encoded.contains('\r') && !encoded.contains('\t'));
    let decoded = decode_txt_payload(&encoded).expect("decode_txt_payload failed");
    assert_eq!(decoded, tricky_payload);
    assert_eq!(decode_txt_payload(&encode_txt_payload(b"")).unwrap(), b"");

    let tricky_field = "session\twith\nweird\\chars\r";
    let enc_f = encode_txt_field(tricky_field);
    assert!(!enc_f.contains('\t') && !enc_f.contains('\n') && !enc_f.contains('\r'));
    assert_eq!(decode_txt_field(&enc_f).unwrap(), tricky_field);
  }

  #[cfg(unix)]
  #[test]
  fn test_txt_log_permissions_and_concurrent_writers_survive_rewrite() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().expect("tempdir");
    let log_path = dir.path().join("concurrent.txt");

    let logger_a = ConnectionLogger::new(&log_path, "sess_a", "127.0.0.1", 4002, 1).unwrap();
    let mode = std::fs::metadata(&log_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "Session log must be created with 0600 permissions, got {:o}", mode);

    // Create and then overwrite "sess_b" while `logger_a` is STILL live on the same file:
    // `logger_a` must seamlessly write to the new renamed inode without losing frames!
    let logger_b1 = ConnectionLogger::new(&log_path, "sess_b", "127.0.0.1", 4002, 2).unwrap();
    logger_b1.log_message(LogDirection::Send, b"71\02\02\0\0");
    drop(logger_b1);

    let logger_b2 = ConnectionLogger::new(&log_path, "sess_b", "127.0.0.1", 4002, 2).unwrap();
    logger_a.set_session_server_version(178);
    logger_b2.set_session_server_version(178);

    let big_payload = vec![b'X'; 16_384]; // > 8 KiB
    let a_clone = logger_a.clone();
    let b_clone = logger_b2.clone();
    let p1 = big_payload.clone();
    let p2 = big_payload;
    let h1 = std::thread::spawn(move || {
      for _ in 0..10 {
        a_clone.log_message(LogDirection::Recv, &p1);
      }
    });
    let h2 = std::thread::spawn(move || {
      for _ in 0..10 {
        b_clone.log_message(LogDirection::Recv, &p2);
      }
    });
    h1.join().unwrap();
    h2.join().unwrap();

    let contents = std::fs::read_to_string(&log_path).unwrap();
    assert_eq!(contents.lines().filter(|l| l.starts_with("MSG\tsess_a\t")).count(), 10);
    assert_eq!(contents.lines().filter(|l| l.starts_with("MSG\tsess_b\t")).count(), 10);
    let mode_after = std::fs::metadata(&log_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode_after, 0o600);
  }
}
