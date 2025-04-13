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
    // TWS API requires sending Double.MAX_VALUE (as a string) to represent "not set" or infinity for many optional double fields.
    const MAX_STR: &str = "1.7976931348623157E308"; // String representation of f64::MAX
    match val {
      Some(value) if value.is_finite() && value != f64::MAX => {
        self.write_double_to_cursor(cursor, value) // Use regular double writing for valid numbers
      }
      _ => { // Handles None, MAX, INFINITY, -INFINITY
        trace!("Encoding optional double as MAX_VALUE string.");
        self.write_str_to_cursor(cursor, MAX_STR)
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
  fn format_datetime_tws(&self, dt: Option<DateTime<Utc>>, include_time: bool) -> String {
    dt.map(|d| {
      if include_time {
        d.format("%Y%m%d %H:%M:%S").to_string() // Common format
        // d.format("%Y%m%d %H:%M:%S %Z").to_string() // If timezone needed (rarely)
      } else {
        d.format("%Y%m%d").to_string()
      }
    }).unwrap_or_default()
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
    debug!("Encoding place order message for contract {}: ID={}", contract.symbol, id);

    // --- Perform server version checks for features used ---
    // Example: Check if scale orders are used and if server supports them
    if (request.scale_init_level_size.is_some() || request.scale_price_increment.is_some())
      && self.server_version < min_server_ver::SCALE_ORDERS {
        return Err(IBKRError::Unsupported("Scale orders require server version >= SCALE_ORDERS".to_string()));
      }
    // Add many more checks for Algo orders, Pegged orders, Conditions, MiFID, etc.

    let mut cursor = self.start_encoding(OutgoingMessageType::PlaceOrder as i32)?;

    // --- Determine and Write Version ---
    // Version is NOT sent as the first field in PlaceOrder.
    // Instead, the number and type of fields implicitly determine the version recognized by TWS.
    // We need to send fields based on server_version support.
    // The *highest* feature version number used dictates the effective message version.

    // --- Write Order ID ---
    self.write_int_to_cursor(&mut cursor, id)?;

    // --- Write Contract Fields ---
    // Use a consolidated helper for contract fields relevant to placing orders
    self.encode_contract_for_order(&mut cursor, contract)?;

    // --- Write Order Fields (Main Part) ---
    self.write_str_to_cursor(&mut cursor, &request.side.to_string())?;

    if self.server_version >= min_server_ver::FRACTIONAL_SIZE_SUPPORT {
      self.write_str_to_cursor(&mut cursor, &request.quantity.to_string())?; // Use string for fractional
    } else {
      if request.quantity != request.quantity.trunc() {
        warn!("Fractional quantity {} provided but server version {} doesn't support it. Sending truncated integer.", request.quantity, self.server_version);
      }
      self.write_int_to_cursor(&mut cursor, request.quantity as i32)?; // Send integer
    }

    self.write_str_to_cursor(&mut cursor, &request.order_type.to_string())?;

    // Limit Price (LmtPrice)
    if self.server_version < min_server_ver::ORDER_COMBO_LEGS_PRICE {
      self.write_optional_double_to_cursor(&mut cursor, request.limit_price)?; // 0.0 if None
    } else {
      self.write_optional_double_max_to_cursor(&mut cursor, request.limit_price)?; // MAX if None
    }

    // Aux Price (Stop Price, Trail Amount, Offset, etc.)
    if self.server_version < min_server_ver::TRAILING_PERCENT {
      self.write_optional_double_to_cursor(&mut cursor, request.aux_price)?; // 0.0 if None
    } else {
      self.write_optional_double_max_to_cursor(&mut cursor, request.aux_price)?; // MAX if None
    }

    // --- Write Extended Order Fields ---
    self.write_str_to_cursor(&mut cursor, &request.time_in_force.to_string())?;
    self.write_optional_str_to_cursor(&mut cursor, request.oca_group.as_deref())?;
    self.write_optional_str_to_cursor(&mut cursor, request.account.as_deref())?;
    self.write_optional_str_to_cursor(&mut cursor, request.open_close.as_deref())?; // Defaults to "" (O) if None
    self.write_int_to_cursor(&mut cursor, request.origin)?; // 0=Customer, 1=Firm
    self.write_optional_str_to_cursor(&mut cursor, request.order_ref.as_deref())?;
    self.write_bool_to_cursor(&mut cursor, request.transmit)?; // MUST BE TRUE unless part of combo/OCA leg transmit=false
    self.write_optional_i64_to_cursor(&mut cursor, request.parent_id)?; // 0 if None
    self.write_bool_to_cursor(&mut cursor, request.block_order)?;
    self.write_bool_to_cursor(&mut cursor, request.sweep_to_fill)?;
    self.write_optional_int_to_cursor(&mut cursor, request.display_size)?; // 0 if None
    self.write_optional_int_to_cursor(&mut cursor, request.trigger_method)?; // 0 if None
    self.write_bool_to_cursor(&mut cursor, request.outside_rth)?;
    self.write_bool_to_cursor(&mut cursor, request.hidden)?;

    // --- Conditional Fields based on Contract Type / Server Version ---

    // Combo Contract Legs (Price per leg) - Only if contract is Combo
    if contract.sec_type == SecType::Combo && self.server_version >= min_server_ver::ORDER_COMBO_LEGS_PRICE {
      // if !contract.combo_legs_price.is_empty() {
      //   self.write_int_to_cursor(&mut cursor, contract.combo_legs_price.len() as i32)?;
      //   for leg_price in &contract.combo_legs_price {
      //     self.write_optional_double_max_to_cursor(&mut cursor, *leg_price)?;
      //   }
      // } else {
      //   self.write_int_to_cursor(&mut cursor, 0)?;
      // }
      return Err(IBKRError::Unsupported("Combo not implemented yet".to_string()));
    }

    // Smart Combo Routing Params
    if contract.sec_type == SecType::Combo && self.server_version >= min_server_ver::SMART_COMBO_ROUTING_PARAMS {
      // self.write_int_to_cursor(&mut cursor, request.smart_combo_routing_params.len() as i32)?; // Add smart_combo_routing_params to OrderRequest
      // for (tag, value) in &request.smart_combo_routing_params {
      //   self.write_str_to_cursor(&mut cursor, tag)?;
      //   self.write_str_to_cursor(&mut cursor, value)?;
      // }
      return Err(IBKRError::Unsupported("Combo not implemented yet".to_string()));
    }

    // --- More Extended Order Fields (with version checks) ---
    // not implemented
    // if !request.shares_allocation.is_empty() {
    //   self.write_optional_str_to_cursor(&mut cursor, Some(&request.shares_allocation))?; // Add shares_allocation to OrderRequest
    // }

    if self.server_version >= min_server_ver::POST_TO_ATS {
      self.write_optional_int_to_cursor(&mut cursor, request.post_to_ats)?;
    }

    if self.server_version >= min_server_ver::PTA_ORDERS {
      self.write_optional_str_to_cursor(&mut cursor, request.clearing_account.as_deref())?;
      self.write_optional_str_to_cursor(&mut cursor, request.clearing_intent.as_deref())?;
    }

    // --- Fields requiring TRAILING_PERCENT ---
    if self.server_version >= min_server_ver::TRAILING_PERCENT {
      self.write_optional_double_max_to_cursor(&mut cursor, request.trailing_percent)?;
    }

    // --- Fields requiring SCALE_ORDERS ---
    if self.server_version >= min_server_ver::SCALE_ORDERS {
      self.write_optional_int_to_cursor(&mut cursor, request.scale_init_level_size)?;
      self.write_optional_int_to_cursor(&mut cursor, request.scale_subs_level_size)?;
      self.write_optional_double_max_to_cursor(&mut cursor, request.scale_price_increment)?;
    }
    if self.server_version >= min_server_ver::SCALE_ORDERS2 {
      self.write_optional_double_max_to_cursor(&mut cursor, request.scale_price_adjust_value)?;
      self.write_optional_int_to_cursor(&mut cursor, request.scale_price_adjust_interval)?;
      self.write_optional_double_max_to_cursor(&mut cursor, request.scale_profit_offset)?;
      self.write_bool_to_cursor(&mut cursor, request.scale_auto_reset)?;
      self.write_optional_int_to_cursor(&mut cursor, request.scale_init_position)?;
      self.write_optional_int_to_cursor(&mut cursor, request.scale_init_fill_qty)?;
      self.write_bool_to_cursor(&mut cursor, request.scale_random_percent)?;
    }

    // --- Fields requiring HEDGE_ORDERS ---
    if self.server_version >= min_server_ver::HEDGE_ORDERS {
      self.write_optional_str_to_cursor(&mut cursor, request.hedge_type.as_deref())?;
      if request.hedge_type.is_some() {
        self.write_optional_str_to_cursor(&mut cursor, request.hedge_param.as_deref())?; // Beta or Ratio
      }
    }

    // --- Fields requiring OPT_OUT_SMART_ROUTING ---
    if self.server_version >= min_server_ver::OPT_OUT_SMART_ROUTING {
      self.write_bool_to_cursor(&mut cursor, request.opt_out_smart_routing)?;
    }

    // --- Fields requiring DELTA_NEUTRAL ---
    if self.server_version >= min_server_ver::DELTA_NEUTRAL_CONID {
      if let Some(dn) = &contract.delta_neutral_contract {
        self.write_bool_to_cursor(&mut cursor, true)?;
        self.write_int_to_cursor(&mut cursor, dn.con_id)?;
        // Delta and price are now sent later if server >= min_server_ver::DELTA_NEUTRAL_OPEN_CLOSE
      } else {
        self.write_bool_to_cursor(&mut cursor, false)?;
      }
    }
    if self.server_version >= min_server_ver::DELTA_NEUTRAL_OPEN_CLOSE {
      if let Some(dn) = &contract.delta_neutral_contract {
        self.write_optional_double_max_to_cursor(&mut cursor, Some(dn.delta))?; // Send delta
        self.write_optional_double_max_to_cursor(&mut cursor, Some(dn.price))?; // Send price
      } else {
        // Need to send defaults if delta_neutral_contract was present earlier but fields aren't? Check TWS behavior
        // self.write_optional_double_max_to_cursor(&mut cursor, None)?;
        // self.write_optional_double_max_to_cursor(&mut cursor, None)?;
      }
    }

    // --- Fields requiring ALGO_ORDERS ---
    if self.server_version >= min_server_ver::ALGO_ORDERS {
      self.write_optional_str_to_cursor(&mut cursor, request.algo_strategy.as_deref())?;
      if request.algo_strategy.is_some() {
        self.write_tag_value_list(&mut cursor, &request.algo_params)?;
      }
    }

    // --- WhatIf Flag ---
    self.write_bool_to_cursor(&mut cursor, request.what_if)?;

    // --- Order Misc Options (TagValue List) ---
    // not implemented yet
    // self.write_tag_value_list(&mut cursor, &request.order_misc_options)?; // Add order_misc_options: Vec<(String, String)> to OrderRequest

    // --- Solicited Flag ---
    self.write_bool_to_cursor(&mut cursor, request.solicited)?;

    // --- Randomize Size/Price Flags ---
    if self.server_version >= min_server_ver::RANDOMIZE_SIZE_AND_PRICE {
      self.write_bool_to_cursor(&mut cursor, request.randomize_size)?;
      self.write_bool_to_cursor(&mut cursor, request.randomize_price)?;
    }

    // --- Pegged To Benchmark Orders ---
    if self.server_version >= min_server_ver::PEGGED_TO_BENCHMARK {
      if request.order_type == OrderType::PeggedToBenchmark ||
        request.order_type == OrderType::PeggedBest ||
        request.order_type == OrderType::PeggedPrimary {
          self.write_int_to_cursor(&mut cursor, request.reference_contract_id.unwrap_or(0))?;
          self.write_bool_to_cursor(&mut cursor, request.is_pegged_change_amount_decrease)?;
          self.write_double_to_cursor(&mut cursor, request.pegged_change_amount.unwrap_or(0.0))?;
          self.write_double_to_cursor(&mut cursor, request.reference_change_amount.unwrap_or(0.0))?;
          self.write_optional_str_to_cursor(&mut cursor, request.reference_exchange_id.as_deref())?;
        }
    }

    // --- Box/Volatility Order Components ---
    if request.order_type == OrderType::Volatility {
      self.write_optional_double_max_to_cursor(&mut cursor, request.volatility)?;
      self.write_optional_int_to_cursor(&mut cursor, request.volatility_type)?; // 1=Daily, 2=Annual
      self.write_optional_int_to_cursor(&mut cursor, request.continuous_update)?; // 0=false, 1=true
      self.write_optional_int_to_cursor(&mut cursor, request.reference_price_type)?; // 1=Avg Bid/Ask, 2=Bid or Ask
    }
    if request.order_type == OrderType::BoxTop { // BOX TOP no longer supported?
      self.write_optional_double_max_to_cursor(&mut cursor, request.delta)?;
      self.write_optional_double_max_to_cursor(&mut cursor, request.stock_ref_price)?;
      self.write_optional_double_max_to_cursor(&mut cursor, request.stock_range_lower)?;
      self.write_optional_double_max_to_cursor(&mut cursor, request.stock_range_upper)?;
    }

    // --- Scale Table ---
    if self.server_version >= min_server_ver::SCALE_TABLE {
      self.write_optional_str_to_cursor(&mut cursor, request.scale_table.as_deref())?;
    }

    // --- Active Start/Stop Time ---
    self.write_optional_str_to_cursor(&mut cursor, request.active_start_time.as_deref())?; // "YYYYMMDD HH:MM:SS {tz}"
    self.write_optional_str_to_cursor(&mut cursor, request.active_stop_time.as_deref())?;

    // --- Cash Quantity ---
    if self.server_version >= min_server_ver::CASH_QTY {
      self.write_optional_double_max_to_cursor(&mut cursor, request.cash_qty)?;
    }

    // --- MiFID Fields ---
    if self.server_version >= min_server_ver::MIFID_EXECUTION {
      self.write_optional_str_to_cursor(&mut cursor, request.mifid2_decision_maker.as_deref())?;
      self.write_optional_str_to_cursor(&mut cursor, request.mifid2_decision_algo.as_deref())?;
      self.write_optional_str_to_cursor(&mut cursor, request.mifid2_execution_trader.as_deref())?;
      self.write_optional_str_to_cursor(&mut cursor, request.mifid2_execution_algo.as_deref())?;
    }

    // --- Auto Price for Hedge ---
    if self.server_version >= min_server_ver::AUTO_PRICE_FOR_HEDGE {
      self.write_bool_to_cursor(&mut cursor, request.dont_use_auto_price_for_hedge)?;
    }

    // --- Price Mgmt Algo ---
    if self.server_version >= min_server_ver::PRICE_MGMT_ALGO {
      self.write_optional_bool_to_cursor(&mut cursor, request.use_price_mgmt_algo)?;
    }

    // --- Extended Percent Offset for REL orders ---
    self.write_optional_double_max_to_cursor(&mut cursor, request.percent_offset)?;

    // ... Continue adding fields based on server version checks ...
    // This includes: SoftDollarTier, Duration, PostToATS, AdvancedErrorOverride, ManualOrderTime,
    // MinTradeQty, MinCompeteSize, CompeteAgainstBestOffset, MidOffsetAtWhole/Half,
    // BondAccruedInterest, ExternalUserId, ManualOrderIndicator, ExtOperator,
    // ImbalanceOnly, RouteMarketableToBBO, RefFuturesConId, AutoCancelDate, AutoCancelParent,
    // ParentPermId, ...

    Ok(self.finish_encoding(cursor))
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


  // Helper for combo legs (used by encode_contract_for_order and potentially others)
  fn encode_combo_legs(&self, cursor: &mut Cursor<Vec<u8>>, legs: &[ComboLeg]) -> Result<(), IBKRError> {
    // ComboLeg fields are part of the Contract definition when placing orders
    if self.server_version >= min_server_ver::ORDER_COMBO_LEGS_PRICE { // Check relevant version for combo legs
      self.write_int_to_cursor(cursor, legs.len() as i32)?;
      for leg in legs {
        self.write_int_to_cursor(cursor, leg.con_id)?;
        self.write_int_to_cursor(cursor, leg.ratio)?;
        self.write_str_to_cursor(cursor, &leg.action)?; // BUY/SELL/SSHORT
        self.write_str_to_cursor(cursor, &leg.exchange)?;
        self.write_int_to_cursor(cursor, leg.open_close)?; // 0=Same, 1=Open, 2=Close, 3=Unknown

        if self.server_version >= min_server_ver::SSHORT_COMBO_LEGS {
          self.write_int_to_cursor(cursor, leg.short_sale_slot)?; // 0 = Clearing default, 1 = Retail, 2 = Institution
          if !leg.designated_location.is_empty() {
            self.write_optional_str_to_cursor(cursor, Some(&leg.designated_location))?;
          }
        }
        if self.server_version >= min_server_ver::SSHORTX_OLD { // SSHORTX_OLD version check
          self.write_int_to_cursor(cursor, leg.exempt_code)?;
        }
      }
    } else {
      if !legs.is_empty() {
        warn!("Combo legs provided but server version {} does not support ORDER_COMBO_LEGS_PRICE", self.server_version);
        // Or return error?
      }
      // Send count 0 if required by older version?
      // self.write_int_to_cursor(cursor, 0)?;
    }
    Ok(())
  }


  // --- Other encoding methods ---
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
    self.write_str_to_cursor(&mut cursor, &self.format_datetime_tws(end_date_time, true))?;
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
