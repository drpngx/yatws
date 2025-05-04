// yatws/src/parser_data_market.rs
use std::sync::Arc;
use chrono::{Utc, TimeZone}; // Added TimeZone
use crate::handler::MarketDataHandler;
use crate::base::IBKRError;
use crate::protocol_dec_parser::FieldParser;
use crate::contract::{ContractDetails, OptionRight, Bar, SecType}; // Added Contract
use crate::data::{TickAttrib, TickAttribLast, TickAttribBidAsk, MarketDataType, TickOptionComputationData}; // Added new types
use crate::min_server_ver::min_server_ver; // Added min_server_ver
use std::str::FromStr;


// Helper for parsing TickAttrib (similar to Java BitMask logic)
fn parse_tick_attrib(mask_val: i32, server_version: i32) -> TickAttrib {
  let mut attrib = TickAttrib::default();
  if server_version >= min_server_ver::PAST_LIMIT {
    // Use bit positions 0, 1, 2
    attrib.can_auto_execute = (mask_val & 1) != 0; // Bit 0
    attrib.past_limit = (mask_val & 2) != 0;       // Bit 1
    if server_version >= min_server_ver::PRE_OPEN_BID_ASK {
      attrib.pre_open = (mask_val & 4) != 0;     // Bit 2
    }
  } else {
    // Older logic: 0=false, 1=true
    attrib.can_auto_execute = mask_val == 1;
  }
  attrib
}

// Helper for parsing TickAttribLast for Tick-By-Tick
fn parse_tick_attrib_last(mask_val: i32) -> TickAttribLast {
  TickAttribLast {
    past_limit: (mask_val & 1) != 0, // Bit 0
    unreported: (mask_val & 2) != 0, // Bit 1
  }
}

// Helper for parsing TickAttribBidAsk for Tick-By-Tick
fn parse_tick_attrib_bid_ask(mask_val: i32) -> TickAttribBidAsk {
  TickAttribBidAsk {
    bid_past_low: (mask_val & 1) != 0,  // Bit 0
    ask_past_high: (mask_val & 2) != 0, // Bit 1
  }
}


/// Process tick price message
pub fn process_tick_price(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser, server_version: i32) -> Result<(), IBKRError> { // Added server_version
  let _version = parser.read_int()?; // Version of the tick price message itself
  let ticker_id = parser.read_int()?;
  let tick_type = parser.read_int()?;
  let price = parser.read_double()?;
  let size = parser.read_decimal_max()?.unwrap_or(-1.0); // Read size, default to -1.0 if MAX/empty
  let attr_mask = parser.read_int()?; // Read attribute mask

  let attrib = parse_tick_attrib(attr_mask, server_version);

  log::trace!("Tick Price: ID={}, Type={}, Price={}, Size={}, Attrib={:?}", ticker_id, tick_type, price, size, attrib);
  handler.tick_price(ticker_id, tick_type, price, attrib);

  // Handle implied TickSize based on TickType (as in Java EDecoder)
  let size_tick_type = match tick_type {
    1 => Some(0),  // BID -> BID_SIZE
    2 => Some(3),  // ASK -> ASK_SIZE
    4 => Some(5),  // LAST -> LAST_SIZE
    66 => Some(69), // DELAYED_BID -> DELAYED_BID_SIZE
    67 => Some(70), // DELAYED_ASK -> DELAYED_ASK_SIZE
    68 => Some(71), // DELAYED_LAST -> DELAYED_LAST_SIZE
    _ => None,
  };

  if let Some(stt) = size_tick_type {
    if size >= 0.0 { // Only send size if valid
      log::trace!("Implied Tick Size: ID={}, Type={}, Size={}", ticker_id, stt, size);
      handler.tick_size(ticker_id, stt, size);
    }
  }

  Ok(())
}

/// Process tick size message
pub fn process_tick_size(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let ticker_id = parser.read_int()?;
  let tick_type = parser.read_int()?;
  let size = parser.read_decimal_max()?.unwrap_or(0.0); // Read as optional decimal

  log::trace!("Tick Size: ID={}, Type={}, Size={}", ticker_id, tick_type, size);
  handler.tick_size(ticker_id, tick_type, size);
  Ok(())
}

