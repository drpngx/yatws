// yatws/src/parser_account.rs

use std::str::FromStr;

use crate::handler::AccountHandler;

use crate::base::IBKRError;
use crate::protocol_dec_parser::FieldParser;
use crate::contract::{Contract, SecType, OptionRight};

/// Process account value message
pub fn process_account_value(handler: &mut Box<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _msg_type = parser.read_int()?; // Skip message type
  let version = parser.read_int()?;

  let key = parser.read_string()?;
  let value = parser.read_string()?;
  let currency = parser.read_string()?;

  let mut account_name = String::new();
  if version >= 2 {
    account_name = parser.read_string()?;
  }

  log::debug!("Account Value: Key={}, Value={}, Currency={}, Account={}",
         key, value, currency, account_name);

  Ok(())
}

/// Process account update time message
pub fn process_account_update_time(handler: &mut Box<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _msg_type = parser.read_int()?; // Skip message type
  let _version = parser.read_int()?;
  let time_stamp = parser.read_string()?;

  log::debug!("Account Update Time: {}", time_stamp);

  Ok(())
}

/// Process portfolio value message
pub fn process_portfolio_value(handler: &mut Box<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _msg_type = parser.read_int()?; // Skip message type
  let version = parser.read_int()?;

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

  if version >= 7 {
    let multiplier = parser.read_string()?;
    if !multiplier.is_empty() {
      contract.multiplier = Some(multiplier);
    }

    contract.primary_exchange = Some(parser.read_string()?);
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

  log::debug!("Portfolio Value: Symbol={}, Position={}, MarketPrice={}, MarketValue={}, UnrealizedPnL={}",
         contract.symbol, position, market_price, market_value, unrealized_pnl);

  Ok(())
}

/// Process managed accounts message
pub fn process_managed_accounts(handler: &mut Box<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _msg_type = parser.read_int()?; // Skip message type
  let _version = parser.read_int()?;

  let accounts_list = parser.read_string()?;
  let accounts: Vec<&str> = accounts_list.split(',').collect();

  log::debug!("Managed Accounts: {}", accounts_list);

  Ok(())
}

/// Process position message
pub fn process_position(handler: &mut Box<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _msg_type = parser.read_int()?; // Skip message type
  let version = parser.read_int()?;

  let account = parser.read_string()?;

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
  if !right_str.is_empty() {
    contract.right = match right_str.as_str() {
      "C" => Some(OptionRight::Call),
      "P" => Some(OptionRight::Put),
      _ => None,
    };
  }

  let multiplier = parser.read_string()?;
  if !multiplier.is_empty() {
    contract.multiplier = Some(multiplier);
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

  let position = parser.read_double()?;

  let mut avg_cost = 0.0;
  if version >= 3 {
    avg_cost = parser.read_double()?;
  }

  log::debug!("Position: Account={}, Symbol={}, Position={}, AvgCost={}",
         account, contract.symbol, position, avg_cost);

  Ok(())
}

/// Process account download end message
pub fn process_account_download_end(handler: &mut Box<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _msg_type = parser.read_int()?; // Skip message type
  let _version = parser.read_int()?;
  let account = parser.read_string()?;

  log::debug!("Account Download End: {}", account);

  Ok(())
}

/// Process commission report message
pub fn process_commission_report(handler: &mut Box<dyn AccountHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse commission report data
  Ok(())
}

/// Process account summary message
pub fn process_account_summary(handler: &mut Box<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _msg_type = parser.read_int()?; // Skip message type
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let account = parser.read_string()?;
  let tag = parser.read_string()?;
  let value = parser.read_string()?;
  let currency = parser.read_string()?;

  log::debug!("Account Summary: ReqId={}, Account={}, Tag={}, Value={}, Currency={}",
         req_id, account, tag, value, currency);

  Ok(())
}

/// Process account summary end message
pub fn process_account_summary_end(handler: &mut Box<dyn AccountHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _msg_type = parser.read_int()?; // Skip message type
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;

  log::debug!("Account Summary End: {}", req_id);

  Ok(())
}

/// Process position multi message
pub fn process_position_multi(handler: &mut Box<dyn AccountHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse position multi message
  Ok(())
}

/// Process position multi end message
pub fn process_position_multi_end(handler: &mut Box<dyn AccountHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse position multi end message
  Ok(())
}

/// Process account update multi message
pub fn process_account_update_multi(handler: &mut Box<dyn AccountHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse account update multi message
  Ok(())
}

/// Process account update multi end message
pub fn process_account_update_multi_end(handler: &mut Box<dyn AccountHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse account update multi end message
  Ok(())
}

/// Process PnL message
pub fn process_pnl(handler: &mut Box<dyn AccountHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse PnL message
  Ok(())
}

/// Process PnL single message
pub fn process_pnl_single(handler: &mut Box<dyn AccountHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse PnL single message
  Ok(())
}

/// Process position end message
pub fn process_position_end(handler: &mut Box<dyn AccountHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  log::debug!("Position End");
  Ok(())
}
