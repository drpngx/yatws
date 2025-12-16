// yatws/src/parser_data_ref.rs
use std::str::FromStr;
use std::sync::Arc;
use std::collections::HashMap;
use crate::handler::ReferenceDataHandler;
use crate::base::IBKRError;
use crate::protocol_dec_parser::{FieldParser, parse_tws_date_time, parse_tws_date_or_month, parse_tws_date, parse_opt_tws_month, parse_opt_tws_date, parse_tws_time};
use crate::contract::{
  BondDetails, Contract, ContractDetails, SecType, OptionRight, SoftDollarTier, FamilyCode,
  ContractDescription, DepthMktDataDescription, PriceIncrement,
  HistoricalSession, TagValue, IneligibilityReason
};
use crate::min_server_ver::min_server_ver;
use log::{debug, warn};

fn read_last_trade_date(parser: &mut FieldParser, contract_details: &mut ContractDetails, is_bond: bool) -> Result<(), IBKRError> {
  let last_trade_date_or_contract_month = parser.read_str()?;
  if !last_trade_date_or_contract_month.is_empty() {
    if is_bond {
      if contract_details.bond_details.is_none() {
        contract_details.bond_details = Some(BondDetails::default());
      }
    }
    let parts: Vec<&str> = last_trade_date_or_contract_month
      .split(|c: char| c.is_whitespace() || c == '-')
      .filter(|s| !s.is_empty())
      .collect();

    if !parts.is_empty() {
      if is_bond {
        contract_details.bond_details.as_mut().unwrap().maturity = Some(parse_tws_date(parts[0])?);
      } else {
        contract_details.contract.last_trade_date_or_contract_month = Some(parse_tws_date_or_month(parts[0])?);
      }
    }
    if parts.len() > 1 {
      contract_details.contract.last_trade_time = Some(parse_tws_time(parts[1])?);
    }
    if is_bond && parts.len() > 2 {
      contract_details.time_zone_id = parts[2].to_string();
    }
  }
  Ok(())
}


