// yatws/src/parser_account.rs
use std::sync::Arc;
use std::str::FromStr;
use chrono::Utc;
use crate::handler::AccountHandler;

use crate::base::IBKRError;
use crate::protocol_dec_parser::FieldParser;
use crate::contract::{Contract, SecType, OptionRight}; // Keep imports needed for parsing
use crate::account::Execution;
use crate::order::OrderSide;
use crate::protocol_dec_parser::{parse_tws_date_time, parse_tws_date_or_month, parse_opt_tws_date_or_month};

/// Process account value message
pub fn process_account_value(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let version = parser.read_int(false)?;

  let key = parser.read_string()?;
  let value = parser.read_string()?;
  let currency = parser.read_string()?;

  let mut account_name = String::new();
  if version >= 2 {
    account_name = parser.read_string()?;
  }

  // --- Call the handler ---
  let currency_opt = if currency.is_empty() { None } else { Some(currency.as_str()) };
  handler.account_value(&key, &value, currency_opt, &account_name);
  // ---

  // Original log can be kept for debugging if desired
  log::debug!("Parsed Account Value: Key={}, Value={}, Currency={}, Account={}",
              key, value, currency, account_name);

  Ok(())
}

/// Process portfolio value message
pub fn process_portfolio_value(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let version = parser.read_int(false)?;

  // --- Parse Contract ---
  let mut contract = Contract::new();
  if version >= 6 {
    contract.con_id = parser.read_int(false)?;
  }
  contract.symbol = parser.read_string()?;
  let sec_type_str = parser.read_string()?;
  contract.sec_type = SecType::from_str(&sec_type_str).unwrap_or(SecType::Stock);
  let expiry = parser.read_string()?;
  if !expiry.is_empty() {
    contract.last_trade_date_or_contract_month = Some(parse_tws_date_or_month(&expiry)?);
  }
  let strike = parser.read_double(false)?;
  if strike > 0.0 { // Use > 0.0 check as per typical TWS API examples
    contract.strike = Some(strike);
  }
  let right_str = parser.read_string()?;
  if !right_str.is_empty() && right_str != "0" && right_str != "?" { // Check for valid right strings
    contract.right = match right_str.as_str() {
      "C" | "CALL" => Some(OptionRight::Call), // Handle variations
      "P" | "PUT" => Some(OptionRight::Put),
      _ => {
        log::warn!("Unknown option right string: {}", right_str);
        None
      },
    };
  }
  if version >= 7 {
    let multiplier_str = parser.read_string()?;
    if !multiplier_str.is_empty() {
      contract.multiplier = Some(multiplier_str);
    }
    let prim_exch = parser.read_string()?;
    if !prim_exch.is_empty() { // Store primary exchange if provided
      contract.primary_exchange = Some(prim_exch);
    }
  }
  contract.currency = parser.read_string()?;
  if version >= 2 {
    let local_symbol = parser.read_string()?;
    if !local_symbol.is_empty() {
      contract.local_symbol = Some(local_symbol);
    }
  }
  if version >= 8 {
    let trading_class = parser.read_string()?;
    if !trading_class.is_empty() {
      contract.trading_class = Some(trading_class);
    }
  }
  // --- End Parse Contract ---

  let position = parser.read_double(false)?;
  let market_price = parser.read_double(false)?;
  let market_value = parser.read_double(false)?;

  let mut average_cost = 0.0;
  let mut unrealized_pnl = 0.0;
  let mut realized_pnl = 0.0;

  if version >= 3 {
    average_cost = parser.read_double(false)?;
    unrealized_pnl = parser.read_double(false)?;
    realized_pnl = parser.read_double(false)?;
  }

  let mut account_name = String::new();
  if version >= 4 {
    account_name = parser.read_string()?;
  }

  handler.portfolio_value(
    &contract,
    position,
    market_price,
    market_value,
    average_cost,
    unrealized_pnl,
    realized_pnl,
    &account_name,
  );

  log::debug!("Parsed Portfolio Value: Account={}, Symbol={}, Pos={}, MktPx={}, MktVal={}, AvgCost={}, UnPNL={}, RealPNL={}",
              account_name, contract.symbol, position, market_price, market_value, average_cost, unrealized_pnl, realized_pnl);

  Ok(())
}