/// Process historical data message (handles one row at a time, end signaled separately)
pub fn process_historical_data(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser, server_version: i32) -> Result<(), IBKRError> {
  // Version handling differs slightly from Java - we read fields based on their presence in the stream
  let _version = if server_version < min_server_ver::SYNT_REALTIME_BARS { parser.read_int()? } else { i32::MAX }; // Consume version if present

  let req_id = parser.read_int()?;

  let start_date_str = parser.read_string()?; // Consume if present (version >= 2)
  let end_date_str = parser.read_string()?; // Consume if present (version >= 2)

  let item_count = parser.read_int()?;
  log::trace!("Historical Data: ReqID={}, ItemCount={}", req_id, item_count);

  for i in 0..item_count {
    let date_str = parser.read_string()?;
    let open = parser.read_double()?;
    let high = parser.read_double()?;
    let low = parser.read_double()?;
    let close = parser.read_double()?;
    let volume = parser.read_decimal_max()?.unwrap_or(-1.0); // Use -1 if invalid/max
    let wap = parser.read_decimal_max()?.unwrap_or(-1.0); // Use -1 if invalid/max

    if server_version < min_server_ver::SYNT_REALTIME_BARS {
      let _has_gaps = parser.read_string()?; // Consume "hasGaps" field if present
    }

    let bar_count = parser.read_int_max()?.unwrap_or(-1); // Use -1 if invalid/max

    // Parse timestamp (handle different formats)
    let time = crate::protocol_dec_parser::parse_tws_date_time(&date_str)
      .or_else(|_| {
        // Try parsing as epoch seconds if primary format fails
        date_str.parse::<i64>().map(|epoch| {
          Utc.timestamp_opt(epoch, 0).single().unwrap_or(Utc::now()) // Handle potential errors/ambiguity
        })
      })
      .unwrap_or_else(|e| {
        log::warn!("Failed to parse historical bar timestamp '{}' for ReqID {}: {}. Using current time.", date_str, req_id, e);
        Utc::now() // Fallback
      });


    let bar = Bar {
      time,
      open,
      high,
      low,
      close,
      volume: if volume < 0.0 { 0 } else { volume as i64 }, // Convert to i64, default 0
      wap: if wap < 0.0 { 0.0 } else { wap },              // Default 0.0
      count: if bar_count < 0 { 0 } else { bar_count },    // Default 0
    };
    log::trace!("--> Bar[{}]: {:?}", i, bar);
    handler.historical_data(req_id, &bar);
  }

  handler.historical_data_end(req_id, &start_date_str, &end_date_str);

  Ok(())
}

/// Process market depth message (L1 - Top of Book Update)
pub fn process_market_depth(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let position = parser.read_int()?; // Row
  let operation = parser.read_int()?; // 0=insert, 1=update, 2=delete
  let side = parser.read_int()?; // 0=ask, 1=bid
  let price = parser.read_double()?;
  let size = parser.read_decimal_max()?.unwrap_or(0.0); // Size as optional decimal

  log::trace!("Market Depth (L1): ID={}, Pos={}, Op={}, Side={}, Price={}, Size={}", req_id, position, operation, side, price, size);
  handler.update_mkt_depth(req_id, position, operation, side, price, size);
  Ok(())
}

/// Process market depth L2 message (Full Book Update)
pub fn process_market_depth_l2(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser, server_version: i32) -> Result<(), IBKRError> { // Added server_version
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let position = parser.read_int()?;
  let market_maker = parser.read_string()?;
  let operation = parser.read_int()?;
  let side = parser.read_int()?;
  let price = parser.read_double()?;
  let size = parser.read_decimal_max()?.unwrap_or(0.0);

  let is_smart_depth = if server_version >= min_server_ver::SMART_DEPTH {
    parser.read_bool()?
  } else {
    false
  };

  log::trace!("Market Depth L2: ID={}, Pos={}, MM={}, Op={}, Side={}, Price={}, Size={}, Smart={}",
              req_id, position, market_maker, operation, side, price, size, is_smart_depth);
  handler.update_mkt_depth_l2(req_id, position, &market_maker, operation, side, price, size, is_smart_depth);
  Ok(())
}


