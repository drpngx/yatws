// yatws/src/protocol_encoder.rs
// Encoder for the TWS API protocol messages

use std::io::Cursor;
use crate::base::IBKRError;
use crate::contract::{Contract, ContractDetails, OptionRight, SecType, ComboLeg};
use crate::order::{Order, OrderRequest, TimeInForce, OrderType, OrderSide, OrderState};
use crate::account::{AccountInfo, ExecutionFilter};
use crate::min_server_ver::min_server_ver;
use chrono::{DateTime, Utc};
use log::{debug, trace, warn, error};
use std::io::{self, Write};
use num_traits::cast::ToPrimitive;
use std::collections::HashMap;

/// Message tags for outgoing messages
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutgoingMessageType {
  RequestMarketData = 1,
  CancelMarketData = 2,
  PlaceOrder = 3,
  CancelOrder = 4,
  RequestOpenOrders = 5,
  RequestAccountData = 6,
  RequestExecutions = 7,
  RequestIds = 8,
  RequestContractData = 9,
  RequestMarketDepth = 10,
  CancelMarketDepth = 11,
  RequestNewsBulletins = 12,
  CancelNewsBulletins = 13,
  SetServerLogLevel = 14,
  RequestAutoOpenOrders = 15,
  RequestAllOpenOrders = 16,
  RequestManagedAccts = 17,
  RequestFinancialAdvisor = 18,
  ReplaceFinancialAdvisor = 19,
  RequestHistoricalData = 20,
  ExerciseOptions = 21,
  RequestScannerSubscription = 22,
  CancelScannerSubscription = 23,
  RequestScannerParameters = 24,
  CancelHistoricalData = 25,
  RequestCurrentTime = 49,
  RequestRealTimeBars = 50,
  CancelRealTimeBars = 51,
  RequestFundamentalData = 52,
  CancelFundamentalData = 53,
  RequestCalcImpliedVolat = 54,
  RequestCalcOptionPrice = 55,
  CancelCalcImpliedVolat = 56,
  CancelCalcOptionPrice = 57,
  RequestGlobalCancel = 58,
  RequestMarketDataType = 59,
  RequestPositions = 61,
  RequestAccountSummary = 62,
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
  RequestPositionsMulti = 74,
  CancelPositionsMulti = 75,
  RequestAccountUpdatesMulti = 76,
  CancelAccountUpdatesMulti = 77,
  RequestSecDefOptParams = 78,
  RequestSoftDollarTiers = 79,
  RequestFamilyCodes = 80,
  RequestMatchingSymbols = 81,
  RequestMktDepthExchanges = 82,
  RequestSmartComponents = 83,
  RequestNewsArticle = 84,
  RequestNewsProviders = 85,
  RequestHistoricalNews = 86,
  RequestHeadTimestamp = 87,
  RequestHistogramData = 88,
  CancelHistogramData = 89,
  CancelHeadTimestamp = 90,
  RequestMarketRule = 91,
  RequestPnL = 92,
  CancelPnL = 93,
  RequestPnLSingle = 94,
  CancelPnLSingle = 95,
  RequestHistoricalTicks = 96,
  RequestTickByTickData = 97,
  CancelTickByTickData = 98,
  RequestCompletedOrders = 99,
  RequestWshMetaData = 100,
  CancelWshMetaData = 101,
  RequestWshEventData = 102,
  CancelWshEventData = 103,
  RequestUserInfo = 104,
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
    Ok(OutgoingMessageType::RequestMarketData) => Some("REQ_MKT_DATA"),
    Ok(OutgoingMessageType::CancelMarketData) => Some("CANCEL_MKT_DATA"),
    Ok(OutgoingMessageType::PlaceOrder) => Some("PLACE_ORDER"),
    Ok(OutgoingMessageType::CancelOrder) => Some("CANCEL_ORDER"),
    Ok(OutgoingMessageType::RequestOpenOrders) => Some("REQ_OPEN_ORDERS"),
    Ok(OutgoingMessageType::RequestAccountData) => Some("REQ_ACCOUNT_DATA"),
    Ok(OutgoingMessageType::RequestExecutions) => Some("REQ_EXECUTIONS"),
    Ok(OutgoingMessageType::RequestIds) => Some("REQ_IDS"),
    Ok(OutgoingMessageType::RequestContractData) => Some("REQ_CONTRACT_DATA"),
    Ok(OutgoingMessageType::RequestMarketDepth) => Some("REQ_MKT_DEPTH"),
    Ok(OutgoingMessageType::CancelMarketDepth) => Some("CANCEL_MKT_DEPTH"),
    Ok(OutgoingMessageType::RequestNewsBulletins) => Some("REQ_NEWS_BULLETINS"),
    Ok(OutgoingMessageType::CancelNewsBulletins) => Some("CANCEL_NEWS_BULLETINS"),
    Ok(OutgoingMessageType::SetServerLogLevel) => Some("SET_SERVER_LOGLEVEL"),
    Ok(OutgoingMessageType::RequestAutoOpenOrders) => Some("REQ_AUTO_OPEN_ORDERS"),
    Ok(OutgoingMessageType::RequestAllOpenOrders) => Some("REQ_ALL_OPEN_ORDERS"),
    Ok(OutgoingMessageType::RequestManagedAccts) => Some("REQ_MANAGED_ACCTS"),
    Ok(OutgoingMessageType::RequestFinancialAdvisor) => Some("REQ_FA"),
    Ok(OutgoingMessageType::ReplaceFinancialAdvisor) => Some("REPLACE_FA"),
    Ok(OutgoingMessageType::RequestHistoricalData) => Some("REQ_HISTORICAL_DATA"),
    Ok(OutgoingMessageType::ExerciseOptions) => Some("EXERCISE_OPTIONS"),
    Ok(OutgoingMessageType::RequestScannerSubscription) => Some("REQ_SCANNER_SUBSCRIPTION"),
    Ok(OutgoingMessageType::CancelScannerSubscription) => Some("CANCEL_SCANNER_SUBSCRIPTION"),
    Ok(OutgoingMessageType::RequestScannerParameters) => Some("REQ_SCANNER_PARAMETERS"),
    Ok(OutgoingMessageType::CancelHistoricalData) => Some("CANCEL_HISTORICAL_DATA"),
    Ok(OutgoingMessageType::RequestCurrentTime) => Some("REQ_CURRENT_TIME"),
    Ok(OutgoingMessageType::RequestRealTimeBars) => Some("REQ_REAL_TIME_BARS"),
    Ok(OutgoingMessageType::CancelRealTimeBars) => Some("CANCEL_REAL_TIME_BARS"),
    Ok(OutgoingMessageType::RequestFundamentalData) => Some("REQ_FUNDAMENTAL_DATA"),
    Ok(OutgoingMessageType::CancelFundamentalData) => Some("CANCEL_FUNDAMENTAL_DATA"),
    Ok(OutgoingMessageType::RequestCalcImpliedVolat) => Some("REQ_CALC_IMPLIED_VOLAT"),
    Ok(OutgoingMessageType::RequestCalcOptionPrice) => Some("REQ_CALC_OPTION_PRICE"),
    Ok(OutgoingMessageType::CancelCalcImpliedVolat) => Some("CANCEL_CALC_IMPLIED_VOLAT"),
    Ok(OutgoingMessageType::CancelCalcOptionPrice) => Some("CANCEL_CALC_OPTION_PRICE"),
    Ok(OutgoingMessageType::RequestGlobalCancel) => Some("REQ_GLOBAL_CANCEL"),
    Ok(OutgoingMessageType::RequestMarketDataType) => Some("REQ_MARKET_DATA_TYPE"),
    Ok(OutgoingMessageType::RequestPositions) => Some("REQ_POSITIONS"),
    Ok(OutgoingMessageType::RequestAccountSummary) => Some("REQ_ACCOUNT_SUMMARY"),
    Ok(OutgoingMessageType::CancelAccountSummary) => Some("CANCEL_ACCOUNT_SUMMARY"),
    Ok(OutgoingMessageType::CancelPositions) => Some("CANCEL_POSITIONS"),
    Ok(OutgoingMessageType::VerifyRequest) => Some("VERIFY_REQUEST"),
    Ok(OutgoingMessageType::VerifyMessage) => Some("VERIFY_MESSAGE"),
    Ok(OutgoingMessageType::QueryDisplayGroups) => Some("QUERY_DISPLAY_GROUPS"),
    Ok(OutgoingMessageType::SubscribeToGroupEvents) => Some("SUBSCRIBE_TO_GROUP_EVENTS"),
    Ok(OutgoingMessageType::UpdateDisplayGroup) => Some("UPDATE_DISPLAY_GROUP"),
    Ok(OutgoingMessageType::UnsubscribeFromGroupEvents) => Some("UNSUBSCRIBE_FROM_GROUP_EVENTS"),
    Ok(OutgoingMessageType::StartApi) => Some("H3_START_API"), // Use H3 alias
    Ok(OutgoingMessageType::VerifyAndAuthRequest) => Some("VERIFY_AND_AUTH_REQUEST"),
    Ok(OutgoingMessageType::VerifyAndAuthMessage) => Some("VERIFY_AND_AUTH_MESSAGE"),
    Ok(OutgoingMessageType::RequestPositionsMulti) => Some("REQ_POSITIONS_MULTI"),
    Ok(OutgoingMessageType::CancelPositionsMulti) => Some("CANCEL_POSITIONS_MULTI"),
    Ok(OutgoingMessageType::RequestAccountUpdatesMulti) => Some("REQ_ACCOUNT_UPDATES_MULTI"),
    Ok(OutgoingMessageType::CancelAccountUpdatesMulti) => Some("CANCEL_ACCOUNT_UPDATES_MULTI"),
    Ok(OutgoingMessageType::RequestSecDefOptParams) => Some("REQ_SEC_DEF_OPT_PARAMS"),
    Ok(OutgoingMessageType::RequestSoftDollarTiers) => Some("REQ_SOFT_DOLLAR_TIERS"),
    Ok(OutgoingMessageType::RequestFamilyCodes) => Some("REQ_FAMILY_CODES"),
    Ok(OutgoingMessageType::RequestMatchingSymbols) => Some("REQ_MATCHING_SYMBOLS"),
    Ok(OutgoingMessageType::RequestMktDepthExchanges) => Some("REQ_MKT_DEPTH_EXCHANGES"),
    Ok(OutgoingMessageType::RequestSmartComponents) => Some("REQ_SMART_COMPONENTS"),
    Ok(OutgoingMessageType::RequestNewsArticle) => Some("REQ_NEWS_ARTICLE"),
    Ok(OutgoingMessageType::RequestNewsProviders) => Some("REQ_NEWS_PROVIDERS"),
    Ok(OutgoingMessageType::RequestHistoricalNews) => Some("REQ_HISTORICAL_NEWS"),
    Ok(OutgoingMessageType::RequestHeadTimestamp) => Some("REQ_HEAD_TIMESTAMP"),
    Ok(OutgoingMessageType::RequestHistogramData) => Some("REQ_HISTOGRAM_DATA"),
    Ok(OutgoingMessageType::CancelHistogramData) => Some("CANCEL_HISTOGRAM_DATA"),
    Ok(OutgoingMessageType::CancelHeadTimestamp) => Some("CANCEL_HEAD_TIMESTAMP"),
    Ok(OutgoingMessageType::RequestMarketRule) => Some("REQ_MARKET_RULE"),
    Ok(OutgoingMessageType::RequestPnL) => Some("REQ_PNL"),
    Ok(OutgoingMessageType::CancelPnL) => Some("CANCEL_PNL"),
    Ok(OutgoingMessageType::RequestPnLSingle) => Some("REQ_PNL_SINGLE"),
    Ok(OutgoingMessageType::CancelPnLSingle) => Some("CANCEL_PNL_SINGLE"),
    Ok(OutgoingMessageType::RequestHistoricalTicks) => Some("REQ_HISTORICAL_TICKS"),
    Ok(OutgoingMessageType::RequestTickByTickData) => Some("REQ_TICK_BY_TICK_DATA"),
    Ok(OutgoingMessageType::CancelTickByTickData) => Some("CANCEL_TICK_BY_TICK_DATA"),
    Ok(OutgoingMessageType::RequestCompletedOrders) => Some("REQ_COMPLETED_ORDERS"),
    Ok(OutgoingMessageType::RequestWshMetaData) => Some("REQ_WSH_META_DATA"),
    Ok(OutgoingMessageType::CancelWshMetaData) => Some("CANCEL_WSH_META_DATA"),
    Ok(OutgoingMessageType::RequestWshEventData) => Some("REQ_WSH_EVENT_DATA"),
    Ok(OutgoingMessageType::CancelWshEventData) => Some("CANCEL_WSH_EVENT_DATA"),
    Ok(OutgoingMessageType::RequestUserInfo) => Some("REQ_USER_INFO"),
    // Add any missing outgoing message IDs here
    Err(_) => None, // Unknown type ID
  }
}

