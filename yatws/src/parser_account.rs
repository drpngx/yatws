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

/// Process account value message
pub fn process_account_value(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let version = parser.read_int()?;

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
  let version = parser.read_int()?;

  // --- Parse Contract ---
  let mut contract = Contract::new();
  if version >= 6 {
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


  let position = parser.read_double()?;
  let market_price = parser.read_double()?;
  let market_value = parser.read_double()?;

  let mut average_cost = 0.0;
  let mut unrealized_pnl = 0.0;
  let mut realized_pnl = 0.0;

  if version >= 3 {
    average_cost = parser.read_double()?;
    unrealized_pnl = parser.read_double()?;
    realized_pnl = parser.read_double()?;
  }

  let mut account_name = String::new();
  if version >= 4 {
    account_name = parser.read_string()?;
  }

  // --- Call the handler ---
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
  // ---

  log::debug!("Parsed Portfolio Value: Account={}, Symbol={}, Pos={}, MktPx={}, MktVal={}, AvgCost={}, UnPNL={}, RealPNL={}",
              account_name, contract.symbol, position, market_price, market_value, average_cost, unrealized_pnl, realized_pnl);


  Ok(())
}

/// Process account update time message
pub fn process_account_update_time(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let time_stamp = parser.read_string()?;

  // --- Call the handler ---
  handler.account_update_time(&time_stamp);
  // ---

  log::debug!("Parsed Account Update Time: {}", time_stamp);

  Ok(())
}

/// Process account download end message
pub fn process_account_download_end(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let account = parser.read_string()?;

  // --- Call the handler ---
  handler.account_download_end(&account);
  // ---

  log::debug!("Parsed Account Download End: {}", account);
  Ok(())
}

/// Process managed accounts message
pub fn process_managed_accounts(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let accounts_list = parser.read_string()?;

  // --- Call the handler ---
  handler.managed_accounts(&accounts_list);
  // ---

  log::debug!("Parsed Managed Accounts: {}", accounts_list);
  Ok(())
}


/// Process position message
pub fn process_position(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let version = parser.read_int()?;
  let account = parser.read_string()?;

  // --- Parse Contract ---
  let mut contract = Contract::new();
  contract.con_id = parser.read_int()?;
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

  let position = parser.read_double()?;
  let mut avg_cost = 0.0;
  if version >= 3 {
    avg_cost = parser.read_double()?;
  }

  // --- Call the handler ---
  handler.position(&account, &contract, position, avg_cost);
  // ---

  log::debug!("Parsed Position: Account={}, Symbol={}, Position={}, AvgCost={}",
              account, contract.symbol, position, avg_cost);
  Ok(())
}

/// Process position end message
pub fn process_position_end(handler: &Arc<dyn AccountHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // --- Call the handler ---
  handler.position_end();
  // ---
  log::debug!("Parsed Position End");
  Ok(())
}


/// Process account summary message
pub fn process_account_summary(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let account = parser.read_string()?;
  let tag = parser.read_string()?;
  let value = parser.read_string()?;
  let currency = parser.read_string()?;

  // --- Call the handler ---
  handler.account_summary(req_id, &account, &tag, &value, &currency);
  // ---

  log::debug!("Parsed Account Summary: ReqId={}, Account={}, Tag={}, Value={}, Currency={}",
              req_id, account, tag, value, currency);
  Ok(())
}

/// Process account summary end message
pub fn process_account_summary_end(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;

  // --- Call the handler ---
  handler.account_summary_end(req_id);
  // ---

  log::debug!("Parsed Account Summary End: {}", req_id);
  Ok(())
}


/// Process PnL message
pub fn process_pnl(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let daily_pnl = parser.read_double()?;

  // Unrealized and realized PnL are optional
  let mut unrealized_pnl: Option<f64> = None;
  let mut realized_pnl: Option<f64> = None;

  // Check remaining fields before attempting to read optional values
  if parser.remaining_fields() > 0 {
    let u_pnl_str = parser.peek_string()?;
    if !u_pnl_str.is_empty() {
      unrealized_pnl = Some(parser.read_double()?);
    } else {
      parser.skip_field()?; // Skip empty unrealized PnL field
    }
  }
  if parser.remaining_fields() > 0 {
    let r_pnl_str = parser.peek_string()?;
    if !r_pnl_str.is_empty() {
      realized_pnl = Some(parser.read_double()?);
    } else {
      parser.skip_field()?; // Skip empty realized PnL field
    }
  }


  // --- Call the handler ---
  handler.pnl(req_id, daily_pnl, unrealized_pnl, realized_pnl);
  // ---

  log::debug!("Parsed PnL: ReqId={}, Daily={}, Unrealized={:?}, Realized={:?}",
              req_id, daily_pnl, unrealized_pnl, realized_pnl);
  Ok(())
}

/// Process PnL single message
pub fn process_pnl_single(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let pos = parser.read_int()?;
  let daily_pnl = parser.read_double()?;

  let mut unrealized_pnl: Option<f64> = None;
  let mut realized_pnl: Option<f64> = None;
  let mut value: f64 = 0.0; // Market value

  // Optional fields
  if parser.remaining_fields() > 0 {
    let u_pnl_str = parser.peek_string()?;
    if !u_pnl_str.is_empty() {
      unrealized_pnl = Some(parser.read_double()?);
    } else {
      parser.skip_field()?;
    }
  }
  if parser.remaining_fields() > 0 {
    let r_pnl_str = parser.peek_string()?;
    if !r_pnl_str.is_empty() {
      realized_pnl = Some(parser.read_double()?);
    } else {
      parser.skip_field()?;
    }
  }
  if parser.remaining_fields() > 0 {
    let val_str = parser.peek_string()?;
    if !val_str.is_empty() {
      value = parser.read_double()?;
    } else {
      parser.skip_field()?;
    }
  }

  // --- Call the handler ---
  handler.pnl_single(req_id, pos, daily_pnl, unrealized_pnl, realized_pnl, value);
  // ---

  log::debug!("Parsed PnLSingle: ReqId={}, Pos={}, Daily={}, Unrealized={:?}, Realized={:?}, Value={}",
              req_id, pos, daily_pnl, unrealized_pnl, realized_pnl, value);
  Ok(())
}


// --- Update other process functions similarly ---
// process_commission_report, process_position_multi, process_position_multi_end,
// process_account_update_multi, process_account_update_multi_end
// Need definitions for CommissionReport struct etc.

// Add a placeholder implementation for CommissionReport parsing if needed
pub fn process_commission_report(handler: &Arc<dyn AccountHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  log::warn!("Parsing CommissionReport not implemented yet.");
  // Implementation would parse commission report data and call handler.commission_report()
  Ok(())
}

pub fn process_position_multi(handler: &Arc<dyn AccountHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  log::warn!("Parsing PositionMulti not implemented yet.");
  // Implementation would parse position multi message and call handler.position_multi()
  Ok(())
}

pub fn process_position_multi_end(handler: &Arc<dyn AccountHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  log::warn!("Parsing PositionMultiEnd not implemented yet.");
  // Implementation would parse position multi end message and call handler.position_multi_end()
  Ok(())
}

pub fn process_account_update_multi(handler: &Arc<dyn AccountHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  log::warn!("Parsing AccountUpdateMulti not implemented yet.");
  // Implementation would parse account update multi message and call handler.account_update_multi()
  Ok(())
}

pub fn process_account_update_multi_end(handler: &Arc<dyn AccountHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  log::warn!("Parsing AccountUpdateMultiEnd not implemented yet.");
  // Implementation would parse account update multi end message and call handler.account_update_multi_end()
  Ok(())
}

/// Process execution data message
pub fn process_execution_data(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let version = parser.read_int()?;

  let mut req_id = -1;
  if version >= 7 {
    req_id = parser.read_int()?;
  }

  let order_id = parser.read_int()?;

  // Read contract fields
  let mut contract = Contract::new();

  if version >= 5 {
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

  if version >= 9 {
    let multiplier = parser.read_string()?;
    if !multiplier.is_empty() {
      contract.multiplier = Some(multiplier);
    }
  }

  contract.exchange = parser.read_string()?;
  contract.currency = parser.read_string()?;

  let local_symbol = parser.read_string()?;
  if !local_symbol.is_empty() {
    contract.local_symbol = Some(local_symbol);
  }

  if version >= 10 {
    let trading_class = parser.read_string()?;
    if !trading_class.is_empty() {
      contract.trading_class = Some(trading_class);
    }
  }

  // Read execution fields
  let mut execution = Execution {
    execution_id: parser.read_string()?,
    order_id: order_id.to_string(),
    symbol: contract.symbol.clone(),
    side: match parser.read_string()?.as_str() {
      "BOT" => OrderSide::Buy,
      "SLD" => OrderSide::Sell,
      "SSHORT" => OrderSide::SellShort,
      _ => OrderSide::Buy,
    },
    quantity: parser.read_double()?,
    price: parser.read_double()?,
    time: Utc::now(), // Default time
    commission: 0.0,
    exchange: parser.read_string()?,
    account: parser.read_string()?,
  };

  if version >= 6 {
    // More execution fields can be parsed here
  }

  log::debug!("Execution Data: ReqID={}, OrderID={}, Symbol={}, Side={}, Quantity={}, Price={}",
         req_id, order_id, contract.symbol, execution.side, execution.quantity, execution.price);

  Ok(())
}

/// Process execution data end message
pub fn process_execution_data_end(handler: &Arc<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;

  log::debug!("Execution Data End: {}", req_id);

  Ok(())
}