/// Process scanner parameters message
pub fn process_scanner_parameters(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let xml = parser.read_string()?;
  log::debug!("Scanner Parameters Received ({} bytes)", xml.len());
  handler.scanner_parameters(&xml);
  Ok(())
}

/// Process scanner data message (individual rows)
pub fn process_scanner_data(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let version = parser.read_int()?;
  let req_id = parser.read_int()?; // Ticker ID in Java code, but maps to request ID

  let number_of_elements = parser.read_int()?;
  log::trace!("Scanner Data: ReqID={}, Elements={}", req_id, number_of_elements);

  // If number_of_elements is -1, it signals the end
  if number_of_elements == -1 {
    log::debug!("Scanner Data End: ReqID={}", req_id);
    handler.scanner_data_end(req_id);
    return Ok(());
  }


  for _ in 0..number_of_elements {
    let rank = parser.read_int()?;
    let mut contract_details = ContractDetails::default(); // Use ContractDetails struct

    if version >= 3 {
      contract_details.contract.con_id = parser.read_int()?;
    }
    contract_details.contract.symbol = parser.read_string()?;
    contract_details.contract.sec_type = SecType::from_str(&parser.read_string()?).unwrap_or(SecType::Stock);
    contract_details.contract.last_trade_date_or_contract_month = parser.read_string_opt()?;
    contract_details.contract.strike = Some(parser.read_double()?);
    contract_details.contract.right = OptionRight::from_str(&parser.read_string()?).ok();
    contract_details.contract.exchange = parser.read_string()?;
    contract_details.contract.currency = parser.read_string()?;
    contract_details.contract.local_symbol = parser.read_string_opt()?;
    contract_details.market_name = parser.read_string()?;
    contract_details.contract.trading_class = parser.read_string_opt()?;

    let distance = parser.read_string()?;
    let benchmark = parser.read_string()?;
    let projection = parser.read_string()?;
    let legs_str = if version >= 2 { parser.read_string_opt()? } else { None };

    log::trace!("--> Scanner Row: Rank={}, ConID={}, Symbol={}, Dist={}, Bench={}, Proj={}",
                rank, contract_details.contract.con_id, contract_details.contract.symbol, distance, benchmark, projection);

    handler.scanner_data(req_id, rank, &contract_details, &distance, &benchmark, &projection, legs_str.as_deref());
  }
  Ok(())
}

/// Process tick EFP message
pub fn process_tick_efp(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let tick_type = parser.read_int()?;
  let basis_points = parser.read_double()?;
  let formatted_basis_points = parser.read_string()?;
  let implied_futures_price = parser.read_double()?;
  let hold_days = parser.read_int()?;
  let future_last_trade_date = parser.read_string()?;
  let dividend_impact = parser.read_double()?;
  let dividends_to_last_trade_date = parser.read_double()?;

  log::trace!("Tick EFP: ID={}, Type={}, BasisPts={}, FmtBasisPts={}, ImpFutPx={}, HoldDays={}, FutLastTrade={}, DivImpact={}, DivsToDate={}",
              req_id, tick_type, basis_points, formatted_basis_points, implied_futures_price, hold_days, future_last_trade_date, dividend_impact, dividends_to_last_trade_date);

  handler.tick_efp(req_id, tick_type, basis_points, &formatted_basis_points, implied_futures_price, hold_days, &future_last_trade_date, dividend_impact, dividends_to_last_trade_date);
  Ok(())
}

/// Process real-time bars message
pub fn process_real_time_bars(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let time_unix = parser.read_i64()?; // Timestamp is Unix epoch seconds
  let open = parser.read_double()?;
  let high = parser.read_double()?;
  let low = parser.read_double()?;
  let close = parser.read_double()?;
  let volume = parser.read_decimal_max()?.unwrap_or(0.0);
  let wap = parser.read_decimal_max()?.unwrap_or(0.0);
  let count = parser.read_int()?;

  log::trace!("Real Time Bar: ID={}, Time={}, O={}, H={}, L={}, C={}, Vol={}, WAP={}, Count={}",
              req_id, time_unix, open, high, low, close, volume, wap, count);
  handler.real_time_bar(req_id, time_unix, open, high, low, close, volume, wap, count);
  Ok(())
}