/// Process account update time message
pub fn process_account_update_time(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int(false)?;
  let time_stamp = parser.read_string()?;

  handler.account_update_time(&time_stamp);

  log::debug!("Parsed Account Update Time: {}", time_stamp);

  Ok(())
}

/// Process account download end message
pub fn process_account_download_end(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int(false)?;
  let account = parser.read_string()?;

  handler.account_download_end(&account);

  log::debug!("Parsed Account Download End: {}", account);
  Ok(())
}

/// Process managed accounts message
pub fn process_managed_accounts(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int(false)?;
  let accounts_list = parser.read_string()?;

  handler.managed_accounts(&accounts_list);

  log::debug!("Parsed Managed Accounts: {}", accounts_list);
  Ok(())
}


/// Process position message
pub fn process_position(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let version = parser.read_int(false)?;
  let account = parser.read_string()?;

  // --- Parse Contract ---
  let mut contract = Contract::new();
  contract.con_id = parser.read_int(false)?;
  contract.symbol = parser.read_string()?;
  let sec_type_str = parser.read_string()?;
  contract.sec_type = SecType::from_str(&sec_type_str).unwrap_or(SecType::Stock);
  let expiry = parser.read_string()?;
  if !expiry.is_empty() {
    contract.last_trade_date_or_contract_month = Some(parse_tws_date_or_month(&expiry)?);
  }
  let strike = parser.read_double(false)?;
  if strike > 0.0 {
    contract.strike = Some(strike);
  }
  let right_str = parser.read_string()?;
  if !right_str.is_empty() && right_str != "0" && right_str != "?" {
    contract.right = match right_str.as_str() {
      "C" | "CALL" => Some(OptionRight::Call),
      "P" | "PUT" => Some(OptionRight::Put),
      _ => {
        log::warn!("Unknown option right string: {}", right_str);
        None
      },
    };
  }
  let multiplier_str = parser.read_string()?;
  if !multiplier_str.is_empty() {
    contract.multiplier = Some(multiplier_str);
  }
  contract.exchange = parser.read_string()?;
  contract.currency = parser.read_string()?;
  let local_symbol = parser.read_string()?;
  if !local_symbol.is_empty() {
    contract.local_symbol = Some(local_symbol);
  }
  if version >= 2 {
    let trading_class = parser.read_string()?;
    if !trading_class.is_empty() {
      contract.trading_class = Some(trading_class);
    }
  }
  // --- End Parse Contract ---

  let position = parser.read_double(false)?;
  let mut avg_cost = 0.0;
  if version >= 3 {
    avg_cost = parser.read_double(false)?;
  }

  handler.position(&account, &contract, position, avg_cost);

  log::debug!("Parsed Position: Account={}, Symbol={}, Position={}, AvgCost={}",
              account, contract.symbol, position, avg_cost);
  Ok(())
}

/// Process position end message
pub fn process_position_end(handler: &Arc<dyn AccountHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  handler.position_end();
  log::debug!("Parsed Position End");
  Ok(())
}


/// Process account summary message
pub fn process_account_summary(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int(false)?;
  let req_id = parser.read_int(false)?;
  let account = parser.read_string()?;
  let tag = parser.read_string()?;
  let value = parser.read_string()?;
  let currency = parser.read_string()?;

  handler.account_summary(req_id, &account, &tag, &value, &currency);

  log::debug!("Parsed Account Summary: ReqId={}, Account={}, Tag={}, Value={}, Currency={}",
              req_id, account, tag, value, currency);
  Ok(())
}

/// Process account summary end message
pub fn process_account_summary_end(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int(false)?;
  let req_id = parser.read_int(false)?;

  handler.account_summary_end(req_id);

  log::debug!("Parsed Account Summary End: {}", req_id);
  Ok(())
}


