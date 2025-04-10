// yatws/src/parser_data_ref.rs
use std::str::FromStr;

use crate::handler::ReferenceDataHandler;

use crate::base::IBKRError;
use crate::protocol_dec_parser::FieldParser;
use crate::contract::{ContractDetails, SecType, OptionRight};

/// Process bond contract data message
pub fn process_bond_contract_data(handler: &mut Box<dyn ReferenceDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse bond contract data
  Ok(())
}

/// Process tick option computation message
pub fn process_tick_option_computation(handler: &mut Box<dyn ReferenceDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse option computation data
  Ok(())
}

/// Process contract data end message
pub fn process_contract_data_end(handler: &mut Box<dyn ReferenceDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _msg_type = parser.read_int()?; // Skip message type
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;

  log::debug!("Contract Data End: {}", req_id);

  Ok(())
}

/// Process security definition option parameter message
pub fn process_security_definition_option_parameter(handler: &mut Box<dyn ReferenceDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse security definition option parameter message
  Ok(())
}

/// Process security definition option parameter end message
pub fn process_security_definition_option_parameter_end(handler: &mut Box<dyn ReferenceDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse security definition option parameter end message
  Ok(())
}

/// Process soft dollar tiers message
pub fn process_soft_dollar_tiers(handler: &mut Box<dyn ReferenceDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse soft dollar tiers message
  Ok(())
}

/// Process family codes message
pub fn process_family_codes(handler: &mut Box<dyn ReferenceDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse family codes message
  Ok(())
}

/// Process symbol samples message
pub fn process_symbol_samples(handler: &mut Box<dyn ReferenceDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse symbol samples message
  Ok(())
}

/// Process smart components message
pub fn process_smart_components(handler: &mut Box<dyn ReferenceDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse smart components message
  Ok(())
}

/// Process market rule message
pub fn process_market_rule(handler: &mut Box<dyn ReferenceDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse market rule message
  Ok(())
}

/// Process historical schedule message
pub fn process_historical_schedule(handler: &mut Box<dyn ReferenceDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse historical schedule message
  Ok(())
}

/// Process contract data message
pub fn process_contract_data(handler: &mut Box<dyn ReferenceDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _msg_type = parser.read_int()?; // Skip message type
  let version = parser.read_int()?;

  let mut req_id = -1;
  if version >= 3 {
    req_id = parser.read_int()?;
  }

  let mut contract_details = ContractDetails::default();

  contract_details.contract.symbol = parser.read_string()?;

  let sec_type_str = parser.read_string()?;
  contract_details.contract.sec_type = SecType::from_str(&sec_type_str).unwrap_or(SecType::Stock);

  // Parse expiry date
  let expiry = parser.read_string()?;
  if !expiry.is_empty() {
    contract_details.contract.last_trade_date_or_contract_month = Some(expiry);
  }

  contract_details.contract.strike = Some(parser.read_double()?);

  let right_str = parser.read_string()?;
  if !right_str.is_empty() {
    contract_details.contract.right = match right_str.as_str() {
      "C" => Some(OptionRight::Call),
      "P" => Some(OptionRight::Put),
      _ => None,
    };
  }

  contract_details.contract.exchange = parser.read_string()?;
  contract_details.contract.currency = parser.read_string()?;

  let local_symbol = parser.read_string()?;
  if !local_symbol.is_empty() {
    contract_details.contract.local_symbol = Some(local_symbol);
  }

  contract_details.market_name = parser.read_string()?;

  let trading_class = parser.read_string()?;
  if !trading_class.is_empty() {
    contract_details.contract.trading_class = Some(trading_class);
  }

  contract_details.contract.con_id = parser.read_int()?;
  contract_details.min_tick = parser.read_double()?;

  log::debug!("Contract Data: ReqID={}, Symbol={}, SecType={}, Exchange={}",
         req_id, contract_details.contract.symbol, sec_type_str, contract_details.contract.exchange);

  Ok(())
}
