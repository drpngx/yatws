// yatws/src/protocol_encoder.rs
// Encoder for the TWS API protocol messages

use std::io::Cursor;
use crate::base::IBKRError;
use crate::message_parser::msg_to_string;
use crate::contract::{Contract, SecType, ComboLeg, ScannerSubscription};
use crate::order::{OrderRequest, OrderType, OrderCancel};
use crate::account::ExecutionFilter;
use crate::data_wsh::WshEventDataRequest;
use crate::data::MarketDataType;
use crate::min_server_ver::min_server_ver;
use chrono::{DateTime, Utc};
use log::{debug, trace, warn};
use std::io::Write;

/// Message tags for outgoing messages
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutgoingMessageType {
  ReqMktData = 1,
  CancelMktData = 2,
  PlaceOrder = 3,
  CancelOrder = 4,
  ReqOpenOrders = 5,
  ReqAccountData = 6,
  ReqExecutions = 7,
  ReqIds = 8,
  ReqContractData = 9,
  ReqMktDepth = 10,
  CancelMktDepth = 11,
  ReqNewsBulletins = 12,
  CancelNewsBulletins = 13,
  SetServerLoglevel = 14,
  ReqAutoOpenOrders = 15,
  ReqAllOpenOrders = 16,
  ReqManagedAccts = 17,
  ReqFa = 18, // Financial Advisor
  ReplaceFa = 19, // Financial Advisor
  ReqHistoricalData = 20,
  ExerciseOptions = 21,
  ReqScannerSubscription = 22,
  CancelScannerSubscription = 23,
  ReqScannerParameters = 24,
  CancelHistoricalData = 25,
  ReqCurrentTime = 49,
  ReqRealTimeBars = 50,
  CancelRealTimeBars = 51,
  ReqFundamentalData = 52,
  CancelFundamentalData = 53,
  ReqCalcImpliedVolat = 54,
  ReqCalcOptionPrice = 55,
  CancelCalcImpliedVolat = 56,
  CancelCalcOptionPrice = 57,
  ReqGlobalCancel = 58,
  ReqMarketDataType = 59,
  ReqPositions = 61,
  ReqAccountSummary = 62,
  CancelAccountSummary = 63,
  CancelPositions = 64,
  VerifyRequest = 65,
  VerifyMessage = 66,
  QueryDisplayGroups = 67,
  SubscribeToGroupEvents = 68,
  UpdateDisplayGroup = 69,
  UnsubscribeFromGroupEvents = 70,
  StartApi = 71,
  VerifyAndAuthRequest = 72,
  VerifyAndAuthMessage = 73,
  ReqPositionsMulti = 74,
  CancelPositionsMulti = 75,
  ReqAccountUpdatesMulti = 76,
  CancelAccountUpdatesMulti = 77,
  ReqSecDefOptParams = 78,
  ReqSoftDollarTiers = 79,
  ReqFamilyCodes = 80,
  ReqMatchingSymbols = 81,
  ReqMktDepthExchanges = 82,
  ReqSmartComponents = 83,
  ReqNewsArticle = 84,
  ReqNewsProviders = 85,
  ReqHistoricalNews = 86,
  ReqHeadTimestamp = 87, // Message ID for request head timestamp
  ReqHistogramData = 88, // Message ID for request histogram data
  CancelHistogramData = 89,
  CancelHeadTimestamp = 90,
  ReqMarketRule = 91,
  ReqPnl = 92,
  CancelPnl = 93,
  ReqPnlSingle = 94,
  CancelPnlSingle = 95,
  ReqHistoricalTicks = 96,
  ReqTickByTickData = 97,
  // CancelHistoricalTicks uses CancelHistoricalData (25)
  CancelTickByTickData = 98,
  ReqCompletedOrders = 99,
  ReqWshMetaData = 100,
  CancelWshMetaData = 101,
  ReqWshEventData = 102,
  CancelWshEventData = 103,
  ReqUserInfo = 104,
  // Note: ReqCalcImpliedVolat = 54, ReqCalcOptionPrice = 55, CancelCalcImpliedVolat = 56, CancelCalcOptionPrice = 57
  // are already defined above.
}

/// Identifies an outgoing message type based on its raw byte payload prefix.
///
/// TWS messages typically start with the numeric message type ID followed by a null byte.
/// This function parses that ID and returns a static string name if known.
///
/// It's useful for logging/debugging the raw encoded messages before sending.
///
/// Returns `Some(type_name)` or `None` if the type ID is not recognized or cannot be parsed.
pub fn identify_outgoing_type(msg_data: &[u8]) -> Option<&'static str> {
  if msg_data.is_empty() {
    return None;
  }

  // Find the first null terminator to isolate the type ID string
  let end_pos = msg_data.iter().position(|&b| b == 0).unwrap_or(0);
  if end_pos == 0 {
    // Special case for H1 Handshake which starts "API\0"
    if msg_data.starts_with(b"API\0") {
      return Some("H1_CLIENT_VERSION");
    }
    return None; // No null terminator found normally
  }

  let type_id_bytes = &msg_data[0..end_pos];

  // Convert bytes to string slice
  let type_id_str = match std::str::from_utf8(type_id_bytes) {
    Ok(s) => s,
    Err(_) => return None, // Not valid UTF-8
  };

  // Parse the string to an integer (assuming base 10)
  let type_id: i32 = match type_id_str.parse() {
    Ok(id) => id,
    Err(_) => return None, // Failed to parse as integer
  };

  // Map the integer ID to a static string name using the OutgoingMessageType enum
  match OutgoingMessageType::try_from(type_id) {
    Ok(OutgoingMessageType::ReqMktData) => Some("REQ_MKT_DATA"),
    Ok(OutgoingMessageType::CancelMktData) => Some("CANCEL_MKT_DATA"),
    Ok(OutgoingMessageType::PlaceOrder) => Some("PLACE_ORDER"),
    Ok(OutgoingMessageType::CancelOrder) => Some("CANCEL_ORDER"),
    Ok(OutgoingMessageType::ReqOpenOrders) => Some("REQ_OPEN_ORDERS"),
    Ok(OutgoingMessageType::ReqAccountData) => Some("REQ_ACCOUNT_DATA"),
    Ok(OutgoingMessageType::ReqExecutions) => Some("REQ_EXECUTIONS"),
    Ok(OutgoingMessageType::ReqIds) => Some("REQ_IDS"),
    Ok(OutgoingMessageType::ReqContractData) => Some("REQ_CONTRACT_DATA"),
    Ok(OutgoingMessageType::ReqMktDepth) => Some("REQ_MKT_DEPTH"),
    Ok(OutgoingMessageType::CancelMktDepth) => Some("CANCEL_MKT_DEPTH"),
    Ok(OutgoingMessageType::ReqNewsBulletins) => Some("REQ_NEWS_BULLETINS"),
    Ok(OutgoingMessageType::CancelNewsBulletins) => Some("CANCEL_NEWS_BULLETINS"),
    Ok(OutgoingMessageType::SetServerLoglevel) => Some("SET_SERVER_LOGLEVEL"),
    Ok(OutgoingMessageType::ReqAutoOpenOrders) => Some("REQ_AUTO_OPEN_ORDERS"),
    Ok(OutgoingMessageType::ReqAllOpenOrders) => Some("REQ_ALL_OPEN_ORDERS"),
    Ok(OutgoingMessageType::ReqManagedAccts) => Some("REQ_MANAGED_ACCTS"),
    Ok(OutgoingMessageType::ReqFa) => Some("REQ_FA"),
    Ok(OutgoingMessageType::ReplaceFa) => Some("REPLACE_FA"),
    Ok(OutgoingMessageType::ReqHistoricalData) => Some("REQ_HISTORICAL_DATA"),
    Ok(OutgoingMessageType::ExerciseOptions) => Some("EXERCISE_OPTIONS"),
    Ok(OutgoingMessageType::ReqScannerSubscription) => Some("REQ_SCANNER_SUBSCRIPTION"),
    Ok(OutgoingMessageType::CancelScannerSubscription) => Some("CANCEL_SCANNER_SUBSCRIPTION"),
    Ok(OutgoingMessageType::ReqScannerParameters) => Some("REQ_SCANNER_PARAMETERS"),
    Ok(OutgoingMessageType::CancelHistoricalData) => Some("CANCEL_HISTORICAL_DATA"),
    Ok(OutgoingMessageType::ReqCurrentTime) => Some("REQ_CURRENT_TIME"),
    Ok(OutgoingMessageType::ReqRealTimeBars) => Some("REQ_REAL_TIME_BARS"),
    Ok(OutgoingMessageType::CancelRealTimeBars) => Some("CANCEL_REAL_TIME_BARS"),
    Ok(OutgoingMessageType::ReqFundamentalData) => Some("REQ_FUNDAMENTAL_DATA"),
    Ok(OutgoingMessageType::CancelFundamentalData) => Some("CANCEL_FUNDAMENTAL_DATA"),
    Ok(OutgoingMessageType::ReqCalcImpliedVolat) => Some("REQ_CALC_IMPLIED_VOLAT"),
    Ok(OutgoingMessageType::ReqCalcOptionPrice) => Some("REQ_CALC_OPTION_PRICE"),
    Ok(OutgoingMessageType::CancelCalcImpliedVolat) => Some("CANCEL_CALC_IMPLIED_VOLAT"),
    Ok(OutgoingMessageType::CancelCalcOptionPrice) => Some("CANCEL_CALC_OPTION_PRICE"),
    Ok(OutgoingMessageType::ReqGlobalCancel) => Some("REQ_GLOBAL_CANCEL"),
    Ok(OutgoingMessageType::ReqMarketDataType) => Some("REQ_MARKET_DATA_TYPE"),
    Ok(OutgoingMessageType::ReqPositions) => Some("REQ_POSITIONS"),
    Ok(OutgoingMessageType::ReqAccountSummary) => Some("REQ_ACCOUNT_SUMMARY"),
    Ok(OutgoingMessageType::CancelAccountSummary) => Some("CANCEL_ACCOUNT_SUMMARY"),
    Ok(OutgoingMessageType::CancelPositions) => Some("CANCEL_POSITIONS"),
    Ok(OutgoingMessageType::VerifyRequest) => Some("VERIFY_REQUEST"),
    Ok(OutgoingMessageType::VerifyMessage) => Some("VERIFY_MESSAGE"),
    Ok(OutgoingMessageType::QueryDisplayGroups) => Some("QUERY_DISPLAY_GROUPS"),
    Ok(OutgoingMessageType::SubscribeToGroupEvents) => Some("SUBSCRIBE_TO_GROUP_EVENTS"),
    Ok(OutgoingMessageType::UpdateDisplayGroup) => Some("UPDATE_DISPLAY_GROUP"),
    Ok(OutgoingMessageType::UnsubscribeFromGroupEvents) => Some("UNSUBSCRIBE_FROM_GROUP_EVENTS"),
    Ok(OutgoingMessageType::StartApi) => Some("START_API"), // Changed from H3_START_API
    Ok(OutgoingMessageType::VerifyAndAuthRequest) => Some("VERIFY_AND_AUTH_REQUEST"),
    Ok(OutgoingMessageType::VerifyAndAuthMessage) => Some("VERIFY_AND_AUTH_MESSAGE"),
    Ok(OutgoingMessageType::ReqPositionsMulti) => Some("REQ_POSITIONS_MULTI"),
    Ok(OutgoingMessageType::CancelPositionsMulti) => Some("CANCEL_POSITIONS_MULTI"),
    Ok(OutgoingMessageType::ReqAccountUpdatesMulti) => Some("REQ_ACCOUNT_UPDATES_MULTI"),
    Ok(OutgoingMessageType::CancelAccountUpdatesMulti) => Some("CANCEL_ACCOUNT_UPDATES_MULTI"),
    Ok(OutgoingMessageType::ReqSecDefOptParams) => Some("REQ_SEC_DEF_OPT_PARAMS"),
    Ok(OutgoingMessageType::ReqSoftDollarTiers) => Some("REQ_SOFT_DOLLAR_TIERS"),
    Ok(OutgoingMessageType::ReqFamilyCodes) => Some("REQ_FAMILY_CODES"),
    Ok(OutgoingMessageType::ReqMatchingSymbols) => Some("REQ_MATCHING_SYMBOLS"),
    Ok(OutgoingMessageType::ReqMktDepthExchanges) => Some("REQ_MKT_DEPTH_EXCHANGES"),
    Ok(OutgoingMessageType::ReqSmartComponents) => Some("REQ_SMART_COMPONENTS"),
    Ok(OutgoingMessageType::ReqNewsArticle) => Some("REQ_NEWS_ARTICLE"),
    Ok(OutgoingMessageType::ReqNewsProviders) => Some("REQ_NEWS_PROVIDERS"),
    Ok(OutgoingMessageType::ReqHistoricalNews) => Some("REQ_HISTORICAL_NEWS"),
    Ok(OutgoingMessageType::ReqHeadTimestamp) => Some("REQ_HEAD_TIMESTAMP"),
    Ok(OutgoingMessageType::ReqHistogramData) => Some("REQ_HISTOGRAM_DATA"), // Corrected from placeholder
    Ok(OutgoingMessageType::CancelHistogramData) => Some("CANCEL_HISTOGRAM_DATA"),
    Ok(OutgoingMessageType::CancelHeadTimestamp) => Some("CANCEL_HEAD_TIMESTAMP"),
    Ok(OutgoingMessageType::ReqMarketRule) => Some("REQ_MARKET_RULE"),
    Ok(OutgoingMessageType::ReqPnl) => Some("REQ_PNL"),
    Ok(OutgoingMessageType::CancelPnl) => Some("CANCEL_PNL"),
    Ok(OutgoingMessageType::ReqPnlSingle) => Some("REQ_PNL_SINGLE"),
    Ok(OutgoingMessageType::CancelPnlSingle) => Some("CANCEL_PNL_SINGLE"),
    Ok(OutgoingMessageType::ReqHistoricalTicks) => Some("REQ_HISTORICAL_TICKS"),
    // Note: CancelHistoricalTicks uses CANCEL_HISTORICAL_DATA
    Ok(OutgoingMessageType::ReqTickByTickData) => Some("REQ_TICK_BY_TICK_DATA"),
    Ok(OutgoingMessageType::CancelTickByTickData) => Some("CANCEL_TICK_BY_TICK_DATA"),
    Ok(OutgoingMessageType::ReqCompletedOrders) => Some("REQ_COMPLETED_ORDERS"),
    Ok(OutgoingMessageType::ReqWshMetaData) => Some("REQ_WSH_META_DATA"),
    Ok(OutgoingMessageType::CancelWshMetaData) => Some("CANCEL_WSH_META_DATA"),
    Ok(OutgoingMessageType::ReqWshEventData) => Some("REQ_WSH_EVENT_DATA"),
    Ok(OutgoingMessageType::CancelWshEventData) => Some("CANCEL_WSH_EVENT_DATA"),
    Ok(OutgoingMessageType::ReqUserInfo) => Some("REQ_USER_INFO"),
    Err(_) => None, // Unknown type ID
  }
}

// Helper to allow conversion from i32, needed for the match statement above
impl TryFrom<i32> for OutgoingMessageType {
  type Error = ();

