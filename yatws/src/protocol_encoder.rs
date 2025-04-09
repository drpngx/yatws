// yatws/src/protocol_encoder.rs
// Encoder for the TWS API protocol messages

use crate::base::IBKRError;
use crate::contract::Contract;
use crate::order::{Order, OrderRequest, TimeInForce, OrderType, OrderSide};
use crate::account::AccountInfo;
use chrono::{DateTime, Utc};
use log::debug;
use std::io::{self, Write};
use num_traits::cast::ToPrimitive;

/// Message tags for outgoing messages
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
  RequestHistoricalData = 20,
  RequestGlobalCancel = 56,
  RequestPositions = 61,
  RequestAccountSummary = 62,
  CancelAccountSummary = 63,
}

/// Message encoder for the TWS API protocol
pub struct Encoder<W: Write> {
  writer: W,
  server_version: i32,
  next_valid_id: i32,
}

impl<W: Write> Encoder<W> {
  /// Create a new message encoder
  pub fn new(writer: W, server_version: i32) -> Self {
    Self {
      writer,
      server_version,
      next_valid_id: 0,
    }
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
    self.write_int(contract.con_id)?;
    self.write_str(&contract.symbol)?;
    self.write_str(&contract.sec_type.to_string())?;
    let empty = "".to_string();
    self.write_str(&contract.last_trade_date_or_contract_month.as_ref().unwrap_or_else(|| &empty))?;
    self.write_double(contract.strike.unwrap_or_default())?;
    self.write_str(&contract.right.map_or_else(|| "".to_string(), |r| r.to_string()))?;
    self.write_str(&contract.multiplier.as_ref().unwrap_or_else(|| &empty))?;
    self.write_str(&contract.exchange)?;
    self.write_str(&contract.primary_exchange.as_ref().unwrap_or_else(|| &empty))?;
    self.write_str(&contract.currency)?;
    self.write_str(&contract.local_symbol.as_ref().unwrap_or_else(|| &empty))?;
    self.write_str(&contract.trading_class.as_ref().unwrap_or_else(|| &empty))?;

    // Market data options
    self.write_bool(false)?; // Not including expired
    self.write_bool(snapshot)?; // Snapshot

    // Generic tick list
    self.write_str("100,101,104,105,106,107,165,221,225,233,236,258,295,318")?;

    // Additional options if needed in the future
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
    self.write_int(req_id)?;

    let empty = "".to_string();

    // Contract fields
    self.write_int(contract.con_id)?;
    self.write_str(&contract.symbol)?;
    self.write_str(&contract.sec_type.to_string())?;
    self.write_str(&contract.last_trade_date_or_contract_month.as_ref().unwrap_or_else(|| &empty))?;
    self.write_double(contract.strike.unwrap_or_default())?;
    self.write_str(&contract.right.map_or_else(|| "".to_string(), |r| r.to_string()))?;
    self.write_str(&contract.multiplier.as_ref().unwrap_or_else(|| &empty))?;
    self.write_str(&contract.exchange)?;
    self.write_str(&contract.primary_exchange.as_ref().unwrap_or_else(|| &empty))?;
    self.write_str(&contract.currency)?;
    self.write_str(&contract.local_symbol.as_ref().unwrap_or_else(|| &empty))?;
    self.write_str(&contract.trading_class.as_ref().unwrap_or_else(|| &empty))?;
    self.write_bool(contract.include_expired)?;

    Ok(())
  }

