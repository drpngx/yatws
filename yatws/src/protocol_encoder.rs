// yatws/src/protocol_encoder.rs
// Encoder for the TWS API protocol messages

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
  writer: Box<dyn Write + Send>,
  server_version: i32,
  next_valid_id: i32,
}

impl Encoder {
  /// Create a new message encoder
  pub fn new<W: Write + Send + 'static>(writer: W, server_version: i32) -> Self {
    Self {
      writer: Box::new(writer),
      server_version,
      next_valid_id: 0,
    }
  }

  /// Set the server version
  pub fn set_server_version(&mut self, version: i32) {
    self.server_version = version;
  }

  /// Set the next valid ID (received from the server)
  pub fn set_next_valid_id(&mut self, id: i32) {
    self.next_valid_id = id;
  }

  /// Get the next valid ID and increment the counter
  pub fn get_next_valid_id(&mut self) -> i32 {
    let id = self.next_valid_id;
    self.next_valid_id += 1;
    id
  }

  /// Start encoding a message with the given type
  pub fn start_message(&mut self, msg_type: i32) -> Result<(), IBKRError> {
    self.write_int(msg_type)
  }

  /// Finish the message by flushing the writer
  pub fn finish_message(&mut self) -> Result<(), IBKRError> {
    self.flush()
  }

  /// Write a client version message
  pub fn encode_client_version(&mut self) -> Result<(), IBKRError> {
    debug!("Encoding client version message");
    self.write_str("API")?;
    // For versions >= 3, add client version
    self.write_str(&self.server_version.to_string())?;
    Ok(())
  }

  /// Request the next valid order ID
  pub fn encode_request_ids(&mut self) -> Result<(), IBKRError> {
    debug!("Encoding request IDs message");
    self.write_int(OutgoingMessageType::RequestIds as i32)?;
    self.write_int(1)?; // Version
    Ok(())
  }

  /// Request market data for a contract
  pub fn encode_request_market_data(&mut self, req_id: i32, contract: &Contract, snapshot: bool) -> Result<(), IBKRError> {
    debug!("Encoding request market data message for contract {}", contract.symbol);
    self.write_int(OutgoingMessageType::RequestMarketData as i32)?;
    self.write_int(11)?; // Version
    self.write_int(req_id)?;

    // Contract fields
    if self.server_version >= min_server_ver::REQ_MKT_DATA_CONID {
      self.write_int(contract.con_id)?;
    }

    self.write_str(&contract.symbol)?;
    self.write_str(&contract.sec_type.to_string())?;
    self.write_str(&contract.last_trade_date_or_contract_month.as_ref().unwrap_or(&String::new()))?;
    self.write_double(contract.strike.unwrap_or_default())?;
    self.write_str(&contract.right.map_or_else(|| String::new(), |r| r.to_string()))?;

    if self.server_version >= 15 {
      self.write_str(&contract.multiplier.as_ref().unwrap_or(&String::new()))?;
    }

    self.write_str(&contract.exchange)?;

    if self.server_version >= 14 {
      self.write_str(&contract.primary_exchange.as_ref().unwrap_or(&String::new()))?;
    }

    self.write_str(&contract.currency)?;

    if self.server_version >= 2 {
      self.write_str(&contract.local_symbol.as_ref().unwrap_or(&String::new()))?;
    }

    if self.server_version >= min_server_ver::TRADING_CLASS {
      self.write_str(&contract.trading_class.as_ref().unwrap_or(&String::new()))?;
    }

    // Combo legs for BAG requests
    if self.server_version >= 8 && contract.sec_type == SecType::Combo {
      if contract.combo_legs.is_empty() {
        self.write_int(0)?;
      } else {
        self.write_int(contract.combo_legs.len() as i32)?;

        for leg in &contract.combo_legs {
          self.write_int(leg.con_id)?;
          self.write_int(leg.ratio)?;
          self.write_str(&leg.action)?;
          self.write_str(&leg.exchange)?;
        }
      }
    }

    // Delta neutral contract
    if self.server_version >= min_server_ver::DELTA_NEUTRAL {
      if let Some(delta_neutral) = &contract.delta_neutral_contract {
        self.write_bool(true)?;
        self.write_int(delta_neutral.con_id)?;
        self.write_double(delta_neutral.delta)?;
        self.write_double(delta_neutral.price)?;
      } else {
        self.write_bool(false)?;
      }
    }

    // Generic tick list
    if self.server_version >= 31 {
      self.write_str("100,101,104,105,106,107,165,221,225,233,236,258,295,318")?;
    }

    // Snapshot parameter
    if self.server_version >= min_server_ver::SNAPSHOT_MKT_DATA {
      self.write_bool(snapshot)?;
    }

    // Regulatory snapshot parameter
    if self.server_version >= min_server_ver::REQ_SMART_COMPONENTS {
      self.write_bool(false)?; // regulatorySnapshot
    }

    // Market data options
    if self.server_version >= min_server_ver::LINKING {
      // No tag-value pairs
      self.write_int(0)?;
    }

    Ok(())
  }

  /// Cancel market data subscription
  pub fn encode_cancel_market_data(&mut self, req_id: i32) -> Result<(), IBKRError> {
    debug!("Encoding cancel market data message for req_id {}", req_id);
    self.write_int(OutgoingMessageType::CancelMarketData as i32)?;
    self.write_int(1)?; // Version
    self.write_int(req_id)?;
    Ok(())
  }

  /// Request all open orders
  pub fn encode_request_all_open_orders(&mut self) -> Result<(), IBKRError> {
    debug!("Encoding request all open orders message");
    self.write_int(OutgoingMessageType::RequestAllOpenOrders as i32)?;
    self.write_int(1)?; // Version
    Ok(())
  }

  /// Request open orders for the client
  pub fn encode_request_open_orders(&mut self) -> Result<(), IBKRError> {
    debug!("Encoding request open orders message");
    self.write_int(OutgoingMessageType::RequestOpenOrders as i32)?;
    self.write_int(1)?; // Version
    Ok(())
  }

  /// Request account data stream
  pub fn encode_request_account_data(&mut self, subscribe: bool, account_code: &str) -> Result<(), IBKRError> {
    debug!("Encoding request account data message for account {}", account_code);
    self.write_int(OutgoingMessageType::RequestAccountData as i32)?;
    self.write_int(2)?; // Version
    self.write_bool(subscribe)?;
    self.write_str(account_code)?;
    Ok(())
  }

  /// Request positions
  pub fn encode_request_positions(&mut self) -> Result<(), IBKRError> {
    debug!("Encoding request positions message");
    self.write_int(OutgoingMessageType::RequestPositions as i32)?;
    self.write_int(1)?; // Version
    Ok(())
  }

  /// Request contract details
  pub fn encode_request_contract_data(&mut self, req_id: i32, contract: &Contract) -> Result<(), IBKRError> {
    debug!("Encoding request contract data message for contract {}", contract.symbol);
    self.write_int(OutgoingMessageType::RequestContractData as i32)?;
    self.write_int(8)?; // Version

    if self.server_version >= min_server_ver::CONTRACT_DATA_CHAIN {
      self.write_int(req_id)?;
    }

    // Contract fields
    if self.server_version >= min_server_ver::CONTRACT_CONID {
      self.write_int(contract.con_id)?;
    }

    self.write_str(&contract.symbol)?;
    self.write_str(&contract.sec_type.to_string())?;
    self.write_str(&contract.last_trade_date_or_contract_month.as_ref().unwrap_or(&String::new()))?;
    self.write_double(contract.strike.unwrap_or_default())?;
    self.write_str(&contract.right.map_or_else(|| String::new(), |r| r.to_string()))?;

    if self.server_version >= 15 {
      self.write_str(&contract.multiplier.as_ref().unwrap_or(&String::new()))?;
    }

    if self.server_version >= min_server_ver::PRIMARYEXCH {
      self.write_str(&contract.exchange)?;
      self.write_str(&contract.primary_exchange.as_ref().unwrap_or(&String::new()))?;
    } else if self.server_version >= min_server_ver::LINKING {
      if !contract.primary_exchange.as_ref().unwrap_or(&String::new()).is_empty() &&
        (contract.exchange == "BEST" || contract.exchange == "SMART") {
          self.write_str(&format!("{}:{}", contract.exchange,
                                  contract.primary_exchange.as_ref().unwrap()))?;
        } else {
          self.write_str(&contract.exchange)?;
        }
    }

    self.write_str(&contract.currency)?;
    self.write_str(&contract.local_symbol.as_ref().unwrap_or(&String::new()))?;

    if self.server_version >= min_server_ver::TRADING_CLASS {
      self.write_str(&contract.trading_class.as_ref().unwrap_or(&String::new()))?;
    }

    if self.server_version >= 31 {
      self.write_bool(contract.include_expired)?;
    }

    if self.server_version >= min_server_ver::SEC_ID_TYPE {
      self.write_str(&contract.sec_id_type.as_ref().map_or_else(|| String::new(), |t| t.to_string()))?;
      self.write_str(&contract.sec_id.as_ref().unwrap_or(&String::new()))?;
    }

    if self.server_version >= min_server_ver::BOND_ISSUERID {
      self.write_str(&contract.issuer_id.as_ref().unwrap_or(&String::new()))?;
    }

    Ok(())
  }

  /// Place an order
  pub fn encode_place_order(&mut self, id: i32, contract: &Contract, order: &Order) -> Result<(), IBKRError> {
    debug!("Encoding place order message for contract {}", contract.symbol);

    // Check minimum server version requirements
    if self.server_version < min_server_ver::SCALE_ORDERS &&
      order.request.scale_init_level_size != Some(i32::MAX) &&
      order.request.scale_price_increment != Some(f64::MAX) {
        return Err(IBKRError::UpdateTws("Server does not support scale orders".to_string()));
      }

    if self.server_version < min_server_ver::SSHORT_COMBO_LEGS &&
      contract.sec_type == SecType::Combo {
        for leg in &contract.combo_legs {
          if leg.short_sale_slot != 0 || !leg.designated_location.is_empty() {
            return Err(IBKRError::UpdateTws("Server does not support SSHORT flag for combo legs".to_string()));
          }
        }
      }

    if self.server_version < min_server_ver::WHAT_IF_ORDERS && order.request.what_if {
      return Err(IBKRError::UpdateTws("Server does not support what-if orders".to_string()));
    }

    // Version depends on server version
    let version = if self.server_version < min_server_ver::NOT_HELD { 27 } else { 45 };

    self.write_int(OutgoingMessageType::PlaceOrder as i32)?;

    if self.server_version < min_server_ver::ORDER_CONTAINER {
      self.write_int(version)?;
    }

    self.write_int(id)?;

    // Send contract fields
    if self.server_version >= min_server_ver::PLACE_ORDER_CONID {
      self.write_int(contract.con_id)?;
    }

    self.write_str(&contract.symbol)?;
    self.write_str(&contract.sec_type.to_string())?;
    self.write_str(&contract.last_trade_date_or_contract_month.as_ref().unwrap_or(&String::new()))?;
    self.write_double(contract.strike.unwrap_or_default())?;
    self.write_str(&contract.right.map_or_else(|| String::new(), |r| r.to_string()))?;

    if self.server_version >= 15 {
      self.write_str(&contract.multiplier.as_ref().unwrap_or(&String::new()))?;
    }

    self.write_str(&contract.exchange)?;

    if self.server_version >= 14 {
      self.write_str(&contract.primary_exchange.as_ref().unwrap_or(&String::new()))?;
    }

    self.write_str(&contract.currency)?;

    if self.server_version >= 2 {
      self.write_str(&contract.local_symbol.as_ref().unwrap_or(&String::new()))?;
    }

    if self.server_version >= min_server_ver::TRADING_CLASS {
      self.write_str(&contract.trading_class.as_ref().unwrap_or(&String::new()))?;
    }

    // Send main order fields
    self.write_str(&order.request.side.to_string())?;

    if self.server_version >= min_server_ver::FRACTIONAL_POSITIONS {
      self.write_str(&order.request.quantity.to_string())?;
    } else {
      self.write_int(order.request.quantity as i32)?;
    }

    self.write_str(&order.request.order_type.to_string())?;

    if self.server_version < min_server_ver::ORDER_COMBO_LEGS_PRICE {
      self.write_double(order.request.limit_price.unwrap_or(0.0))?;
    } else {
      self.write_double_max(order.request.limit_price)?;
    }

    if self.server_version < min_server_ver::TRAILING_PERCENT {
      self.write_double(order.request.aux_price.unwrap_or(0.0))?;
    } else {
      self.write_double_max(order.request.aux_price)?;
    }

    // Extended order fields
    self.write_str(&order.request.time_in_force.to_string())?;
    self.write_str(&order.request.oca_group.as_ref().unwrap_or(&String::new()))?;
    self.write_str(&order.request.account.as_ref().unwrap_or(&String::new()))?;
    self.write_str(&order.request.open_close.as_ref().unwrap_or(&String::new()))?;
    self.write_int(order.request.origin)?;
    self.write_str(&order.request.order_ref.as_ref().unwrap_or(&String::new()))?;
    self.write_bool(order.request.transmit)?;

    if self.server_version >= 4 {
      self.write_int(order.request.parent_id.unwrap_or(0) as i32)?;
    }

    // Additional parameters would be added based on server version

    Ok(())
  }

  /// Cancel an order
  pub fn encode_cancel_order(&mut self, order_id: i32) -> Result<(), IBKRError> {
    debug!("Encoding cancel order message for order ID {}", order_id);
    self.write_int(OutgoingMessageType::CancelOrder as i32)?;
    self.write_int(1)?; // Version
    self.write_int(order_id)?;

    // For newer server versions, additional fields would be added here

    Ok(())
  }

  /// Request managed accounts
  pub fn encode_request_managed_accounts(&mut self) -> Result<(), IBKRError> {
    debug!("Encoding request managed accounts message");
    self.write_int(OutgoingMessageType::RequestManagedAccts as i32)?;
    self.write_int(1)?; // Version
    Ok(())
  }

  /// Request historical data for a contract
  pub fn encode_request_historical_data(
    &mut self,
    req_id: i32,
    contract: &Contract,
    end_date_time: DateTime<Utc>,
    duration_str: &str,
    bar_size_setting: &str,
    what_to_show: &str,
    use_rth: bool,
    format_date: bool,
  ) -> Result<(), IBKRError> {
    debug!("Encoding request historical data message for contract {}", contract.symbol);

    if self.server_version < 16 {
      return Err(IBKRError::UpdateTws("Server does not support historical data".to_string()));
    }

    if self.server_version < min_server_ver::TRADING_CLASS {
      if !contract.trading_class.as_ref().unwrap_or(&String::new()).is_empty() || contract.con_id > 0 {
        return Err(IBKRError::UpdateTws(
          "Server does not support conId and tradingClass for historical data".to_string()));
      }
    }

    self.write_int(OutgoingMessageType::RequestHistoricalData as i32)?;

    if self.server_version < min_server_ver::SYNT_REALTIME_BARS {
      self.write_int(6)?; // Version
    }

    self.write_int(req_id)?;

    // Contract fields
    if self.server_version >= min_server_ver::TRADING_CLASS {
      self.write_int(contract.con_id)?;
    }

    self.write_str(&contract.symbol)?;
    self.write_str(&contract.sec_type.to_string())?;
    self.write_str(&contract.last_trade_date_or_contract_month.as_ref().unwrap_or(&String::new()))?;
    self.write_double(contract.strike.unwrap_or_default())?;
    self.write_str(&contract.right.map_or_else(|| String::new(), |r| r.to_string()))?;
    self.write_str(&contract.multiplier.as_ref().unwrap_or(&String::new()))?;
    self.write_str(&contract.exchange)?;
    self.write_str(&contract.primary_exchange.as_ref().unwrap_or(&String::new()))?;
    self.write_str(&contract.currency)?;
    self.write_str(&contract.local_symbol.as_ref().unwrap_or(&String::new()))?;

    if self.server_version >= min_server_ver::TRADING_CLASS {
      self.write_str(&contract.trading_class.as_ref().unwrap_or(&String::new()))?;
    }

    if self.server_version >= 31 {
      self.write_bool(contract.include_expired)?;
    }

    if self.server_version >= 20 {
      self.write_str(&end_date_time.format("%Y%m%d %H:%M:%S").to_string())?;
      self.write_str(bar_size_setting)?;
    }

    self.write_str(duration_str)?;
    self.write_bool(use_rth)?;
    self.write_str(what_to_show)?;

    if self.server_version > 16 {
      self.write_int(if format_date { 1 } else { 2 })?;
    }

    // Combo legs for BAG requests
    if contract.sec_type == SecType::Combo {
      if contract.combo_legs.is_empty() {
        self.write_int(0)?;
      } else {
        self.write_int(contract.combo_legs.len() as i32)?;

        for leg in &contract.combo_legs {
          self.write_int(leg.con_id)?;
          self.write_int(leg.ratio)?;
          self.write_str(&leg.action)?;
          self.write_str(&leg.exchange)?;
        }
      }
    }

    if self.server_version >= min_server_ver::SYNT_REALTIME_BARS {
      self.write_bool(false)?; // keepUpToDate
    }

    if self.server_version >= min_server_ver::LINKING {
      // No chart options
      self.write_int(0)?;
    }

    Ok(())
  }

  /// Write a string to the output stream
  fn write_str(&mut self, s: &str) -> Result<(), IBKRError> {
    trace!("Writing string: {}", s);
    self.writer.write_all(s.as_bytes()).map_err(|e| IBKRError::SocketError(e.to_string()))?;
    self.writer.write_all(&[0]).map_err(|e| IBKRError::SocketError(e.to_string()))?;
    Ok(())
  }

  /// Write an integer to the output stream
  fn write_int(&mut self, val: i32) -> Result<(), IBKRError> {
    trace!("Writing int: {}", val);
    self.write_str(&val.to_string())
  }

  /// Write a double to the output stream
  fn write_double(&mut self, val: f64) -> Result<(), IBKRError> {
    trace!("Writing double: {}", val);
    self.write_str(&val.to_string())
  }

  /// Write a double to the output stream, handling MAX_VALUE
  fn write_double_max(&mut self, val: Option<f64>) -> Result<(), IBKRError> {
    match val {
      Some(value) if value != f64::MAX => self.write_double(value),
      _ => self.write_double(f64::MAX),
    }
  }

  /// Write a boolean to the output stream (as 0 or 1)
  fn write_bool(&mut self, val: bool) -> Result<(), IBKRError> {
    trace!("Writing bool: {}", val);
    self.write_int(if val { 1 } else { 0 })
  }

  /// Flush the writer
  pub fn flush(&mut self) -> Result<(), IBKRError> {
    self.writer.flush().map_err(|e| IBKRError::SocketError(e.to_string()))
  }
}