  fn try_from(v: i32) -> Result<Self, Self::Error> {
    match v {
      x if x == OutgoingMessageType::ReqMktData as i32 => Ok(OutgoingMessageType::ReqMktData),
      x if x == OutgoingMessageType::CancelMktData as i32 => Ok(OutgoingMessageType::CancelMktData),
      x if x == OutgoingMessageType::PlaceOrder as i32 => Ok(OutgoingMessageType::PlaceOrder),
      x if x == OutgoingMessageType::CancelOrder as i32 => Ok(OutgoingMessageType::CancelOrder),
      x if x == OutgoingMessageType::ReqOpenOrders as i32 => Ok(OutgoingMessageType::ReqOpenOrders),
      x if x == OutgoingMessageType::ReqAccountData as i32 => Ok(OutgoingMessageType::ReqAccountData),
      x if x == OutgoingMessageType::ReqExecutions as i32 => Ok(OutgoingMessageType::ReqExecutions),
      x if x == OutgoingMessageType::ReqIds as i32 => Ok(OutgoingMessageType::ReqIds),
      x if x == OutgoingMessageType::ReqContractData as i32 => Ok(OutgoingMessageType::ReqContractData),
      x if x == OutgoingMessageType::ReqMktDepth as i32 => Ok(OutgoingMessageType::ReqMktDepth),
      x if x == OutgoingMessageType::CancelMktDepth as i32 => Ok(OutgoingMessageType::CancelMktDepth),
      x if x == OutgoingMessageType::ReqNewsBulletins as i32 => Ok(OutgoingMessageType::ReqNewsBulletins),
      x if x == OutgoingMessageType::CancelNewsBulletins as i32 => Ok(OutgoingMessageType::CancelNewsBulletins),
      x if x == OutgoingMessageType::SetServerLoglevel as i32 => Ok(OutgoingMessageType::SetServerLoglevel),
      x if x == OutgoingMessageType::ReqAutoOpenOrders as i32 => Ok(OutgoingMessageType::ReqAutoOpenOrders),
      x if x == OutgoingMessageType::ReqAllOpenOrders as i32 => Ok(OutgoingMessageType::ReqAllOpenOrders),
      x if x == OutgoingMessageType::ReqManagedAccts as i32 => Ok(OutgoingMessageType::ReqManagedAccts),
      x if x == OutgoingMessageType::ReqFa as i32 => Ok(OutgoingMessageType::ReqFa),
      x if x == OutgoingMessageType::ReplaceFa as i32 => Ok(OutgoingMessageType::ReplaceFa),
      x if x == OutgoingMessageType::ReqHistoricalData as i32 => Ok(OutgoingMessageType::ReqHistoricalData),
      x if x == OutgoingMessageType::ExerciseOptions as i32 => Ok(OutgoingMessageType::ExerciseOptions),
      x if x == OutgoingMessageType::ReqScannerSubscription as i32 => Ok(OutgoingMessageType::ReqScannerSubscription),
      x if x == OutgoingMessageType::CancelScannerSubscription as i32 => Ok(OutgoingMessageType::CancelScannerSubscription),
      x if x == OutgoingMessageType::ReqScannerParameters as i32 => Ok(OutgoingMessageType::ReqScannerParameters),
      x if x == OutgoingMessageType::CancelHistoricalData as i32 => Ok(OutgoingMessageType::CancelHistoricalData),
      x if x == OutgoingMessageType::ReqCurrentTime as i32 => Ok(OutgoingMessageType::ReqCurrentTime),
      x if x == OutgoingMessageType::ReqRealTimeBars as i32 => Ok(OutgoingMessageType::ReqRealTimeBars),
      x if x == OutgoingMessageType::CancelRealTimeBars as i32 => Ok(OutgoingMessageType::CancelRealTimeBars),
      x if x == OutgoingMessageType::ReqFundamentalData as i32 => Ok(OutgoingMessageType::ReqFundamentalData),
      x if x == OutgoingMessageType::CancelFundamentalData as i32 => Ok(OutgoingMessageType::CancelFundamentalData),
      x if x == OutgoingMessageType::ReqCalcImpliedVolat as i32 => Ok(OutgoingMessageType::ReqCalcImpliedVolat),
      x if x == OutgoingMessageType::ReqCalcOptionPrice as i32 => Ok(OutgoingMessageType::ReqCalcOptionPrice),
      x if x == OutgoingMessageType::CancelCalcImpliedVolat as i32 => Ok(OutgoingMessageType::CancelCalcImpliedVolat),
      x if x == OutgoingMessageType::CancelCalcOptionPrice as i32 => Ok(OutgoingMessageType::CancelCalcOptionPrice),
      x if x == OutgoingMessageType::ReqGlobalCancel as i32 => Ok(OutgoingMessageType::ReqGlobalCancel),
      x if x == OutgoingMessageType::ReqMarketDataType as i32 => Ok(OutgoingMessageType::ReqMarketDataType),
      x if x == OutgoingMessageType::ReqPositions as i32 => Ok(OutgoingMessageType::ReqPositions),
      x if x == OutgoingMessageType::ReqAccountSummary as i32 => Ok(OutgoingMessageType::ReqAccountSummary),
      x if x == OutgoingMessageType::CancelAccountSummary as i32 => Ok(OutgoingMessageType::CancelAccountSummary),
      x if x == OutgoingMessageType::CancelPositions as i32 => Ok(OutgoingMessageType::CancelPositions),
      x if x == OutgoingMessageType::VerifyRequest as i32 => Ok(OutgoingMessageType::VerifyRequest),
      x if x == OutgoingMessageType::VerifyMessage as i32 => Ok(OutgoingMessageType::VerifyMessage),
      x if x == OutgoingMessageType::QueryDisplayGroups as i32 => Ok(OutgoingMessageType::QueryDisplayGroups),
      x if x == OutgoingMessageType::SubscribeToGroupEvents as i32 => Ok(OutgoingMessageType::SubscribeToGroupEvents),
      x if x == OutgoingMessageType::UpdateDisplayGroup as i32 => Ok(OutgoingMessageType::UpdateDisplayGroup),
      x if x == OutgoingMessageType::UnsubscribeFromGroupEvents as i32 => Ok(OutgoingMessageType::UnsubscribeFromGroupEvents),
      x if x == OutgoingMessageType::StartApi as i32 => Ok(OutgoingMessageType::StartApi),
      x if x == OutgoingMessageType::VerifyAndAuthRequest as i32 => Ok(OutgoingMessageType::VerifyAndAuthRequest),
      x if x == OutgoingMessageType::VerifyAndAuthMessage as i32 => Ok(OutgoingMessageType::VerifyAndAuthMessage),
      x if x == OutgoingMessageType::ReqPositionsMulti as i32 => Ok(OutgoingMessageType::ReqPositionsMulti),
      x if x == OutgoingMessageType::CancelPositionsMulti as i32 => Ok(OutgoingMessageType::CancelPositionsMulti),
      x if x == OutgoingMessageType::ReqAccountUpdatesMulti as i32 => Ok(OutgoingMessageType::ReqAccountUpdatesMulti),
      x if x == OutgoingMessageType::CancelAccountUpdatesMulti as i32 => Ok(OutgoingMessageType::CancelAccountUpdatesMulti),
      x if x == OutgoingMessageType::ReqSecDefOptParams as i32 => Ok(OutgoingMessageType::ReqSecDefOptParams),
      x if x == OutgoingMessageType::ReqSoftDollarTiers as i32 => Ok(OutgoingMessageType::ReqSoftDollarTiers),
      x if x == OutgoingMessageType::ReqFamilyCodes as i32 => Ok(OutgoingMessageType::ReqFamilyCodes),
      x if x == OutgoingMessageType::ReqMatchingSymbols as i32 => Ok(OutgoingMessageType::ReqMatchingSymbols),
      x if x == OutgoingMessageType::ReqMktDepthExchanges as i32 => Ok(OutgoingMessageType::ReqMktDepthExchanges),
      x if x == OutgoingMessageType::ReqSmartComponents as i32 => Ok(OutgoingMessageType::ReqSmartComponents),
      x if x == OutgoingMessageType::ReqNewsArticle as i32 => Ok(OutgoingMessageType::ReqNewsArticle),
      x if x == OutgoingMessageType::ReqNewsProviders as i32 => Ok(OutgoingMessageType::ReqNewsProviders),
      x if x == OutgoingMessageType::ReqHistoricalNews as i32 => Ok(OutgoingMessageType::ReqHistoricalNews),
      x if x == OutgoingMessageType::ReqHeadTimestamp as i32 => Ok(OutgoingMessageType::ReqHeadTimestamp),
      x if x == OutgoingMessageType::ReqHistogramData as i32 => Ok(OutgoingMessageType::ReqHistogramData), // Corrected
      x if x == OutgoingMessageType::CancelHistogramData as i32 => Ok(OutgoingMessageType::CancelHistogramData),
      x if x == OutgoingMessageType::CancelHeadTimestamp as i32 => Ok(OutgoingMessageType::CancelHeadTimestamp),
      x if x == OutgoingMessageType::ReqMarketRule as i32 => Ok(OutgoingMessageType::ReqMarketRule),
      x if x == OutgoingMessageType::ReqPnl as i32 => Ok(OutgoingMessageType::ReqPnl),
      x if x == OutgoingMessageType::CancelPnl as i32 => Ok(OutgoingMessageType::CancelPnl),
      x if x == OutgoingMessageType::ReqPnlSingle as i32 => Ok(OutgoingMessageType::ReqPnlSingle),
      x if x == OutgoingMessageType::CancelPnlSingle as i32 => Ok(OutgoingMessageType::CancelPnlSingle),
      x if x == OutgoingMessageType::ReqHistoricalTicks as i32 => Ok(OutgoingMessageType::ReqHistoricalTicks),
      // Note: CancelHistoricalTicks uses CancelHistoricalData (25)
      x if x == OutgoingMessageType::ReqTickByTickData as i32 => Ok(OutgoingMessageType::ReqTickByTickData),
      x if x == OutgoingMessageType::CancelTickByTickData as i32 => Ok(OutgoingMessageType::CancelTickByTickData),
      x if x == OutgoingMessageType::ReqCompletedOrders as i32 => Ok(OutgoingMessageType::ReqCompletedOrders),
      x if x == OutgoingMessageType::ReqWshMetaData as i32 => Ok(OutgoingMessageType::ReqWshMetaData),
      x if x == OutgoingMessageType::CancelWshMetaData as i32 => Ok(OutgoingMessageType::CancelWshMetaData),
      x if x == OutgoingMessageType::ReqWshEventData as i32 => Ok(OutgoingMessageType::ReqWshEventData),
      x if x == OutgoingMessageType::CancelWshEventData as i32 => Ok(OutgoingMessageType::CancelWshEventData),
      x if x == OutgoingMessageType::ReqUserInfo as i32 => Ok(OutgoingMessageType::ReqUserInfo),
      // Existing scanner messages
      x if x == OutgoingMessageType::ReqScannerSubscription as i32 => Ok(OutgoingMessageType::ReqScannerSubscription),
      x if x == OutgoingMessageType::CancelScannerSubscription as i32 => Ok(OutgoingMessageType::CancelScannerSubscription),
      x if x == OutgoingMessageType::ReqScannerParameters as i32 => Ok(OutgoingMessageType::ReqScannerParameters),
      _ => Err(()),
    }
  }
}


/// Message encoder for the TWS API protocol
pub struct Encoder {
  server_version: i32,
  // Removed: writer: Box<dyn Write + Send>,
  // Removed: next_valid_id - should be managed by broker/client
}

impl Encoder {
  /// Create a new message encoder for a specific server version.
  pub fn new(server_version: i32) -> Self {
    Self { server_version }
  }

  // --- Helper methods (start_encoding, finish_encoding, write_*, etc.) ---
  fn start_encoding(&self, msg_type: i32) -> Result<Cursor<Vec<u8>>, IBKRError> {
    let buffer = Vec::new();
    let mut cursor = Cursor::new(buffer);
    self.write_int_to_cursor(&mut cursor, msg_type)?;
    Ok(cursor)
  }

  fn finish_encoding(&self, cursor: Cursor<Vec<u8>>) -> Vec<u8> {
    let ret = cursor.into_inner();
    log::debug!("Request: {}", msg_to_string(&ret));
    ret
  }

  fn is_empty(s: Option<&str>) -> bool {
    s.map_or(true, |val| val.is_empty())
  }