/// Process PnL message
pub fn process_pnl(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int(false)?;
  let req_id = parser.read_int(false)?;
  let daily_pnl = parser.read_double(false)?;

  // Unrealized and realized PnL are optional
  let mut unrealized_pnl: Option<f64> = None;
  let mut realized_pnl: Option<f64> = None;

  // Check remaining fields before attempting to read optional values
  if parser.remaining_fields() > 0 {
    let u_pnl_str = parser.peek_string()?;
    if !u_pnl_str.is_empty() {
      unrealized_pnl = Some(parser.read_double(false)?);
    } else {
      parser.skip_field()?; // Skip empty unrealized PnL field
    }
  }
  if parser.remaining_fields() > 0 {
    let r_pnl_str = parser.peek_string()?;
    if !r_pnl_str.is_empty() {
      realized_pnl = Some(parser.read_double(false)?);
    } else {
      parser.skip_field()?; // Skip empty realized PnL field
    }
  }

  handler.pnl(req_id, daily_pnl, unrealized_pnl, realized_pnl);

  log::debug!("Parsed PnL: ReqId={}, Daily={}, Unrealized={:?}, Realized={:?}",
              req_id, daily_pnl, unrealized_pnl, realized_pnl);
  Ok(())
}

/// Process PnL single message
pub fn process_pnl_single(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Example: 95·11·20·-2.2151342773440774·-2.2150912773440723·1.7976931348623157E308·4920.799865722656·
  let req_id = parser.read_int(false)?;
  let pos = parser.read_int(false)?;
  let daily_pnl = parser.read_double(false)?;

  let mut unrealized_pnl: Option<f64> = None;
  let mut realized_pnl: Option<f64> = None;
  let mut value: f64 = 0.0; // Market value

  // Optional fields
  if parser.remaining_fields() > 0 {
    let u_pnl_str = parser.peek_string()?;
    if !u_pnl_str.is_empty() {
      unrealized_pnl = Some(parser.read_double(false)?);
    } else {
      parser.skip_field()?;
    }
  }
  if parser.remaining_fields() > 0 {
    let r_pnl_str = parser.peek_string()?;
    if !r_pnl_str.is_empty() {
      realized_pnl = Some(parser.read_double(false)?);
    } else {
      parser.skip_field()?;
    }
  }
  if parser.remaining_fields() > 0 {
    let val_str = parser.peek_string()?;
    if !val_str.is_empty() {
      value = parser.read_double(false)?;
    } else {
      parser.skip_field()?;
    }
  }

  handler.pnl_single(req_id, pos, daily_pnl, unrealized_pnl, realized_pnl, value);

  log::debug!("Parsed PnLSingle: ReqId={}, Pos={}, Daily={}, Unrealized={:?}, Realized={:?}, Value={}",
              req_id, pos, daily_pnl, unrealized_pnl, realized_pnl, value);
  Ok(())
}


// Add a placeholder implementation for CommissionReport parsing if needed
pub fn process_commission_report(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int(false)?;
  let exec_id = parser.read_string().map_err(|e| IBKRError::ParseError(format!("CommissionReport exec_id: {}", e)))?;
  let commission = parser.read_double(false).map_err(|e| IBKRError::ParseError(format!("CommissionReport comm: {}", e)))?;
  let currency = parser.read_string().map_err(|e| IBKRError::ParseError(format!("CommissionReport currency: {}", e)))?;
  let yld = parser.read_double_max(false).map_err(|e| IBKRError::ParseError(format!("CommissionReport yield: {}", e)))?;
  let yld_red = parser.read_double_max(false).map_err(|e| IBKRError::ParseError(format!("CommissionReport yield redemption: {}", e)))?;

  handler.commission_report(&exec_id, commission, &currency, yld, yld_red);
  Ok(())
}

pub fn process_position_multi(_handler: &Arc<dyn AccountHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  log::warn!("Parsing PositionMulti not implemented yet.");
  // Implementation would parse position multi message and call handler.position_multi()
  Ok(())
}

pub fn process_position_multi_end(_handler: &Arc<dyn AccountHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  log::warn!("Parsing PositionMultiEnd not implemented yet.");
  // Implementation would parse position multi end message and call handler.position_multi_end()
  Ok(())
}

pub fn process_account_update_multi(_handler: &Arc<dyn AccountHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  log::warn!("Parsing AccountUpdateMulti not implemented yet.");
  // Implementation would parse account update multi message and call handler.account_update_multi()
  Ok(())
}

pub fn process_account_update_multi_end(_handler: &Arc<dyn AccountHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  log::warn!("Parsing AccountUpdateMultiEnd not implemented yet.");
  // Implementation would parse account update multi end message and call handler.account_update_multi_end()
  Ok(())
}

