// yatws/src/parser_order.rs
use std::sync::Arc;
use crate::handler::OrderHandler;
use chrono::Utc;
use std::str::FromStr;
use crate::base::IBKRError;
use crate::protocol_dec_parser::FieldParser;
use crate::contract::{Contract, OptionRight, SecType};
use crate::order::{Order, OrderType, OrderSide, TimeInForce, OrderRequest};
use crate::min_server_ver::min_server_ver;


/// Process next valid ID message
pub fn process_next_valid_id(handler: &Arc<dyn OrderHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let id = parser.read_int()?;

  log::debug!("Next valid ID: {}", id);

  Ok(())
}

/// Process order status message
pub fn process_order_status(handler: &Arc<dyn OrderHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let id = parser.read_int()?;
  let status = parser.read_string()?;
  let filled = parser.read_double()?;
  let remaining = parser.read_double()?;
  let avg_fill_price = parser.read_double()?;

  log::debug!("Order Status: ID={}, Status={}, Filled={}, Remaining={}, AvgPrice={}",
         id, status, filled, remaining, avg_fill_price);

  Ok(())
}

/// Process open order message
pub fn process_open_order(handler: &Arc<dyn OrderHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let version = parser.read_int()?;
  let order_id = parser.read_int()?;

  // Read contract fields
  let mut contract = Contract::new();

  if version >= min_server_ver::TRADING_CLASS {
    contract.con_id = parser.read_int()?;
  }

  contract.symbol = parser.read_string()?;
  let sec_type_str = parser.read_string()?;
  contract.sec_type = SecType::from_str(&sec_type_str).unwrap_or(SecType::Stock);

  let expiry = parser.read_string()?;
  if !expiry.is_empty() {
    contract.last_trade_date_or_contract_month = Some(expiry);
  }

  let strike = parser.read_double()?;
  if strike > 0.0 {
    contract.strike = Some(strike);
  }

  let right_str = parser.read_string()?;
  if !right_str.is_empty() {
    contract.right = match right_str.as_str() {
      "C" => Some(OptionRight::Call),
      "P" => Some(OptionRight::Put),
      _ => None,
    };
  }

  if version >= 32 {
    let multiplier = parser.read_string()?;
    if !multiplier.is_empty() {
      contract.multiplier = Some(multiplier);
    }
  }

  contract.exchange = parser.read_string()?;
  contract.currency = parser.read_string()?;

  if version >= 2 {
    let local_symbol = parser.read_string()?;
    if !local_symbol.is_empty() {
      contract.local_symbol = Some(local_symbol);
    }
  }

  if version >= min_server_ver::TRADING_CLASS {
    let trading_class = parser.read_string()?;
    if !trading_class.is_empty() {
      contract.trading_class = Some(trading_class);
    }
  }

  // Read order fields
  let side_str = parser.read_string()?;
  let quantity = parser.read_double()?;
  let order_type_str = parser.read_string()?;
  let limit_price = parser.read_double()?;
  let aux_price = parser.read_double()?;
  let tif_str = parser.read_string()?;

  // Create OrderRequest with basic fields
  let mut order_request = OrderRequest::default();
  order_request.order_type = match order_type_str.as_str() {
    "MKT" => OrderType::Market,
    "LMT" => OrderType::Limit,
    "STP" => OrderType::Stop,
    "STP LMT" => OrderType::StopLimit,
    _ => OrderType::None,
  };

  order_request.side = match side_str.as_str() {
    "BUY" => OrderSide::Buy,
    "SELL" => OrderSide::Sell,
    "SSHORT" => OrderSide::SellShort,
    _ => OrderSide::Buy,
  };

  order_request.quantity = quantity;

  if limit_price > 0.0 {
    order_request.limit_price = Some(limit_price);
  }

  if aux_price > 0.0 {
    order_request.aux_price = Some(aux_price);
  }

  order_request.time_in_force = match tif_str.as_str() {
    "DAY" => TimeInForce::Day,
    "GTC" => TimeInForce::GoodTillCancelled,
    "FOK" => TimeInForce::FillOrKill,
    "IOC" => TimeInForce::ImmediateOrCancel,
    "GTD" => TimeInForce::GoodTillDate,
    _ => TimeInForce::Day,
  };

  log::debug!("Open Order: ID={}, Symbol={}, Side={}, Type={}, Quantity={}",
         order_id, contract.symbol, side_str, order_type_str, quantity);

  Ok(())
}


/// Process open order end message
pub fn process_open_order_end(handler: &Arc<dyn OrderHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  log::debug!("Open Order End");
  Ok(())
}

/// Process order bound message
pub fn process_order_bound(handler: &Arc<dyn OrderHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse order bound message
  Ok(())
}

/// Process completed order message
pub fn process_completed_order(handler: &Arc<dyn OrderHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse completed order message
  Ok(())
}

/// Process completed orders end message
pub fn process_completed_orders_end(handler: &Arc<dyn OrderHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse completed orders end message
  Ok(())
}