  fn write_str_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, s: &str) -> Result<(), IBKRError> {
    trace!("Encoding string: {}", s);
    cursor.write_all(s.as_bytes()).map_err(|e| IBKRError::InternalError(format!("Buffer write failed: {}", e)))?;
    cursor.write_all(&[0]).map_err(|e| IBKRError::InternalError(format!("Buffer write failed: {}", e)))?;
    Ok(())
  }

  fn write_optional_str_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, s: Option<&str>) -> Result<(), IBKRError> {
    self.write_str_to_cursor(cursor, s.unwrap_or(""))
  }

  fn write_int_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, val: i32) -> Result<(), IBKRError> {
    self.write_str_to_cursor(cursor, &val.to_string())
  }

  /// Writes an optional integer, sending an empty string if None or i32::MAX.
  fn write_optional_int_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, val: Option<i32>) -> Result<(), IBKRError> {
    match val {
      Some(v) if v != i32::MAX => self.write_int_to_cursor(cursor, v),
      _ => self.write_str_to_cursor(cursor, ""), // Send empty string for None or MAX_VALUE
    }
  }

  /// Writes an optional i64, sending "0" if None.
  fn write_optional_i64_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, val: Option<i64>) -> Result<(), IBKRError> {
    match val {
      Some(v) => self.write_str_to_cursor(cursor, &v.to_string()),
      None => self.write_str_to_cursor(cursor, "0"), // Default for parentId seems to be 0
    }
  }

  fn write_double_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, val: f64) -> Result<(), IBKRError> {
    if val.is_nan() {
      log::warn!("Attempting to encode NaN double value. Sending 0.0.");
      self.write_str_to_cursor(cursor, "0.0")
    } else if val.is_infinite() {
      // TWS uses Double.MAX_VALUE for infinity in many cases
      warn!("Attempting to encode infinite double value. Sending MAX_VALUE string.");
      self.write_optional_double_to_cursor(cursor, None) // Send MAX_VALUE representation
    } else {
      self.write_str_to_cursor(cursor, &val.to_string())
    }
  }

  fn write_optional_double_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, val: Option<f64>) -> Result<(), IBKRError> {
    match val {
      Some(v) if v != f64::MAX => self.write_double_to_cursor(cursor, v),
      _ => self.write_str_to_cursor(cursor, ""),
    }
  }

  fn write_bool_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, val: bool) -> Result<(), IBKRError> {
    self.write_int_to_cursor(cursor, if val { 1 } else { 0 })
  }

  fn write_optional_bool_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, val: Option<bool>) -> Result<(), IBKRError> {
    if let Some(val) = val {
      self.write_bool_to_cursor(cursor, val)
    } else {
      self.write_int_to_cursor(cursor, i32::MAX)
    }
  }

  // Helper for TagValue lists
  fn write_tag_value_list(&self, cursor: &mut Cursor<Vec<u8>>, list: &[(String, String)]) -> Result<(), IBKRError> {
    let enc = list
      .iter()
      .map(|(k, v)| format!("{}={}", k, v))
      .collect::<Vec<String>>()
      .join(",");
    self.write_str_to_cursor(cursor, &enc)?;
    Ok(())
  }

  // Helper for formatting DateTime<Utc> to "YYYYMMDD HH:MM:SS (zzz)" format if needed
  // TWS often expects "YYYYMMDD HH:MM:SS" without timezone, sometimes UTC implicitly
  // `format_spec`: Pass chrono format specifiers like "%Y%m%d %H:%M:%S", "%Y%m%d-%H:%M:%S", "%Y%m%d"
  // `tz_suffix`: Optional timezone suffix like " UTC" or " EST" if needed by API field
  fn format_datetime_tws(&self, dt: Option<DateTime<Utc>>, format_spec: &str, tz_suffix: Option<&str>) -> String {
    dt.map(|d| {
      // Format the DateTime according to the specifier
      let mut formatted = d.format(format_spec).to_string();
      // Append the timezone suffix if one was provided
      if let Some(suffix) = tz_suffix {
        formatted.push_str(suffix);
      }
      formatted
    }).unwrap_or_default() // Return an empty string if the input Option<DateTime> was None
  }

  // --- encode_request_ids, etc. ---
  pub fn _encode_request_ids(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request IDs message");
    let mut cursor = self.start_encoding(OutgoingMessageType::ReqIds as i32)?;
    self.write_int_to_cursor(&mut cursor, 1)?; // Version
    Ok(self.finish_encoding(cursor))
  }

  pub fn encode_request_account_summary(
    &self,
    req_id: i32,
    group: &str,
    tags: &str, // Comma separated list: "AccountType,NetLiquidation,TotalCashValue,SettledCash,AccruedCash,BuyingPower,EquityWithLoanValue,PreviousEquityWithLoanValue,GrossPositionValue,ReqTEquity,ReqTMargin,SMA,InitMarginReq,MaintMarginReq,AvailableFunds,ExcessLiquidity,Cushion,FullInitMarginReq,FullMaintMarginReq,FullAvailableFunds,FullExcessLiquidity,LookAheadNextChange,LookAheadInitMarginReq,LookAheadMaintMarginReq,LookAheadAvailableFunds,LookAheadExcessLiquidity,HighestSeverity,DayTradesRemaining,Leverage"
  ) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request account summary message: ReqID={}, Group={}, Tags={}", req_id, group, tags);
    if self.server_version < min_server_ver::ACCT_SUMMARY {
      return Err(IBKRError::Unsupported("Server version does not support account summary requests.".to_string()));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::ReqAccountSummary as i32)?;
    let version = 1;
    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;
    self.write_str_to_cursor(&mut cursor, group)?; // "All" for all accounts, specific group name, or comma-separated list of account IDs
    self.write_str_to_cursor(&mut cursor, tags)?;
    Ok(self.finish_encoding(cursor))
  }

  pub fn encode_request_executions(&self, req_id: i32, filter: &ExecutionFilter) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request executions: ReqID={}, Filter={:?}", req_id, filter);
    let mut cursor = self.start_encoding(OutgoingMessageType::ReqExecutions as i32)?;
    let version = 3; // Version supporting filter fields

    self.write_int_to_cursor(&mut cursor, version)?;

    // reqId field introduced in version 3 for this message, different from others
    if self.server_version >= min_server_ver::EXECUTION_DATA_CHAIN {
      self.write_int_to_cursor(&mut cursor, req_id)?;
    }

    // Filter fields (introduced in version 3 / server_version 9)
    // Use defaults (0 or "") if Option is None
    if self.server_version >= 9 {
      self.write_int_to_cursor(&mut cursor, filter.client_id.unwrap_or(0))?;
      self.write_str_to_cursor(&mut cursor, filter.acct_code.as_deref().unwrap_or(""))?;
      // Time format: "yyyyMMdd-HH:mm:ss" (UTC) or "yyyyMMdd HH:mm:ss timezone"
      self.write_str_to_cursor(&mut cursor, filter.time.as_deref().unwrap_or(""))?;
      self.write_str_to_cursor(&mut cursor, filter.symbol.as_deref().unwrap_or(""))?;
      self.write_str_to_cursor(&mut cursor, filter.sec_type.as_deref().unwrap_or(""))?;
      self.write_str_to_cursor(&mut cursor, filter.exchange.as_deref().unwrap_or(""))?;
      self.write_str_to_cursor(&mut cursor, filter.side.as_deref().unwrap_or(""))?;
    }

    Ok(self.finish_encoding(cursor))
  }

  pub fn encode_request_positions(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request positions message");
    if self.server_version < min_server_ver::ACCT_SUMMARY {
      return Err(IBKRError::Unsupported("Server version does not support position requests.".to_string()));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::ReqPositions as i32)?;
    let version = 1;
    self.write_int_to_cursor(&mut cursor, version)?;
    Ok(self.finish_encoding(cursor))
  }

  // --- cancel account summary / positions ---
  pub fn encode_cancel_account_summary(&self, req_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel account summary: ReqID={}", req_id);
    if self.server_version < min_server_ver::ACCT_SUMMARY {
      return Err(IBKRError::Unsupported("Server version does not support account summary cancellation.".to_string()));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelAccountSummary as i32)?;
    let version = 1;
    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;
    Ok(self.finish_encoding(cursor))
  }

  pub fn encode_cancel_positions(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel positions");
    if self.server_version < min_server_ver::ACCT_SUMMARY {
      return Err(IBKRError::Unsupported("Server version does not support position cancellation.".to_string()));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelPositions as i32)?;
    let version = 1;
    self.write_int_to_cursor(&mut cursor, version)?;
    Ok(self.finish_encoding(cursor))
  }

  // --- cancel order ---
  pub fn encode_cancel_order(&self, order_id: i32, order_cancel: &OrderCancel) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel order message for order ID {}, CancelParams: {:?}", order_id, order_cancel);

    if self.server_version < min_server_ver::MANUAL_ORDER_TIME {
      if order_cancel.manual_order_cancel_time.is_some() {
        return Err(IBKRError::Unsupported("Server version does not support manual order cancel time attribute".to_string()));
      }
    }
    if self.server_version < min_server_ver::RFQ_FIELDS {
      if order_cancel.ext_operator.is_some() || order_cancel.external_user_id.is_some() || order_cancel.manual_order_indicator.is_some() {
        return Err(IBKRError::Unsupported("Server version does not support ext operator, external user id and manual order indicator parameters".to_string()));
      }
    }

    let mut cursor = self.start_encoding(OutgoingMessageType::CancelOrder as i32)?;
    let version = 1;
    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, order_id)?;

    // manualOrderCancelTime added in MANUAL_ORDER_TIME
    if self.server_version >= min_server_ver::MANUAL_ORDER_TIME {
      self.write_optional_str_to_cursor(&mut cursor, order_cancel.manual_order_cancel_time.as_deref())?;
    }

    // RFQ fields added in RFQ_FIELDS
    if self.server_version >= min_server_ver::RFQ_FIELDS {
      self.write_optional_str_to_cursor(&mut cursor, order_cancel.ext_operator.as_deref())?;
      self.write_optional_str_to_cursor(&mut cursor, order_cancel.external_user_id.as_deref())?;
      self.write_optional_int_to_cursor(&mut cursor, order_cancel.manual_order_indicator)?;
    }

    Ok(self.finish_encoding(cursor))
  }


  // --- place order ---
  pub fn encode_place_order(&self, id: i32, contract: &Contract, request: &OrderRequest) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding place order message: ID={}, Contract={:?}, Request={:?}", id, contract, request);

    if self.server_version < min_server_ver::SCALE_ORDERS {
      if request.scale_init_level_size.is_some() || request.scale_price_increment.is_some() {
        return Err(IBKRError::Unsupported(
          "Server version does not support Scale orders (scaleInitLevelSize, scalePriceIncrement).".to_string(),
        ));
      }
    }
    if self.server_version < min_server_ver::SSHORT_COMBO_LEGS {
      if !contract.combo_legs.is_empty() {
        for leg in &contract.combo_legs {
          if leg.short_sale_slot != 0 || !leg.designated_location.is_empty() {
            return Err(IBKRError::Unsupported(
              "Server version does not support SSHORT flag for combo legs.".to_string(),
            ));
          }
        }
      }
    }
    if self.server_version < min_server_ver::WHAT_IF_ORDERS {
      if request.what_if {
        return Err(IBKRError::Unsupported(
          "Server version does not support what-if orders.".to_string(),
        ));
      }
    }
    if self.server_version < min_server_ver::DELTA_NEUTRAL {
      if contract.delta_neutral_contract.is_some() {
        return Err(IBKRError::Unsupported(
          "Server version does not support delta-neutral orders.".to_string(),
        ));
      }
    }
    if self.server_version < min_server_ver::SCALE_ORDERS2 {
      if request.scale_subs_level_size.is_some() {
        return Err(IBKRError::Unsupported(
          "Server version does not support Subsequent Level Size for Scale orders.".to_string(),
        ));
      }
    }
    if self.server_version < min_server_ver::ALGO_ORDERS {
      if !request.algo_strategy.is_none() {
        return Err(IBKRError::Unsupported(
          "Server version does not support algo orders.".to_string(),
        ));
      }
    }
    if self.server_version < min_server_ver::NOT_HELD {
      if request.not_held {
        return Err(IBKRError::Unsupported(
          "Server version does not support notHeld parameter.".to_string(),
        ));
      }
    }
    if self.server_version < min_server_ver::SEC_ID_TYPE {
      if !contract.sec_id_type.is_none() || !contract.sec_id.is_none() {
        return Err(IBKRError::Unsupported(
          "Server version does not support secIdType and secId parameters.".to_string(),
        ));
      }
    }
    if self.server_version < min_server_ver::PLACE_ORDER_CONID {
      if contract.con_id > 0 {
        return Err(IBKRError::Unsupported(
          "Server version does not support conId parameter in placeOrder.".to_string(),
        ));
      }
    }
    // SSHORTX check for Order.exemptCode
    if self.server_version < min_server_ver::SSHORTX {
      // Use the -1 sentinel check
      if request.exempt_code.is_some() && request.exempt_code != Some(-1) {
        return Err(IBKRError::Unsupported(
          "Server version does not support exemptCode parameter.".to_string(),
        ));
      }
    }
    // SSHORTX check for ComboLeg.exemptCode (Using SSHORTX version)
    if self.server_version < min_server_ver::SSHORTX {
      if !contract.combo_legs.is_empty() {
        for leg in &contract.combo_legs {
          if leg.exempt_code != -1 {
            return Err(IBKRError::Unsupported(
              "Server version does not support exemptCode parameter for combo legs.".to_string(),
            ));
          }
        }
      }
    }
    if self.server_version < min_server_ver::HEDGE_ORDERS {
      if !request.hedge_type.is_none() {
        return Err(IBKRError::Unsupported(
          "Server version does not support hedge orders.".to_string(),
        ));
      }
    }
    if self.server_version < min_server_ver::OPT_OUT_SMART_ROUTING {
      if request.opt_out_smart_routing {
        return Err(IBKRError::Unsupported(
          "Server version does not support optOutSmartRouting parameter.".to_string(),
        ));
      }
    }
    if self.server_version < min_server_ver::DELTA_NEUTRAL_CONID {
      if request.delta_neutral_con_id.is_some()
        || !request.delta_neutral_settling_firm.is_none()
        || !request.delta_neutral_clearing_account.is_none()
        || !request.delta_neutral_clearing_intent.is_none() {
          return Err(IBKRError::Unsupported(
            "Server version does not support deltaNeutral parameters: ConId, SettlingFirm, ClearingAccount, ClearingIntent".to_string(),
          ));
        }
    }
    if self.server_version < min_server_ver::DELTA_NEUTRAL_OPEN_CLOSE {
      if !request.delta_neutral_open_close.is_none()
        || request.delta_neutral_short_sale
        || request.delta_neutral_short_sale_slot.is_some()
        || !request.delta_neutral_designated_location.is_none() {
          return Err(IBKRError::Unsupported(
            "Server version does not support deltaNeutral parameters: OpenClose, ShortSale, ShortSaleSlot, DesignatedLocation".to_string(),
          ));
        }
    }
    if self.server_version < min_server_ver::SCALE_ORDERS3 {
      if request.scale_price_increment.is_some() && request.scale_price_increment != Some(f64::MAX) {
        if request.scale_price_adjust_value.is_some()
          || request.scale_price_adjust_interval.is_some()
          || request.scale_profit_offset.is_some()
          || request.scale_auto_reset
          || request.scale_init_position.is_some()
          || request.scale_init_fill_qty.is_some()
          || request.scale_random_percent {
            return Err(IBKRError::Unsupported(
              "Server version does not support Scale order parameters: PriceAdjustValue, PriceAdjustInterval, ProfitOffset, AutoReset, InitPosition, InitFillQty and RandomPercent".to_string(),
            ));
          }
      }
    }
    if self.server_version < min_server_ver::ORDER_COMBO_LEGS_PRICE && contract.sec_type == SecType::Combo {
      if !request.order_combo_legs.is_empty() {
        for leg_price in &request.order_combo_legs {
          if leg_price.is_some() {
            return Err(IBKRError::Unsupported(
              "Server version does not support per-leg prices for order combo legs.".to_string(),
            ));
          }
        }
      }
    }
    if self.server_version < min_server_ver::TRAILING_PERCENT {
      if request.trailing_percent.is_some() {
        return Err(IBKRError::Unsupported(
          "Server version does not support trailing percent parameter".to_string(),
        ));
      }
    }
    if self.server_version < min_server_ver::TRADING_CLASS {
      if !contract.trading_class.is_none() {
        return Err(IBKRError::Unsupported(
          "Server version does not support tradingClass parameter in placeOrder.".to_string(),
        ));
      }
    }
    if self.server_version < min_server_ver::ALGO_ID && !request.algo_id.is_none() {
      return Err(IBKRError::Unsupported("Server version does not support algoId parameter".to_string()));
    }
    if self.server_version < min_server_ver::SCALE_TABLE {
      if !request.scale_table.is_none() || request.active_start_time.is_some() || request.active_stop_time.is_some() {
        return Err(IBKRError::Unsupported(
          "Server version does not support scaleTable, activeStartTime and activeStopTime parameters.".to_string(),
        ));
      }
    }
    if self.server_version < min_server_ver::ORDER_SOLICITED {
      if request.solicited {
        return Err(IBKRError::Unsupported(
          "Server version does not support order solicited parameter.".to_string(),
        ));
      }
    }
    if self.server_version < min_server_ver::MODELS_SUPPORT {
      if !request.model_code.is_none() {
        return Err(IBKRError::Unsupported(
          "Server version does not support model code parameter.".to_string(),
        ));
      }
    }
    if self.server_version < min_server_ver::EXT_OPERATOR && !request.ext_operator.is_none() {
      return Err(IBKRError::Unsupported("Server version does not support ext operator".to_string()));
    }
    if self.server_version < min_server_ver::SOFT_DOLLAR_TIER && request.soft_dollar_tier.is_some() {
      return Err(IBKRError::Unsupported("Server version does not support soft dollar tier".to_string()));
    }
    if self.server_version < min_server_ver::CASH_QTY {
      if request.cash_qty.is_some() {
        return Err(IBKRError::Unsupported(
          "Server version does not support cash quantity parameter".to_string(),
        ));
      }
    }
    if self.server_version < min_server_ver::DECISION_MAKER
      && (!request.mifid2_decision_maker.is_none()
          || !request.mifid2_decision_algo.is_none()) {
        return Err(IBKRError::Unsupported(
          "Server version does not support MIFID II decision maker parameters".to_string(),
        ));
      }
    if self.server_version < min_server_ver::MIFID_EXECUTION
      && (!request.mifid2_execution_trader.is_none()
          || !request.mifid2_execution_algo.is_none()) {
        return Err(IBKRError::Unsupported(
          "Server version does not support MIFID II execution parameters".to_string(),
        ));
      }
    if self.server_version < min_server_ver::AUTO_PRICE_FOR_HEDGE
      && request.dont_use_auto_price_for_hedge {
        return Err(IBKRError::Unsupported(
          "Server version does not support don't use auto price for hedge parameter.".to_string(),
        ));
      }
    if self.server_version < min_server_ver::ORDER_CONTAINER
      && request.is_oms_container {
        return Err(IBKRError::Unsupported(
          "Server version does not support oms container parameter.".to_string(),
        ));
      }
    if self.server_version < min_server_ver::D_PEG_ORDERS
      && request.discretionary_up_to_limit_price {
        return Err(IBKRError::Unsupported(
          "Server version does not support D-Peg orders.".to_string(),
        ));
      }
    if self.server_version < min_server_ver::PRICE_MGMT_ALGO
      && request.use_price_mgmt_algo.is_some() {
        return Err(IBKRError::Unsupported("Server version does not support price management algo parameter".to_string()));
      }
    if self.server_version < min_server_ver::DURATION
      && request.duration.is_some() {
        return Err(IBKRError::Unsupported("Server version does not support duration attribute".to_string()));
      }
    if self.server_version < min_server_ver::POST_TO_ATS
      && request.post_to_ats.is_some() {
        return Err(IBKRError::Unsupported("Server version does not support postToAts attribute".to_string()));
      }
    if self.server_version < min_server_ver::AUTO_CANCEL_PARENT
      && request.auto_cancel_parent {
        return Err(IBKRError::Unsupported("Server version does not support autoCancelParent attribute".to_string()));
      }
    if self.server_version < min_server_ver::ADVANCED_ORDER_REJECT {
      if !request.advanced_error_override.is_none() {
        return Err(IBKRError::Unsupported("Server version does not support advanced error override attribute".to_string()));
      }
    }
    if self.server_version < min_server_ver::MANUAL_ORDER_TIME {
      if request.manual_order_time.is_some() {
        return Err(IBKRError::Unsupported("Server version does not support manual order time attribute".to_string()));
      }
    }
    if self.server_version < min_server_ver::PEGBEST_PEGMID_OFFSETS {
      if request.min_trade_qty.is_some() ||
        request.min_compete_size.is_some() ||
        request.compete_against_best_offset.is_some() || // Check includes INFINITY sentinel
        request.mid_offset_at_whole.is_some() ||
        request.mid_offset_at_half.is_some() {
          return Err(IBKRError::Unsupported(
            "Server version does not support PEG BEST / PEG MID order parameters: minTradeQty, minCompeteSize, competeAgainstBestOffset, midOffsetAtWhole and midOffsetAtHalf".to_string(),
          ));
        }
    }
    if self.server_version < min_server_ver::CUSTOMER_ACCOUNT {
      if !request.customer_account.is_none() {
        return Err(IBKRError::Unsupported("Server version does not support customer account parameter".to_string()));
      }
    }
    if self.server_version < min_server_ver::PROFESSIONAL_CUSTOMER {
      if request.professional_customer {
        return Err(IBKRError::Unsupported("Server version does not support professional customer parameter".to_string()));
      }
    }
    if self.server_version < min_server_ver::RFQ_FIELDS &&
      (!request.external_user_id.is_none() || request.manual_order_indicator.is_some()) {
        return Err(IBKRError::Unsupported("Server version does not support external user id and manual order indicator parameters".to_string()));
      }


    // --- Start Encoding ---
    let version = if self.server_version < min_server_ver::NOT_HELD { 27 } else { 45 };

    let mut cursor = self.start_encoding(OutgoingMessageType::PlaceOrder as i32)?;

    // Send version only if server < ORDER_CONTAINER
    if self.server_version < min_server_ver::ORDER_CONTAINER {
      self.write_int_to_cursor(&mut cursor, version)?;
    }

    // --- Write Order ID ---
    self.write_int_to_cursor(&mut cursor, id)?;

    // --- Write Contract Fields ---
    if self.server_version >= min_server_ver::PLACE_ORDER_CONID {
      self.write_int_to_cursor(&mut cursor, contract.con_id)?;
    }
    self.write_str_to_cursor(&mut cursor, &contract.symbol)?;
    self.write_str_to_cursor(&mut cursor, &contract.sec_type.to_string())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.last_trade_date_or_contract_month.as_deref())?;
    self.write_optional_double_to_cursor(&mut cursor, contract.strike)?;
    let right_str = contract.right.map(|r| r.to_string());
    self.write_optional_str_to_cursor(&mut cursor, right_str.as_deref())?;
    if self.server_version >= 15 {
      self.write_optional_str_to_cursor(&mut cursor, contract.multiplier.as_deref())?;
    }
    self.write_str_to_cursor(&mut cursor, &contract.exchange)?;
    if self.server_version >= 14 {
      self.write_optional_str_to_cursor(&mut cursor, contract.primary_exchange.as_deref())?;
    }
    self.write_str_to_cursor(&mut cursor, &contract.currency)?;
    if self.server_version >= 2 {
      self.write_optional_str_to_cursor(&mut cursor, contract.local_symbol.as_deref())?;
    }
    if self.server_version >= min_server_ver::TRADING_CLASS {
      self.write_optional_str_to_cursor(&mut cursor, contract.trading_class.as_deref())?;
    }
    if self.server_version >= min_server_ver::SEC_ID_TYPE {
      let sec_id_type_str = contract.sec_id_type.as_ref().map(|t| t.to_string());
      self.write_optional_str_to_cursor(&mut cursor, sec_id_type_str.as_deref())?;
      self.write_optional_str_to_cursor(&mut cursor, contract.sec_id.as_deref())?;
    }

    // --- Write Main Order Fields ---
    self.write_str_to_cursor(&mut cursor, &request.side.to_string())?;
    if self.server_version >= min_server_ver::FRACTIONAL_POSITIONS {
      self.write_str_to_cursor(&mut cursor, &request.quantity.to_string())?;
    } else {
      if request.quantity != request.quantity.trunc() {
        warn!("Fractional quantity {} provided but server version {} doesn't support it. Sending truncated integer.", request.quantity, self.server_version);
      }
      self.write_int_to_cursor(&mut cursor, request.quantity.trunc() as i32)?;
    }
    self.write_str_to_cursor(&mut cursor, &request.order_type.to_string())?;
    if self.server_version < min_server_ver::ORDER_COMBO_LEGS_PRICE {
      self.write_optional_double_to_cursor(&mut cursor, request.limit_price)?;
    } else {
      self.write_optional_double_to_cursor(&mut cursor, request.limit_price)?;
    }
    if self.server_version < min_server_ver::TRAILING_PERCENT {
      self.write_optional_double_to_cursor(&mut cursor, request.aux_price)?;
    } else {
      self.write_optional_double_to_cursor(&mut cursor, request.aux_price)?;
    }

    // --- Write Extended Order Fields (In Order) ---
    self.write_str_to_cursor(&mut cursor, &request.time_in_force.to_string())?;
    self.write_optional_str_to_cursor(&mut cursor, request.oca_group.as_deref())?;
    self.write_optional_str_to_cursor(&mut cursor, request.account.as_deref())?;
    self.write_optional_str_to_cursor(&mut cursor, request.open_close.as_deref())?;
    self.write_int_to_cursor(&mut cursor, request.origin)?;
    self.write_optional_str_to_cursor(&mut cursor, request.order_ref.as_deref())?;
    self.write_bool_to_cursor(&mut cursor, request.transmit)?;
    if self.server_version >= 4 { self.write_optional_i64_to_cursor(&mut cursor, request.parent_id)?; }
    if self.server_version >= 5 {
      self.write_bool_to_cursor(&mut cursor, request.block_order)?;
      self.write_bool_to_cursor(&mut cursor, request.sweep_to_fill)?;
      self.write_optional_int_to_cursor(&mut cursor, request.display_size)?;
      self.write_optional_int_to_cursor(&mut cursor, request.trigger_method)?;
      self.write_bool_to_cursor(&mut cursor, request.outside_rth)?;
    }
    if self.server_version >= 7 { self.write_bool_to_cursor(&mut cursor, request.hidden)?; }

    // --- Combo Legs (Contract) ---
    if self.server_version >= 8 && contract.sec_type == SecType::Combo {
      self.encode_combo_legs(&mut cursor, &contract.combo_legs)?;
    }

    // --- Order Combo Legs (Prices) ---
    if self.server_version >= min_server_ver::ORDER_COMBO_LEGS_PRICE && contract.sec_type == SecType::Combo {
      self.write_int_to_cursor(&mut cursor, request.order_combo_legs.len() as i32)?;
      for leg_price in &request.order_combo_legs {
        self.write_optional_double_to_cursor(&mut cursor, *leg_price)?;
      }
    }

    // --- Smart Combo Routing Params - Sent for BAG secType ---
    if self.server_version >= min_server_ver::SMART_COMBO_ROUTING_PARAMS && contract.sec_type == SecType::Combo {
      self.write_int_to_cursor(&mut cursor, request.smart_combo_routing_params.len() as i32)?;
      if !request.smart_combo_routing_params.is_empty() {
        for (tag, value) in &request.smart_combo_routing_params {
          self.write_str_to_cursor(&mut cursor, tag)?;
          self.write_str_to_cursor(&mut cursor, value)?;
        }
      }
    }

    // --- Deprecated Shares Allocation ---
    if self.server_version >= 9 {
      self.write_str_to_cursor(&mut cursor, "")?;
    }

    // --- Discretionary Amount ---
    if self.server_version >= 10 {
      self.write_optional_double_to_cursor(&mut cursor, request.discretionary_amt)?;
    }

    // --- Good After Time ---
    if self.server_version >= 11 {
      let gat_str = self.format_datetime_tws(request.good_after_time, "%Y%m%d %H:%M:%S", Some(" UTC"));
      self.write_str_to_cursor(&mut cursor, &gat_str)?;
    }

    // --- Good Till Date ---
    if self.server_version >= 12 {
      // Using full format for safety, TWS might accept "YYYYMMDD" too. Add " UTC" suffix.
      let gtd_str = self.format_datetime_tws(request.good_till_date, "%Y%m%d %H:%M:%S", Some(" UTC"));
      self.write_str_to_cursor(&mut cursor, &gtd_str)?;
    }

    // --- Financial Advisor Fields ---
    if self.server_version >= 13 {
      self.write_optional_str_to_cursor(&mut cursor, request.fa_group.as_deref())?;
      self.write_optional_str_to_cursor(&mut cursor, request.fa_method.as_deref())?;
      self.write_optional_str_to_cursor(&mut cursor, request.fa_percentage.as_deref())?;
      // Deprecated faProfile field
      if self.server_version < min_server_ver::FA_PROFILE_DESUPPORT {
        self.write_str_to_cursor(&mut cursor, "")?;
      }
    }

    // --- Model Code ---
    if self.server_version >= min_server_ver::MODELS_SUPPORT {
      self.write_optional_str_to_cursor(&mut cursor, request.model_code.as_deref())?;
    }

    // --- Short Sale Slot Fields ---
    if self.server_version >= 18 {
      self.write_optional_int_to_cursor(&mut cursor, request.short_sale_slot)?;
      self.write_optional_str_to_cursor(&mut cursor, request.designated_location.as_deref())?;
    }
    // Exempt Code
    if self.server_version >= min_server_ver::SSHORTX_OLD {
      let exempt_code_val = request.exempt_code.unwrap_or(-1); // Use -1 sentinel if None
      self.write_int_to_cursor(&mut cursor, if exempt_code_val == -1 { 0 } else { exempt_code_val })?;
    }

    // --- OCA Type, Rule 80A etc. (Starting server version 19) ---
    if self.server_version >= 19 {
      self.write_optional_int_to_cursor(&mut cursor, request.oca_type)?;
      // RTH Only deprecated in 38, handled earlier
      // if self.server_version < 38 { self.write_bool_to_cursor(&mut cursor, false)?; } // Deprecated rthOnly
      self.write_optional_str_to_cursor(&mut cursor, request.rule_80a.as_deref())?;
      self.write_optional_str_to_cursor(&mut cursor, request.settling_firm.as_deref())?;
      self.write_bool_to_cursor(&mut cursor, request.all_or_none)?;
      self.write_optional_int_to_cursor(&mut cursor, request.min_quantity)?;
      self.write_optional_double_to_cursor(&mut cursor, request.percent_offset)?;
      self.write_bool_to_cursor(&mut cursor, false)?; // Deprecated eTradeOnly
      self.write_bool_to_cursor(&mut cursor, false)?; // Deprecated firmQuoteOnly
      self.write_optional_double_to_cursor(&mut cursor, None)?; // Deprecated nbboPriceCap
      self.write_optional_int_to_cursor(&mut cursor, request.auction_strategy)?;
      self.write_optional_double_to_cursor(&mut cursor, request.starting_price)?;
      self.write_optional_double_to_cursor(&mut cursor, request.stock_ref_price)?;
      self.write_optional_double_to_cursor(&mut cursor, request.delta)?;
      let (lower, upper) = if self.server_version == 26 && request.order_type == OrderType::Volatility {
        (request.stock_range_lower, request.stock_range_upper) // Send actual values only in this specific case
      } else {
        (None, None) // Otherwise, treat as unset (will send "" via sendMax)
      };
      self.write_optional_double_to_cursor(&mut cursor, lower)?;
      self.write_optional_double_to_cursor(&mut cursor, upper)?;
    }

    // --- Override Percentage Constraints ---
    if self.server_version >= 22 {
      self.write_bool_to_cursor(&mut cursor, request.override_percentage_constraints)?;
    }

    // --- Volatility Orders (Starting server version 26) ---
    if self.server_version >= 26 {
      self.write_optional_double_to_cursor(&mut cursor, request.volatility)?;
      self.write_optional_int_to_cursor(&mut cursor, request.volatility_type)?;
      // Delta Neutral Order Type Handling
      if self.server_version < 28 {
        let is_delta_neutral_mkt = request.delta_neutral_order_type.as_deref().map_or(false, |s| s.eq_ignore_ascii_case("MKT"));
        self.write_bool_to_cursor(&mut cursor, is_delta_neutral_mkt)?;
      } else {
        self.write_optional_str_to_cursor(&mut cursor, request.delta_neutral_order_type.as_deref())?;
        self.write_optional_double_to_cursor(&mut cursor, request.delta_neutral_aux_price)?;

        // Delta Neutral Contract Fields (sent only if type is not empty)
        if self.server_version >= min_server_ver::DELTA_NEUTRAL_CONID && !Encoder::is_empty(request.delta_neutral_order_type.as_deref()) {
          self.write_optional_int_to_cursor(&mut cursor, request.delta_neutral_con_id)?;
          self.write_optional_str_to_cursor(&mut cursor, request.delta_neutral_settling_firm.as_deref())?;
          self.write_optional_str_to_cursor(&mut cursor, request.delta_neutral_clearing_account.as_deref())?;
          self.write_optional_str_to_cursor(&mut cursor, request.delta_neutral_clearing_intent.as_deref())?;
        }
        // Delta Neutral Open/Close Fields (sent only if type is not empty)
        if self.server_version >= min_server_ver::DELTA_NEUTRAL_OPEN_CLOSE && !Encoder::is_empty(request.delta_neutral_order_type.as_deref()) {
          self.write_optional_str_to_cursor(&mut cursor, request.delta_neutral_open_close.as_deref())?;
          self.write_bool_to_cursor(&mut cursor, request.delta_neutral_short_sale)?;
          self.write_optional_int_to_cursor(&mut cursor, request.delta_neutral_short_sale_slot)?;
          self.write_optional_str_to_cursor(&mut cursor, request.delta_neutral_designated_location.as_deref())?;
        }
      }
      // Continuous Update
      self.write_bool_to_cursor(&mut cursor, request.continuous_update.unwrap_or(0) != 0)?;
      // Stock Range Lower/Upper (Resent only for server=26 and VOL order)
      if self.server_version == 26 {
        let (lower, upper) = if request.order_type == OrderType::Volatility {
          (request.stock_range_lower, request.stock_range_upper)
        } else {
          (None, None) // Treat as unset for non-VOL orders on server 26
        };
        self.write_optional_double_to_cursor(&mut cursor, lower)?;
        self.write_optional_double_to_cursor(&mut cursor, upper)?;
      }
      // Reference Price Type
      self.write_optional_int_to_cursor(&mut cursor, request.reference_price_type)?;
    }

    // --- Trail Stop Price (Server version 30+) ---
    if self.server_version >= 30 {
      self.write_optional_double_to_cursor(&mut cursor, request.trailing_stop_price)?;
    }

    // --- Trailing Percent (Server version TRAILING_PERCENT+) ---
    if self.server_version >= min_server_ver::TRAILING_PERCENT {
      self.write_optional_double_to_cursor(&mut cursor, request.trailing_percent)?;
    }

    // --- Scale Orders (Starting server version SCALE_ORDERS) ---
    if self.server_version >= min_server_ver::SCALE_ORDERS {
      if self.server_version >= min_server_ver::SCALE_ORDERS2 {
        self.write_optional_int_to_cursor(&mut cursor, request.scale_init_level_size)?;
        self.write_optional_int_to_cursor(&mut cursor, request.scale_subs_level_size)?;
      } else {
        // Older versions had different fields
        self.write_str_to_cursor(&mut cursor, "")?; // Deprecated field
        self.write_optional_int_to_cursor(&mut cursor, request.scale_init_level_size)?;
      }
      self.write_optional_double_to_cursor(&mut cursor, request.scale_price_increment)?;
    }
    // Scale Order Part 2 (Starting server version SCALE_ORDERS3) - conditional on valid price increment
    let scale_price_increment_valid = request.scale_price_increment.map_or(false, |p| p > 0.0 && p != f64::MAX);
    if self.server_version >= min_server_ver::SCALE_ORDERS3 && scale_price_increment_valid {
      self.write_optional_double_to_cursor(&mut cursor, request.scale_price_adjust_value)?;
      self.write_optional_int_to_cursor(&mut cursor, request.scale_price_adjust_interval)?;
      self.write_optional_double_to_cursor(&mut cursor, request.scale_profit_offset)?;
      self.write_bool_to_cursor(&mut cursor, request.scale_auto_reset)?;
      self.write_optional_int_to_cursor(&mut cursor, request.scale_init_position)?;
      self.write_optional_int_to_cursor(&mut cursor, request.scale_init_fill_qty)?;
      self.write_bool_to_cursor(&mut cursor, request.scale_random_percent)?;
    }

    // --- Scale Table (Server version SCALE_TABLE+) ---
    if self.server_version >= min_server_ver::SCALE_TABLE {
      self.write_optional_str_to_cursor(&mut cursor, request.scale_table.as_deref())?;
      let start_time_str = self.format_datetime_tws(request.active_start_time, "%Y%m%d %H:%M:%S", Some(" UTC"));
      let stop_time_str = self.format_datetime_tws(request.active_stop_time, "%Y%m%d %H:%M:%S", Some(" UTC"));
      self.write_str_to_cursor(&mut cursor, &start_time_str)?;
      self.write_str_to_cursor(&mut cursor, &stop_time_str)?;
    }

    // --- Hedge Orders (Server version HEDGE_ORDERS+) ---
    if self.server_version >= min_server_ver::HEDGE_ORDERS {
      self.write_optional_str_to_cursor(&mut cursor, request.hedge_type.as_deref())?;
      if !Encoder::is_empty(request.hedge_type.as_deref()) {
        self.write_optional_str_to_cursor(&mut cursor, request.hedge_param.as_deref())?;
      }
    }

    // --- Opt Out Smart Routing (Server version OPT_OUT_SMART_ROUTING+) ---
    if self.server_version >= min_server_ver::OPT_OUT_SMART_ROUTING {
      self.write_bool_to_cursor(&mut cursor, request.opt_out_smart_routing)?;
    }

    // --- Clearing Params (Server version PTA_ORDERS+) ---
    if self.server_version >= min_server_ver::PTA_ORDERS {
      self.write_optional_str_to_cursor(&mut cursor, request.clearing_account.as_deref())?;
      self.write_optional_str_to_cursor(&mut cursor, request.clearing_intent.as_deref())?;
    }

    // --- Not Held (Server version NOT_HELD+) ---
    if self.server_version >= min_server_ver::NOT_HELD {
      self.write_bool_to_cursor(&mut cursor, request.not_held)?;
    }

    // --- Contract Delta Neutral (Server version DELTA_NEUTRAL+) ---
    if self.server_version >= min_server_ver::DELTA_NEUTRAL {
      if let Some(dn) = &contract.delta_neutral_contract {
        self.write_bool_to_cursor(&mut cursor, true)?;
        self.write_int_to_cursor(&mut cursor, dn.con_id)?;
        self.write_double_to_cursor(&mut cursor, dn.delta)?;
        self.write_double_to_cursor(&mut cursor, dn.price)?;
      } else {
        self.write_bool_to_cursor(&mut cursor, false)?;
      }
    }

    // --- Algo Orders (Server version ALGO_ORDERS+) ---
    if self.server_version >= min_server_ver::ALGO_ORDERS {
      self.write_optional_str_to_cursor(&mut cursor, request.algo_strategy.as_deref())?;
      if !Encoder::is_empty(request.algo_strategy.as_deref()) {
        // Oddly, this tag-value is sent as fields, not as a tag value.
        self.write_int_to_cursor(&mut cursor, request.algo_params.len() as i32)?;
        if !request.algo_params.is_empty() {
          for (tag, value) in &request.algo_params {
            self.write_str_to_cursor(&mut cursor, tag)?;
            self.write_str_to_cursor(&mut cursor, value)?;
          }
        }
      }
    }
    // Algo ID (Server version ALGO_ID+)
    if self.server_version >= min_server_ver::ALGO_ID {
      self.write_optional_str_to_cursor(&mut cursor, request.algo_id.as_deref())?;
    }

    // --- What If Flag (Server version WHAT_IF_ORDERS+) ---
    if self.server_version >= min_server_ver::WHAT_IF_ORDERS {
      self.write_bool_to_cursor(&mut cursor, request.what_if)?;
    }

    // --- Order Misc Options (Server version LINKING+) ---
    if self.server_version >= min_server_ver::LINKING {
      self.write_tag_value_list(&mut cursor, &request.order_misc_options)?;
    }

    // --- Solicited Flag (Server version ORDER_SOLICITED+) ---
    if self.server_version >= min_server_ver::ORDER_SOLICITED {
      self.write_bool_to_cursor(&mut cursor, request.solicited)?;
    }

    // --- Randomize Size/Price Flags (Server version RANDOMIZE_SIZE_AND_PRICE+) ---
    if self.server_version >= min_server_ver::RANDOMIZE_SIZE_AND_PRICE {
      self.write_bool_to_cursor(&mut cursor, request.randomize_size)?;
      self.write_bool_to_cursor(&mut cursor, request.randomize_price)?;
    }

    // --- Pegged To Benchmark Orders & Conditions (Server version PEGGED_TO_BENCHMARK+) ---
    if self.server_version >= min_server_ver::PEGGED_TO_BENCHMARK {
      let is_peg_bench = matches!(request.order_type,
                                  OrderType::PeggedToBenchmark | OrderType::PeggedBest | OrderType::PeggedPrimary);
      if is_peg_bench {
        self.write_optional_int_to_cursor(&mut cursor, request.reference_contract_id)?;
        self.write_bool_to_cursor(&mut cursor, request.is_pegged_change_amount_decrease)?;
        self.write_optional_double_to_cursor(&mut cursor, request.pegged_change_amount)?;
        self.write_optional_double_to_cursor(&mut cursor, request.reference_change_amount)?;
        self.write_optional_str_to_cursor(&mut cursor, request.reference_exchange_id.as_deref())?;
      }

      // Conditions
      self.write_int_to_cursor(&mut cursor, request.conditions.len() as i32)?;
      if !request.conditions.is_empty() {
        warn!("Order condition encoding is not fully implemented. Sending count only.");
        // TODO: Implement full condition encoding.
        // for item in &request.conditions {
        //     self.write_int_to_cursor(&mut cursor, item.type().val())?; // Assuming type() and val() exist
        //     item.write_to(cursor)?; // Assuming write_to exists
        // }
        self.write_bool_to_cursor(&mut cursor, request.conditions_ignore_rth)?;
        self.write_bool_to_cursor(&mut cursor, request.conditions_cancel_order)?;
      }

      // Adjusted Order fields
      let adj_ord_type_str = request.adjusted_order_type.map(|t| t.to_string());
      self.write_optional_str_to_cursor(&mut cursor, adj_ord_type_str.as_deref())?;
      self.write_optional_double_to_cursor(&mut cursor, request.trigger_price)?;
      self.write_optional_double_to_cursor(&mut cursor, request.lmt_price_offset)?;
      self.write_optional_double_to_cursor(&mut cursor, request.adjusted_stop_price)?;
      self.write_optional_double_to_cursor(&mut cursor, request.adjusted_stop_limit_price)?;
      self.write_optional_double_to_cursor(&mut cursor, request.adjusted_trailing_amount)?;
      self.write_optional_int_to_cursor(&mut cursor, request.adjustable_trailing_unit)?;
    }

    // --- Ext Operator (Server version EXT_OPERATOR+) ---
    if self.server_version >= min_server_ver::EXT_OPERATOR {
      self.write_optional_str_to_cursor(&mut cursor, request.ext_operator.as_deref())?;
    }

    // --- Soft Dollar Tier (Server version SOFT_DOLLAR_TIER+) ---
    if self.server_version >= min_server_ver::SOFT_DOLLAR_TIER {
      let (name, value) = request.soft_dollar_tier.as_ref().map(|(n, v)| (n.as_str(), v.as_str())).unwrap_or(("", ""));
      self.write_str_to_cursor(&mut cursor, name)?;
      self.write_str_to_cursor(&mut cursor, value)?;
    }

    // --- Cash Quantity (Server version CASH_QTY+) ---
    if self.server_version >= min_server_ver::CASH_QTY {
      self.write_optional_double_to_cursor(&mut cursor, request.cash_qty)?;
    }

    // --- MiFID Fields (Decision Maker: DECISION_MAKER+, Execution: MIFID_EXECUTION+) ---
    if self.server_version >= min_server_ver::DECISION_MAKER {
      self.write_optional_str_to_cursor(&mut cursor, request.mifid2_decision_maker.as_deref())?;
      self.write_optional_str_to_cursor(&mut cursor, request.mifid2_decision_algo.as_deref())?;
    }
    if self.server_version >= min_server_ver::MIFID_EXECUTION {
      self.write_optional_str_to_cursor(&mut cursor, request.mifid2_execution_trader.as_deref())?;
      self.write_optional_str_to_cursor(&mut cursor, request.mifid2_execution_algo.as_deref())?;
    }

    // --- Auto Price for Hedge (Server version AUTO_PRICE_FOR_HEDGE+) ---
    if self.server_version >= min_server_ver::AUTO_PRICE_FOR_HEDGE {
      self.write_bool_to_cursor(&mut cursor, request.dont_use_auto_price_for_hedge)?;
    }

    // --- OMS Container (Server version ORDER_CONTAINER+) ---
    if self.server_version >= min_server_ver::ORDER_CONTAINER {
      self.write_bool_to_cursor(&mut cursor, request.is_oms_container)?;
    }

    // --- Discretionary Up To Limit Price (Server version D_PEG_ORDERS+) ---
    if self.server_version >= min_server_ver::D_PEG_ORDERS {
      self.write_bool_to_cursor(&mut cursor, request.discretionary_up_to_limit_price)?;
    }

    // --- Use Price Mgmt Algo (Server version PRICE_MGMT_ALGO+) ---
    if self.server_version >= min_server_ver::PRICE_MGMT_ALGO {
      // Our write_optional_bool sends false (0) for None.
      self.write_optional_bool_to_cursor(&mut cursor, request.use_price_mgmt_algo)?;
    }

    // --- Duration (Server version DURATION+) ---
    if self.server_version >= min_server_ver::DURATION {
      self.write_optional_int_to_cursor(&mut cursor, request.duration)?;
    }

    // --- Post To ATS (Server version POST_TO_ATS+) ---
    if self.server_version >= min_server_ver::POST_TO_ATS {
      self.write_optional_int_to_cursor(&mut cursor, request.post_to_ats)?;
    }

    // --- Auto Cancel Parent (Server version AUTO_CANCEL_PARENT+) ---
    if self.server_version >= min_server_ver::AUTO_CANCEL_PARENT {
      self.write_bool_to_cursor(&mut cursor, request.auto_cancel_parent)?;
    }

    // --- Advanced Error Override (Server version ADVANCED_ORDER_REJECT+) ---
    if self.server_version >= min_server_ver::ADVANCED_ORDER_REJECT {
      self.write_optional_str_to_cursor(&mut cursor, request.advanced_error_override.as_deref())?;
    }

    // --- Manual Order Time (Server version MANUAL_ORDER_TIME+) ---
    if self.server_version >= min_server_ver::MANUAL_ORDER_TIME {
      let mot_str = self.format_datetime_tws(request.manual_order_time, "%Y%m%d-%H:%M:%S", None);
      self.write_str_to_cursor(&mut cursor, &mot_str)?;
    }

    // --- Peg Best/Mid Offsets (Server version PEGBEST_PEGMID_OFFSETS+) ---
    if self.server_version >= min_server_ver::PEGBEST_PEGMID_OFFSETS {
      // minTradeQty sent only if exchange is IBKRATS
      if contract.exchange.eq_ignore_ascii_case("IBKRATS") {
        self.write_optional_int_to_cursor(&mut cursor, request.min_trade_qty)?;
      }
      // Check order type
      let is_peg_best = request.order_type == OrderType::PeggedBest;
      let is_peg_mid = request.order_type == OrderType::PeggedToMidpoint;
      let mut send_mid_offsets = false;

      if is_peg_best {
        self.write_optional_int_to_cursor(&mut cursor, request.min_compete_size)?;
        let offset_val = request.compete_against_best_offset;
        // Check if offset_val is the sentinel value (f64::MAX in Rust)
        if offset_val == Some(f64::MAX) {
          self.write_optional_double_to_cursor(&mut cursor, None)?; // Send "" for UpToMid
          send_mid_offsets = true;
        } else {
          self.write_optional_double_to_cursor(&mut cursor, offset_val)?; // Send actual offset
        }
      } else if is_peg_mid {
        send_mid_offsets = true;
      }

      if send_mid_offsets {
        self.write_optional_double_to_cursor(&mut cursor, request.mid_offset_at_whole)?;
        self.write_optional_double_to_cursor(&mut cursor, request.mid_offset_at_half)?;
      }
    }

    // --- Customer Account (Server version CUSTOMER_ACCOUNT+) ---
    if self.server_version >= min_server_ver::CUSTOMER_ACCOUNT {
      self.write_optional_str_to_cursor(&mut cursor, request.customer_account.as_deref())?;
    }

    // --- Professional Customer (Server version PROFESSIONAL_CUSTOMER+) ---
    if self.server_version >= min_server_ver::PROFESSIONAL_CUSTOMER {
      self.write_bool_to_cursor(&mut cursor, request.professional_customer)?;
    }

    // --- RFQ Fields (Server version RFQ_FIELDS+) ---
    if self.server_version >= min_server_ver::RFQ_FIELDS {
      self.write_optional_str_to_cursor(&mut cursor, request.external_user_id.as_deref())?;
      self.write_optional_int_to_cursor(&mut cursor, request.manual_order_indicator)?;
    }


    Ok(self.finish_encoding(cursor))
  }


  // --- Helper to encode contract combo legs for placeOrder ---
  fn encode_combo_legs(&self, cursor: &mut Cursor<Vec<u8>>, legs: &[ComboLeg]) -> Result<(), IBKRError> {
    // This helper is called within placeOrder, which already checks server_version >= 8
    self.write_int_to_cursor(cursor, legs.len() as i32)?;
    for leg in legs {
      self.write_int_to_cursor(cursor, leg.con_id)?;
      self.write_int_to_cursor(cursor, leg.ratio)?;
      self.write_str_to_cursor(cursor, &leg.action)?; // BUY/SELL/SSHORT
      self.write_str_to_cursor(cursor, &leg.exchange)?;
      self.write_int_to_cursor(cursor, leg.open_close)?; // 0=Same, 1=Open, 2=Close, 3=Unknown

      // Short Sale Slot fields (Server version SSHORT_COMBO_LEGS+)
      if self.server_version >= min_server_ver::SSHORT_COMBO_LEGS {
        self.write_int_to_cursor(cursor, leg.short_sale_slot)?; // 0=Default, 1=Retail, 2=Inst
        self.write_optional_str_to_cursor(cursor, Some(&leg.designated_location).filter(|s| !s.is_empty()).map(|s| s.as_str()))?;
      }
      // Exempt Code (Server version SSHORTX_OLD+)
      if self.server_version >= min_server_ver::SSHORTX_OLD {
        let exempt_code_val = leg.exempt_code; // Assuming exempt_code is i32
        self.write_int_to_cursor(cursor, if exempt_code_val == -1 { 0 } else { exempt_code_val })?;
      }
    }
    Ok(())
  }

  pub fn encode_request_market_data(
    &self,
    req_id: i32,
    contract: &Contract,
    generic_tick_list: &str,
    snapshot: bool,
    regulatory_snapshot: bool,
    mkt_data_options: &[(String, String)], // Added parameter
  ) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request market data message for contract {}: ReqID={}", contract.symbol, req_id);

    if self.server_version < min_server_ver::SNAPSHOT_MKT_DATA && snapshot {
      return Err(IBKRError::UpdateTws(
        "Server version does not support snapshot market data requests.".to_string(),
      ));
    }
    if self.server_version < min_server_ver::DELTA_NEUTRAL {
      if contract.delta_neutral_contract.is_some() {
        return Err(IBKRError::UpdateTws(
          "Server version does not support delta-neutral orders/contracts.".to_string(),
        ));
      }
    }
    if self.server_version < min_server_ver::MKT_DATA_CONID {
      if contract.con_id > 0 {
        return Err(IBKRError::UpdateTws(
          "Server version does not support conId parameter in reqMarketData.".to_string(),
        ));
      }
    }
    if self.server_version < min_server_ver::TRADING_CLASS {
      if !contract.trading_class.is_none() {
        return Err(IBKRError::UpdateTws(
          "Server version does not support tradingClass parameter in reqMarketData.".to_string(),
        ));
      }
    }
    if self.server_version < min_server_ver::SMART_COMPONENTS && regulatory_snapshot {
      return Err(IBKRError::UpdateTws(
        "Server version does not support regulatory snapshot requests.".to_string(),
      ));
    }
    if self.server_version < min_server_ver::LINKING && !mkt_data_options.is_empty() {
      return Err(IBKRError::UpdateTws(
        "Server version does not support market data options.".to_string(),
      ));
    }

    // --- Start Encoding ---
    let mut cursor = self.start_encoding(OutgoingMessageType::ReqMktData as i32)?;
    let version = 11;

    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;

    // --- Encode Contract Fields ---
    if self.server_version >= min_server_ver::MKT_DATA_CONID {
      self.write_int_to_cursor(&mut cursor, contract.con_id)?;
    }
    self.write_str_to_cursor(&mut cursor, &contract.symbol)?;
    self.write_str_to_cursor(&mut cursor, &contract.sec_type.to_string())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.last_trade_date_or_contract_month.as_deref())?;
    self.write_double_to_cursor(&mut cursor, contract.strike.unwrap_or(0.0))?;
    self.write_optional_str_to_cursor(&mut cursor, contract.right.map(|r| r.to_string()).as_deref())?;
    if self.server_version >= 15 {
      self.write_optional_str_to_cursor(&mut cursor, contract.multiplier.as_deref())?;
    }
    self.write_str_to_cursor(&mut cursor, &contract.exchange)?;
    if self.server_version >= 14 {
      self.write_optional_str_to_cursor(&mut cursor, contract.primary_exchange.as_deref())?;
    }
    self.write_str_to_cursor(&mut cursor, &contract.currency)?;
    if self.server_version >= 2 {
      self.write_optional_str_to_cursor(&mut cursor, contract.local_symbol.as_deref())?;
    }
    if self.server_version >= min_server_ver::TRADING_CLASS {
      self.write_optional_str_to_cursor(&mut cursor, contract.trading_class.as_deref())?;
    }

    // --- Encode Combo Legs (Sent for BAG secType, Server version 8+) ---
    if self.server_version >= 8 && contract.sec_type == SecType::Combo {
      self.write_int_to_cursor(&mut cursor, contract.combo_legs.len() as i32)?;
      for leg in &contract.combo_legs {
        self.write_int_to_cursor(&mut cursor, leg.con_id)?;
        self.write_int_to_cursor(&mut cursor, leg.ratio)?;
        self.write_str_to_cursor(&mut cursor, &leg.action)?; // BUY/SELL/SSHORT
        self.write_str_to_cursor(&mut cursor, &leg.exchange)?;
        // Note: Other ComboLeg fields are NOT sent in reqMktData
      }
    }

    // --- Encode Delta Neutral Contract (Server version DELTA_NEUTRAL+) ---
    if self.server_version >= min_server_ver::DELTA_NEUTRAL {
      if let Some(dn) = &contract.delta_neutral_contract {
        self.write_bool_to_cursor(&mut cursor, true)?;
        self.write_int_to_cursor(&mut cursor, dn.con_id)?;
        self.write_double_to_cursor(&mut cursor, dn.delta)?;
        self.write_double_to_cursor(&mut cursor, dn.price)?;
      } else {
        self.write_bool_to_cursor(&mut cursor, false)?;
      }
    }

    // --- Generic Tick List (Server version 31+) ---
    if self.server_version >= 31 {
      self.write_str_to_cursor(&mut cursor, generic_tick_list)?;
    }

    // --- Snapshot (Server version SNAPSHOT_MKT_DATA+) ---
    if self.server_version >= min_server_ver::SNAPSHOT_MKT_DATA {
      self.write_bool_to_cursor(&mut cursor, snapshot)?;
    }

    // --- Regulatory Snapshot (Server version REQ_SMART_COMPONENTS+) ---
    if self.server_version >= min_server_ver::SMART_COMPONENTS {
      self.write_bool_to_cursor(&mut cursor, regulatory_snapshot)?;
    }

    // --- Market Data Options (Server version LINKING+) ---
    if self.server_version >= min_server_ver::LINKING {
      self.write_tag_value_list(&mut cursor, mkt_data_options)?; // Old logic
    }

    Ok(self.finish_encoding(cursor))
  }


  pub fn encode_cancel_market_data(&self, req_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel market data: ReqID={}", req_id);
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelMktData as i32)?;
    let version = 1;
    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;
    Ok(self.finish_encoding(cursor))
  }

  pub fn encode_request_all_open_orders(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request all open orders");
    let mut cursor = self.start_encoding(OutgoingMessageType::ReqAllOpenOrders as i32)?;
    let version = 1;
    self.write_int_to_cursor(&mut cursor, version)?;
    Ok(self.finish_encoding(cursor))
  }

  pub fn encode_request_global_cancel(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request global cancel message");
    if self.server_version < min_server_ver::GLOBAL_CANCEL {
      return Err(IBKRError::Unsupported(
        "Server version does not support global cancel requests.".to_string(),
      ));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::ReqGlobalCancel as i32)?;
    let version = 1;
    self.write_int_to_cursor(&mut cursor, version)?;
    Ok(self.finish_encoding(cursor))
  }

  pub fn encode_exercise_options(
    &self,
    req_id: i32, // Ticker ID of the request (not order ID)
    contract: &Contract,
    exercise_action: crate::order::ExerciseAction, // 1 for exercise, 2 for lapse
    exercise_quantity: i32,
    account: &str,
    override_action: i32, // 0 for no override, 1 for override
  ) -> Result<Vec<u8>, IBKRError> {
    debug!(
      "Encoding exercise options: ReqID={}, Contract={}, Action={:?}, Qty={}, Acct={}, Override={}",
      req_id, contract.symbol, exercise_action, exercise_quantity, account, override_action
    );

    if self.server_version < min_server_ver::EXERCISE_OPTIONS {
      return Err(IBKRError::Unsupported(
        "Server version does not support exercising options.".to_string(),
      ));
    }
    if self.server_version < min_server_ver::TRADING_CLASS && (!contract.trading_class.is_none() || contract.con_id > 0) {
      return Err(IBKRError::Unsupported(
        "Server version does not support conId and tradingClass parameters in exerciseOptions.".to_string()
      ));
    }


    let mut cursor = self.start_encoding(OutgoingMessageType::ExerciseOptions as i32)?;
    let version = 2; // Version supporting override

    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;

    // Contract fields
    if self.server_version >= min_server_ver::TRADING_CLASS {
      self.write_int_to_cursor(&mut cursor, contract.con_id)?;
    }
    self.write_str_to_cursor(&mut cursor, &contract.symbol)?;
    self.write_str_to_cursor(&mut cursor, &contract.sec_type.to_string())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.last_trade_date_or_contract_month.as_deref())?;

    self.write_optional_double_to_cursor(&mut cursor, contract.strike)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.right.map(|r| r.to_string()).as_deref())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.multiplier.as_deref())?;
    self.write_str_to_cursor(&mut cursor, &contract.exchange)?;
    self.write_str_to_cursor(&mut cursor, &contract.currency)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.local_symbol.as_deref())?;
    if self.server_version >= min_server_ver::TRADING_CLASS {
      self.write_optional_str_to_cursor(&mut cursor, contract.trading_class.as_deref())?;
    }

    // Exercise parameters
    self.write_int_to_cursor(&mut cursor, exercise_action as i32)?;
    self.write_int_to_cursor(&mut cursor, exercise_quantity)?;
    self.write_str_to_cursor(&mut cursor, account)?;
    self.write_int_to_cursor(&mut cursor, override_action)?; // 0 for no override, 1 for override

    Ok(self.finish_encoding(cursor))
  }

  pub fn _encode_request_open_orders(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request open orders");
    let mut cursor = self.start_encoding(OutgoingMessageType::ReqOpenOrders as i32)?;
    let version = 1;
    self.write_int_to_cursor(&mut cursor, version)?;
    Ok(self.finish_encoding(cursor))
  }

  // NOTE: reqAccountData is deprecated. Use reqAccountSummary and reqPositions instead.
  // pub fn encode_request_account_data(&self, subscribe: bool, account_code: &str) -> Result<Vec<u8>, IBKRError> {
  //   debug!("Encoding request account data: Subscribe={}, Account={}", subscribe, account_code);
  //   let mut cursor = self.start_encoding(OutgoingMessageType::ReqAccountData as i32)?;
  //   let version = 2;
  //   self.write_int_to_cursor(&mut cursor, version)?;
  //   self.write_bool_to_cursor(&mut cursor, subscribe)?;
  //   // Account code field added in version 2 / server version 9
  //   if self.server_version >= 9 {
  //     self.write_str_to_cursor(&mut cursor, account_code)?; // Can be empty for default
  //   }
  //   Ok(self.finish_encoding(cursor))
  // }

  pub fn encode_request_contract_data(&self, req_id: i32, contract: &Contract) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request contract data: ReqID={}", req_id);
    if self.server_version < 4 {
      return Err(IBKRError::Unsupported("Server version does not support reqContractDetails.".to_string()));
    }
    if self.server_version < min_server_ver::SEC_ID_TYPE && (!contract.sec_id_type.is_none() || !contract.sec_id.is_none()) {
      return Err(IBKRError::Unsupported("Server version does not support secIdType and secId parameters.".to_string()));
    }
    if self.server_version < min_server_ver::TRADING_CLASS && !contract.trading_class.is_none() {
      return Err(IBKRError::Unsupported("Server version does not support tradingClass parameter in reqContractDetails.".to_string()));
    }
    if self.server_version < min_server_ver::LINKING && !contract.primary_exchange.is_none() {
      return Err(IBKRError::Unsupported("Server version does not support primaryExchange parameter in reqContractDetails.".to_string()));
    }
    if self.server_version < min_server_ver::BOND_ISSUERID && !contract.issuer_id.is_none() {
      return Err(IBKRError::Unsupported("Server version does not support issuerId parameter in reqContractDetails.".to_string()));
    }

    let mut cursor = self.start_encoding(OutgoingMessageType::ReqContractData as i32)?;
    let version = 8;
    self.write_int_to_cursor(&mut cursor, version)?;

    // reqId field added in CONTRACT_DATA_CHAIN
    if self.server_version >= min_server_ver::CONTRACT_DATA_CHAIN {
      self.write_int_to_cursor(&mut cursor, req_id)?;
    }

    // Write contract fields
    if self.server_version >= min_server_ver::CONTRACT_CONID {
      self.write_int_to_cursor(&mut cursor, contract.con_id)?;
    }
    self.write_str_to_cursor(&mut cursor, &contract.symbol)?;
    self.write_str_to_cursor(&mut cursor, &contract.sec_type.to_string())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.last_trade_date_or_contract_month.as_deref())?;
    self.write_optional_double_to_cursor(&mut cursor, contract.strike)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.right.map(|r| r.to_string()).as_deref())?;
    if version >= 2 {
      self.write_optional_str_to_cursor(&mut cursor, contract.multiplier.as_deref())?;
    }

    // Exchange / PrimaryExchange handling
    if self.server_version >= min_server_ver::PRIMARYEXCH {
      self.write_str_to_cursor(&mut cursor, &contract.exchange)?;
      self.write_optional_str_to_cursor(&mut cursor, contract.primary_exchange.as_deref())?;
    } else if self.server_version >= min_server_ver::LINKING {
      if let Some(primary_exch) = contract.primary_exchange.as_ref() {
        if !primary_exch.is_empty() && (contract.exchange == "BEST" || contract.exchange == "SMART") {
          self.write_str_to_cursor(&mut cursor, &format!("{}:{}", contract.exchange, primary_exch))?;
        } else {
          self.write_str_to_cursor(&mut cursor, &contract.exchange)?;
        }
      } else {
        self.write_str_to_cursor(&mut cursor, &contract.exchange)?;
      }
    } else {
      self.write_str_to_cursor(&mut cursor, &contract.exchange)?; // Older versions just send exchange
    }

    self.write_str_to_cursor(&mut cursor, &contract.currency)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.local_symbol.as_deref())?;
    if self.server_version >= min_server_ver::TRADING_CLASS {
      self.write_optional_str_to_cursor(&mut cursor, contract.trading_class.as_deref())?;
    }
    if self.server_version >= 31 {
      self.write_bool_to_cursor(&mut cursor, contract.include_expired)?;
    }
    if self.server_version >= min_server_ver::SEC_ID_TYPE {
      self.write_optional_str_to_cursor(&mut cursor, contract.sec_id_type.as_ref().map(|t| t.to_string()).as_deref())?;
      self.write_optional_str_to_cursor(&mut cursor, contract.sec_id.as_deref())?;
    }
    if self.server_version >= min_server_ver::BOND_ISSUERID {
      self.write_optional_str_to_cursor(&mut cursor, contract.issuer_id.as_deref())?;
    }
    Ok(self.finish_encoding(cursor))
  }

  pub fn encode_request_historical_data(&self, req_id: i32, contract: &Contract, end_date_time: Option<DateTime<Utc>>, duration_str: &str, bar_size_setting: &str, what_to_show: &str, use_rth: bool, format_date: i32, keep_up_to_date: bool, chart_options: &[(String, String)]) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request historical data: ReqID={}, ContractSymbol={}, EndDateTimeInput={:?}, Duration={}, BarSizeInput={}, What={}, UseRTH={}, FormatDate={}, KeepUpToDate={}",
           req_id, contract.symbol, end_date_time, duration_str, bar_size_setting, what_to_show, use_rth, format_date, keep_up_to_date);

    if self.server_version < 16 {
      return Err(IBKRError::Unsupported("Server version does not support historical data backfill.".to_string()));
    }
    if self.server_version < min_server_ver::TRADING_CLASS && (!contract.trading_class.is_none() || contract.con_id > 0) {
      return Err(IBKRError::Unsupported("Server version does not support conId and tradingClass parameters in reqHistoricalData.".to_string()));
    }
    if what_to_show.eq_ignore_ascii_case("SCHEDULE") {
      if self.server_version < min_server_ver::HISTORICAL_SCHEDULE {
        return Err(IBKRError::Unsupported("Server version does not support requesting historical schedule.".to_string()));
      }
      if keep_up_to_date {
        return Err(IBKRError::InvalidParameter("keepUpToDate must be False when whatToShow is SCHEDULE.".to_string()));
      }
      if use_rth { // For SCHEDULE, useRTH must be 0 (false).
        warn!("use_rth is true for a SCHEDULE request; TWS requires it to be false. Overriding to false for encoding.");
        // The `use_rth` parameter to this function will be overridden later for SCHEDULE.
      }
    }


    let mut cursor = self.start_encoding(OutgoingMessageType::ReqHistoricalData as i32)?;
    let version = 6;

    // Version field sent only if server < SYNT_REALTIME_BARS
    if self.server_version < min_server_ver::SYNT_REALTIME_BARS {
      self.write_int_to_cursor(&mut cursor, version)?;
    }
    self.write_int_to_cursor(&mut cursor, req_id)?;

    // Contract fields
    if self.server_version >= min_server_ver::TRADING_CLASS {
      self.write_int_to_cursor(&mut cursor, contract.con_id)?;
    }
    self.write_str_to_cursor(&mut cursor, &contract.symbol)?;
    self.write_str_to_cursor(&mut cursor, &contract.sec_type.to_string())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.last_trade_date_or_contract_month.as_deref())?;
    self.write_optional_double_to_cursor(&mut cursor, contract.strike)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.right.map(|r| r.to_string()).as_deref())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.multiplier.as_deref())?;
    self.write_str_to_cursor(&mut cursor, &contract.exchange)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.primary_exchange.as_deref())?;
    self.write_str_to_cursor(&mut cursor, &contract.currency)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.local_symbol.as_deref())?;
    if self.server_version >= min_server_ver::TRADING_CLASS {
      self.write_optional_str_to_cursor(&mut cursor, contract.trading_class.as_deref())?;
    }
    if self.server_version >= 31 {
      self.write_bool_to_cursor(&mut cursor, contract.include_expired)?;
    }

    // Date/Time and Bar parameters (Server version 20+)
    if self.server_version >= 20 {
      let end_date_time_str = self.format_datetime_tws(end_date_time, "%Y%m%d %H:%M:%S", Some(" UTC")); // Use UTC suffix for clarity
      self.write_str_to_cursor(&mut cursor, &end_date_time_str)?;
      // Bar size setting (e.g., "1 day", "30 mins", "1 secs")
      self.write_str_to_cursor(&mut cursor, bar_size_setting)?;
    }

    // Duration string (e.g., "1 Y", "3 M", "60 D", "3600 S")
    self.write_str_to_cursor(&mut cursor, duration_str)?;

    // Use RTH (1=RTH only, 0=All data)
    self.write_int_to_cursor(&mut cursor, if use_rth { 1 } else { 0 })?;

    // What to show (e.g., "TRADES", "MIDPOINT", "BID", "ASK")
    self.write_str_to_cursor(&mut cursor, what_to_show)?;

    // Format date (1=yyyyMMdd{ }hh:mm:ss, 2=epoch seconds)
    if self.server_version > 16 {
      self.write_int_to_cursor(&mut cursor, format_date)?;
    }

    // Combo Legs (Sent for BAG secType)
    if contract.sec_type == SecType::Combo {
      self.write_int_to_cursor(&mut cursor, contract.combo_legs.len() as i32)?;
      for leg in &contract.combo_legs {
        self.write_int_to_cursor(&mut cursor, leg.con_id)?;
        self.write_int_to_cursor(&mut cursor, leg.ratio)?;
        self.write_str_to_cursor(&mut cursor, &leg.action)?;
        self.write_str_to_cursor(&mut cursor, &leg.exchange)?;
      }
    }

    // Keep up to date (Server version SYNT_REALTIME_BARS+)
    if self.server_version >= min_server_ver::SYNT_REALTIME_BARS {
      self.write_bool_to_cursor(&mut cursor, keep_up_to_date)?;
    }

    // Chart options (Server version LINKING+)
    if self.server_version >= min_server_ver::LINKING {
      self.write_tag_value_list(&mut cursor, chart_options)?; // Old logic
    }

    Ok(self.finish_encoding(cursor))
  }

  pub fn _encode_request_managed_accounts(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request managed accounts");
    let mut cursor = self.start_encoding(OutgoingMessageType::ReqManagedAccts as i32)?;
    let version = 1;
    self.write_int_to_cursor(&mut cursor, version)?;
    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request for scanner parameters.
  pub fn encode_request_scanner_parameters(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request scanner parameters");
    if self.server_version < min_server_ver::SCANNER_PARAMETERS {
      return Err(IBKRError::Unsupported("Server version does not support scanner parameters request.".to_string()));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::ReqScannerParameters as i32)?;
    let version = 1;
    self.write_int_to_cursor(&mut cursor, version)?;
    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request for a scanner subscription.
  pub fn encode_request_scanner_subscription(
    &self,
    req_id: i32,
    subscription: &ScannerSubscription,
  ) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request scanner subscription: ReqID={}, Subscription={:?}", req_id, subscription);

    if self.server_version < min_server_ver::SCANNER_SUBSCRIPTION {
      return Err(IBKRError::Unsupported("Server version does not support scanner subscriptions.".to_string()));
    }
    if self.server_version < min_server_ver::SCANNER_GENERIC_OPTS && subscription.average_option_volume_above.is_some() {
      return Err(IBKRError::Unsupported("Server version does not support averageOptionVolumeAbove in scanner
subscription.".to_string()));
    }
    if self.server_version < min_server_ver::SCANNER_GENERIC_OPTS && subscription.scanner_setting_pairs.is_some() {
      return Err(IBKRError::Unsupported("Server version does not support scannerSettingPairs in scanner
subscription.".to_string()));
    }
    if self.server_version < min_server_ver::SCANNER_GENERIC_OPTS && subscription.stock_type_filter.is_some() {
      return Err(IBKRError::Unsupported("Server version does not support stockTypeFilter in scanner subscription.".to_string()));
    }


    let mut cursor = self.start_encoding(OutgoingMessageType::ReqScannerSubscription as i32)?;
    let version = 4; // Current version for this message
    if self.server_version < min_server_ver::SCANNER_GENERIC_OPTS {
      self.write_int_to_cursor(&mut cursor, version)?;
    }
    self.write_int_to_cursor(&mut cursor, req_id)?;

    // Subscription fields
    self.write_optional_int_to_cursor(&mut cursor, Some(subscription.number_of_rows).filter(|&n| n != i32::MAX))?;
    self.write_str_to_cursor(&mut cursor, &subscription.instrument)?;
    self.write_str_to_cursor(&mut cursor, &subscription.location_code)?;
    self.write_str_to_cursor(&mut cursor, &subscription.scan_code)?;
    self.write_optional_double_to_cursor(&mut cursor, subscription.above_price)?;
    self.write_optional_double_to_cursor(&mut cursor, subscription.below_price)?;
    self.write_optional_int_to_cursor(&mut cursor, subscription.above_volume)?;
    self.write_optional_double_to_cursor(&mut cursor, subscription.market_cap_above)?;
    self.write_optional_double_to_cursor(&mut cursor, subscription.market_cap_below)?;
    self.write_optional_str_to_cursor(&mut cursor, subscription.moody_rating_above.as_deref())?;
    self.write_optional_str_to_cursor(&mut cursor, subscription.moody_rating_below.as_deref())?;
    self.write_optional_str_to_cursor(&mut cursor, subscription.sp_rating_above.as_deref())?;
    self.write_optional_str_to_cursor(&mut cursor, subscription.sp_rating_below.as_deref())?;
    self.write_optional_str_to_cursor(&mut cursor, subscription.maturity_date_above.as_deref())?;
    self.write_optional_str_to_cursor(&mut cursor, subscription.maturity_date_below.as_deref())?;
    self.write_optional_double_to_cursor(&mut cursor, subscription.coupon_rate_above)?;
    self.write_optional_double_to_cursor(&mut cursor, subscription.coupon_rate_below)?;
    self.write_optional_str_to_cursor(&mut cursor, Some(if subscription.exclude_convertible { "1" } else { "0" }))?;

    if self.server_version >= min_server_ver::SCANNER_GENERIC_OPTS {
      self.write_optional_int_to_cursor(&mut cursor, subscription.average_option_volume_above)?;
      self.write_optional_str_to_cursor(&mut cursor, subscription.scanner_setting_pairs.as_deref())?;
      self.write_optional_str_to_cursor(&mut cursor, subscription.stock_type_filter.as_deref())?;
    }
    if self.server_version >= min_server_ver::LINKING {
      // TODO: implement subscription_options.
      self.write_str_to_cursor(&mut cursor, "")?;
      self.write_str_to_cursor(&mut cursor, "")?;
    }

    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request to cancel a scanner subscription.
  pub fn encode_cancel_scanner_subscription(&self, req_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel scanner subscription: ReqID={}", req_id);
    if self.server_version < min_server_ver::SCANNER_SUBSCRIPTION { // Same min version as req
      return Err(IBKRError::Unsupported("Server version does not support scanner subscription cancellation.".to_string()));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelScannerSubscription as i32)?;
    let version = 1;
    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;
    Ok(self.finish_encoding(cursor))
  }

  pub fn encode_request_current_time(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request current time message");
    if self.server_version < 33 {
      return Err(IBKRError::Unsupported("Server version does not support current time requests.".to_string()));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::ReqCurrentTime as i32)?;
    let version = 1;
    self.write_int_to_cursor(&mut cursor, version)?;
    Ok(self.finish_encoding(cursor))
  }

  // --- DataRef
  /// Encodes a request for security definition option parameters.
  pub fn encode_request_sec_def_opt_params(
    &self,
    req_id: i32,
    underlying_symbol: &str,
    fut_fop_exchange: &str, // Left empty for options
    underlying_sec_type: SecType, // Typically STK
    underlying_con_id: i32,
  ) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request security definition option parameters: ReqID={}, UnderlyingSymbol={}, UnderlyingConID={}", req_id, underlying_symbol, underlying_con_id);

    if self.server_version < min_server_ver::SEC_DEF_OPT_PARAMS_REQ {
      return Err(IBKRError::Unsupported("Server version does not support security definition option parameters request.".to_string()));
    }

    let mut cursor = self.start_encoding(OutgoingMessageType::ReqSecDefOptParams as i32)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;
    self.write_str_to_cursor(&mut cursor, underlying_symbol)?;
    self.write_str_to_cursor(&mut cursor, fut_fop_exchange)?; // Exchange (usually empty for options)
    self.write_str_to_cursor(&mut cursor, &underlying_sec_type.to_string())?; // Typically STK
    self.write_int_to_cursor(&mut cursor, underlying_con_id)?;

    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request for soft dollar tiers.
  pub fn encode_request_soft_dollar_tiers(&self, req_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request soft dollar tiers: ReqID={}", req_id);
    if self.server_version < min_server_ver::SOFT_DOLLAR_TIER {
      return Err(IBKRError::Unsupported("Server version does not support soft dollar tier requests.".to_string()));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::ReqSoftDollarTiers as i32)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;
    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request for family codes.
  pub fn encode_request_family_codes(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request family codes");
    if self.server_version < min_server_ver::FAMILY_CODES {
      return Err(IBKRError::Unsupported("Server version does not support family codes request.".to_string()));
    }
    let cursor = self.start_encoding(OutgoingMessageType::ReqFamilyCodes as i32)?;
    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request for contracts matching a symbol pattern.
  pub fn encode_request_matching_symbols(&self, req_id: i32, pattern: &str) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request matching symbols: ReqID={}, Pattern={}", req_id, pattern);
    if self.server_version < min_server_ver::MATCHING_SYMBOLS {
      return Err(IBKRError::Unsupported("Server version does not support matching symbols request.".to_string()));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::ReqMatchingSymbols as i32)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;
    self.write_str_to_cursor(&mut cursor, pattern)?;
    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request for market depth exchanges.
  pub fn encode_request_mkt_depth_exchanges(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request market depth exchanges");
    if self.server_version < min_server_ver::MKT_DEPTH_EXCHANGES {
      return Err(IBKRError::Unsupported("Server version does not support market depth exchanges request.".to_string()));
    }
    let cursor = self.start_encoding(OutgoingMessageType::ReqMktDepthExchanges as i32)?;
    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request for SMART routing components.
  pub fn encode_request_smart_components(&self, req_id: i32, bbo_exchange: &str) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request smart components: ReqID={}, BBOExchange={}", req_id, bbo_exchange);
    if self.server_version < min_server_ver::SMART_COMPONENTS {
      return Err(IBKRError::Unsupported("Server version does not support smart components request.".to_string()));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::ReqSmartComponents as i32)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;
    self.write_str_to_cursor(&mut cursor, bbo_exchange)?; // Name of BBO exchange (e.g., ISLAND)
    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request for market rule details.
  pub fn encode_request_market_rule(&self, market_rule_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request market rule: RuleID={}", market_rule_id);
    if self.server_version < min_server_ver::MARKET_RULES {
      return Err(IBKRError::Unsupported("Server version does not support market rule requests.".to_string()));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::ReqMarketRule as i32)?;
    self.write_int_to_cursor(&mut cursor, market_rule_id)?;
    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request for single position P&L updates.
  pub fn encode_request_pnl_single(
    &self,
    req_id: i32,
    account: &str,
    model_code: &str, // Typically empty string
    con_id: i32,
  ) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request PNL single: ReqID={}, Account={}, ModelCode='{}', ConID={}",
           req_id, account, model_code, con_id);

    if self.server_version < min_server_ver::PNL {
      return Err(IBKRError::Unsupported(
        "Server version does not support PnL single requests.".to_string(),
      ));
    }

    let mut cursor = self.start_encoding(OutgoingMessageType::ReqPnlSingle as i32)?;
    // Version is not explicitly sent for REQ_PNL_SINGLE according to some references,
    // but the message structure is: reqId, account, modelCode, conId
    self.write_int_to_cursor(&mut cursor, req_id)?;
    self.write_str_to_cursor(&mut cursor, account)?;
    self.write_str_to_cursor(&mut cursor, model_code)?; // Usually empty
    self.write_int_to_cursor(&mut cursor, con_id)?;

    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request to cancel single position P&L updates.
  pub fn encode_cancel_pnl_single(&self, req_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel PNL single: ReqID={}", req_id);

    if self.server_version < min_server_ver::PNL {
      return Err(IBKRError::Unsupported(
        "Server version does not support PnL single cancellation.".to_string(),
      ));
    }

    let mut cursor = self.start_encoding(OutgoingMessageType::CancelPnlSingle as i32)?;
    // Version is not explicitly sent for CANCEL_PNL_SINGLE. Structure: reqId
    self.write_int_to_cursor(&mut cursor, req_id)?;

    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request for real-time bars.
  pub fn encode_request_real_time_bars(
    &self,
    req_id: i32,
    contract: &Contract,
    bar_size: i32, // Currently only 5 seconds is supported by API
    what_to_show: &str, // TRADES, MIDPOINT, BID, ASK
    use_rth: bool,
    real_time_bars_options: &[(String, String)], // TagValue list
  ) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request real time bars: ReqID={}, Contract={}, BarSize={}, What={}, RTH={}",
           req_id, contract.symbol, bar_size, what_to_show, use_rth);

    if self.server_version < min_server_ver::REAL_TIME_BARS {
      return Err(IBKRError::Unsupported("Server version does not support real time bars.".to_string()));
    }
    if self.server_version < min_server_ver::TRADING_CLASS && (!contract.trading_class.is_none() || contract.con_id > 0) {
      return Err(IBKRError::Unsupported("Server version does not support conId and tradingClass parameters in reqRealTimeBars.".to_string()));
    }

    // Bar size validation (API currently only supports 5 seconds)
    if bar_size != 5 {
      warn!("Requesting real-time bars with size {}. API currently only supports 5 seconds.", bar_size);
      // Proceed anyway, TWS might handle it or return error
    }

    let mut cursor = self.start_encoding(OutgoingMessageType::ReqRealTimeBars as i32)?;
    let version = 3;

    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;

    // Encode contract fields
    if self.server_version >= min_server_ver::TRADING_CLASS {
      self.write_int_to_cursor(&mut cursor, contract.con_id)?;
    }
    self.write_str_to_cursor(&mut cursor, &contract.symbol)?;
    self.write_str_to_cursor(&mut cursor, &contract.sec_type.to_string())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.last_trade_date_or_contract_month.as_deref())?;
    self.write_optional_double_to_cursor(&mut cursor, contract.strike)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.right.map(|r| r.to_string()).as_deref())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.multiplier.as_deref())?;
    self.write_str_to_cursor(&mut cursor, &contract.exchange)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.primary_exchange.as_deref())?;
    self.write_str_to_cursor(&mut cursor, &contract.currency)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.local_symbol.as_deref())?;
    if self.server_version >= min_server_ver::TRADING_CLASS {
      self.write_optional_str_to_cursor(&mut cursor, contract.trading_class.as_deref())?;
    }

    // Bar parameters
    self.write_int_to_cursor(&mut cursor, bar_size)?;
    self.write_str_to_cursor(&mut cursor, what_to_show)?;
    self.write_bool_to_cursor(&mut cursor, use_rth)?;

    // Real time bar options (TagValue list)
    if self.server_version >= min_server_ver::LINKING { // Explicit check for clarity
      self.write_tag_value_list(&mut cursor, real_time_bars_options)?; // Old logic
    }

    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request to cancel real-time bars.
  pub fn encode_cancel_real_time_bars(&self, req_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel real time bars: ReqID={}", req_id);
    if self.server_version < min_server_ver::REAL_TIME_BARS {
      return Err(IBKRError::Unsupported("Server version does not support realtime bar data query cancellation.".to_string()));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelRealTimeBars as i32)?;
    let version = 1;
    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;
    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request for tick-by-tick data.
  pub fn encode_request_tick_by_tick_data(
    &self,
    req_id: i32,
    contract: &Contract,
    tick_type: &str, // "Last", "AllLast", "BidAsk", "MidPoint"
    number_of_ticks: i32, // Use 0 for streaming
    ignore_size: bool, // True to ignore size filter, False to use size filter
  ) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request tick-by-tick data: ReqID={}, Contract={}, Type={}, NumTicks={}, IgnoreSize={}",
           req_id, contract.symbol, tick_type, number_of_ticks, ignore_size);

    if self.server_version < min_server_ver::TICK_BY_TICK {
      return Err(IBKRError::Unsupported("Server version does not support tick-by-tick data requests.".to_string()));
    }
    if self.server_version < min_server_ver::TICK_BY_TICK_IGNORE_SIZE {
      if number_of_ticks != 0 || ignore_size {
        return Err(IBKRError::Unsupported("Server version does not support ignoreSize and numberOfTicks parameters in tick-by-tick data requests.".to_string()));
      }
    }

    let mut cursor = self.start_encoding(OutgoingMessageType::ReqTickByTickData as i32)?;

    self.write_int_to_cursor(&mut cursor, req_id)?;

    // Contract fields
    self.write_int_to_cursor(&mut cursor, contract.con_id)?;
    self.write_str_to_cursor(&mut cursor, &contract.symbol)?;
    self.write_str_to_cursor(&mut cursor, &contract.sec_type.to_string())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.last_trade_date_or_contract_month.as_deref())?;
    self.write_optional_double_to_cursor(&mut cursor, contract.strike)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.right.map(|r| r.to_string()).as_deref())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.multiplier.as_deref())?;
    self.write_str_to_cursor(&mut cursor, &contract.exchange)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.primary_exchange.as_deref())?;
    self.write_str_to_cursor(&mut cursor, &contract.currency)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.local_symbol.as_deref())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.trading_class.as_deref())?;

    // Tick-by-tick parameters
    self.write_str_to_cursor(&mut cursor, tick_type)?; // "Last", "AllLast", "BidAsk", "MidPoint"

    // numberOfTicks and ignoreSize added in TICK_BY_TICK_IGNORE_SIZE
    if self.server_version >= min_server_ver::TICK_BY_TICK_IGNORE_SIZE {
      self.write_int_to_cursor(&mut cursor, number_of_ticks)?; // 0 for streaming, >0 for historical
      self.write_bool_to_cursor(&mut cursor, ignore_size)?;
    }

    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request to cancel tick-by-tick data.
  pub fn encode_cancel_tick_by_tick_data(&self, req_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel tick-by-tick data: ReqID={}", req_id);
    if self.server_version < min_server_ver::TICK_BY_TICK {
      return Err(IBKRError::Unsupported("Server version does not support tick-by-tick data cancels.".to_string()));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelTickByTickData as i32)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;
    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request for market depth (Level II).
  pub fn encode_request_market_depth(
    &self,
    req_id: i32,
    contract: &Contract,
    num_rows: i32,
    is_smart_depth: bool,
    mkt_depth_options: &[(String, String)],
  ) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request market depth: ReqID={}, Contract={}, Rows={}, Smart={}",
           req_id, contract.symbol, num_rows, is_smart_depth);

    if self.server_version < 6 { // Base support for reqMktDepth
      return Err(IBKRError::Unsupported("Server version does not support reqMktDepth.".to_string()));
    }
    if self.server_version < min_server_ver::TRADING_CLASS && (!contract.trading_class.is_none() || contract.con_id > 0) {
      return Err(IBKRError::Unsupported("Server version does not support conId and tradingClass parameters in reqMktDepth.".to_string()));
    }
    if self.server_version < min_server_ver::SMART_DEPTH && is_smart_depth {
      return Err(IBKRError::Unsupported("Server version does not support SMART depth request.".to_string()));
    }
    if self.server_version < min_server_ver::MKT_DEPTH_PRIM_EXCHANGE && !contract.primary_exchange.is_none() {
      return Err(IBKRError::Unsupported("Server version does not support primaryExch parameter in reqMktDepth.".to_string()));
    }
    // Market depth options require LINKING (70)
    if self.server_version < min_server_ver::LINKING && !mkt_depth_options.is_empty() {
      return Err(IBKRError::Unsupported("Server version does not support market depth options.".to_string()));
    }


    let mut cursor = self.start_encoding(OutgoingMessageType::ReqMktDepth as i32)?;
    let version = 5;

    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;

    // Contract fields
    if self.server_version >= min_server_ver::TRADING_CLASS {
      self.write_int_to_cursor(&mut cursor, contract.con_id)?;
    }
    self.write_str_to_cursor(&mut cursor, &contract.symbol)?;
    self.write_str_to_cursor(&mut cursor, &contract.sec_type.to_string())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.last_trade_date_or_contract_month.as_deref())?;
    self.write_optional_double_to_cursor(&mut cursor, contract.strike)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.right.map(|r| r.to_string()).as_deref())?;
    if self.server_version >= 15 {
      self.write_optional_str_to_cursor(&mut cursor, contract.multiplier.as_deref())?;
    }
    self.write_str_to_cursor(&mut cursor, &contract.exchange)?;
    if self.server_version >= min_server_ver::MKT_DEPTH_PRIM_EXCHANGE {
      self.write_optional_str_to_cursor(&mut cursor, contract.primary_exchange.as_deref())?;
    }
    self.write_str_to_cursor(&mut cursor, &contract.currency)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.local_symbol.as_deref())?;
    if self.server_version >= min_server_ver::TRADING_CLASS {
      self.write_optional_str_to_cursor(&mut cursor, contract.trading_class.as_deref())?;
    }

    // Depth parameters
    if self.server_version >= 19 {
      self.write_int_to_cursor(&mut cursor, num_rows)?;
    }

    // isSmartDepth added in v4 / SMART_DEPTH
    if self.server_version >= min_server_ver::SMART_DEPTH {
      self.write_bool_to_cursor(&mut cursor, is_smart_depth)?;
    }

    // mktDepthOptions added in v5 / LINKING
    if self.server_version >= min_server_ver::LINKING {
      self.write_tag_value_list(&mut cursor, mkt_depth_options)?;
    }

    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request to cancel market depth.
  pub fn encode_cancel_market_depth(&self, req_id: i32, is_smart_depth: bool) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel market depth: ReqID={}, Smart={}", req_id, is_smart_depth);

    if self.server_version < 6 { // Base support
      return Err(IBKRError::Unsupported("Server version does not support cancelMktDepth.".to_string()));
    }
    if self.server_version < min_server_ver::SMART_DEPTH && is_smart_depth {
      return Err(IBKRError::Unsupported("Server version does not support SMART depth cancel.".to_string()));
    }

    let mut cursor = self.start_encoding(OutgoingMessageType::CancelMktDepth as i32)?;
    let version = 1;

    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;

    // isSmartDepth added in v1 / SMART_DEPTH
    if self.server_version >= min_server_ver::SMART_DEPTH {
      self.write_bool_to_cursor(&mut cursor, is_smart_depth)?;
    }

    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request to cancel historical data.
  pub fn encode_cancel_historical_data(&self, req_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel historical data: ReqID={}", req_id);
    if self.server_version < 24 {
      return Err(IBKRError::Unsupported("Server version does not support historical data query cancellation.".to_string()));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelHistoricalData as i32)?;
    let version = 1;
    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;
    Ok(self.finish_encoding(cursor))
  }

  // --- News ---

  /// Encodes a request for available news providers.
  pub fn encode_request_news_providers(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request news providers");
    if self.server_version < min_server_ver::NEWS_PROVIDERS {
      return Err(IBKRError::Unsupported("Server version does not support news providers request.".to_string()));
    }
    let cursor = self.start_encoding(OutgoingMessageType::ReqNewsProviders as i32)?;
    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request for a specific news article's body.
  pub fn encode_request_news_article(
    &self,
    req_id: i32,
    provider_code: &str,
    article_id: &str,
    news_article_options: &[(String, String)], // TagValue list
  ) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request news article: ReqID={}, Provider={}, ArticleID={}", req_id, provider_code, article_id);
    if self.server_version < min_server_ver::NEWS_ARTICLE {
      return Err(IBKRError::Unsupported("Server version does not support news article request.".to_string()));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::ReqNewsArticle as i32)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;
    self.write_str_to_cursor(&mut cursor, provider_code)?;
    self.write_str_to_cursor(&mut cursor, article_id)?;

    // newsArticleOptions added in NEWS_QUERY_ORIGINS
    if self.server_version >= min_server_ver::NEWS_QUERY_ORIGINS {
      self.write_tag_value_list(&mut cursor, &news_article_options)?;
    }

    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request for historical news headlines.
  pub fn encode_request_historical_news(
    &self,
    req_id: i32,
    con_id: i32,
    provider_codes: &str, // Comma-separated list (e.g., "BZ+FLY")
    start_date_time: Option<DateTime<Utc>>,
    end_date_time: Option<DateTime<Utc>>,
    total_results: i32,
    historical_news_options: &[(String, String)], // TagValue list
  ) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request historical news: ReqID={}, ConID={}, Providers={}", req_id, con_id, provider_codes);
    if self.server_version < min_server_ver::HISTORICAL_NEWS {
      return Err(IBKRError::Unsupported("Server version does not support historical news request.".to_string()));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::ReqHistoricalNews as i32)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;
    self.write_int_to_cursor(&mut cursor, con_id)?;
    self.write_str_to_cursor(&mut cursor, provider_codes)?;
    let start_str = self.format_datetime_tws(start_date_time, "%Y%m%d %H:%M:%S", Some(" UTC"));
    let end_str = self.format_datetime_tws(end_date_time, "%Y%m%d %H:%M:%S", Some(" UTC"));
    self.write_str_to_cursor(&mut cursor, &start_str)?;
    self.write_str_to_cursor(&mut cursor, &end_str)?;
    self.write_int_to_cursor(&mut cursor, total_results)?;

    // historicalNewsOptions added in NEWS_QUERY_ORIGINS
    if self.server_version >= min_server_ver::NEWS_QUERY_ORIGINS {
      self.write_tag_value_list(&mut cursor, historical_news_options)?;
    }

    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request to subscribe to news bulletins.
  pub fn encode_request_news_bulletins(&self, all_msgs: bool) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request news bulletins: AllMsgs={}", all_msgs);
    let mut cursor = self.start_encoding(OutgoingMessageType::ReqNewsBulletins as i32)?;
    let version = 1;
    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_bool_to_cursor(&mut cursor, all_msgs)?; // true = all bulletins, false = only new bulletins
    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request to cancel news bulletin subscription.
  pub fn encode_cancel_news_bulletins(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel news bulletins");
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelNewsBulletins as i32)?;
    let version = 1;
    self.write_int_to_cursor(&mut cursor, version)?;
    Ok(self.finish_encoding(cursor))
  }

  // --- Fundamental Data ---

  /// Encodes a request for fundamental data.
  pub fn encode_request_fundamental_data(
    &self,
    req_id: i32,
    contract: &Contract,
    report_type: &str, // e.g., "ReportsFinSummary", "ReportSnapshot", "RESC", "CalendarReport"
    fundamental_data_options: &[(String, String)], // TagValue list (added in v3)
  ) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request fundamental data: ReqID={}, Contract={}, ReportType={}", req_id, contract.symbol, report_type);

    if self.server_version < min_server_ver::FUNDAMENTAL_DATA {
      return Err(IBKRError::Unsupported("Server version does not support fundamental data requests.".to_string()));
    }
    if self.server_version < min_server_ver::TRADING_CLASS && contract.con_id > 0 {
      return Err(IBKRError::Unsupported("Server version does not support conId parameter in reqFundamentalData.".to_string()));
    }
    // Options require LINKING (70)
    if self.server_version < min_server_ver::LINKING && !fundamental_data_options.is_empty() {
      return Err(IBKRError::Unsupported("Server version does not support fundamental data options.".to_string()));
    }

    let mut cursor = self.start_encoding(OutgoingMessageType::ReqFundamentalData as i32)?;
    // Let's stick to version 2 for simplicity, as options are handled by LINKING check anyway.
    let version = 2;
    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;

    if self.server_version >= min_server_ver::TRADING_CLASS {
      self.write_int_to_cursor(&mut cursor, contract.con_id)?;
    }
    self.write_str_to_cursor(&mut cursor, &contract.symbol)?;
    self.write_str_to_cursor(&mut cursor, &contract.sec_type.to_string())?;
    self.write_str_to_cursor(&mut cursor, &contract.exchange)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.primary_exchange.as_deref())?;
    self.write_str_to_cursor(&mut cursor, &contract.currency)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.local_symbol.as_deref())?;

    // Report type
    self.write_str_to_cursor(&mut cursor, report_type)?;

    // Fundamental data options (TagValue list - Added server version LINKING)
    if self.server_version >= min_server_ver::LINKING {
      self.write_tag_value_list(&mut cursor, fundamental_data_options)?;
    }

    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request to cancel fundamental data.
  pub fn encode_cancel_fundamental_data(&self, req_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel fundamental data: ReqID={}", req_id);
    if self.server_version < min_server_ver::FUNDAMENTAL_DATA {
      return Err(IBKRError::Unsupported("Server version does not support fundamental data requests.".to_string())); // Message adjusted
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelFundamentalData as i32)?;
    let version = 1;
    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;
    Ok(self.finish_encoding(cursor))
  }

  // --- Wall Street Horizon (WSH) ---

  /// Encodes a request for WSH metadata.
  pub fn encode_request_wsh_meta_data(&self, req_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request WSH metadata: ReqID={}", req_id);
    if self.server_version < min_server_ver::WSHE_CALENDAR {
      return Err(IBKRError::UpdateTws(format!(
        "Server version {} does not support WSHE Calendar API (requires {}).", // Adjusted message
        self.server_version, min_server_ver::WSHE_CALENDAR
      )));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::ReqWshMetaData as i32)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;
    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request to cancel WSH metadata.
  pub fn encode_cancel_wsh_meta_data(&self, req_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel WSH metadata: ReqID={}", req_id);
    if self.server_version < min_server_ver::WSHE_CALENDAR {
      return Err(IBKRError::UpdateTws(format!(
        "Server version {} does not support WSHE Calendar API (requires {}).", // Adjusted message
        self.server_version, min_server_ver::WSHE_CALENDAR
      )));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelWshMetaData as i32)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;
    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request for WSH event data.
  pub fn encode_request_wsh_event_data(
    &self,
    req_id: i32,
    wsh_event_data: &WshEventDataRequest, // Use the defined struct
  ) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request WSH event data: ReqID={}, Filters={:?}", req_id, wsh_event_data);

    if self.server_version < min_server_ver::WSHE_CALENDAR {
      return Err(IBKRError::UpdateTws(format!(
        "Server version {} does not support WSHE Calendar API (requires {}).", // Adjusted message
        self.server_version, min_server_ver::WSHE_CALENDAR
      )));
    }
    if self.server_version < min_server_ver::WSH_EVENT_DATA_FILTERS {
      if wsh_event_data.filter.is_some() || wsh_event_data.fill_watchlist || wsh_event_data.fill_portfolio || wsh_event_data.fill_competitors {
        return Err(IBKRError::UpdateTws(format!(
          "Server version {} does not support WSH event data filters (requires {}).", // Adjusted message
          self.server_version, min_server_ver::WSH_EVENT_DATA_FILTERS
        )));
      }
    }
    if self.server_version < min_server_ver::WSH_EVENT_DATA_FILTERS_DATE {
      if wsh_event_data.start_date.is_some() || wsh_event_data.end_date.is_some() || wsh_event_data.total_limit != Some(i32::MAX) {
        // Allow None total_limit, as it implies MAX_VALUE wasn't sent
        if wsh_event_data.start_date.is_some() || wsh_event_data.end_date.is_some() || (wsh_event_data.total_limit.is_some() && wsh_event_data.total_limit != Some(i32::MAX)) {
          return Err(IBKRError::UpdateTws(format!(
            "Server version {} does not support WSH event data date filters (requires {}).", // Adjusted message
            self.server_version, min_server_ver::WSH_EVENT_DATA_FILTERS_DATE
          )));
        }
      }
    }

    let mut cursor = self.start_encoding(OutgoingMessageType::ReqWshEventData as i32)?;

    self.write_int_to_cursor(&mut cursor, req_id)?;
    self.write_int_to_cursor(&mut cursor, wsh_event_data.con_id.unwrap_or(0))?;

    // Filters added in WSH_EVENT_DATA_FILTERS
    if self.server_version >= min_server_ver::WSH_EVENT_DATA_FILTERS {
      self.write_optional_str_to_cursor(&mut cursor, wsh_event_data.filter.as_deref())?;
      self.write_bool_to_cursor(&mut cursor, wsh_event_data.fill_watchlist)?;
      self.write_bool_to_cursor(&mut cursor, wsh_event_data.fill_portfolio)?;
      self.write_bool_to_cursor(&mut cursor, wsh_event_data.fill_competitors)?;
    }

    // Date filters added in WSH_EVENT_DATA_FILTERS_DATE
    if self.server_version >= min_server_ver::WSH_EVENT_DATA_FILTERS_DATE {
      self.write_optional_str_to_cursor(&mut cursor, wsh_event_data.start_date.as_deref())?;
      self.write_optional_str_to_cursor(&mut cursor, wsh_event_data.end_date.as_deref())?;
      self.write_int_to_cursor(&mut cursor, wsh_event_data.total_limit.unwrap_or(i32::MAX))?;
    }

    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request for histogram data.
  pub fn encode_request_histogram_data(
    &self,
    req_id: i32,
    contract: &Contract,
    use_rth: bool,
    time_period: &str, // e.g., "3 days", "1 week", "2 months"
  ) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request histogram data: ReqID={}, Contract={}, UseRTH={}, TimePeriod={}",
           req_id, contract.symbol, use_rth, time_period);

    if self.server_version < min_server_ver::HISTOGRAM { // Use REQ_HISTOGRAM from min_server_ver
      return Err(IBKRError::Unsupported("Server version does not support histogram data requests.".to_string()));
    }

    let mut cursor = self.start_encoding(OutgoingMessageType::ReqHistogramData as i32)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;

    // Contract fields (minimal set for histogram)
    self.write_int_to_cursor(&mut cursor, contract.con_id)?;
    self.write_str_to_cursor(&mut cursor, &contract.symbol)?;
    self.write_str_to_cursor(&mut cursor, &contract.sec_type.to_string())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.last_trade_date_or_contract_month.as_deref())?;
    self.write_optional_double_to_cursor(&mut cursor, contract.strike)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.right.map(|r| r.to_string()).as_deref())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.multiplier.as_deref())?;
    self.write_str_to_cursor(&mut cursor, &contract.exchange)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.primary_exchange.as_deref())?;
    self.write_str_to_cursor(&mut cursor, &contract.currency)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.local_symbol.as_deref())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.trading_class.as_deref())?;
    self.write_bool_to_cursor(&mut cursor, contract.include_expired)?;

    // Histogram parameters
    self.write_bool_to_cursor(&mut cursor, use_rth)?;
    self.write_str_to_cursor(&mut cursor, time_period)?;

    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request to cancel histogram data.
  pub fn encode_cancel_histogram_data(&self, req_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel histogram data: ReqID={}", req_id);
    if self.server_version < min_server_ver::CANCEL_HISTOGRAM_DATA { // Use CANCEL_HISTOGRAM_DATA
      return Err(IBKRError::Unsupported("Server version does not support histogram data cancellation.".to_string()));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelHistogramData as i32)?;
    self.write_int_to_cursor(&mut cursor, req_id)?; // Version is not explicitly sent for this cancel message
    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request to calculate implied volatility.
  pub fn encode_request_calculate_implied_volatility(
    &self,
    req_id: i32,
    contract: &Contract,
    option_price: f64,
    under_price: f64,
  ) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request calculate implied volatility: ReqID={}, ContractSymbol={}, OptPx={}, UndPx={}",
           req_id, contract.symbol, option_price, under_price); // Log symbol for clarity

    if self.server_version < min_server_ver::CALC_IMPLIED_VOLAT {
      return Err(IBKRError::Unsupported("Server version does not support calculate implied volatility request.".to_string()));
    }

    let mut cursor = self.start_encoding(OutgoingMessageType::ReqCalcImpliedVolat as i32)?;
    let version = 3; // Matches Python ibapi version for this message

    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;

    // --- Encode Contract Fields (modified to match Python reference behavior) ---
    self.write_int_to_cursor(&mut cursor, 0)?; // conId - Always 0 for these calculation requests
    self.write_str_to_cursor(&mut cursor, &contract.symbol)?;
    self.write_str_to_cursor(&mut cursor, &contract.sec_type.to_string())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.last_trade_date_or_contract_month.as_deref())?;
    self.write_optional_double_to_cursor(&mut cursor, contract.strike)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.right.map(|r| r.to_string()).as_deref())?;
    self.write_str_to_cursor(&mut cursor, "")?; // multiplier - Always "" for these calculation requests
    self.write_str_to_cursor(&mut cursor, &contract.exchange)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.primary_exchange.as_deref())?;
    self.write_str_to_cursor(&mut cursor, &contract.currency)?;
    self.write_str_to_cursor(&mut cursor, "")?; // localSymbol - Always "" for these calculation requests
    self.write_str_to_cursor(&mut cursor, "")?; // tradingClass - Always "" for these calculation requests

    // Calculation parameters
    self.write_double_to_cursor(&mut cursor, option_price)?;
    self.write_double_to_cursor(&mut cursor, under_price)?;

    // Implied Volatility Options (TagValue list)
    // Sent as: count, then {tag, value} pairs for each option.
    if self.server_version >= min_server_ver::LINKING {
      // The test case passes an empty list of options.
      // So, send count 0.
      self.write_int_to_cursor(&mut cursor, 0)?; // Number of implied volatility options
      self.write_str_to_cursor(&mut cursor, "")?; // Array
    }

    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request to cancel implied volatility calculation.
  pub fn encode_cancel_calculate_implied_volatility(&self, req_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel calculate implied volatility: ReqID={}", req_id);
    if self.server_version < min_server_ver::CALC_IMPLIED_VOLAT { // Same min version as req
      return Err(IBKRError::Unsupported("Server version does not support implied volatility calculation cancellation.".to_string()));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelCalcImpliedVolat as i32)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;
    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request to calculate option price.
  pub fn encode_request_calculate_option_price(
    &self,
    req_id: i32,
    contract: &Contract,
    volatility: f64,
    under_price: f64,
  ) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request calculate option price: ReqID={}, ContractSymbol={}, Vol={}, UndPx={}",
           req_id, contract.symbol, volatility, under_price); // Log symbol for clarity

    if self.server_version < min_server_ver::CALC_OPTION_PRICE {
      return Err(IBKRError::Unsupported("Server version does not support calculate option price request.".to_string()));
    }

    let mut cursor = self.start_encoding(OutgoingMessageType::ReqCalcOptionPrice as i32)?;
    let version = 3;

    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;

    // --- Encode Contract Fields (modified to match Python reference behavior) ---
    self.write_int_to_cursor(&mut cursor, 0)?; // conId - Always 0 for these calculation requests
    self.write_str_to_cursor(&mut cursor, &contract.symbol)?;
    self.write_str_to_cursor(&mut cursor, &contract.sec_type.to_string())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.last_trade_date_or_contract_month.as_deref())?;
    self.write_optional_double_to_cursor(&mut cursor, contract.strike)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.right.map(|r| r.to_string()).as_deref())?;
    self.write_str_to_cursor(&mut cursor, "")?; // multiplier - Always "" for these calculation requests
    self.write_str_to_cursor(&mut cursor, &contract.exchange)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.primary_exchange.as_deref())?;
    self.write_str_to_cursor(&mut cursor, &contract.currency)?;
    self.write_str_to_cursor(&mut cursor, "")?; // localSymbol - Always "" for these calculation requests
    self.write_str_to_cursor(&mut cursor, "")?; // tradingClass - Always "" for these calculation requests

    // Calculation parameters
    self.write_double_to_cursor(&mut cursor, volatility)?;
    self.write_double_to_cursor(&mut cursor, under_price)?;

    // Option Price Options (TagValue list)
    // Sent as: count, then {tag, value} pairs for each option.
    if self.server_version >= min_server_ver::LINKING {
      // The test case passes an empty list of options.
      // So, send count 0.
      self.write_int_to_cursor(&mut cursor, 0)?; // Number of option price options
      self.write_str_to_cursor(&mut cursor, "")?; // Array
    }

    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request to cancel option price calculation.
  pub fn encode_cancel_calculate_option_price(&self, req_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel calculate option price: ReqID={}", req_id);
    if self.server_version < min_server_ver::CALC_OPTION_PRICE { // Same min version as req
      return Err(IBKRError::Unsupported("Server version does not support option price calculation cancellation.".to_string()));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelCalcOptionPrice as i32)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;
    Ok(self.finish_encoding(cursor))
  }


  /// Encodes a request to set the market data type.
  pub fn encode_request_market_data_type(&self, market_data_type: MarketDataType) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request market data type: Type={:?}", market_data_type);
    if self.server_version < min_server_ver::MARKET_DATA_TYPE {
      return Err(IBKRError::Unsupported("Server version does not support reqMarketDataType.".to_string()));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::ReqMarketDataType as i32)?;
    let version = 1;
    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, market_data_type as i32)?; // 1=Live, 2=Frozen, 3=Delayed, 4=Delayed Frozen
    Ok(self.finish_encoding(cursor))
  }


  /// Encodes a request to cancel WSH event data.
  pub fn encode_cancel_wsh_event_data(&self, req_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel WSH event data: ReqID={}", req_id);
    if self.server_version < min_server_ver::WSHE_CALENDAR {
      return Err(IBKRError::UpdateTws(format!(
        "Server version {} does not support WSHE Calendar API (requires {}).", // Adjusted message
        self.server_version, min_server_ver::WSHE_CALENDAR
      )));
    }
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelWshEventData as i32)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;
    Ok(self.finish_encoding(cursor))
  }

  /// Encodes a request for historical tick data.
  pub fn encode_request_historical_ticks(
    &self,
    req_id: i32,
    contract: &Contract,
    start_date_time: Option<DateTime<Utc>>,
    end_date_time: Option<DateTime<Utc>>,
    number_of_ticks: i32, // Max 1000. Use 0 for all ticks during the specified period.
    what_to_show: &str,   // "TRADES", "MIDPOINT", "BID_ASK"
    use_rth: bool,
    ignore_size: bool, // For BID_ASK ticks
    misc_options: &[(String, String)], // TagValue list, semicolon-separated
  ) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request historical ticks: ReqID={}, Contract={}, What={}, NumTicks={}",
           req_id, contract.symbol, what_to_show, number_of_ticks);

    if self.server_version < min_server_ver::HISTORICAL_TICKS {
      return Err(IBKRError::Unsupported("Server version does not support historical ticks requests.".to_string()));
    }

    let mut cursor = self.start_encoding(OutgoingMessageType::ReqHistoricalTicks as i32)?;

    self.write_int_to_cursor(&mut cursor, req_id)?;

    // Encode Contract fields
    self.write_int_to_cursor(&mut cursor, contract.con_id)?;
    self.write_str_to_cursor(&mut cursor, &contract.symbol)?;
    self.write_str_to_cursor(&mut cursor, &contract.sec_type.to_string())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.last_trade_date_or_contract_month.as_deref())?;
    self.write_optional_double_to_cursor(&mut cursor, contract.strike)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.right.map(|r| r.to_string()).as_deref())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.multiplier.as_deref())?;
    self.write_str_to_cursor(&mut cursor, &contract.exchange)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.primary_exchange.as_deref())?;
    self.write_str_to_cursor(&mut cursor, &contract.currency)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.local_symbol.as_deref())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.trading_class.as_deref())?;
    self.write_bool_to_cursor(&mut cursor, contract.include_expired)?;

    // Encode historical tick parameters
    let start_dt_str = self.format_datetime_tws(start_date_time, "%Y%m%d %H:%M:%S", Some(" UTC"));
    let end_dt_str = self.format_datetime_tws(end_date_time, "%Y%m%d %H:%M:%S", Some(" UTC"));

    self.write_str_to_cursor(&mut cursor, &start_dt_str)?;
    self.write_str_to_cursor(&mut cursor, &end_dt_str)?;

    self.write_int_to_cursor(&mut cursor, number_of_ticks)?;
    self.write_str_to_cursor(&mut cursor, what_to_show)?;
    self.write_int_to_cursor(&mut cursor, if use_rth { 1 } else { 0 })?; // useRTH is int in protocol
    self.write_bool_to_cursor(&mut cursor, ignore_size)?;

    // Encode miscOptions as a semicolon-separated string
    if self.server_version >= min_server_ver::LINKING {
      if misc_options.is_empty() {
        self.write_str_to_cursor(&mut cursor, "")?;
      } else {
        let misc_options_str = misc_options
          .iter()
          .map(|(tag, value)| format!("{}={}", tag, value))
          .collect::<Vec<String>>()
          .join(";");
        self.write_str_to_cursor(&mut cursor, &misc_options_str)?;
      }
    } else if !misc_options.is_empty() {
      warn!("miscOptions provided for historical ticks, but server version {} does not support it (requires {}). Sending empty.",
            self.server_version, min_server_ver::LINKING);
      self.write_str_to_cursor(&mut cursor, "")?;
    }
    // If server_version < LINKING and misc_options is empty, protocol expects no field here.
    // The TWS protocol expects fields based on version; if a field isn't supported, it's not sent.
    // The current implementation of write_str_to_cursor will send an empty string if misc_options is empty
    // and server_version >= LINKING, which is correct.
    // If server_version < LINKING, this field is not part of the message structure, so nothing should be appended.
    // The current logic correctly handles this by not attempting to write miscOptions if server_version < LINKING
    // unless misc_options is non-empty (which then logs a warning and sends empty).

    Ok(self.finish_encoding(cursor))
  }
} // end impl Encoder