// Helper to allow conversion from i32, needed for the match statement above
impl TryFrom<i32> for OutgoingMessageType {
  type Error = ();

  fn try_from(v: i32) -> Result<Self, Self::Error> {
    match v {
      x if x == OutgoingMessageType::RequestMarketData as i32 => Ok(OutgoingMessageType::RequestMarketData),
      x if x == OutgoingMessageType::CancelMarketData as i32 => Ok(OutgoingMessageType::CancelMarketData),
      x if x == OutgoingMessageType::PlaceOrder as i32 => Ok(OutgoingMessageType::PlaceOrder),
      x if x == OutgoingMessageType::CancelOrder as i32 => Ok(OutgoingMessageType::CancelOrder),
      x if x == OutgoingMessageType::RequestOpenOrders as i32 => Ok(OutgoingMessageType::RequestOpenOrders),
      x if x == OutgoingMessageType::RequestAccountData as i32 => Ok(OutgoingMessageType::RequestAccountData),
      x if x == OutgoingMessageType::RequestExecutions as i32 => Ok(OutgoingMessageType::RequestExecutions),
      x if x == OutgoingMessageType::RequestIds as i32 => Ok(OutgoingMessageType::RequestIds),
      x if x == OutgoingMessageType::RequestContractData as i32 => Ok(OutgoingMessageType::RequestContractData),
      x if x == OutgoingMessageType::RequestMarketDepth as i32 => Ok(OutgoingMessageType::RequestMarketDepth),
      x if x == OutgoingMessageType::CancelMarketDepth as i32 => Ok(OutgoingMessageType::CancelMarketDepth),
      x if x == OutgoingMessageType::RequestNewsBulletins as i32 => Ok(OutgoingMessageType::RequestNewsBulletins),
      x if x == OutgoingMessageType::CancelNewsBulletins as i32 => Ok(OutgoingMessageType::CancelNewsBulletins),
      x if x == OutgoingMessageType::SetServerLogLevel as i32 => Ok(OutgoingMessageType::SetServerLogLevel),
      x if x == OutgoingMessageType::RequestAutoOpenOrders as i32 => Ok(OutgoingMessageType::RequestAutoOpenOrders),
      x if x == OutgoingMessageType::RequestAllOpenOrders as i32 => Ok(OutgoingMessageType::RequestAllOpenOrders),
      x if x == OutgoingMessageType::RequestManagedAccts as i32 => Ok(OutgoingMessageType::RequestManagedAccts),
      x if x == OutgoingMessageType::RequestFinancialAdvisor as i32 => Ok(OutgoingMessageType::RequestFinancialAdvisor),
      x if x == OutgoingMessageType::ReplaceFinancialAdvisor as i32 => Ok(OutgoingMessageType::ReplaceFinancialAdvisor),
      x if x == OutgoingMessageType::RequestHistoricalData as i32 => Ok(OutgoingMessageType::RequestHistoricalData),
      x if x == OutgoingMessageType::ExerciseOptions as i32 => Ok(OutgoingMessageType::ExerciseOptions),
      x if x == OutgoingMessageType::RequestScannerSubscription as i32 => Ok(OutgoingMessageType::RequestScannerSubscription),
      x if x == OutgoingMessageType::CancelScannerSubscription as i32 => Ok(OutgoingMessageType::CancelScannerSubscription),
      x if x == OutgoingMessageType::RequestScannerParameters as i32 => Ok(OutgoingMessageType::RequestScannerParameters),
      x if x == OutgoingMessageType::CancelHistoricalData as i32 => Ok(OutgoingMessageType::CancelHistoricalData),
      x if x == OutgoingMessageType::RequestCurrentTime as i32 => Ok(OutgoingMessageType::RequestCurrentTime),
      x if x == OutgoingMessageType::RequestRealTimeBars as i32 => Ok(OutgoingMessageType::RequestRealTimeBars),
      x if x == OutgoingMessageType::CancelRealTimeBars as i32 => Ok(OutgoingMessageType::CancelRealTimeBars),
      x if x == OutgoingMessageType::RequestFundamentalData as i32 => Ok(OutgoingMessageType::RequestFundamentalData),
      x if x == OutgoingMessageType::CancelFundamentalData as i32 => Ok(OutgoingMessageType::CancelFundamentalData),
      x if x == OutgoingMessageType::RequestCalcImpliedVolat as i32 => Ok(OutgoingMessageType::RequestCalcImpliedVolat),
      x if x == OutgoingMessageType::RequestCalcOptionPrice as i32 => Ok(OutgoingMessageType::RequestCalcOptionPrice),
      x if x == OutgoingMessageType::CancelCalcImpliedVolat as i32 => Ok(OutgoingMessageType::CancelCalcImpliedVolat),
      x if x == OutgoingMessageType::CancelCalcOptionPrice as i32 => Ok(OutgoingMessageType::CancelCalcOptionPrice),
      x if x == OutgoingMessageType::RequestGlobalCancel as i32 => Ok(OutgoingMessageType::RequestGlobalCancel),
      x if x == OutgoingMessageType::RequestMarketDataType as i32 => Ok(OutgoingMessageType::RequestMarketDataType),
      x if x == OutgoingMessageType::RequestPositions as i32 => Ok(OutgoingMessageType::RequestPositions),
      x if x == OutgoingMessageType::RequestAccountSummary as i32 => Ok(OutgoingMessageType::RequestAccountSummary),
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
      x if x == OutgoingMessageType::RequestPositionsMulti as i32 => Ok(OutgoingMessageType::RequestPositionsMulti),
      x if x == OutgoingMessageType::CancelPositionsMulti as i32 => Ok(OutgoingMessageType::CancelPositionsMulti),
      x if x == OutgoingMessageType::RequestAccountUpdatesMulti as i32 => Ok(OutgoingMessageType::RequestAccountUpdatesMulti),
      x if x == OutgoingMessageType::CancelAccountUpdatesMulti as i32 => Ok(OutgoingMessageType::CancelAccountUpdatesMulti),
      x if x == OutgoingMessageType::RequestSecDefOptParams as i32 => Ok(OutgoingMessageType::RequestSecDefOptParams),
      x if x == OutgoingMessageType::RequestSoftDollarTiers as i32 => Ok(OutgoingMessageType::RequestSoftDollarTiers),
      x if x == OutgoingMessageType::RequestFamilyCodes as i32 => Ok(OutgoingMessageType::RequestFamilyCodes),
      x if x == OutgoingMessageType::RequestMatchingSymbols as i32 => Ok(OutgoingMessageType::RequestMatchingSymbols),
      x if x == OutgoingMessageType::RequestMktDepthExchanges as i32 => Ok(OutgoingMessageType::RequestMktDepthExchanges),
      x if x == OutgoingMessageType::RequestSmartComponents as i32 => Ok(OutgoingMessageType::RequestSmartComponents),
      x if x == OutgoingMessageType::RequestNewsArticle as i32 => Ok(OutgoingMessageType::RequestNewsArticle),
      x if x == OutgoingMessageType::RequestNewsProviders as i32 => Ok(OutgoingMessageType::RequestNewsProviders),
      x if x == OutgoingMessageType::RequestHistoricalNews as i32 => Ok(OutgoingMessageType::RequestHistoricalNews),
      x if x == OutgoingMessageType::RequestHeadTimestamp as i32 => Ok(OutgoingMessageType::RequestHeadTimestamp),
      x if x == OutgoingMessageType::RequestHistogramData as i32 => Ok(OutgoingMessageType::RequestHistogramData),
      x if x == OutgoingMessageType::CancelHistogramData as i32 => Ok(OutgoingMessageType::CancelHistogramData),
      x if x == OutgoingMessageType::CancelHeadTimestamp as i32 => Ok(OutgoingMessageType::CancelHeadTimestamp),
      x if x == OutgoingMessageType::RequestMarketRule as i32 => Ok(OutgoingMessageType::RequestMarketRule),
      x if x == OutgoingMessageType::RequestPnL as i32 => Ok(OutgoingMessageType::RequestPnL),
      x if x == OutgoingMessageType::CancelPnL as i32 => Ok(OutgoingMessageType::CancelPnL),
      x if x == OutgoingMessageType::RequestPnLSingle as i32 => Ok(OutgoingMessageType::RequestPnLSingle),
      x if x == OutgoingMessageType::CancelPnLSingle as i32 => Ok(OutgoingMessageType::CancelPnLSingle),
      x if x == OutgoingMessageType::RequestHistoricalTicks as i32 => Ok(OutgoingMessageType::RequestHistoricalTicks),
      x if x == OutgoingMessageType::RequestTickByTickData as i32 => Ok(OutgoingMessageType::RequestTickByTickData),
      x if x == OutgoingMessageType::CancelTickByTickData as i32 => Ok(OutgoingMessageType::CancelTickByTickData),
      x if x == OutgoingMessageType::RequestCompletedOrders as i32 => Ok(OutgoingMessageType::RequestCompletedOrders),
      x if x == OutgoingMessageType::RequestWshMetaData as i32 => Ok(OutgoingMessageType::RequestWshMetaData),
      x if x == OutgoingMessageType::CancelWshMetaData as i32 => Ok(OutgoingMessageType::CancelWshMetaData),
      x if x == OutgoingMessageType::RequestWshEventData as i32 => Ok(OutgoingMessageType::RequestWshEventData),
      x if x == OutgoingMessageType::CancelWshEventData as i32 => Ok(OutgoingMessageType::CancelWshEventData),
      x if x == OutgoingMessageType::RequestUserInfo as i32 => Ok(OutgoingMessageType::RequestUserInfo),
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
    let mut buffer = Vec::new();
    let mut cursor = Cursor::new(buffer);
    self.write_int_to_cursor(&mut cursor, msg_type)?;
    Ok(cursor)
  }

  fn finish_encoding(&self, cursor: Cursor<Vec<u8>>) -> Vec<u8> {
    cursor.into_inner()
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

  fn write_optional_int_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, val: Option<i32>) -> Result<(), IBKRError> {
    self.write_int_to_cursor(cursor, val.unwrap_or(0))
  }

  fn write_optional_i64_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, val: Option<i64>) -> Result<(), IBKRError> {
    match val {
      Some(v) => self.write_str_to_cursor(cursor, &v.to_string()),
      None => self.write_str_to_cursor(cursor, "0"), // Or empty string ""? Check TWS behavior for optional longs
    }
  }