/// Process bond contract data message
pub fn process_bond_contract_data(handler: &Arc<dyn ReferenceDataHandler>, parser: &mut FieldParser, server_version: i32) -> Result<(), IBKRError> {
  let version = if server_version < min_server_ver::SIZE_RULES { parser.read_int(false)? } else { 6 }; // Default to 6 for newer servers

  let mut req_id = -1;
  if version >= 3 {
    req_id = parser.read_int(false)?;
  }

  let mut contract_details = ContractDetails::default();

  if contract_details.bond_details.is_none() {
    contract_details.bond_details = Some(BondDetails::default());
  }
  contract_details.contract.symbol = parser.read_str()?.to_string();
  contract_details.contract.sec_type = SecType::Bond; // It's always Bond here
  contract_details.bond_details.as_mut().unwrap().cusip = parser.read_str()?.to_string();
  contract_details.bond_details.as_mut().unwrap().coupon = parser.read_double(false)?;
  read_last_trade_date(parser, &mut contract_details, true)?;
  contract_details.bond_details.as_mut().unwrap().issue_date = Some(parse_tws_date(parser.read_str()?)?);
  contract_details.bond_details.as_mut().unwrap().ratings = parser.read_str()?.to_string();
  contract_details.bond_details.as_mut().unwrap().bond_type = parser.read_str()?.to_string();
  contract_details.bond_details.as_mut().unwrap().coupon_type = parser.read_str()?.to_string();
  contract_details.bond_details.as_mut().unwrap().convertible = parser.read_bool(false)?;
  contract_details.bond_details.as_mut().unwrap().callable = parser.read_bool(false)?;
  contract_details.bond_details.as_mut().unwrap().puttable = parser.read_bool(false)?;
  contract_details.bond_details.as_mut().unwrap().desc_append = parser.read_str()?.to_string();
  contract_details.contract.exchange = parser.read_str()?.to_string();
  contract_details.contract.currency = parser.read_str()?.to_string();
  contract_details.market_name = parser.read_str()?.to_string();
  contract_details.contract.trading_class = parser.read_str_opt()?.map(|s| s.to_string());
  contract_details.contract.con_id = parser.read_int(false)?;
  contract_details.min_tick = parser.read_double(false)?;
  if server_version >= min_server_ver::MD_SIZE_MULTIPLIER && server_version < min_server_ver::SIZE_RULES {
    let _md_size_multiplier = parser.read_int(false)?; // Not used anymore
  }
  contract_details.order_types = parser.read_str()?.to_string();
  contract_details.valid_exchanges = parser.read_str()?.to_string();
  if version >= 2 {
    let dt = parser.read_str()?;
    contract_details.bond_details.as_mut().unwrap().next_option_date = Some(parse_tws_date(dt)?);
    contract_details.bond_details.as_mut().unwrap().next_option_type = Some(OptionRight::from_str(parser.read_str()?)?);
    contract_details.bond_details.as_mut().unwrap().next_option_partial = parser.read_bool(false)?;
    contract_details.bond_details.as_mut().unwrap().notes = parser.read_str()?.to_string();
  }
  if version >= 4 {
    contract_details.long_name = parser.read_str()?.to_string();
  }
  if version >= 6 {
    contract_details.ev_rule = parser.read_str()?.to_string();
    contract_details.ev_multiplier = parser.read_double(false)?;
  }
  if version >= 5 {
    let sec_id_list_count = parser.read_int(false)?;
    if sec_id_list_count > 0 {
      let mut sec_id_list = Vec::with_capacity(sec_id_list_count as usize);
      for _ in 0..sec_id_list_count {
        sec_id_list.push(TagValue {
          tag: parser.read_str()?.to_string(),
          value: parser.read_str()?.to_string(),
        });
      }
      contract_details.sec_id_list = sec_id_list;
    }
  }
  if server_version >= min_server_ver::AGG_GROUP {
    contract_details.agg_group = parser.read_int_max(false)?;
  }
  if server_version >= min_server_ver::MARKET_RULES {
    contract_details.market_rule_ids = parser.read_str()?.to_string();
  }
  if server_version >= min_server_ver::SIZE_RULES {
    contract_details.min_size = parser.read_decimal_max()?.unwrap_or(0.);
    contract_details.size_increment = parser.read_decimal_max()?.unwrap_or(0.);
    contract_details.suggested_size_increment = parser.read_decimal_max()?.unwrap_or(0.); // Assuming f64 mapping for Decimal
  }
  if server_version >= min_server_ver::BOND_ISSUERID {
    contract_details.contract.issuer_id = parser.read_str_opt()?.map(|s| s.to_string());
  }
  if server_version >= min_server_ver::BOND_ACCRUED_INTEREST { // Added missing BOND_ACCRUED_INTEREST check
    // contract_details.accrued_interest = parser.read_double_max(false)?;
    // The accrued interest is in Order. We may have to include Order.
    if let Some(acc_int) = parser.read_double_max(false)? {
      warn!("Ignoring bond accrued interest {}", acc_int);
    }
  }


  debug!("Bond Contract Data: ReqID={}, Symbol={}, ConID={}", req_id, contract_details.contract.symbol, contract_details.contract.con_id);
  handler.bond_contract_details(req_id, &contract_details);
  Ok(())
}

/// Process contract data end message
pub fn process_contract_data_end(handler: &Arc<dyn ReferenceDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int(false)?; // Version field exists but isn't usually used
  let req_id = parser.read_int(false)?;
  debug!("Contract Data End: ReqID={}", req_id);
  handler.contract_details_end(req_id);
  Ok(())
}