/// Process execution data message
pub fn process_execution_data(
  handler: &Arc<dyn AccountHandler>, // Uses AccountHandler
  parser: &mut FieldParser,
  server_version: i32, // Need server_version for newer fields
) -> Result<(), IBKRError> {

  // Version Handling: Java EClientSocket reads version *only* if server < LAST_LIQUIDITY.
  // Otherwise, it assumes version >= 7. Let's replicate this.
  let msg_version = server_version; // Assume high version initially

  let mut req_id = -1; // Default for messages before version 7
  if msg_version >= 7 {
    // If version is high enough (or assumed high enough), read req_id
    req_id = parser.read_int(false).map_err(|e| IBKRError::ParseError(format!("ExecDetails ReqId: {}", e)))?;
    log::trace!("  Req ID: {}", req_id);
  }

  let order_id = parser.read_int(false).map_err(|e| IBKRError::ParseError(format!("ExecDetails OrderId: {}", e)))?;
  log::trace!("  Order ID: {}", order_id);

  // --- Read contract fields ---
  let mut contract = Contract::new();
  if msg_version >= 5 {
    contract.con_id = parser.read_int(false).map_err(|e| IBKRError::ParseError(format!("ExecDetails Contract ConId: {}", e)))?;
    log::trace!("  Contract ConID: {}", contract.con_id);
  }
  contract.symbol = parser.read_string().map_err(|e| IBKRError::ParseError(format!("ExecDetails Contract Symbol: {}", e)))?;
  contract.sec_type = parser.read_string().map_err(|e| IBKRError::ParseError(format!("ExecDetails Contract SecType Str: {}", e)))?
    .parse().unwrap_or(SecType::Stock);
  contract.last_trade_date_or_contract_month = parse_opt_tws_date_or_month(parser.read_string_opt().map_err(|e| IBKRError::ParseError(format!("ExecDetails Contract LastTradeDate: {}", e)))?)?;
  contract.strike = parser.read_double_max(false).map_err(|e| IBKRError::ParseError(format!("ExecDetails Contract Strike: {}", e)))?;
  contract.right = parser.read_string().map_err(|e| IBKRError::ParseError(format!("ExecDetails Contract Right Str: {}", e)))?.parse().ok();
  if msg_version >= 9 {
    contract.multiplier = parser.read_string_opt().map_err(|e| IBKRError::ParseError(format!("ExecDetails Contract Multiplier: {}", e)))?;
  }
  contract.exchange = parser.read_string().map_err(|e| IBKRError::ParseError(format!("ExecDetails Contract Exchange: {}", e)))?;
  contract.currency = parser.read_string().map_err(|e| IBKRError::ParseError(format!("ExecDetails Contract Currency: {}", e)))?;
  contract.local_symbol = parser.read_string_opt().map_err(|e| IBKRError::ParseError(format!("ExecDetails Contract LocalSymbol: {}", e)))?;
  if msg_version >= 10 {
    contract.trading_class = parser.read_string_opt().map_err(|e| IBKRError::ParseError(format!("ExecDetails Contract TradingClass: {}", e)))?;
  }
  log::trace!("  Contract Symbol: {}, SecType: {:?}, Expiry: {:?}, Strike: {:?}, Right: {:?}, Exch: {}, Curr: {}",
              contract.symbol, contract.sec_type, contract.last_trade_date_or_contract_month, contract.strike, contract.right, contract.exchange, contract.currency);
  // --- End Contract ---


  // --- Read Execution fields ---
  let execution_id = parser.read_string().map_err(|e| IBKRError::ParseError(format!("ExecDetails ExecId: {}", e)))?;
  let time_str = parser.read_string().map_err(|e| IBKRError::ParseError(format!("ExecDetails TimeStr: {}", e)))?;
  let account_id = parser.read_string().map_err(|e| IBKRError::ParseError(format!("ExecDetails AccountId: {}", e)))?;
  let exec_exchange = parser.read_string().map_err(|e| IBKRError::ParseError(format!("ExecDetails ExecExchange: {}", e)))?;
  let side_str = parser.read_string().map_err(|e| IBKRError::ParseError(format!("ExecDetails SideStr: {}", e)))?;
  let quantity = parser.read_decimal_max().map_err(|e| IBKRError::ParseError(format!("ExecDetails Quantity: {}", e)))?.unwrap_or(0.); // Shares/Quantity is Decimal
  let price = parser.read_double(false).map_err(|e| IBKRError::ParseError(format!("ExecDetails Price: {}", e)))?;

  let mut perm_id = 0;
  if msg_version >= 2 {
    perm_id = parser.read_int(false).map_err(|e| IBKRError::ParseError(format!("ExecDetails PermId: {}", e)))?;
  }
  let mut client_id = 0;
  if msg_version >= 3 {
    client_id = parser.read_int(false).map_err(|e| IBKRError::ParseError(format!("ExecDetails ClientId: {}", e)))?;
  }
  let mut liquidation_int = 0;
  if msg_version >= 4 {
    liquidation_int = parser.read_int(false).map_err(|e| IBKRError::ParseError(format!("ExecDetails Liquidation: {}", e)))?;
  }
  let mut cum_qty = 0.0;
  let mut avg_price = 0.0;
  if msg_version >= 6 {
    cum_qty = parser.read_decimal_max().map_err(|e| IBKRError::ParseError(format!("ExecDetails CumQty: {}", e)))?.unwrap_or(0.);
    avg_price = parser.read_double(false).map_err(|e| IBKRError::ParseError(format!("ExecDetails AvgPrice: {}", e)))?;
  }
  let mut order_ref = None;
  if msg_version >= 8 {
    order_ref = parser.read_string_opt().map_err(|e| IBKRError::ParseError(format!("ExecDetails OrderRef: {}", e)))?;
  }
  let mut ev_rule = None;
  let mut ev_multiplier = None;
  if msg_version >= 9 {
    ev_rule = parser.read_string_opt().map_err(|e| IBKRError::ParseError(format!("ExecDetails EvRule: {}", e)))?;
    ev_multiplier = parser.read_double_max(false).map_err(|e| IBKRError::ParseError(format!("ExecDetails EvMultiplier: {}", e)))?;
  }
  let model_code = parser.read_string_opt().map_err(|e| IBKRError::ParseError(format!("ExecDetails ModelCode: {}", e)))?;
  let last_liquidity = parser.read_int_max(false).map_err(|e| IBKRError::ParseError(format!("ExecDetails LastLiquidity: {}", e)))?;
  let pending_price_revision = parser.read_bool(false).map_err(|e| IBKRError::ParseError(format!("ExecDetails PendingPriceRevision: {}", e)))?.into();

  log::debug!("Parsed Execution Detail: ExecID={}, OrderID={}, Account={}, Symbol={}, Side={}, Qty={}, Price={}, Time={}",
              execution_id, order_id, account_id, contract.symbol, side_str, quantity, price, time_str);

  // Create Execution struct
  let execution = Execution {
    execution_id,
    order_id: order_id.to_string(), // Convert order_id to string if Execution struct expects it
    perm_id,
    client_id,
    symbol: contract.symbol.clone(), // Keep convenience field if Execution struct has it
    contract, // Pass the full contract
    side: OrderSide::from_str(&side_str).unwrap_or_else(|e| {
      log::warn!("Failed to parse execution side '{}': {}", side_str, e);
      OrderSide::Buy // Default or error handling
    }),
    quantity,
    price,
    time: parse_tws_date_time(&time_str).unwrap_or_else(|e| {
      log::warn!("Failed to parse execution time '{}', using current time: {}", time_str, e);
      Utc::now()
    }),
    avg_price,
    commission: None, // Commission comes in a separate message
    commission_currency: None,
    exchange: exec_exchange,
    account_id,
    liquidation: liquidation_int != 0,
    cum_qty,
    order_ref,
    ev_rule,
    ev_multiplier,
    model_code,
    last_liquidity,
    pending_price_revision,
  };

  handler.execution_details(req_id, &execution.contract, &execution);

  Ok(())
}

/// Process execution data end message
pub fn process_execution_data_end(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int(false)?;
  let req_id = parser.read_int(false)?;

  log::debug!("Execution Data End: {}", req_id);

  handler.execution_details_end(req_id);
  Ok(())
}
