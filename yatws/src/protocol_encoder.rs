// yatws/src/protocol_encoder.rs
// Encoder for the TWS API protocol messages

use std::io::Cursor;
use crate::base::IBKRError;
use crate::contract::{Contract, ContractDetails, OptionRight, SecType, ComboLeg};
use crate::order::{Order, OrderRequest, TimeInForce, OrderType, OrderSide};
use crate::account::AccountInfo;
use crate::min_server_ver::min_server_ver;
use chrono::{DateTime, Utc};
use log::{debug, trace};
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
    // Removed: next_valid_id initialization
  }

  // --- Keep set_server_version if needed, but usually set at construction ---
  // pub fn set_server_version(&mut self, version: i32) { ... }

  // --- Remove ID management ---
  // pub fn set_next_valid_id(&mut self, id: i32) { ... }
  // pub fn get_next_valid_id(&mut self) -> i32 { ... }


  // --- Modify encoding methods to return Vec<u8> ---

  // Helper to start encoding into a buffer
  fn start_encoding(&self, msg_type: i32) -> Result<Cursor<Vec<u8>>, IBKRError> {
    let mut buffer = Vec::new();
    let mut cursor = Cursor::new(buffer);
    self.write_int_to_cursor(&mut cursor, msg_type)?;
    Ok(cursor)
  }

  // Helper to finish encoding
  fn finish_encoding(&self, cursor: Cursor<Vec<u8>>) -> Vec<u8> {
    cursor.into_inner()
  }

  // --- Modify write helpers to take a Cursor ---
  fn write_str_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, s: &str) -> Result<(), IBKRError> {
    trace!("Encoding string: {}", s);
    cursor.write_all(s.as_bytes()).map_err(|e| IBKRError::InternalError(format!("Buffer write failed: {}", e)))?;
    cursor.write_all(&[0]).map_err(|e| IBKRError::InternalError(format!("Buffer write failed: {}", e)))?;
    Ok(())
  }

  fn write_int_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, val: i32) -> Result<(), IBKRError> {
    self.write_str_to_cursor(cursor, &val.to_string())
  }

  fn write_double_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, val: f64) -> Result<(), IBKRError> {
    // Handle potential NaN/Infinity - TWS expects strings like "Infinity" or specific max values
    if val.is_nan() {
      // TWS usually doesn't accept NaN. Sending 0 or MAX might be safer depending on context.
      log::warn!("Attempting to encode NaN double value. Sending 0.0.");
      self.write_str_to_cursor(cursor, "0.0")
    } else if val.is_infinite() {
      // Encode infinity appropriately if needed, or use MAX_VALUE
      self.write_str_to_cursor(cursor, if val.is_sign_positive() { "Infinity" } else { "-Infinity" }) // Check TWS handling
      // Alternative: self.write_double_max_to_cursor(cursor, None)? // Send MAX_VALUE
    } else {
      self.write_str_to_cursor(cursor, &val.to_string())
    }
  }

  fn write_double_max_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, val: Option<f64>) -> Result<(), IBKRError> {
    // Use a representation for "max value" if val is None or f64::MAX
    // TWS often uses an empty string for optional doubles when MAX_VALUE is implied. Check specific field docs.
    // For simplicity here, let's encode None/MAX as MAX's string rep, but empty string might be better.
    const MAX_STR: &str = "1.7976931348623157E308"; // String representation of f64::MAX
    match val {
      Some(value) if value != f64::MAX && value != f64::INFINITY && value != f64::NEG_INFINITY => {
        self.write_double_to_cursor(cursor, value) // Use regular double writing for valid numbers
      }
      _ => self.write_str_to_cursor(cursor, MAX_STR), // Encode None/MAX/Infinity as MAX string
      // Alternative: self.write_str_to_cursor(cursor, ""), // Encode as empty string
    }
  }

  fn write_bool_to_cursor(&self, cursor: &mut Cursor<Vec<u8>>, val: bool) -> Result<(), IBKRError> {
    self.write_int_to_cursor(cursor, if val { 1 } else { 0 })
  }


  // --- Update encode methods ---
  // Example: encode_request_ids
  pub fn encode_request_ids(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request IDs message");
    let mut cursor = self.start_encoding(OutgoingMessageType::RequestIds as i32)?;
    self.write_int_to_cursor(&mut cursor, 1)?; // Version
    Ok(self.finish_encoding(cursor))
  }

  // Example: encode_request_account_summary
  pub fn encode_request_account_summary(
    &self,
    req_id: i32,
    group: &str,
    tags: &str,
  ) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request account summary message: ReqID={}, Group={}, Tags={}", req_id, group, tags);
    // Version 1 (set implicitly by field count)
    let mut cursor = self.start_encoding(OutgoingMessageType::RequestAccountSummary as i32)?;
    self.write_int_to_cursor(&mut cursor, 1)?; // Version
    self.write_int_to_cursor(&mut cursor, req_id)?;
    self.write_str_to_cursor(&mut cursor, group)?;
    self.write_str_to_cursor(&mut cursor, tags)?;
    Ok(self.finish_encoding(cursor))
  }

  // Example: encode_request_positions
  pub fn encode_request_positions(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request positions message");
    let mut cursor = self.start_encoding(OutgoingMessageType::RequestPositions as i32)?;
    self.write_int_to_cursor(&mut cursor, 1)?; // Version
    Ok(self.finish_encoding(cursor))
  }


  // --- Update ALL other encode_... methods similarly ---
  // They should call start_encoding, write fields to cursor, then finish_encoding

  // Example: encode_place_order needs modification
  pub fn encode_place_order(&self, id: i32, contract: &Contract, order: &Order) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding place order message for contract {}", contract.symbol);
    // ... (Perform server version checks as before) ...
    if self.server_version < min_server_ver::SCALE_ORDERS /* ... etc ... */ {
      // return Err(...)
    }

    let mut cursor = self.start_encoding(OutgoingMessageType::PlaceOrder as i32)?;

    // Version depends on server version - NOT sent as first field anymore for PlaceOrder
    let version = if self.server_version < min_server_ver::NOT_HELD { 27 } else { 45 };
    // Only send version if server < ORDER_CONTAINER
    if self.server_version < min_server_ver::ORDER_CONTAINER {
      self.write_int_to_cursor(&mut cursor, version)?;
    }

    self.write_int_to_cursor(&mut cursor, id)?;

    // --- Send contract fields using helpers ---
    self.encode_contract_fields(&mut cursor, contract)?;

    // --- Send order fields ---
    self.encode_order_fields(&mut cursor, &order.request, contract)?; // Pass contract for context if needed


    // ... Add logic for remaining fields based on server version and OrderRequest content ...
    // This part requires careful porting of the original logic to use the cursor writers.

    Ok(self.finish_encoding(cursor))
  }

  // --- Helper to encode contract fields ---
  fn encode_contract_fields(&self, cursor: &mut Cursor<Vec<u8>>, contract: &Contract) -> Result<(), IBKRError> {
    if self.server_version >= min_server_ver::PLACE_ORDER_CONID ||
      self.server_version >= min_server_ver::REQ_MKT_DATA_CONID || // Check for other relevant messages
      self.server_version >= min_server_ver::CONTRACT_CONID // General check
    {
      self.write_int_to_cursor(cursor, contract.con_id)?;
    }
    self.write_str_to_cursor(cursor, &contract.symbol)?;
    self.write_str_to_cursor(cursor, &contract.sec_type.to_string())?;
    self.write_str_to_cursor(cursor, contract.last_trade_date_or_contract_month.as_deref().unwrap_or(""))?;
    self.write_double_to_cursor(cursor, contract.strike.unwrap_or(0.0))?; // Send 0.0 for None strike
    self.write_str_to_cursor(cursor, &contract.right.map_or_else(String::new, |r| r.to_string()))?; // Empty if None
    if self.server_version >= min_server_ver::PLACE_ORDER_CONID { // Example version check for multiplier
      self.write_str_to_cursor(cursor, contract.multiplier.as_deref().unwrap_or(""))?;
    }
    self.write_str_to_cursor(cursor, &contract.exchange)?;
    if self.server_version >= min_server_ver::REQ_MKT_DATA_CONID { // Example version check
      self.write_str_to_cursor(cursor, contract.primary_exchange.as_deref().unwrap_or(""))?;
    }
    self.write_str_to_cursor(cursor, &contract.currency)?;
    if self.server_version >= min_server_ver::REQ_MKT_DATA_CONID { // Example version check
      self.write_str_to_cursor(cursor, contract.local_symbol.as_deref().unwrap_or(""))?;
    }
    if self.server_version >= min_server_ver::TRADING_CLASS {
      self.write_str_to_cursor(cursor, contract.trading_class.as_deref().unwrap_or(""))?;
    }
    if self.server_version >= min_server_ver::SEC_ID_TYPE {
      self.write_str_to_cursor(cursor, &contract.sec_id_type.as_ref().map_or_else(String::new, |t| t.to_string()))?;
      self.write_str_to_cursor(cursor, contract.sec_id.as_deref().unwrap_or(""))?;
    }
    if self.server_version >= min_server_ver::BOND_ISSUERID {
      self.write_str_to_cursor(cursor, contract.issuer_id.as_deref().unwrap_or(""))?;
    }
    // ... other contract fields based on context and version ...
    Ok(())
  }

  // --- Helper to encode basic order fields ---
  fn encode_order_fields(&self, cursor: &mut Cursor<Vec<u8>>, request: &OrderRequest, contract: &Contract) -> Result<(), IBKRError> {
    self.write_str_to_cursor(cursor, &request.side.to_string())?;

    if self.server_version >= min_server_ver::FRACTIONAL_SIZE_SUPPORT {
      // Send quantity as string for fractional support
      self.write_str_to_cursor(cursor, &request.quantity.to_string())?;
    } else {
      // Send quantity as integer (potential truncation)
      self.write_int_to_cursor(cursor, request.quantity as i32)?;
    }

    self.write_str_to_cursor(cursor, &request.order_type.to_string())?;

    if self.server_version < min_server_ver::ORDER_COMBO_LEGS_PRICE {
      self.write_double_to_cursor(cursor, request.limit_price.unwrap_or(0.0))?;
    } else {
      self.write_double_max_to_cursor(cursor, request.limit_price)?;
    }

    if self.server_version < min_server_ver::TRAILING_PERCENT {
      self.write_double_to_cursor(cursor, request.aux_price.unwrap_or(0.0))?;
    } else {
      self.write_double_max_to_cursor(cursor, request.aux_price)?;
    }

    // Extended order fields
    self.write_str_to_cursor(cursor, &request.time_in_force.to_string())?;
    self.write_str_to_cursor(cursor, request.oca_group.as_deref().unwrap_or(""))?;
    self.write_str_to_cursor(cursor, request.account.as_deref().unwrap_or(""))?;
    self.write_str_to_cursor(cursor, request.open_close.as_deref().unwrap_or("O"))?; // Default "O"
    self.write_int_to_cursor(cursor, request.origin)?; // 0=Customer, 1=Firm
    self.write_str_to_cursor(cursor, request.order_ref.as_deref().unwrap_or(""))?;
    self.write_bool_to_cursor(cursor, request.transmit)?; // Usually true

    if self.server_version >= min_server_ver::PLACE_ORDER_CONID { // Example version check
      self.write_int_to_cursor(cursor, request.parent_id.unwrap_or(0) as i32)?;
    }
    self.write_bool_to_cursor(cursor, request.block_order)?;
    self.write_bool_to_cursor(cursor, request.sweep_to_fill)?;
    self.write_int_to_cursor(cursor, request.display_size.unwrap_or(0))?; // 0 if None
    self.write_int_to_cursor(cursor, request.trigger_method.unwrap_or(0))?; // 0=Default
    self.write_bool_to_cursor(cursor, request.outside_rth)?;
    self.write_bool_to_cursor(cursor, request.hidden)?;

    // --- Conditional fields based on secType ---
    if contract.sec_type == SecType::Combo && self.server_version >= min_server_ver::ORDER_COMBO_LEGS_PRICE {
      if !contract.combo_legs.is_empty() {
        self.write_int_to_cursor(cursor, contract.combo_legs.len() as i32)?;
        for leg in &contract.combo_legs {
          self.write_double_max_to_cursor(cursor, leg.price)?; // Assuming ComboLeg has optional price
        }
      } else {
        self.write_int_to_cursor(cursor, 0)?;
      }
    }

    // --- Other fields based on versions and order type ---
    // Add checks and writes for:
    // - GoodAfterTime, GoodTillDate
    // - Rule80A, AllOrNone, MinQty, PercentOffset
    // - TrailStopPrice, TrailingPercent
    // - Financial Advisor fields (faGroup, faMethod, faPercentage)
    // - OpenClose, ShortSaleSlot, DesignatedLocation, ExemptCode
    // - DiscretionaryAmt, OptOutSmartRouting
    // - AuctionStrategy, StartingPrice, StockRefPrice, Delta (for BOX)
    // - StockRangeLower, StockRangeUpper
    // - Volatility, VolatilityType, ContinuousUpdate, ReferencePriceType, DeltaNeutralOrderType, etc.
    // - Scale order fields
    // - Algo fields
    // - Pegged order fields
    // - MiFID fields
    // - etc...
    // This requires careful mapping from OrderRequest to the TWS protocol based on server_version.

    Ok(())
  }


  // TODO: Add encode_cancel_order, encode_request_market_data, etc. following the new pattern

  pub fn encode_cancel_order(&self, order_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel order message for order ID {}", order_id);
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelOrder as i32)?;
    self.write_int_to_cursor(&mut cursor, 1)?; // Version
    self.write_int_to_cursor(&mut cursor, order_id)?;

    // Add manual order cancel time if server version supports it
    if self.server_version >= min_server_ver::MANUAL_ORDER_TIME {
      self.write_str_to_cursor(&mut cursor, "")?; // Empty manual time for cancel usually
    }

    Ok(self.finish_encoding(cursor))
  }

  // Needs more complex field logic based on versions
  pub fn encode_request_market_data(&self, req_id: i32, contract: &Contract, generic_tick_list: &str, snapshot: bool, regulatory_snapshot: bool /* Add mkt data options */) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding request market data message for contract {}", contract.symbol);
    let mut cursor = self.start_encoding(OutgoingMessageType::RequestMarketData as i32)?;
    // Determine version based on server capabilities
    let version = 11; // Example: Adjust based on max feature used
    self.write_int_to_cursor(&mut cursor, version)?;
    self.write_int_to_cursor(&mut cursor, req_id)?;

    // Encode contract fields (use helper or inline)
    self.encode_contract_fields(&mut cursor, contract)?;

    // Encode combo legs if applicable
    if contract.sec_type == SecType::Combo {
      self.encode_combo_legs(&mut cursor, &contract.combo_legs)?;
    }

    // Encode delta neutral if applicable
    if self.server_version >= min_server_ver::DELTA_NEUTRAL_CONID {
      if let Some(dn) = &contract.delta_neutral_contract {
        self.write_bool_to_cursor(&mut cursor, true)?;
        self.write_int_to_cursor(&mut cursor, dn.con_id)?;
        if self.server_version >= min_server_ver::DELTA_NEUTRAL_OPEN_CLOSE { // Assuming DELTA_NEUTRAL_OPEN_CLOSE implies sending delta/price here
          self.write_double_to_cursor(&mut cursor, dn.delta)?;
          self.write_double_to_cursor(&mut cursor, dn.price)?;
        }
      } else {
        self.write_bool_to_cursor(&mut cursor, false)?;
      }
    }

    // Generic tick list
    if self.server_version >= min_server_ver::REQ_MKT_DATA_CONID { // Example check
      self.write_str_to_cursor(&mut cursor, generic_tick_list)?;
    }

    // Snapshot
    if self.server_version >= min_server_ver::SNAPSHOT_MKT_DATA {
      self.write_bool_to_cursor(&mut cursor, snapshot)?;
    }

    // Regulatory snapshot
    if self.server_version >= min_server_ver::REQ_SMART_COMPONENTS {
      self.write_bool_to_cursor(&mut cursor, regulatory_snapshot)?;
    }

    // Market data options (TagValue list)
    if self.server_version >= min_server_ver::LINKING {
      // Placeholder for actual options - assumed empty for now
      self.write_int_to_cursor(&mut cursor, 0)?; // Number of options
    }

    Ok(self.finish_encoding(cursor))
  }

  // Helper for combo legs
  fn encode_combo_legs(&self, cursor: &mut Cursor<Vec<u8>>, legs: &[ComboLeg]) -> Result<(), IBKRError> {
    if self.server_version >= min_server_ver::ORDER_COMBO_LEGS_PRICE { // Check relevant version for combo legs
      self.write_int_to_cursor(cursor, legs.len() as i32)?;
      for leg in legs {
        self.write_int_to_cursor(cursor, leg.con_id)?;
        self.write_int_to_cursor(cursor, leg.ratio)?;
        self.write_str_to_cursor(cursor, &leg.action)?;
        self.write_str_to_cursor(cursor, &leg.exchange)?;
        self.write_int_to_cursor(cursor, leg.open_close)?;

        if self.server_version >= min_server_ver::SSHORT_COMBO_LEGS {
          self.write_int_to_cursor(cursor, leg.short_sale_slot)?; // 0 = Clearing default, 1 = Retail, 2 = Institution
          self.write_str_to_cursor(cursor, &leg.designated_location)?;
        }
        if self.server_version >= min_server_ver::SSHORTX { // SSHORTX implies exempt_code
          self.write_int_to_cursor(cursor, leg.exempt_code)?;
        }
      }
    }
    Ok(())
  }


  // --- Need placeholders or full implementations for other encode methods ---
  pub fn encode_cancel_market_data(&self, req_id: i32) -> Result<Vec<u8>, IBKRError> { /* ... */ Ok(vec![]) }
  pub fn encode_request_all_open_orders(&self) -> Result<Vec<u8>, IBKRError> { /* ... */ Ok(vec![]) }
  pub fn encode_request_open_orders(&self) -> Result<Vec<u8>, IBKRError> { /* ... */ Ok(vec![]) }
  pub fn encode_request_account_data(&self, subscribe: bool, account_code: &str) -> Result<Vec<u8>, IBKRError> { /* ... */ Ok(vec![]) }
  pub fn encode_request_contract_data(&self, req_id: i32, contract: &Contract) -> Result<Vec<u8>, IBKRError> { /* ... */ Ok(vec![]) }
  pub fn encode_request_historical_data(&self, req_id: i32, contract: &Contract, end_date_time: DateTime<Utc>, duration_str: &str, bar_size_setting: &str, what_to_show: &str, use_rth: bool, format_date: bool, keep_up_to_date: bool /* Add chart options */) -> Result<Vec<u8>, IBKRError> { /* ... */ Ok(vec![]) }
  pub fn encode_request_managed_accounts(&self) -> Result<Vec<u8>, IBKRError> { /* ... */ Ok(vec![]) }

  // Add cancel account summary
  pub fn encode_cancel_account_summary(&self, req_id: i32) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel account summary: ReqID={}", req_id);
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelAccountSummary as i32)?;
    self.write_int_to_cursor(&mut cursor, 1)?; // version
    self.write_int_to_cursor(&mut cursor, req_id)?;
    Ok(self.finish_encoding(cursor))
  }

  // Add cancel positions
  pub fn encode_cancel_positions(&self) -> Result<Vec<u8>, IBKRError> {
    debug!("Encoding cancel positions");
    let mut cursor = self.start_encoding(OutgoingMessageType::CancelPositions as i32)?;
    self.write_int_to_cursor(&mut cursor, 1)?; // version
    Ok(self.finish_encoding(cursor))
  }


} // end impl Encoder