  /// Place an order
  pub fn encode_place_order(&mut self, id: i32, contract: &Contract, order: &Order) -> Result<(), IBKRError> {
    debug!("Encoding place order message for contract {}", contract.symbol);
    self.write_int(OutgoingMessageType::PlaceOrder as i32)?;
    self.write_int(45)?; // Version
    self.write_int(id)?;

    let empty = "".to_string();
    // Contract fields
    self.write_int(contract.con_id)?;
    self.write_str(&contract.symbol)?;
    self.write_str(&contract.sec_type.to_string())?;
    self.write_str(&contract.last_trade_date_or_contract_month.as_ref().unwrap_or_else(|| &empty))?;
    self.write_double(contract.strike.unwrap_or_default())?;
    self.write_str(&contract.right.map_or_else(|| "".to_string(), |r| r.to_string()))?;
    self.write_str(&contract.multiplier.as_ref().unwrap_or_else(|| &empty))?;
    self.write_str(&contract.exchange)?;
    self.write_str(&contract.primary_exchange.as_ref().unwrap_or_else(|| &empty))?;
    self.write_str(&contract.currency)?;
    self.write_str(&contract.local_symbol.as_ref().unwrap_or_else(|| &empty))?;
    self.write_str(&contract.trading_class.as_ref().unwrap_or_else(|| &empty))?;
    self.write_bool(contract.include_expired)?;

    // Order fields
    self.write_str(&order.request.side.to_string())?;
    self.write_double(order.request.quantity.to_f64().unwrap())?;
    self.write_str(&order.request.order_type.to_string())?;

    // Limit price
    match order.request.limit_price {
      Some(price) => self.write_double(price)?,
      None => self.write_double(0.0)?,
    }

    // Aux price (stop price)
    match order.request.aux_price {
      Some(price) => self.write_double(price)?,
      None => self.write_double(0.0)?,
    }

    // Time in force
    self.write_str(&order.request.time_in_force.to_string())?;

    // Additional order parameters
    let empty = "".to_string();
    self.write_str(&order.request.order_ref.as_ref().unwrap_or_else(|| &empty));
    self.write_bool(order.request.transmit)?;
    self.write_int(order.id.parse::<i32>().unwrap_or(0))?;
    self.write_bool(order.request.outside_rth)?;
    self.write_bool(order.request.hidden)?;

    // More parameters could be added in the future

    Ok(())
  }

  /// Cancel an order
  pub fn encode_cancel_order(&mut self, order_id: i32) -> Result<(), IBKRError> {
    debug!("Encoding cancel order message for order ID {}", order_id);
    self.write_int(OutgoingMessageType::CancelOrder as i32)?;
    self.write_int(1)?; // Version
    self.write_int(order_id)?;
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
    self.write_int(OutgoingMessageType::RequestHistoricalData as i32)?;
    self.write_int(6)?; // Version
    self.write_int(req_id)?;

    let empty = "".to_string();
    // Contract fields
    self.write_int(contract.con_id)?;
    self.write_str(&contract.symbol)?;
    self.write_str(&contract.sec_type.to_string())?;
    self.write_str(&contract.last_trade_date_or_contract_month.as_ref().unwrap_or_else(|| &empty))?;
    self.write_double(contract.strike.unwrap_or_default())?;
    self.write_str(&contract.right.map_or_else(|| "".to_string(), |r| r.to_string()))?;
    self.write_str(&contract.multiplier.as_ref().unwrap_or_else(|| &empty))?;
    self.write_str(&contract.exchange)?;
    self.write_str(&contract.primary_exchange.as_ref().unwrap_or_else(|| &empty))?;
    self.write_str(&contract.currency)?;
    self.write_str(&contract.local_symbol.as_ref().unwrap_or_else(|| &empty))?;
    self.write_str(&contract.trading_class.as_ref().unwrap_or_else(|| &empty))?;
    self.write_bool(contract.include_expired)?;

    // Historical data parameters
    self.write_str(&end_date_time.format("%Y%m%d %H:%M:%S").to_string())?;
    self.write_str(bar_size_setting)?;
    self.write_str(duration_str)?;
    self.write_bool(use_rth)?;
    self.write_str(what_to_show)?;
    self.write_int(if format_date { 1 } else { 2 })?;

    // Chart options - empty for now
    self.write_str("")?;

    Ok(())
  }

  /// Write a string to the output stream
  fn write_str(&mut self, s: &str) -> Result<(), IBKRError> {
    self.writer.write_all(s.as_bytes()).map_err(|e| IBKRError::SocketError(e.to_string()))?;
    self.writer.write_all(&[0]).map_err(|e| IBKRError::SocketError(e.to_string()))?;
    Ok(())
  }

  /// Write an integer to the output stream
  fn write_int(&mut self, val: i32) -> Result<(), IBKRError> {
    self.write_str(&val.to_string())
  }

  /// Write a double to the output stream
  fn write_double(&mut self, val: f64) -> Result<(), IBKRError> {
    self.write_str(&val.to_string())
  }

  /// Write a boolean to the output stream (as 0 or 1)
  fn write_bool(&mut self, val: bool) -> Result<(), IBKRError> {
    self.write_int(if val { 1 } else { 0 })
  }

  /// Flush the writer
  pub fn flush(&mut self) -> Result<(), IBKRError> {
    self.writer.flush().map_err(|e| IBKRError::SocketError(e.to_string()))
  }
}