/// Process delta neutral validation message
pub fn process_delta_neutral_validation(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let _con_id = parser.read_int()?;
  let _delta = parser.read_double()?;
  let _price = parser.read_double()?;
  log::debug!("Delta Neutral Validation: ReqID={}", req_id);
  // TODO: Reconstruct DeltaNeutralContract struct if needed by handler
  handler.delta_neutral_validation(req_id);
  Ok(())
}

/// Process tick snapshot end message
pub fn process_tick_snapshot_end(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  log::debug!("Tick Snapshot End: ReqID={}", req_id);
  handler.tick_snapshot_end(req_id);
  Ok(())
}

/// Process market data type message
pub fn process_market_data_type(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let market_data_type_int = parser.read_int()?;
  let market_data_type = MarketDataType::from(market_data_type_int);

  log::debug!("Market Data Type: ReqId={}, Type={:?} ({})", req_id, market_data_type, market_data_type_int);
  handler.market_data_type(req_id, market_data_type);
  Ok(())
}

/// Process tick option computation message
pub fn process_tick_option_computation(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser, server_version: i32) -> Result<(), IBKRError> { // Added server_version
  let version = if server_version >= min_server_ver::PRICE_BASED_VOLATILITY { i32::MAX } else { parser.read_int()? };
  let req_id = parser.read_int()?;
  let tick_type = parser.read_int()?;

  let tick_attrib = if server_version >= min_server_ver::PRICE_BASED_VOLATILITY {
    parser.read_int()?
  } else {
    i32::MAX // Not available
  };

  // Read implied vol, handle -1 sentinel -> None
  let implied_vol = match parser.read_double()? {
    -1.0 => None,
    vol if vol < 0.0 => None, // Treat other negatives as None too
    vol => Some(vol),
  };

  // Read delta, handle -2 sentinel -> None
  let delta = match parser.read_double()? {
    -2.0 => None,
    d => Some(d), // Allow any other value, including < -1 or > 1 if API sends it
  };

  let mut opt_price = None;
  let mut pv_dividend = None;
  let mut gamma = None;
  let mut vega = None;
  let mut theta = None;
  let mut und_price = None;

  // Fields introduced in version 6 OR for specific model tick types
  if version >= 6 || tick_type == 21 || tick_type == 48 { // 21=TICK_OPTION_COMPUTATION, 48=MODEL_OPTION (Java constants) -> Need Rust TickType enum
    // 21: TICK_OPTION_COMPUTATION, needs index mapping
    // Let's assume tick_type 21 and 48 correspond to model option computations
    // FIXME: Need a proper TickType enum mapping
    // Using raw integers for now based on Java comment
    let is_model_tick = tick_type == 21 || tick_type == 48; // Placeholder check

    if version >= 6 || is_model_tick {
      opt_price = match parser.read_double()? { -1.0 => None, p => Some(p) };
      pv_dividend = match parser.read_double()? { -1.0 => None, p => Some(p) };
    }
    if version >= 6 {
      gamma = match parser.read_double()? { -2.0 => None, g => Some(g) };
      vega = match parser.read_double()? { -2.0 => None, v => Some(v) };
      theta = match parser.read_double()? { -2.0 => None, t => Some(t) };
      und_price = match parser.read_double()? { -1.0 => None, u => Some(u) };
    }
  }


  let data = TickOptionComputationData {
    tick_type,
    tick_attrib,
    implied_vol,
    delta,
    opt_price,
    pv_dividend,
    gamma,
    vega,
    theta,
    und_price,
  };

  log::trace!("Tick Option Computation: ID={}, Data={:?}", req_id, data);
  handler.tick_option_computation(req_id, data);
  Ok(())
}

/// Process tick req params message
pub fn process_tick_req_params(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int()?;
  let min_tick = parser.read_double()?;
  let bbo_exchange = parser.read_string()?;
  let snapshot_permissions = parser.read_int()?;

  log::debug!("Tick Req Params: ID={}, MinTick={}, BBOExch={}, Permissions={}", req_id, min_tick, bbo_exchange, snapshot_permissions);
  handler.tick_req_params(req_id, min_tick, &bbo_exchange, snapshot_permissions);
  Ok(())
}