/// Process security definition option parameter message
pub fn process_security_definition_option_parameter(handler: &Arc<dyn ReferenceDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int(false)?;
  let exchange = parser.read_str()?;
  let underlying_con_id = parser.read_int(false)?;
  let trading_class = parser.read_str()?;
  let multiplier = parser.read_str()?;

  let expirations_size = parser.read_int(false)?;
  let mut expirations = Vec::with_capacity(expirations_size as usize);
  for _ in 0..expirations_size {
    expirations.push(parser.read_str()?.to_string());
  }

  let strikes_size = parser.read_int(false)?;
  let mut strikes = Vec::with_capacity(strikes_size as usize);
  for _ in 0..strikes_size {
    strikes.push(parser.read_double(false)?);
  }

  debug!("Security Definition Option Parameter: ReqID={}, Exchange={}, UnderlyingConID={}, TC={}, Mult={}, {} Expirations, {} Strikes",
         req_id, exchange, underlying_con_id, trading_class, multiplier, expirations.len(), strikes.len());

  handler.security_definition_option_parameter(
    req_id,
    exchange,
    underlying_con_id,
    trading_class,
    multiplier,
    &expirations,
    &strikes,
  );
  Ok(())
}

/// Process security definition option parameter end message
pub fn process_security_definition_option_parameter_end(handler: &Arc<dyn ReferenceDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int(false)?;
  debug!("Security Definition Option Parameter End: ReqID={}", req_id);
  handler.security_definition_option_parameter_end(req_id);
  Ok(())
}

/// Process soft dollar tiers message
pub fn process_soft_dollar_tiers(handler: &Arc<dyn ReferenceDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int(false)?;
  let n_tiers = parser.read_int(false)?;
  let mut tiers = Vec::with_capacity(n_tiers as usize);
  for _ in 0..n_tiers {
    tiers.push(SoftDollarTier {
      name: parser.read_str()?.to_string(),
      value: parser.read_str()?.to_string(),
      display_name: parser.read_str()?.to_string(), // Assuming display name is always present
    });
  }
  debug!("Soft Dollar Tiers: ReqID={}, Count={}", req_id, tiers.len());
  handler.soft_dollar_tiers(req_id, &tiers);
  Ok(())
}

/// Process family codes message
pub fn process_family_codes(handler: &Arc<dyn ReferenceDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let n_family_codes = parser.read_int(false)?;
  let mut family_codes = Vec::with_capacity(n_family_codes as usize);
  for _ in 0..n_family_codes {
    family_codes.push(FamilyCode {
      account_id: parser.read_str()?.to_string(), // Java reads account ID first
      family_code_str: parser.read_str()?.to_string(),
    });
  }
  debug!("Family Codes: Count={}", family_codes.len());
  handler.family_codes(&family_codes);
  Ok(())
}

/// Process symbol samples message
pub fn process_symbol_samples(handler: &Arc<dyn ReferenceDataHandler>, parser: &mut FieldParser, server_version: i32) -> Result<(), IBKRError> {
  let req_id = parser.read_int(false)?;
  let n_contract_descriptions = parser.read_int(false)?;
  let mut contract_descriptions = Vec::with_capacity(n_contract_descriptions as usize);

  for _ in 0..n_contract_descriptions {
    let mut contract = Contract::new();
    contract.con_id = parser.read_int(false)?;
    contract.symbol = parser.read_str()?.to_string();
    contract.sec_type = SecType::from_str(parser.read_str()?).unwrap_or(SecType::Stock);
    contract.primary_exchange = parser.read_str_opt()?.map(|s| s.to_string());
    contract.currency = parser.read_str()?.to_string();

    let n_derivative_sec_types = parser.read_int(false)?;
    let mut derivative_sec_types = Vec::with_capacity(n_derivative_sec_types as usize);
    for _ in 0..n_derivative_sec_types {
      derivative_sec_types.push(parser.read_str()?.to_string());
    }

    if server_version >= min_server_ver::BOND_ISSUERID {
      contract.description = parser.read_str_opt()?.map(|s| s.to_string());
      contract.issuer_id = parser.read_str_opt()?.map(|s| s.to_string());
    }


    contract_descriptions.push(ContractDescription {
      contract,
      derivative_sec_types,
    });
  }
  debug!("Symbol Samples: ReqID={}, Count={}", req_id, contract_descriptions.len());
  handler.symbol_samples(req_id, &contract_descriptions);
  Ok(())
}

