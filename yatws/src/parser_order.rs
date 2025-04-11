// yatws/src/parser_order.rs
use std::sync::Arc;
use chrono::{DateTime, Utc}; // Added DateTime
use std::str::FromStr;
use crate::base::IBKRError;
use crate::protocol_dec_parser::FieldParser;
use crate::contract::{Contract, OptionRight, SecType, DeltaNeutralContract, ComboLeg}; // Added DeltaNeutralContract, ComboLeg
use crate::order::{Order, OrderState, OrderStatus, OrderType, OrderSide, TimeInForce, OrderRequest, OrderUpdates}; // Added OrderState, OrderStatus, OrderUpdates
use crate::min_server_ver::min_server_ver;
use log::{debug, warn}; // Added warn
use crate::handler::OrderHandler;


// --- Helper to parse OrderStatus string ---
fn parse_order_status(status_str: &str) -> OrderStatus {
  match status_str {
    "PendingSubmit" => OrderStatus::PendingSubmit,
    "PendingCancel" => OrderStatus::PendingCancel,
    "PreSubmitted" => OrderStatus::PreSubmitted,
    "Submitted" => OrderStatus::Submitted,
    "ApiPending" => OrderStatus::ApiPending,
    "ApiCancelled" => OrderStatus::ApiCancelled,
    "Cancelled" => OrderStatus::Cancelled,
    "Filled" => OrderStatus::Filled,
    "Inactive" => OrderStatus::Inactive,
    _ => {
      warn!("Received unknown order status string: '{}'. Mapping to Inactive.", status_str);
      OrderStatus::Inactive
    }
  }
}