/// Process histogram data message
pub fn process_histogram_data(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int()?;
  let n = parser.read_int()?;
  // let mut items = Vec::with_capacity(n as usize);
  for _ in 0..n {
    let _price = parser.read_double()?;
    let _size = parser.read_decimal_max()?.unwrap_or(0.0);
    // items.push((price, size));
  }
  log::debug!("Histogram Data: ReqID={}, Count={}", req_id, n);
  // handler.histogram_data(req_id, &items); // Uncomment when handler accepts it
  handler.histogram_data(req_id); // Placeholder call
  Ok(())
}

/// Process historical data update message (for real-time updates after initial load)
pub fn process_historical_data_update(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int()?;
  let bar_count = parser.read_int()?; // Usually 1 for updates
  let date_str = parser.read_string()?; // Should be epoch seconds for updates? Check format.
  let open = parser.read_double()?;
  let close = parser.read_double()?;
  let high = parser.read_double()?;
  let low = parser.read_double()?;
  let wap = parser.read_decimal_max()?.unwrap_or(0.0);
  let volume = parser.read_decimal_max()?.unwrap_or(0.0);

  // Parse timestamp (expecting epoch seconds for updates)
  let time = date_str.parse::<i64>()
    .map(|epoch| Utc.timestamp_opt(epoch, 0).single().unwrap_or(Utc::now()))
    .unwrap_or_else(|e| {
      log::warn!("Failed to parse historical update timestamp '{}' for ReqID {}: {}. Using current time.", date_str, req_id, e);
      Utc::now() // Fallback
    });


  let bar = Bar {
    time,
    open,
    high,
    low,
    close,
    volume: volume as i64,
    wap,
    count: bar_count,
  };
  log::debug!("Historical Data Update: ReqID={}, Bar={:?}", req_id, bar);
  handler.historical_data_update(req_id, &bar);
  Ok(())
}

/// Process reroute market data request message
pub fn process_reroute_mkt_data_req(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int()?;
  let con_id = parser.read_int()?;
  let exchange = parser.read_string()?;
  log::info!("Reroute Market Data Request: ReqID={}, ConID={}, Exchange={}", req_id, con_id, exchange);
  handler.reroute_mkt_data_req(req_id, con_id, &exchange);
  Ok(())
}

/// Process reroute market depth request message
pub fn process_reroute_mkt_depth_req(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int()?;
  let con_id = parser.read_int()?;
  let exchange = parser.read_string()?;
  log::info!("Reroute Market Depth Request: ReqID={}, ConID={}, Exchange={}", req_id, con_id, exchange);
  handler.reroute_mkt_depth_req(req_id, con_id, &exchange);
  Ok(())
}

/// Process historical ticks message (Trades only)
pub fn process_historical_ticks(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int()?;
  let tick_count = parser.read_int()?;
  let mut ticks = Vec::with_capacity(tick_count as usize);

  for _ in 0..tick_count {
    let time = parser.read_i64()?;
    let _ = parser.read_int()?; // Skip unused field (for consistency with Java)
    let price = parser.read_double()?;
    let size = parser.read_decimal_max()?.unwrap_or(0.0);
    ticks.push((time, price, size));
  }
  let done = parser.read_bool()?;
  log::debug!("Historical Ticks: ReqID={}, Count={}, Done={}", req_id, ticks.len(), done);
  handler.historical_ticks(req_id, &ticks, done);
  Ok(())
}

/// Process historical ticks bid ask message
pub fn process_historical_ticks_bid_ask(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int()?;
  let tick_count = parser.read_int()?;
  let mut ticks = Vec::with_capacity(tick_count as usize);

  for _ in 0..tick_count {
    let time = parser.read_i64()?;
    let mask = parser.read_int()?;
    let attrib = parse_tick_attrib_bid_ask(mask);
    let price_bid = parser.read_double()?;
    let price_ask = parser.read_double()?;
    let size_bid = parser.read_decimal_max()?.unwrap_or(0.0);
    let size_ask = parser.read_decimal_max()?.unwrap_or(0.0);
    ticks.push((time, attrib, price_bid, price_ask, size_bid, size_ask));
  }
  let done = parser.read_bool()?;
  log::debug!("Historical Ticks Bid/Ask: ReqID={}, Count={}, Done={}", req_id, ticks.len(), done);
  handler.historical_ticks_bid_ask(req_id, &ticks, done);
  Ok(())
}