/// Process market depth exchanges message
pub fn process_mkt_depth_exchanges(handler: &Arc<dyn ReferenceDataHandler>, parser: &mut FieldParser, server_version: i32) -> Result<(), IBKRError> {
  let n_descriptions = parser.read_int(false)?;
  let mut descriptions = Vec::with_capacity(n_descriptions as usize);
  for _ in 0..n_descriptions {
    if server_version >= min_server_ver::SERVICE_DATA_TYPE {
      descriptions.push(DepthMktDataDescription {
        exchange: parser.read_str()?.to_string(),
        sec_type: parser.read_str()?.to_string(),
        listing_exch: parser.read_str()?.to_string(), // Added field
        service_data_type: parser.read_str()?.to_string(),
        agg_group: parser.read_int_max(false)?, // Use read_int_max for optional int
      });
    } else {
      descriptions.push(DepthMktDataDescription {
        exchange: parser.read_str()?.to_string(),
        sec_type: parser.read_str()?.to_string(),
        listing_exch: "".to_string(), // Not available pre SERVICE_DATA_TYPE
        service_data_type: if parser.read_bool(false)? { "Deep2".to_string() } else { "Deep".to_string() },
        agg_group: None, // Not available pre SERVICE_DATA_TYPE
      });
    }
  }
  debug!("Market Depth Exchanges: Count={}", descriptions.len());
  handler.mkt_depth_exchanges(&descriptions);
  Ok(())
}

/// Process tick req params message
pub fn process_tick_req_params(handler: &Arc<dyn ReferenceDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int(false)?;
  let min_tick = parser.read_double(false)?;
  let bbo_exchange = parser.read_str()?;
  let snapshot_permissions = parser.read_int(false)?;

  log::debug!("Tick Req Params: ID={}, MinTick={}, BBOExch={}, Permissions={}", req_id, min_tick, bbo_exchange, snapshot_permissions);
  handler.tick_req_params(req_id, min_tick, bbo_exchange, snapshot_permissions);
  Ok(())
}

/// Process smart components message
pub fn process_smart_components(handler: &Arc<dyn ReferenceDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int(false)?;
  let n = parser.read_int(false)?;
  let mut components = HashMap::with_capacity(n as usize);
  for _ in 0..n {
    let bit_number = parser.read_int(false)?;
    let exchange = parser.read_str()?;
    // Read char - assumes single byte UTF-8 character
    let exchange_letter_str = parser.read_str()?;
    let exchange_letter = exchange_letter_str.chars().next().unwrap_or('?'); // Handle empty string case

    components.insert(bit_number, (exchange.to_string(), exchange_letter));
  }
  debug!("Smart Components: ReqID={}, Count={}", req_id, components.len());
  handler.smart_components(req_id, &components);
  Ok(())
}

/// Process market rule message
pub fn process_market_rule(handler: &Arc<dyn ReferenceDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let market_rule_id = parser.read_int(false)?;
  let n_price_increments = parser.read_int(false)?;
  let mut price_increments = Vec::with_capacity(n_price_increments as usize);
  for _ in 0..n_price_increments {
    price_increments.push(PriceIncrement {
      low_edge: parser.read_double(false)?,
      increment: parser.read_double(false)?,
    });
  }
  debug!("Market Rule: RuleID={}, Increments={}", market_rule_id, price_increments.len());
  handler.market_rule(market_rule_id, &price_increments);
  Ok(())
}