  fn write_double_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, val: f64) -> Result<(), IBKRError> {
    if val.is_nan() {
      log::warn!("Attempting to encode NaN double value. Sending 0.0.");
      self.write_str_to_cursor(cursor, "0.0")
    } else if val.is_infinite() {
      // TWS uses Double.MAX_VALUE for infinity in many cases
      warn!("Attempting to encode infinite double value. Sending MAX_VALUE string.");
      self.write_double_max_to_cursor(cursor, None) // Send MAX_VALUE representation
    } else {
      self.write_str_to_cursor(cursor, &val.to_string())
    }
  }

  fn write_optional_double_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, val: Option<f64>) -> Result<(), IBKRError> {
    self.write_double_to_cursor(cursor, val.unwrap_or(0.0))
  }

  fn write_double_max_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, val: Option<f64>) -> Result<(), IBKRError> {
    const UNSET_STR: &str = ""; // String representation of f64::MAX
    match val {
      Some(value) if value.is_finite() && value != f64::MAX => {
        self.write_double_to_cursor(cursor, value) // Use regular double writing for valid numbers
      }
      _ => { // Handles None, MAX, INFINITY, -INFINITY
        trace!("Encoding optional double as empty string.");
        self.write_str_to_cursor(cursor, UNSET_STR)
      }
    }
  }

  fn write_optional_double_max_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, val: Option<f64>) -> Result<(), IBKRError> {
    self.write_double_max_to_cursor(cursor, val) // Logic is the same
  }


  fn write_bool_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, val: bool) -> Result<(), IBKRError> {
    self.write_int_to_cursor(cursor, if val { 1 } else { 0 })
  }

  fn write_optional_bool_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, val: Option<bool>) -> Result<(), IBKRError> {
    self.write_bool_to_cursor(cursor, val.unwrap_or(false))
  }

  // Helper for TagValue lists
  fn write_tag_value_list(&self, cursor: &mut Cursor<Vec<u8>>, list: &[(String, String)]) -> Result<(), IBKRError> {
    self.write_int_to_cursor(cursor, list.len() as i32)?;
    for (tag, value) in list {
      self.write_str_to_cursor(cursor, tag)?;
      self.write_str_to_cursor(cursor, value)?;
    }
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
  pub fn encode_request_ids(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request IDs message");
    let mut cursor = self.start_encoding(OutgoingMessageType::RequestIds as i32)?;
    self.write_int_to_cursor(&mut cursor, 1)?; // Version
    Ok(self.finish_encoding(cursor))
  }

  pub fn encode_request_account_summary(
    &self,
    req_id: i32,
    group: &str,
    tags: &str,
  ) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request account summary message: ReqID={}, Group={}, Tags={}", req_id, group, tags);
    let mut cursor = self.start_encoding(OutgoingMessageType::RequestAccountSummary as i32)?;
    self.write_int_to_cursor(&mut cursor, 1)?; // Version
    self.write_int_to_cursor(&mut cursor, req_id)?;
    self.write_str_to_cursor(&mut cursor, group)?;
    self.write_str_to_cursor(&mut cursor, tags)?;
    Ok(self.finish_encoding(cursor))
  }

  pub fn encode_request_executions(&self, req_id: i32, filter: &ExecutionFilter) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request executions: ReqID={}, Filter={:?}", req_id, filter);
    let mut cursor = self.start_encoding(OutgoingMessageType::RequestExecutions as i32)?;
    let version = 3; // Version supporting filter fields

    self.write_int_to_cursor(&mut cursor, version)?;
    // Version 2 field (reqId) is only present if version >= 3 in this specific message
    if version >= 3 {
      self.write_int_to_cursor(&mut cursor, req_id)?;
    }

    // Write filter fields (version 3)
    // Use defaults (0 or "") if Option is None, as per TWS API conventions
    self.write_int_to_cursor(&mut cursor, filter.client_id.unwrap_or(0))?;
    self.write_str_to_cursor(&mut cursor, filter.acct_code.as_deref().unwrap_or(""))?;
    // Time format: "yyyymmdd hh:mm:ss" (TWS docs mention single space usually)
    self.write_str_to_cursor(&mut cursor, filter.time.as_deref().unwrap_or(""))?;
    self.write_str_to_cursor(&mut cursor, filter.symbol.as_deref().unwrap_or(""))?;
    self.write_str_to_cursor(&mut cursor, filter.sec_type.as_deref().unwrap_or(""))?;
    self.write_str_to_cursor(&mut cursor, filter.exchange.as_deref().unwrap_or(""))?;
    self.write_str_to_cursor(&mut cursor, filter.side.as_deref().unwrap_or(""))?;

    Ok(self.finish_encoding(cursor))
  }

  pub fn encode_request_positions(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request positions message");
    let mut cursor = self.start_encoding(OutgoingMessageType::RequestPositions as i32)?;
    self.write_int_to_cursor(&mut cursor, 1)?; // Version
    Ok(self.finish_encoding(cursor))
  }

  // --- cancel account summary / positions ---
  pub fn encode_cancel_account_summary(&self, req_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel account summary: ReqID={}", req_id);
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelAccountSummary as i32)?;
    self.write_int_to_cursor(&mut cursor, 1)?; // version
    self.write_int_to_cursor(&mut cursor, req_id)?;
    Ok(self.finish_encoding(cursor))
  }

  pub fn encode_cancel_positions(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel positions");
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelPositions as i32)?;
    self.write_int_to_cursor(&mut cursor, 1)?; // version
    Ok(self.finish_encoding(cursor))
  }

  // --- cancel order ---
  pub fn encode_cancel_order(&self, order_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel order message for order ID {}", order_id);
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelOrder as i32)?;
    // Version 1 (implicit by field count before MANUAL_ORDER_TIME)
    let version = 2;
    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, order_id)?;

    // Usually empty for cancels unless overriding a specific manual timestamp
    self.write_str_to_cursor(&mut cursor, "")?;

    Ok(self.finish_encoding(cursor))
  }


  // --- place order ---
  pub fn encode_place_order(&self, id: i32, contract: &Contract, request: &OrderRequest) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding place order message: ID={}, Contract={:?}, Request={:?}", id, contract, request);

    // --- Pre-Checks (Matching Java Implementation) ---
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
      // Use the -1 sentinel check like Java for this specific version cutoff if needed
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
          if leg.exempt_code != -1 { // Java checks against -1
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
    let mut cursor = self.start_encoding(OutgoingMessageType::PlaceOrder as i32)?;

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
      self.write_optional_double_max_to_cursor(&mut cursor, request.limit_price)?;
    }
    if self.server_version < min_server_ver::TRAILING_PERCENT {
      self.write_optional_double_to_cursor(&mut cursor, request.aux_price)?;
    } else {
      self.write_optional_double_max_to_cursor(&mut cursor, request.aux_price)?;
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
        self.write_optional_double_max_to_cursor(&mut cursor, *leg_price)?;
      }
    }

    // --- Smart Combo Routing Params ---
    if self.server_version >= min_server_ver::SMART_COMBO_ROUTING_PARAMS && contract.sec_type == SecType::Combo {
      self.write_tag_value_list(&mut cursor, &request.smart_combo_routing_params)?;
    }

    // --- Deprecated Shares Allocation ---
    if self.server_version >= 9 { self.write_str_to_cursor(&mut cursor, "")?; }

    // --- Discretionary Amount ---
    if self.server_version >= 10 { self.write_optional_double_to_cursor(&mut cursor, request.discretionary_amt)?; }

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
    if self.server_version >= min_server_ver::SSHORTX_OLD {
      // Handle -1 sentinel according to Java ref for SSHORTX_OLD
      let exempt_code_to_send = request.exempt_code.unwrap_or(0); // Default to 0 if None
      if exempt_code_to_send == -1 {
        self.write_int_to_cursor(&mut cursor, 0)?;
      } else {
        self.write_int_to_cursor(&mut cursor, exempt_code_to_send)?;
      }
    }

    // --- OCA Type, Rule 80A etc. ---
    if self.server_version >= 19 {
      self.write_optional_int_to_cursor(&mut cursor, request.oca_type)?;
      self.write_optional_str_to_cursor(&mut cursor, request.rule_80a.as_deref())?;
      self.write_optional_str_to_cursor(&mut cursor, request.settling_firm.as_deref())?;
      self.write_bool_to_cursor(&mut cursor, request.all_or_none)?;
      self.write_optional_int_to_cursor(&mut cursor, request.min_quantity)?;
      self.write_optional_double_max_to_cursor(&mut cursor, request.percent_offset)?;
      self.write_bool_to_cursor(&mut cursor, false)?; // deprecated eTradeOnly
      self.write_bool_to_cursor(&mut cursor, false)?; // deprecated firmQuoteOnly
      self.write_optional_double_max_to_cursor(&mut cursor, None)?; // deprecated nbboPriceCap
      self.write_optional_int_to_cursor(&mut cursor, request.auction_strategy)?;
      self.write_optional_double_max_to_cursor(&mut cursor, request.starting_price)?;
      self.write_optional_double_max_to_cursor(&mut cursor, request.stock_ref_price)?;
      self.write_optional_double_max_to_cursor(&mut cursor, request.delta)?;
      let lower = if self.server_version == 26 && request.order_type == OrderType::Volatility { None } else { request.stock_range_lower };
      let upper = if self.server_version == 26 && request.order_type == OrderType::Volatility { None } else { request.stock_range_upper };
      self.write_optional_double_max_to_cursor(&mut cursor, lower)?;
      self.write_optional_double_max_to_cursor(&mut cursor, upper)?;
    }

    // --- Override Percentage Constraints ---
    if self.server_version >= 22 {
      self.write_bool_to_cursor(&mut cursor, request.override_percentage_constraints)?;
    }

    // --- Volatility Orders ---
    if self.server_version >= 26 {
      self.write_optional_double_max_to_cursor(&mut cursor, request.volatility)?;
      self.write_optional_int_to_cursor(&mut cursor, request.volatility_type)?;
      if self.server_version < 28 {
        let is_delta_neutral_mkt = request.delta_neutral_order_type.as_deref().map_or(false, |s| s.eq_ignore_ascii_case("MKT"));
        self.write_bool_to_cursor(&mut cursor, is_delta_neutral_mkt)?;
      } else {
        self.write_optional_str_to_cursor(&mut cursor, request.delta_neutral_order_type.as_deref())?;
        self.write_optional_double_max_to_cursor(&mut cursor, request.delta_neutral_aux_price)?;

        if self.server_version >= min_server_ver::DELTA_NEUTRAL_CONID && !Encoder::is_empty(request.delta_neutral_order_type.as_deref()) {
          self.write_optional_int_to_cursor(&mut cursor, request.delta_neutral_con_id)?;
          self.write_optional_str_to_cursor(&mut cursor, request.delta_neutral_settling_firm.as_deref())?;
          self.write_optional_str_to_cursor(&mut cursor, request.delta_neutral_clearing_account.as_deref())?;
          self.write_optional_str_to_cursor(&mut cursor, request.delta_neutral_clearing_intent.as_deref())?;
        }
        if self.server_version >= min_server_ver::DELTA_NEUTRAL_OPEN_CLOSE && !Encoder::is_empty(request.delta_neutral_order_type.as_deref()) {
          self.write_optional_str_to_cursor(&mut cursor, request.delta_neutral_open_close.as_deref())?;
          self.write_bool_to_cursor(&mut cursor, request.delta_neutral_short_sale)?;
          self.write_optional_int_to_cursor(&mut cursor, request.delta_neutral_short_sale_slot)?;
          self.write_optional_str_to_cursor(&mut cursor, request.delta_neutral_designated_location.as_deref())?;
        }
      }
      self.write_bool_to_cursor(&mut cursor, request.continuous_update.unwrap_or(0) != 0)?;
      if self.server_version == 26 {
        let lower = if request.order_type == OrderType::Volatility { request.stock_range_lower } else { None };
        let upper = if request.order_type == OrderType::Volatility { request.stock_range_upper } else { None };
        self.write_optional_double_max_to_cursor(&mut cursor, lower)?;
        self.write_optional_double_max_to_cursor(&mut cursor, upper)?;
      }
      self.write_optional_int_to_cursor(&mut cursor, request.reference_price_type)?;
    }

    // --- Trail Stop Price ---
    if self.server_version >= 30 {
      self.write_optional_double_max_to_cursor(&mut cursor, request.trailing_stop_price)?;
    }

    // --- Trailing Percent ---
    if self.server_version >= min_server_ver::TRAILING_PERCENT {
      self.write_optional_double_max_to_cursor(&mut cursor, request.trailing_percent)?;
    }

    // --- Scale Orders ---
    if self.server_version >= min_server_ver::SCALE_ORDERS {
      if self.server_version >= min_server_ver::SCALE_ORDERS2 {
        self.write_optional_int_to_cursor(&mut cursor, request.scale_init_level_size)?;
        self.write_optional_int_to_cursor(&mut cursor, request.scale_subs_level_size)?;
      } else {
        self.write_str_to_cursor(&mut cursor, "")?;
        self.write_optional_int_to_cursor(&mut cursor, request.scale_init_level_size)?;
      }
      self.write_optional_double_max_to_cursor(&mut cursor, request.scale_price_increment)?;
    }
    let scale_price_increment_valid = request.scale_price_increment.map_or(false, |p| p > 0.0 && p != f64::MAX);
    if self.server_version >= min_server_ver::SCALE_ORDERS3 && scale_price_increment_valid {
      self.write_optional_double_max_to_cursor(&mut cursor, request.scale_price_adjust_value)?;
      self.write_optional_int_to_cursor(&mut cursor, request.scale_price_adjust_interval)?;
      self.write_optional_double_max_to_cursor(&mut cursor, request.scale_profit_offset)?;
      self.write_bool_to_cursor(&mut cursor, request.scale_auto_reset)?;
      self.write_optional_int_to_cursor(&mut cursor, request.scale_init_position)?;
      self.write_optional_int_to_cursor(&mut cursor, request.scale_init_fill_qty)?;
      self.write_bool_to_cursor(&mut cursor, request.scale_random_percent)?;
    }

    // --- Scale Table ---
    if self.server_version >= min_server_ver::SCALE_TABLE {
      self.write_optional_str_to_cursor(&mut cursor, request.scale_table.as_deref())?;
      // Format DateTime fields
      let start_time_str = self.format_datetime_tws(request.active_start_time, "%Y%m%d %H:%M:%S", Some(" UTC"));
      let stop_time_str = self.format_datetime_tws(request.active_stop_time, "%Y%m%d %H:%M:%S", Some(" UTC"));
      self.write_str_to_cursor(&mut cursor, &start_time_str)?;
      self.write_str_to_cursor(&mut cursor, &stop_time_str)?;
    }

    // --- Hedge Orders ---
    if self.server_version >= min_server_ver::HEDGE_ORDERS {
      self.write_optional_str_to_cursor(&mut cursor, request.hedge_type.as_deref())?;
      if !Encoder::is_empty(request.hedge_type.as_deref()) {
        self.write_optional_str_to_cursor(&mut cursor, request.hedge_param.as_deref())?;
      }
    }

    // --- Opt Out Smart Routing ---
    if self.server_version >= min_server_ver::OPT_OUT_SMART_ROUTING {
      self.write_bool_to_cursor(&mut cursor, request.opt_out_smart_routing)?;
    }

    // --- Clearing Params ---
    if self.server_version >= min_server_ver::PTA_ORDERS {
      self.write_optional_str_to_cursor(&mut cursor, request.clearing_account.as_deref())?;
      self.write_optional_str_to_cursor(&mut cursor, request.clearing_intent.as_deref())?;
    }

    // --- Not Held ---
    if self.server_version >= min_server_ver::NOT_HELD {
      self.write_bool_to_cursor(&mut cursor, request.not_held)?;
    }

    // --- Contract Delta Neutral ---
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

    // --- Algo Orders ---
    if self.server_version >= min_server_ver::ALGO_ORDERS {
      self.write_optional_str_to_cursor(&mut cursor, request.algo_strategy.as_deref())?;
      if !Encoder::is_empty(request.algo_strategy.as_deref()) {
        self.write_tag_value_list(&mut cursor, &request.algo_params)?;
      }
    }
    if self.server_version >= min_server_ver::ALGO_ID {
      self.write_optional_str_to_cursor(&mut cursor, request.algo_id.as_deref())?;
    }

    // --- What If Flag ---
    if self.server_version >= min_server_ver::WHAT_IF_ORDERS {
      self.write_bool_to_cursor(&mut cursor, request.what_if)?;
    }

    // --- Order Misc Options ---
    if self.server_version >= min_server_ver::LINKING {
      let misc_options_str = request.order_misc_options
        .iter()
        .map(|(key, value)| format!("{key}={value}")) // Format each pair
        .collect::<Vec<String>>() // Collect into a vector of strings
        .join(";"); // Join them with semicolons

      // Write the single resulting string (will add null terminator)
      self.write_str_to_cursor(&mut cursor, &misc_options_str)?;
    }

    // --- Solicited Flag ---
    if self.server_version >= min_server_ver::ORDER_SOLICITED {
      self.write_bool_to_cursor(&mut cursor, request.solicited)?;
    }

    // --- Randomize Size/Price Flags ---
    if self.server_version >= min_server_ver::RANDOMIZE_SIZE_AND_PRICE {
      self.write_bool_to_cursor(&mut cursor, request.randomize_size)?;
      self.write_bool_to_cursor(&mut cursor, request.randomize_price)?;
    }

    // --- Pegged To Benchmark Orders & Conditions ---
    if self.server_version >= min_server_ver::PEGGED_TO_BENCHMARK {
      let is_peg_bench = matches!(request.order_type,
                                  OrderType::PeggedToBenchmark | OrderType::PeggedBest | OrderType::PeggedPrimary
      );
      if is_peg_bench {
        self.write_optional_int_to_cursor(&mut cursor, request.reference_contract_id)?;
        self.write_bool_to_cursor(&mut cursor, request.is_pegged_change_amount_decrease)?;
        self.write_optional_double_to_cursor(&mut cursor, request.pegged_change_amount)?;
        self.write_optional_double_to_cursor(&mut cursor, request.reference_change_amount)?;
        self.write_optional_str_to_cursor(&mut cursor, request.reference_exchange_id.as_deref())?;
      }

      // Conditions (Stubbed)
      self.write_int_to_cursor(&mut cursor, request.conditions.len() as i32)?;
      if !request.conditions.is_empty() {
        warn!("Order condition encoding is not fully implemented. Sending count only.");
        // TODO: Implement full condition encoding
        self.write_bool_to_cursor(&mut cursor, request.conditions_ignore_rth)?;
        self.write_bool_to_cursor(&mut cursor, request.conditions_cancel_order)?;
      }

      // Adjusted Order fields
      let adj_ord_type_str = request.adjusted_order_type.map(|t| t.to_string());
      self.write_optional_str_to_cursor(&mut cursor, adj_ord_type_str.as_deref())?;
      self.write_optional_double_max_to_cursor(&mut cursor, request.trigger_price)?;
      self.write_optional_double_max_to_cursor(&mut cursor, request.lmt_price_offset)?;
      self.write_optional_double_max_to_cursor(&mut cursor, request.adjusted_stop_price)?;
      self.write_optional_double_max_to_cursor(&mut cursor, request.adjusted_stop_limit_price)?;
      self.write_optional_double_max_to_cursor(&mut cursor, request.adjusted_trailing_amount)?;
      self.write_optional_int_to_cursor(&mut cursor, request.adjustable_trailing_unit)?;
    }

    // --- Ext Operator ---
    if self.server_version >= min_server_ver::EXT_OPERATOR {
      self.write_optional_str_to_cursor(&mut cursor, request.ext_operator.as_deref())?;
    }

    // --- Soft Dollar Tier ---
    if self.server_version >= min_server_ver::SOFT_DOLLAR_TIER {
      let (name, value) = request.soft_dollar_tier.as_ref().map(|(n, v)| (n.as_str(), v.as_str())).unwrap_or(("", ""));
      self.write_str_to_cursor(&mut cursor, name)?;
      self.write_str_to_cursor(&mut cursor, value)?;
    }

    // --- Cash Quantity ---
    if self.server_version >= min_server_ver::CASH_QTY {
      self.write_optional_double_max_to_cursor(&mut cursor, request.cash_qty)?;
    }

    // --- MiFID Fields ---
    if self.server_version >= min_server_ver::DECISION_MAKER {
      self.write_optional_str_to_cursor(&mut cursor, request.mifid2_decision_maker.as_deref())?;
      self.write_optional_str_to_cursor(&mut cursor, request.mifid2_decision_algo.as_deref())?;
    }
    if self.server_version >= min_server_ver::MIFID_EXECUTION {
      self.write_optional_str_to_cursor(&mut cursor, request.mifid2_execution_trader.as_deref())?;
      self.write_optional_str_to_cursor(&mut cursor, request.mifid2_execution_algo.as_deref())?;
    }

    // --- Auto Price for Hedge ---
    if self.server_version >= min_server_ver::AUTO_PRICE_FOR_HEDGE {
      self.write_bool_to_cursor(&mut cursor, request.dont_use_auto_price_for_hedge)?;
    }

    // --- OMS Container ---
    if self.server_version >= min_server_ver::ORDER_CONTAINER {
      self.write_bool_to_cursor(&mut cursor, request.is_oms_container)?;
    }

    // --- Discretionary Up To Limit Price ---
    if self.server_version >= min_server_ver::D_PEG_ORDERS {
      self.write_bool_to_cursor(&mut cursor, request.discretionary_up_to_limit_price)?;
    }

    // --- Use Price Mgmt Algo ---
    if self.server_version >= min_server_ver::PRICE_MGMT_ALGO {
      self.write_optional_bool_to_cursor(&mut cursor, request.use_price_mgmt_algo)?;
    }

    // --- Duration ---
    if self.server_version >= min_server_ver::DURATION {
      self.write_optional_int_to_cursor(&mut cursor, request.duration)?;
    }

    // --- Post To ATS ---
    if self.server_version >= min_server_ver::POST_TO_ATS {
      self.write_optional_int_to_cursor(&mut cursor, request.post_to_ats)?;
    }

    // --- Auto Cancel Parent ---
    if self.server_version >= min_server_ver::AUTO_CANCEL_PARENT {
      self.write_bool_to_cursor(&mut cursor, request.auto_cancel_parent)?;
    }

    // --- Advanced Error Override ---
    if self.server_version >= min_server_ver::ADVANCED_ORDER_REJECT {
      self.write_optional_str_to_cursor(&mut cursor, request.advanced_error_override.as_deref())?;
    }

    // --- Manual Order Time ---
    if self.server_version >= min_server_ver::MANUAL_ORDER_TIME {
      // Format DateTime to "YYYYMMDD-HH:MM:SS" (No TZ suffix expected)
      let mot_str = self.format_datetime_tws(request.manual_order_time, "%Y%m%d-%H:%M:%S", None);
      self.write_str_to_cursor(&mut cursor, &mot_str)?;
    }

    // --- Peg Best/Mid Offsets ---
    if self.server_version >= min_server_ver::PEGBEST_PEGMID_OFFSETS {
      if contract.exchange.eq_ignore_ascii_case("IBKRATS") {
        self.write_optional_int_to_cursor(&mut cursor, request.min_trade_qty)?;
      }
      let is_peg_best = request.order_type == OrderType::PeggedBest;
      let is_peg_mid = request.order_type == OrderType::PeggedToMidpoint;
      let mut send_mid_offsets = false;

      if is_peg_best {
        self.write_optional_int_to_cursor(&mut cursor, request.min_compete_size)?;
        let offset_val = request.compete_against_best_offset;
        if offset_val == Some(f64::INFINITY) { // Check sentinel
          self.write_optional_double_max_to_cursor(&mut cursor, None)?; // Send "" for UpToMid
          send_mid_offsets = true;
        } else {
          self.write_optional_double_max_to_cursor(&mut cursor, offset_val)?;
        }
      } else if is_peg_mid {
        send_mid_offsets = true;
      }

      if send_mid_offsets {
        self.write_optional_double_max_to_cursor(&mut cursor, request.mid_offset_at_whole)?;
        self.write_optional_double_max_to_cursor(&mut cursor, request.mid_offset_at_half)?;
      }
    }

    // --- Customer Account ---
    if self.server_version >= min_server_ver::CUSTOMER_ACCOUNT {
      self.write_optional_str_to_cursor(&mut cursor, request.customer_account.as_deref())?;
    }

    // --- Professional Customer ---
    if self.server_version >= min_server_ver::PROFESSIONAL_CUSTOMER {
      self.write_bool_to_cursor(&mut cursor, request.professional_customer)?;
    }

    // --- RFQ Fields ---
    if self.server_version >= min_server_ver::RFQ_FIELDS {
      self.write_optional_str_to_cursor(&mut cursor, request.external_user_id.as_deref())?;
      self.write_optional_int_to_cursor(&mut cursor, request.manual_order_indicator)?;
    }


    Ok(self.finish_encoding(cursor))
  }


  // --- Helper to encode contract combo legs (Unchanged from previous version) ---
  fn encode_combo_legs(&self, cursor: &mut Cursor<Vec<u8>>, legs: &[ComboLeg]) -> Result<(), IBKRError> {
    // Check if server supports combo legs fields required in placeOrder (version 8+)
    // Version checks for SSHORT_COMBO_LEGS / SSHORTX_OLD are done inside the loop
    if self.server_version >= 8 {
      self.write_int_to_cursor(cursor, legs.len() as i32)?;
      for leg in legs {
        self.write_int_to_cursor(cursor, leg.con_id)?;
        self.write_int_to_cursor(cursor, leg.ratio)?;
        self.write_str_to_cursor(cursor, &leg.action)?; // BUY/SELL/SSHORT
        self.write_str_to_cursor(cursor, &leg.exchange)?;
        self.write_int_to_cursor(cursor, leg.open_close)?; // 0=Same, 1=Open, 2=Close, 3=Unknown

        if self.server_version >= min_server_ver::SSHORT_COMBO_LEGS {
          self.write_int_to_cursor(cursor, leg.short_sale_slot)?; // 0=Default, 1=Retail, 2=Inst
          // Only send designatedLocation if it's not empty
          self.write_optional_str_to_cursor(cursor, Some(leg.designated_location.as_str()).filter(|s| !s.is_empty()))?;
        }
        // Use SSHORTX_OLD version for placeOrder exemptCode based on Java reference
        if self.server_version >= min_server_ver::SSHORTX_OLD {
          // Java sends int, uses -1 sentinel. We send 0 if None or -1.
          let code_to_send = leg.exempt_code;
          if code_to_send == -1 {
            self.write_int_to_cursor(cursor, 0)?;
          } else {
            self.write_int_to_cursor(cursor, code_to_send)?;
          }
        }
      }
    } else if !legs.is_empty() {
      // Only warn if combo legs are present but server doesn't support them here
      warn!("Combo legs provided but server version {} does not support them in placeOrder message context", self.server_version);
      // Note: Unlike Java pre-check, don't error here, just don't send the fields.
    }
    Ok(())
  }


  // --- Helper to encode contract for placing orders ---
  fn encode_contract_for_order(&self, cursor: &mut Cursor<Vec<u8>>, contract: &Contract) -> Result<(), IBKRError> {
    // Fields included depend on server version and message context (PlaceOrder)
    self.write_optional_int_to_cursor(cursor, Some(contract.con_id))?; // Always send conId if available (usually 0 for new orders unless specified)
    self.write_str_to_cursor(cursor, &contract.symbol)?;
    self.write_str_to_cursor(cursor, &contract.sec_type.to_string())?;
    self.write_optional_str_to_cursor(cursor, contract.last_trade_date_or_contract_month.as_deref())?;
    self.write_optional_double_to_cursor(cursor, contract.strike)?; // 0.0 if None
    self.write_optional_str_to_cursor(cursor, contract.right.map(|r| r.to_string()).as_deref())?; // "" if None
    self.write_optional_str_to_cursor(cursor, contract.multiplier.as_deref())?; // Often needed for options/futures
    self.write_str_to_cursor(cursor, &contract.exchange)?;
    self.write_optional_str_to_cursor(cursor, contract.primary_exchange.as_deref())?; // Important for SMART routing ambiguity
    self.write_str_to_cursor(cursor, &contract.currency)?;
    self.write_optional_str_to_cursor(cursor, contract.local_symbol.as_deref())?;
    if self.server_version >= min_server_ver::TRADING_CLASS {
      self.write_optional_str_to_cursor(cursor, contract.trading_class.as_deref())?;
    }
    if self.server_version >= min_server_ver::SEC_ID_TYPE {
      self.write_optional_str_to_cursor(cursor, contract.sec_id_type.as_ref().map(|t| t.to_string()).as_deref())?;
      self.write_optional_str_to_cursor(cursor, contract.sec_id.as_deref())?;
    }

    // Encode combo legs if it's a combo contract
    if contract.sec_type == SecType::Combo {
      self.encode_combo_legs(cursor, &contract.combo_legs)?;
    }

    // Encode delta neutral underlying if present (part of the contract for placing orders)
    // Note: The delta/price fields are sent later in the Order part based on version
    // This part just sends the DN contract identification fields.
    if self.server_version >= min_server_ver::DELTA_NEUTRAL_CONID {
      if let Some(dn) = &contract.delta_neutral_contract {
        if self.server_version >= min_server_ver::PLACE_ORDER_CONID { // Check if DN fields belong here
          // DN conId, delta, price might be sent later in the order part
        } else {
          // Older versions might expect DN info here? Check docs.
        }
      }
    }

    Ok(())
  }

  pub fn encode_request_market_data(&self, req_id: i32, contract: &Contract, generic_tick_list: &str, snapshot: bool, regulatory_snapshot: bool /* Add mkt data options */) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request market data message for contract {}: ReqID={}", contract.symbol, req_id);
    let mut cursor = self.start_encoding(OutgoingMessageType::RequestMarketData as i32)?;

    // Determine version based on server capabilities and features used
    let mut version = 9; // Base version supporting essential fields
    if self.server_version >= min_server_ver::DELTA_NEUTRAL_CONID { version = 10; }
    if self.server_version >= min_server_ver::REQ_MKT_DATA_CONID && !generic_tick_list.is_empty() { version = 10; } // Generic ticks require v10+
    if self.server_version >= min_server_ver::REQ_SMART_COMPONENTS { version = 10; } // Reg snapshot needs v10+
    if self.server_version >= min_server_ver::LINKING { version = 11; } // Market data options need v11+
    // Add more checks...

    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;

    // Encode contract fields (use helper or specific fields)
    // Version checks are crucial here based on the *RequestMarketData* message structure evolution
    if version >= 3 {
      self.write_optional_int_to_cursor(&mut cursor, Some(contract.con_id))?;
    }
    self.write_str_to_cursor(&mut cursor, &contract.symbol)?;
    self.write_str_to_cursor(&mut cursor, &contract.sec_type.to_string())?;
    if version >= 2 {
      self.write_optional_str_to_cursor(&mut cursor, contract.last_trade_date_or_contract_month.as_deref())?;
      self.write_optional_double_to_cursor(&mut cursor, contract.strike)?;
      self.write_optional_str_to_cursor(&mut cursor, contract.right.map(|r| r.to_string()).as_deref())?;
    }
    if version >= 8 {
      self.write_optional_str_to_cursor(&mut cursor, contract.multiplier.as_deref())?;
    }
    self.write_str_to_cursor(&mut cursor, &contract.exchange)?;
    if version >= 2 {
      self.write_optional_str_to_cursor(&mut cursor, contract.primary_exchange.as_deref())?;
    }
    self.write_str_to_cursor(&mut cursor, &contract.currency)?;
    if version >= 2 {
      self.write_optional_str_to_cursor(&mut cursor, contract.local_symbol.as_deref())?;
    }
    if version >= 8 {
      self.write_optional_str_to_cursor(&mut cursor, contract.trading_class.as_deref())?;
    }
    if version >= 9 {
      if contract.sec_type == SecType::Combo {
        self.encode_combo_legs(&mut cursor, &contract.combo_legs)?;
      }
    }

    // Encode delta neutral if applicable (version check applied inside helper based on `version` variable)
    if version >= 10 {
      if let Some(dn) = &contract.delta_neutral_contract {
        self.write_bool_to_cursor(&mut cursor, true)?;
        self.write_int_to_cursor(&mut cursor, dn.con_id)?;
        // Delta/price for DN contract are sent here in reqMktData unlike PlaceOrder
        self.write_double_to_cursor(&mut cursor, dn.delta)?;
        self.write_double_to_cursor(&mut cursor, dn.price)?;
      } else {
        self.write_bool_to_cursor(&mut cursor, false)?;
      }
    }

    // Generic tick list (comma-separated IDs)
    if version >= 6 { // Check version for generic tick list field
      self.write_str_to_cursor(&mut cursor, generic_tick_list)?;
    }

    // Snapshot
    if version >= 3 { // Check version for snapshot field
      self.write_bool_to_cursor(&mut cursor, snapshot)?;
    }

    // Regulatory snapshot
    if version >= 10 { // Check version for regulatory snapshot
      self.write_bool_to_cursor(&mut cursor, regulatory_snapshot)?;
    }

    // Market data options (TagValue list)
    if version >= 11 { // Check version for market data options
      // Placeholder for actual options - assumed empty for now
      self.write_tag_value_list(&mut cursor, &[])?; // Pass actual options Vec here
    }

    Ok(self.finish_encoding(cursor))
  }


  pub fn encode_cancel_market_data(&self, req_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel market data: ReqID={}", req_id);
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelMarketData as i32)?;
    self.write_int_to_cursor(&mut cursor, 1)?; // Version
    self.write_int_to_cursor(&mut cursor, req_id)?;
    Ok(self.finish_encoding(cursor))
  }

  pub fn encode_request_all_open_orders(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request all open orders");
    let mut cursor = self.start_encoding(OutgoingMessageType::RequestAllOpenOrders as i32)?;
    self.write_int_to_cursor(&mut cursor, 1)?; // Version
    Ok(self.finish_encoding(cursor))
  }

  pub fn encode_request_open_orders(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request open orders");
    let mut cursor = self.start_encoding(OutgoingMessageType::RequestOpenOrders as i32)?;
    self.write_int_to_cursor(&mut cursor, 1)?; // Version
    Ok(self.finish_encoding(cursor))
  }

  pub fn encode_request_account_data(&self, subscribe: bool, account_code: &str) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request account data: Subscribe={}, Account={}", subscribe, account_code);
    let mut cursor = self.start_encoding(OutgoingMessageType::RequestAccountData as i32)?;
    self.write_int_to_cursor(&mut cursor, 2)?; // Version 2 supports account code
    self.write_bool_to_cursor(&mut cursor, subscribe)?;
    self.write_str_to_cursor(&mut cursor, account_code)?; // Can be empty for default
    Ok(self.finish_encoding(cursor))
  }
  pub fn encode_request_contract_data(&self, req_id: i32, contract: &Contract) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request contract data: ReqID={}", req_id);
    let mut cursor = self.start_encoding(OutgoingMessageType::RequestContractData as i32)?;
    // Determine version based on fields used (e.g., SecIdType, IssuerId)
    let version = if self.server_version >= min_server_ver::BOND_ISSUERID { 8 }
    else if self.server_version >= min_server_ver::SEC_ID_TYPE { 7 }
    else { 6 }; // Base version
    self.write_int_to_cursor(&mut cursor, version)?;
    if version >= 3 {
      self.write_int_to_cursor(&mut cursor, req_id)?;
    }
    // Write contract fields relevant for lookup
    self.write_optional_int_to_cursor(&mut cursor, Some(contract.con_id))?;
    self.write_str_to_cursor(&mut cursor, &contract.symbol)?;
    self.write_str_to_cursor(&mut cursor, &contract.sec_type.to_string())?;
    self.write_optional_str_to_cursor(&mut cursor, contract.last_trade_date_or_contract_month.as_deref())?;
    self.write_optional_double_to_cursor(&mut cursor, contract.strike)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.right.map(|r| r.to_string()).as_deref())?;
    if version >= 2 {
      self.write_optional_str_to_cursor(&mut cursor, contract.multiplier.as_deref())?;
    }
    self.write_str_to_cursor(&mut cursor, &contract.exchange)?;
    self.write_str_to_cursor(&mut cursor, &contract.currency)?;
    self.write_optional_str_to_cursor(&mut cursor, contract.local_symbol.as_deref())?;
    if version >= 4 {
      self.write_optional_str_to_cursor(&mut cursor, contract.trading_class.as_deref())?;
    }
    if version >= 5 {
      self.write_bool_to_cursor(&mut cursor, contract.include_expired)?; // Add include_expired to Contract
    }
    if version >= 7 {
      self.write_optional_str_to_cursor(&mut cursor, contract.sec_id_type.as_ref().map(|t| t.to_string()).as_deref())?;
      self.write_optional_str_to_cursor(&mut cursor, contract.sec_id.as_deref())?;
    }
    if version >= 8 {
      self.write_optional_str_to_cursor(&mut cursor, contract.issuer_id.as_deref())?;
    }
    Ok(self.finish_encoding(cursor))
  }

  pub fn encode_request_historical_data(&self, req_id: i32, contract: &Contract, end_date_time: Option<DateTime<Utc>>, duration_str: &str, bar_size_setting: &str, what_to_show: &str, use_rth: bool, format_date: i32, keep_up_to_date: bool /* Add chart options */) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request historical data: ReqID={}", req_id);
    let mut cursor = self.start_encoding(OutgoingMessageType::RequestHistoricalData as i32)?;
    // Version determination is complex based on keepUpToDate, chartOptions, etc.
    let version = 6; // Example base version

    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;

    // Contract fields (similar to reqContractData, check specific reqHistoricalData needs)
    if self.server_version >= min_server_ver::TRADING_CLASS { // Example check
      self.write_optional_int_to_cursor(&mut cursor, Some(contract.con_id))?;
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
    if version >= 2 {
      self.write_bool_to_cursor(&mut cursor, contract.include_expired)?;
    }

    // Historical Data Request Parameters
    self.write_str_to_cursor(&mut cursor, &self.format_datetime_tws(end_date_time, "%Y%m%d-%H:%M:%S", Some(" UTC")))?;
    if version >= 3 {
      self.write_str_to_cursor(&mut cursor, bar_size_setting)?; // e.g., "1 day", "30 mins"
    }
    self.write_str_to_cursor(&mut cursor, duration_str)?; // e.g., "1 Y", "3 M", "600 S"
    self.write_bool_to_cursor(&mut cursor, use_rth)?; // 1=RTH only, 0=All data
    self.write_str_to_cursor(&mut cursor, what_to_show)?; // e.g., "TRADES", "MIDPOINT", "BID", "ASK"
    if version >= 1 {
      self.write_int_to_cursor(&mut cursor, format_date)?; // 1=yyyyMMdd{ }hh:mm:ss, 2=epoch seconds
    }
    if contract.sec_type == SecType::Combo {
      return Err(IBKRError::Unsupported("Combo not implemented yet".to_string()));
      // self.write_bool_to_cursor(&mut cursor, contract.use_combo_data)?; // Add use_combo_data to Contract
    }
    if version >= 5 {
      self.write_bool_to_cursor(&mut cursor, keep_up_to_date)?;
    }
    if version >= 6 {
      // Placeholder for chart options (TagValue list)
      self.write_tag_value_list(&mut cursor, &[])?; // Pass actual chart options here
    }

    Ok(self.finish_encoding(cursor))
  }

  pub fn encode_request_managed_accounts(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request managed accounts");
    let mut cursor = self.start_encoding(OutgoingMessageType::RequestManagedAccts as i32)?;
    self.write_int_to_cursor(&mut cursor, 1)?; // Version
    Ok(self.finish_encoding(cursor))
  }

  pub fn encode_request_current_time(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request current time message");
    let mut cursor = self.start_encoding(OutgoingMessageType::RequestCurrentTime as i32)?;
    self.write_int_to_cursor(&mut cursor, 1)?; // Version field
    Ok(self.finish_encoding(cursor))
  }

} // end impl Encoder