/// Process historical ticks last message
pub fn process_historical_ticks_last(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int()?;
  let tick_count = parser.read_int()?;
  let mut ticks = Vec::with_capacity(tick_count as usize);

  for _ in 0..tick_count {
    let time = parser.read_i64()?;
    let mask = parser.read_int()?;
    let attrib = parse_tick_attrib_last(mask);
    let price = parser.read_double()?;
    let size = parser.read_decimal_max()?.unwrap_or(0.0);
    let exchange = parser.read_string()?;
    let special_conditions = parser.read_string()?;
    ticks.push((time, attrib, price, size, exchange, special_conditions));
  }
  let done = parser.read_bool()?;
  log::debug!("Historical Ticks Last: ReqID={}, Count={}, Done={}", req_id, ticks.len(), done);
  handler.historical_ticks_last(req_id, &ticks, done);
  Ok(())
}

/// Process tick by tick message
pub fn process_tick_by_tick(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int()?;
  let tick_type = parser.read_int()?; // 1=Last, 2=AllLast, 3=BidAsk, 4=MidPoint
  let time = parser.read_i64()?; // Unix epoch seconds

  match tick_type {
    1 | 2 => { // Last or AllLast
      let price = parser.read_double()?;
      let size = parser.read_decimal_max()?.unwrap_or(0.0);
      let mask = parser.read_int()?;
      let tick_attrib_last = parse_tick_attrib_last(mask);
      let exchange = parser.read_string()?;
      let special_conditions = parser.read_string()?;
      log::trace!("Tick-By-Tick Last/AllLast: ID={}, Type={}, Time={}, Px={}, Sz={}, Exch={}, Cond={}", req_id, tick_type, time, price, size, exchange, special_conditions);
      handler.tick_by_tick_all_last(req_id, tick_type, time, price, size, tick_attrib_last, &exchange, &special_conditions);
    },
    3 => { // BidAsk
      let bid_price = parser.read_double()?;
      let ask_price = parser.read_double()?;
      let bid_size = parser.read_decimal_max()?.unwrap_or(0.0);
      let ask_size = parser.read_decimal_max()?.unwrap_or(0.0);
      let mask = parser.read_int()?;
      let tick_attrib_bid_ask = parse_tick_attrib_bid_ask(mask);
      log::trace!("Tick-By-Tick BidAsk: ID={}, Time={}, BidPx={}, AskPx={}, BidSz={}, AskSz={}", req_id, time, bid_price, ask_price, bid_size, ask_size);
      handler.tick_by_tick_bid_ask(req_id, time, bid_price, ask_price, bid_size, ask_size, tick_attrib_bid_ask);
    },
    4 => { // MidPoint
      let mid_point = parser.read_double()?;
      log::trace!("Tick-By-Tick MidPoint: ID={}, Time={}, MidPt={}", req_id, time, mid_point);
      handler.tick_by_tick_mid_point(req_id, time, mid_point);
    },
    _ => {
      log::warn!("Unknown Tick-By-Tick Type: {}", tick_type);
      // Consume remaining fields based on expected structure if possible, or return error
      return Err(IBKRError::ParseError(format!("Unknown tick-by-tick type {}", tick_type)));
    }
  }
  Ok(())
}

/// Process tick generic message
pub fn process_tick_generic(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let tick_type = parser.read_int()?;
  let value = parser.read_double()?;

  log::trace!("Tick Generic: ID={}, Type={}, Value={}", req_id, tick_type, value);
  handler.tick_generic(req_id, tick_type, value);
  Ok(())
}

/// Process tick string message
pub fn process_tick_string(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let tick_type = parser.read_int()?;
  let value = parser.read_string()?;

  log::trace!("Tick String: ID={}, Type={}, Value='{}'", req_id, tick_type, value);
  handler.tick_string(req_id, tick_type, &value);
  Ok(())
}