/// A helper trait for encoding tag-value pairs
pub trait TagValueEncoder {
  /// Encode tag-value pairs into the message
  fn encode_tag_values<W: Write>(&self, encoder: &mut Encoder) -> Result<(), IBKRError>;
}

impl TagValueEncoder for Vec<(String, String)> {
  fn encode_tag_values<W: Write>(&self, encoder: &mut Encoder) -> Result<(), IBKRError> {
    encoder.write_int(self.len() as i32)?;

    for (tag, value) in self {
      encoder.write_str(tag)?;
      encoder.write_str(value)?;
    }

    Ok(())
  }
}

impl TagValueEncoder for Option<Vec<(String, String)>> {
  fn encode_tag_values<W: Write>(&self, encoder: &mut Encoder) -> Result<(), IBKRError> {
    match self {
      Some(vec) => vec.encode_tag_values::<W>(encoder),
      None => encoder.write_int(0), // Empty list
    }
  }
}

/// Extensions for Contract encoding
pub trait ContractEncoder {
  /// Encode a contract for various message types
  fn encode_contract<W: Write>(&self, encoder: &mut Encoder) -> Result<(), IBKRError>;
}

impl ContractEncoder for Contract {
  fn encode_contract<W: Write>(&self, encoder: &mut Encoder) -> Result<(), IBKRError> {
    // Basic contract fields
    encoder.write_int(self.con_id)?;
    encoder.write_str(&self.symbol)?;
    encoder.write_str(&self.sec_type.to_string())?;
    encoder.write_str(&self.last_trade_date_or_contract_month.as_ref().unwrap_or(&String::new()))?;
    encoder.write_double(self.strike.unwrap_or_default())?;
    encoder.write_str(&self.right.map_or_else(|| String::new(), |r| r.to_string()))?;
    encoder.write_str(&self.multiplier.as_ref().unwrap_or(&String::new()))?;
    encoder.write_str(&self.exchange)?;
    encoder.write_str(&self.primary_exchange.as_ref().unwrap_or(&String::new()))?;
    encoder.write_str(&self.currency)?;
    encoder.write_str(&self.local_symbol.as_ref().unwrap_or(&String::new()))?;
    encoder.write_str(&self.trading_class.as_ref().unwrap_or(&String::new()))?;

    Ok(())
  }
}