// --- Helper to read common contract fields ---
fn read_contract_fields(parser: &mut FieldParser, msg_version: i32, server_version: i32) -> Result<Contract, IBKRError> {
  let mut contract = Contract::new();

  if server_version >= min_server_ver::CONTRACT_CONID {
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
  if strike != 0.0 {
    contract.strike = Some(strike);
  }
  let right_str = parser.read_string()?;
  if !right_str.is_empty() && right_str != "?" {
    contract.right = match right_str.to_uppercase().as_str() {
      "C" | "CALL" => Some(OptionRight::Call),
      "P" | "PUT" => Some(OptionRight::Put),
      _ => { warn!("Unknown option right string: {}", right_str); None }
    };
  }
  if msg_version >= 32 { // Multiplier specific to OpenOrder v32+
    let multiplier = parser.read_string()?;
    if !multiplier.is_empty() { contract.multiplier = Some(multiplier); }
  }
  contract.exchange = parser.read_string()?;
  contract.currency = parser.read_string()?;
  if msg_version >= 2 { // LocalSymbol specific to OpenOrder v2+
    let local_symbol = parser.read_string()?;
    if !local_symbol.is_empty() { contract.local_symbol = Some(local_symbol); }
  }
  if server_version >= min_server_ver::TRADING_CLASS {
    let trading_class = parser.read_string()?;
    if !trading_class.is_empty() { contract.trading_class = Some(trading_class); }
  }
  if server_version >= min_server_ver::SEC_ID_TYPE {
    let sec_id_type_str = parser.read_string()?;
    contract.sec_id_type = crate::contract::SecIdType::from_str(&sec_id_type_str).ok();
    let sec_id = parser.read_string()?;
    if !sec_id.is_empty() { contract.sec_id = Some(sec_id); }
  }
  Ok(contract)
}

// --- Helper to read common order request fields ---
fn read_order_request_fields(parser: &mut FieldParser, msg_version: i32, server_version: i32) -> Result<OrderRequest, IBKRError> {
  let mut order_request = OrderRequest::default();
  let action_str = parser.read_string()?;
  order_request.side = match action_str.as_str() {
    "BUY" => OrderSide::Buy, "SELL" => OrderSide::Sell, "SSHORT" => OrderSide::SellShort,
    _ => { warn!("Unknown order side/action string: {}", action_str); return Err(IBKRError::ParseError(format!("Unknown order side: {}", action_str))); }
  };
  if server_version >= min_server_ver::FRACTIONAL_POSITIONS {
    let qty_str = parser.read_string()?;
    order_request.quantity = qty_str.parse().map_err(|e| IBKRError::ParseError(format!("Failed to parse fractional quantity '{}': {}", qty_str, e)))?;
  } else {
    order_request.quantity = parser.read_int()? as f64;
  }
  let order_type_str = parser.read_string()?;
  order_request.order_type = OrderType::from_str(&order_type_str).unwrap_or_else(|_| { warn!("Unknown order type string: {}", order_type_str); OrderType::None });
  let tif_str = parser.read_string()?;
  order_request.time_in_force = TimeInForce::from_str(&tif_str).unwrap_or_else(|_| { warn!("Unknown TimeInForce string: {}", tif_str); TimeInForce::Day });
  order_request.oca_group = Some(parser.read_string()?).filter(|s| !s.is_empty());
  order_request.account = Some(parser.read_string()?).filter(|s| !s.is_empty());
  if msg_version >= 7 {
    order_request.open_close = Some(parser.read_string()?).filter(|s| !s.is_empty());
    order_request.origin = parser.read_int()?;
  }
  order_request.order_ref = Some(parser.read_string()?).filter(|s| !s.is_empty());
  if msg_version >= 4 { order_request.transmit = parser.read_bool()?; }
  if msg_version >= 5 { order_request.parent_id = Some(parser.read_int()? as i64).filter(|&id| id != 0); }
  if msg_version >= 6 {
    order_request.block_order = parser.read_bool()?;
    order_request.sweep_to_fill = parser.read_bool()?;
    order_request.display_size = Some(parser.read_int()?).filter(|&ds| ds > 0);
    order_request.trigger_method = Some(parser.read_int()?).filter(|&tm| tm >= 0);
    order_request.outside_rth = parser.read_bool()?;
  }
  if msg_version >= 7 { order_request.hidden = parser.read_bool()?; }
  if msg_version >= 8 {
    let gat_str = parser.read_string()?;
    if !gat_str.is_empty() { warn!("Parsing for GoodAfterTime ('{}') not implemented.", gat_str); /* TODO */ }
  }
  if msg_version >= 9 {
    let gtd_str = parser.read_string()?;
    if !gtd_str.is_empty() { warn!("Parsing for GoodTillDate ('{}') not implemented.", gtd_str); /* TODO */ }
  }
  if msg_version >= 10 {
    order_request.rule_80a = Some(parser.read_string()?).filter(|s| !s.is_empty());
    order_request.all_or_none = parser.read_bool()?;
    order_request.min_quantity = Some(parser.read_int()?).filter(|&mq| mq > 0);
    order_request.percent_offset = parser.read_double().ok().filter(|&po| po != f64::MAX);
  }
  if msg_version >= 13 {
    order_request.fa_group = Some(parser.read_string()?).filter(|s| !s.is_empty());
    order_request.fa_method = Some(parser.read_string()?).filter(|s| !s.is_empty());
    order_request.fa_percentage = Some(parser.read_string()?).filter(|s| !s.is_empty());
  }
  // LmtPrice / AuxPrice position varies. Read them after FA fields for versions >= ~20
  if msg_version >= 20 { // Need to confirm exact version
    order_request.limit_price = parser.read_double().ok().filter(|&p| p != f64::MAX);
    order_request.aux_price = parser.read_double().ok().filter(|&p| p != f64::MAX);
  } else if msg_version < 4 { // Very old versions
    order_request.limit_price = parser.read_double().ok().filter(|&p| p != f64::MAX);
    order_request.aux_price = parser.read_double().ok().filter(|&p| p != f64::MAX);
  }
  // TODO: Parse TrailStopPrice, TrailingPercent, ShortSaleSlot, DiscretionaryAmt, etc. based on msg_version
  Ok(order_request)
}

// --- Helper to read common OrderState fields ---
fn read_order_state_fields(parser: &mut FieldParser, msg_version: i32, server_version: i32) -> Result<OrderState, IBKRError> {
  let mut order_state = OrderState::default();
  order_state.status = parse_order_status(&parser.read_string()?);
  if msg_version >= 2 {
    order_state.initial_margin_before = Some(parser.read_string()?).filter(|s| !s.is_empty() && s != "?");
    order_state.maintenance_margin_before = Some(parser.read_string()?).filter(|s| !s.is_empty() && s != "?");
    order_state.equity_with_loan_before = Some(parser.read_string()?).filter(|s| !s.is_empty() && s != "?");
  }
  if msg_version >= 10 { // Verify version
    order_state.initial_margin_change = Some(parser.read_string()?).filter(|s| !s.is_empty() && s != "?");
    order_state.maintenance_margin_change = Some(parser.read_string()?).filter(|s| !s.is_empty() && s != "?");
    order_state.equity_with_loan_change = Some(parser.read_string()?).filter(|s| !s.is_empty() && s != "?");
  }
  if msg_version >= 3 { // Verify version
    order_state.initial_margin_after = Some(parser.read_string()?).filter(|s| !s.is_empty() && s != "?");
    order_state.maintenance_margin_after = Some(parser.read_string()?).filter(|s| !s.is_empty() && s != "?");
    order_state.equity_with_loan_after = Some(parser.read_string()?).filter(|s| !s.is_empty() && s != "?");
  }
  if msg_version >= 10 { // Verify version
    order_state.commission = parser.read_double().ok().filter(|&c| c != f64::MAX);
    order_state.min_commission = parser.read_double().ok().filter(|&c| c != f64::MAX);
    order_state.max_commission = parser.read_double().ok().filter(|&c| c != f64::MAX);
    order_state.commission_currency = Some(parser.read_string()?).filter(|s| !s.is_empty());
  }
  if msg_version >= 14 { // Verify version
    order_state.warning_text = Some(parser.read_string()?).filter(|s| !s.is_empty());
  }
  // CompletedTime/Status are read specifically in process_completed_order if present
  Ok(order_state)
}


/// Process next valid ID message
pub fn process_next_valid_id(handler: &Arc<dyn OrderHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?; // Version is typically 1
  let id = parser.read_int()?;
  debug!("Parsed Next valid ID: {}", id);
  handler.next_valid_id(id); // Call handler
  Ok(())
}

/// Process order status message
pub fn process_order_status(handler: &Arc<dyn OrderHandler>, parser: &mut FieldParser, server_version: i32) -> Result<(), IBKRError> {
  let version = parser.read_int()?;
  let id = parser.read_int()?;
  let status_str = parser.read_string()?;
  let status_enum = parse_order_status(&status_str); // Parse the enum here

  let filled = if server_version >= min_server_ver::FRACTIONAL_POSITIONS {
    let filled_str = parser.read_string()?;
    filled_str.parse().map_err(|e| IBKRError::ParseError(format!("Failed to parse fractional filled qty '{}': {}", filled_str, e)))?
  } else {
    parser.read_double()?
  };

  let remaining = if server_version >= min_server_ver::FRACTIONAL_POSITIONS {
    let rem_str = parser.read_string()?;
    rem_str.parse().map_err(|e| IBKRError::ParseError(format!("Failed to parse fractional remaining qty '{}': {}", rem_str, e)))?
  } else {
    parser.read_double()?
  };

  let avg_fill_price = parser.read_double()?;
  let perm_id = if version >= 2 { parser.read_int()? } else { 0 };
  let parent_id = if version >= 3 { parser.read_int()? } else { 0 };
  let last_fill_price = if version >= 4 { parser.read_double()? } else { 0.0 };
  let client_id = if version >= 5 { parser.read_int()? } else { 0 };
  let why_held = parser.read_string()?;
  let mkt_cap_price = if server_version >= min_server_ver::MARKET_CAP_PRICE {
    parser.read_double().ok().filter(|&p| p != f64::MAX)
  } else {
    None
  };

  debug!(
    "Parsed Order Status: ID={}, Status={}, Filled={}, Remaining={}, AvgPrice={}, PermId={}, ParentId={}, LastFillPx={}, ClientId={}, WhyHeld={}, MktCapPx={:?}",
    id, status_str, filled, remaining, avg_fill_price, perm_id, parent_id, last_fill_price, client_id, why_held, mkt_cap_price
  );

  // Call handler with parsed values (Passing status_str for now, needs handler change for status_enum)
  // If handler expects OrderStatus, change the call:
  // handler.order_status(id, status_enum, filled, remaining, ...);
  handler.order_status(
    id,
    status_enum,
    filled,
    remaining,
    avg_fill_price,
    perm_id,
    parent_id,
    last_fill_price,
    client_id,
    &why_held,
    mkt_cap_price,
  );
  Ok(())
}

/// Process open order message
pub fn process_open_order(handler: &Arc<dyn OrderHandler>, parser: &mut FieldParser, server_version: i32) -> Result<(), IBKRError> {
  let msg_version = parser.read_int()?;
  let order_id = parser.read_int()?;
  debug!("Parsing Open Order: ID={}, Version={}", order_id, msg_version);

  let contract = read_contract_fields(parser, msg_version, server_version)?;
  let order_request = read_order_request_fields(parser, msg_version, server_version)?;
  let order_state = read_order_state_fields(parser, msg_version, server_version)?;

  // --- Parse additional fields specific to OpenOrder message structure ---
  if msg_version >= 15 { // Parse Delta-Neutral info if present
    if parser.read_bool()? {
      let _dn_con_id = parser.read_int()?;
      let _dn_delta = parser.read_double()?;
      let _dn_price = parser.read_double()?;
      // Note: This info isn't directly passed to handler.open_order currently.
      // Handler implementation would need access if required.
      debug!("Parsed Delta Neutral Contract Info for Order ID {}", order_id);
    }
  }
  if msg_version >= 21 { // Parse Algo info if present
    let algo_strategy = parser.read_string()?;
    if !algo_strategy.is_empty() {
      let algo_params_count = parser.read_int()?;
      let mut algo_params = Vec::new(); // Consume params
      for _ in 0..algo_params_count { algo_params.push((parser.read_string()?, parser.read_string()?)); }
      // TODO: Potentially add algo_strategy/params to OrderRequest if needed by handler
      if server_version >= min_server_ver::ALGO_ID {
        let _algo_id = parser.read_string()?; // Consume algoId
      }
      debug!("Parsed Algo Info for Order ID {}", order_id);
    }
  }
  if msg_version >= 24 { let _what_if = parser.read_bool()?; /* Consume */ }
  if msg_version >= 26 {
    let misc_options_count = parser.read_int()?; // Consume misc options
    for _ in 0..misc_options_count { let _ = (parser.read_string()?, parser.read_string()?); }
  }
  // TODO: Parse other version-dependent fields

  debug!(
    "Calling handler.open_order: ID={}, Symbol={}, Status={:?}",
    order_id, contract.symbol, order_state.status
  );

  // Call handler with the primary parsed structs
  handler.open_order(order_id, &contract, &order_request, &order_state);
  Ok(())
}

/// Process open order end message
pub fn process_open_order_end(handler: &Arc<dyn OrderHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  // let _version = parser.read_int()?; // If version added later
  debug!("Parsed Open Order End");
  handler.open_order_end(); // Call handler
  Ok(())
}

/// Process order bound message
pub fn process_order_bound(handler: &Arc<dyn OrderHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let order_id = parser.read_i64()?;
  let api_client_id = parser.read_int()?;
  let api_order_id = parser.read_int()?;
  debug!("Parsed Order Bound: OrderId={}, ApiClientId={}, ApiOrderId={}", order_id, api_client_id, api_order_id);
  handler.order_bound(order_id, api_client_id, api_order_id); // Call handler
  Ok(())
}

/// Process completed order message
pub fn process_completed_order(handler: &Arc<dyn OrderHandler>, parser: &mut FieldParser, server_version: i32) -> Result<(), IBKRError> {
  debug!("Parsing Completed Order");
  // Use server_version as proxy for message version, as CompletedOrder has no explicit version field
  let contract = read_contract_fields(parser, server_version, server_version)?;
  let order_request = read_order_request_fields(parser, server_version, server_version)?;
  let mut order_state = read_order_state_fields(parser, server_version, server_version)?;

  // Read trailing CompletedTime/CompletedStatus fields if server supports it
  if parser.remaining_fields() >= 2 {
    order_state.completed_time = Some(parser.read_string()?).filter(|s| !s.is_empty());
    order_state.completed_status = Some(parser.read_string()?).filter(|s| !s.is_empty());
    debug!("Parsed Completed Time: {:?}, Status: {:?}", order_state.completed_time, order_state.completed_status);
  } else {
    warn!("CompletedOrder message structure might be incomplete for server version {} (remaining fields: {})", server_version, parser.remaining_fields());
  }

  debug!(
    "Calling handler.completed_order: Symbol={}, Status={:?}, CompletedTime={:?}",
    contract.symbol, order_state.status, order_state.completed_time
  );

  // Call handler with parsed data
  handler.completed_order(&contract, &order_request, &order_state);
  Ok(())
}

/// Process completed orders end message
pub fn process_completed_orders_end(handler: &Arc<dyn OrderHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  // let _version = parser.read_int()?; // If version added later
  debug!("Parsed Completed Orders End");
  handler.completed_orders_end(); // Call handler
  Ok(())
}