/// Process historical schedule message
pub fn process_historical_schedule(handler: &Arc<dyn ReferenceDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int(false)?;
  let start_date_time = parser.read_str()?;
  let end_date_time = parser.read_str()?;
  let time_zone = parser.read_str()?;

  let sessions_count = parser.read_int(false)?;
  let mut sessions = Vec::with_capacity(sessions_count as usize);
  for _ in 0..sessions_count {
    sessions.push(HistoricalSession {
      start_date_time: parse_tws_date_time(parser.read_str()?)?,
      end_date_time: parse_tws_date_time(parser.read_str()?)?,
      ref_date: parse_opt_tws_date(parser.read_str_opt()?)?,
    });
  }
  debug!("Historical Schedule: ReqID={}, Start={}, End={}, TZ={}, Sessions={}", req_id, start_date_time, end_date_time, time_zone, sessions.len());
  handler.historical_schedule(req_id, start_date_time, end_date_time, time_zone, &sessions);
  Ok(())
}


/// Process contract data message
pub fn process_contract_data(handler: &Arc<dyn ReferenceDataHandler>, parser: &mut FieldParser, server_version: i32) -> Result<(), IBKRError> {
  // Determine version based on server version (matching Java EDecoder logic)
  let version = if server_version < min_server_ver::SIZE_RULES { parser.read_int(false)? } else { 8 }; // Default to 8 for newer servers

  let mut req_id = -1;
  if version >= 3 {
    req_id = parser.read_int(false)?;
  }

  let mut contract_details = ContractDetails::default();

  contract_details.contract.symbol = parser.read_str()?.to_string();
  contract_details.contract.sec_type = SecType::from_str(parser.read_str()?).unwrap_or(SecType::Stock);
  read_last_trade_date(parser, &mut contract_details, false)?;
  if server_version >= min_server_ver::LAST_TRADE_DATE {
    contract_details.contract.last_trade_date = parse_opt_tws_date(parser.read_str_opt()?)?;
  }
  contract_details.contract.strike = Some(parser.read_double(false)?);
  let opt_right = parser.read_str()?;
  contract_details.contract.right = if opt_right.is_empty() { None } else { Some(OptionRight::from_str(opt_right)?) };
  contract_details.contract.exchange = parser.read_str()?.to_string();
  contract_details.contract.currency = parser.read_str()?.to_string();
  contract_details.contract.local_symbol = parser.read_str_opt()?.map(|s| s.to_string());
  contract_details.market_name = parser.read_str()?.to_string();
  contract_details.contract.trading_class = parser.read_str_opt()?.map(|s| s.to_string());
  contract_details.contract.con_id = parser.read_int(false)?;
  contract_details.min_tick = parser.read_double(false)?;
  if server_version >= min_server_ver::MD_SIZE_MULTIPLIER && server_version < min_server_ver::SIZE_RULES {
    let _md_size_multiplier = parser.read_int(false)?; // Not used anymore
  }
  contract_details.contract.multiplier = parser.read_str_opt()?.map(|s| s.to_string());
  contract_details.order_types = parser.read_str()?.to_string();
  contract_details.valid_exchanges = parser.read_str()?.to_string();
  if version >= 2 {
    contract_details.price_magnifier = parser.read_int_max(false)?.unwrap_or(1);
  }
  if version >= 4 {
    contract_details.underlying_con_id = parser.read_int_max(false)?.unwrap_or(0);
  }
  if version >= 5 {
    contract_details.long_name = parser.read_str()?.to_string();
    contract_details.contract.primary_exchange = parser.read_str_opt()?.map(|s| s.to_string());
  }
  if version >= 6 {
    contract_details.contract_month = parse_opt_tws_month(parser.read_str_opt()?)?;
    contract_details.industry = parser.read_str()?.to_string();
    contract_details.category = parser.read_str()?.to_string();
    contract_details.subcategory = parser.read_str()?.to_string();
    contract_details.time_zone_id = parser.read_str()?.to_string();
    contract_details.trading_hours = parser.read_str()?.to_string();
    contract_details.liquid_hours = parser.read_str()?.to_string();
  }
  if version >= 8 {
    contract_details.ev_rule = parser.read_str()?.to_string();
    contract_details.ev_multiplier = parser.read_double(false)?;
  }
  if version >= 7 {
    let sec_id_list_count = parser.read_int(false)?;
    if sec_id_list_count > 0 {
      let mut sec_id_list = Vec::with_capacity(sec_id_list_count as usize);
      for _ in 0..sec_id_list_count {
        sec_id_list.push(TagValue {
          tag: parser.read_str()?.to_string(),
          value: parser.read_str()?.to_string(),
        });
      }
      contract_details.sec_id_list = sec_id_list;
    }
  }
  if server_version >= min_server_ver::AGG_GROUP {
    contract_details.agg_group = parser.read_int_max(false)?;
  }
  if server_version >= min_server_ver::MARKET_RULES {
    contract_details.market_rule_ids = parser.read_str()?.to_string();
  }
  if server_version >= min_server_ver::REAL_EXPIRATION_DATE {
    contract_details.real_expiration_date = parse_opt_tws_date(parser.read_str_opt()?)?;
  }
  if server_version >= min_server_ver::STOCK_TYPE {
    contract_details.stock_type = parser.read_str()?.to_string();
  }
  if server_version >= min_server_ver::FRACTIONAL_SIZE_SUPPORT && server_version < min_server_ver::SIZE_RULES {
    let _size_min_tick = parser.read_decimal_max()?; // Not used anymore
  }
  if server_version >= min_server_ver::SIZE_RULES {
    contract_details.min_size = parser.read_decimal_max()?.unwrap_or(0.);
    contract_details.size_increment = parser.read_decimal_max()?.unwrap_or(0.);
    contract_details.suggested_size_increment = parser.read_decimal_max()?.unwrap_or(0.);
  }
  if server_version >= min_server_ver::FUND_DATA_FIELDS && contract_details.contract.sec_type == SecType::Fund {
    contract_details.fund_name = parser.read_str_opt()?.map(|s| s.to_string());
    contract_details.fund_family = parser.read_str_opt()?.map(|s| s.to_string());
    contract_details.fund_type = parser.read_str_opt()?.map(|s| s.to_string());
    contract_details.fund_front_load = parser.read_str_opt()?.map(|s| s.to_string());
    contract_details.fund_back_load = parser.read_str_opt()?.map(|s| s.to_string());
    contract_details.fund_back_load_time_interval = parser.read_str_opt()?.map(|s| s.to_string());
    contract_details.fund_management_fee = parser.read_str_opt()?.map(|s| s.to_string());
    contract_details.fund_closed = parser.read_bool_opt(false)?;
    contract_details.fund_closed_for_new_investors = parser.read_bool_opt(false)?;
    contract_details.fund_closed_for_new_money = parser.read_bool_opt(false)?;
    contract_details.fund_notify_amount = parser.read_str_opt()?.map(|s| s.to_string());
    contract_details.fund_minimum_initial_purchase = parser.read_str_opt()?.map(|s| s.to_string());
    contract_details.fund_subsequent_minimum_purchase = parser.read_str_opt()?.map(|s| s.to_string());
    contract_details.fund_blue_sky_states = parser.read_str_opt()?.map(|s| s.to_string());
    contract_details.fund_blue_sky_territories = parser.read_str_opt()?.map(|s| s.to_string());
    contract_details.fund_distribution_policy_indicator = parser.read_str_opt()?.map(|s| s.to_string());
    contract_details.fund_asset_type = parser.read_str_opt()?.map(|s| s.to_string());
  }
  if server_version >= min_server_ver::INELIGIBILITY_REASONS {
    let count = parser.read_int(false)?;
    if count > 0 {
      let mut reasons = Vec::with_capacity(count as usize);
      for _ in 0..count {
        reasons.push(IneligibilityReason {
          id: parser.read_str()?.to_string(),
          description: parser.read_str()?.to_string(),
        });
      }
      contract_details.ineligibility_reason_list = reasons;
    }
  }

  debug!("Contract Data: ReqID={}, Symbol={}, SecType={}, Exchange={}, ConID={}",
         req_id, contract_details.contract.symbol, contract_details.contract.sec_type, contract_details.contract.exchange, contract_details.contract.con_id);
  handler.contract_details(req_id, &contract_details);
  Ok(())
}