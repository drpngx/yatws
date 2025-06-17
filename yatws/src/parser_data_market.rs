// yatws/src/parser_data_market.rs
use std::sync::Arc;
use chrono::{Utc, TimeZone};
use std::convert::TryFrom;
use crate::handler::MarketDataHandler;
use crate::base::IBKRError;
use crate::protocol_dec_parser::{FieldParser, parse_opt_tws_date_time, parse_opt_tws_date_or_month};
use crate::contract::{ContractDetails, OptionRight, Bar, SecType};
use crate::data::{TickType, TickAttrib, TickAttribLast, TickAttribBidAsk, MarketDataType, TickOptionComputationData}; // Added TickType
use crate::min_server_ver::min_server_ver;
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
pub fn process_tick_price(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser, server_version: i32) -> Result<(), IBKRError> {
  let _version = parser.read_int()?; // Version of the tick price message itself
  let ticker_id = parser.read_int()?;
  let tick_type_int = parser.read_int()?;
  let price = parser.read_double()?;
  let size = parser.read_decimal_max()?.unwrap_or(-1.0); // Read size, default to -1.0 if MAX/empty
  let attr_mask = parser.read_int()?; // Read attribute mask

  let tick_type = TickType::try_from(tick_type_int)
    .unwrap_or_else(|_| {
      log::warn!("Received unknown TickType integer {} in process_tick_price", tick_type_int);
      TickType::Unknown // Fallback to Unknown
    });

  let attrib = parse_tick_attrib(attr_mask, server_version);

  log::trace!("Tick Price: ID={}, Type={:?}({}), Price={}, Size={}, Attrib={:?}", ticker_id, tick_type, tick_type_int, price, size, attrib);
  handler.tick_price(ticker_id, tick_type, price, attrib);

  // Handle implied TickSize based on TickType enum
  if let Some(size_tick_type) = tick_type.get_corresponding_size_tick() {
    if size >= 0.0 { // Only send size if valid
      log::trace!("Implied Tick Size: ID={}, Type={:?}, Size={}", ticker_id, size_tick_type, size);
      // Pass the implied TickType enum variant to the handler
      handler.tick_size(ticker_id, size_tick_type, size);
    }
  }

  Ok(())
}

/// Process tick size message
pub fn process_tick_size(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let ticker_id = parser.read_int()?;
  let tick_type_int = parser.read_int()?;
  let size = parser.read_decimal_max()?.unwrap_or(0.0); // Read as optional decimal

  let tick_type = TickType::try_from(tick_type_int)
    .unwrap_or_else(|_| {
      log::warn!("Received unknown TickType integer {} in process_tick_size", tick_type_int);
      TickType::Unknown // Fallback to Unknown
    });

  log::trace!("Tick Size: ID={}, Type={:?}({}), Size={}", ticker_id, tick_type, tick_type_int, size);
  handler.tick_size(ticker_id, tick_type, size);
  Ok(())
}

/// Process historical data message (handles one row at a time, end signaled separately)
pub fn process_historical_data(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser, server_version: i32) -> Result<(), IBKRError> {
  // Version handling differs slightly from Java - we read fields based on their presence in the stream
  let _version = if server_version < min_server_ver::SYNT_REALTIME_BARS { parser.read_int()? } else { i32::MAX }; // Consume version if present

  let req_id = parser.read_int()?;

  let start_date_str = parser.read_string_opt()?; // Consume if present (version >= 2)
  let end_date_str = parser.read_string_opt()?; // Consume if present (version >= 2)

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

  handler.historical_data_end(req_id,
                              parse_opt_tws_date_time(start_date_str)?,
                              parse_opt_tws_date_time(end_date_str)?);

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

/// Process tick EFP message
pub fn process_tick_efp(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let tick_type_int = parser.read_int()?;
  let basis_points = parser.read_double()?;
  let formatted_basis_points = parser.read_string()?;
  let implied_futures_price = parser.read_double()?;
  let hold_days = parser.read_int()?;
  let future_last_trade_date = parser.read_string()?;
  let dividend_impact = parser.read_double()?;
  let dividends_to_last_trade_date = parser.read_double()?;

  let tick_type = TickType::try_from(tick_type_int)
    .unwrap_or_else(|_| {
      log::warn!("Received unknown TickType integer {} in process_tick_efp", tick_type_int);
      TickType::Unknown // Fallback to Unknown
    });

  log::trace!("Tick EFP: ID={}, Type={:?}({}), BasisPts={}, FmtBasisPts={}, ImpFutPx={}, HoldDays={}, FutLastTrade={}, DivImpact={}, DivsToDate={}",
              req_id, tick_type, tick_type_int, basis_points, formatted_basis_points, implied_futures_price, hold_days, future_last_trade_date, dividend_impact, dividends_to_last_trade_date);

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
pub fn process_tick_option_computation(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser, server_version: i32) -> Result<(), IBKRError> {
  let version = if server_version >= min_server_ver::PRICE_BASED_VOLATILITY { i32::MAX } else { parser.read_int()? };
  let req_id = parser.read_int()?;
  let tick_type_int = parser.read_int()?;

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

  let tick_type = TickType::try_from(tick_type_int)
    .unwrap_or_else(|_| {
      log::warn!("Received unknown TickType integer {} in process_tick_option_computation", tick_type_int);
      TickType::Unknown // Fallback to Unknown
    });

  // Fields introduced in version 6 OR for specific model tick types
  let is_model_tick = matches!(tick_type, TickType::BidOptionComputation | TickType::AskOptionComputation | TickType::LastOptionComputation | TickType::ModelOptionComputation | TickType::CustomOptionComputation | TickType::DelayedBidOption | TickType::DelayedAskOption | TickType::DelayedLastOption | TickType::DelayedModelOption);

  if version >= 6 || is_model_tick {
    if version >= 6 || is_model_tick { // Check again for clarity, logic from Java
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
    tick_type, // Use the parsed enum variant
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

  log::trace!("Tick Option Computation: ID={}, Type={:?}({}), Data={:?}", req_id, tick_type, tick_type_int, data);
  handler.tick_option_computation(req_id, data);
  Ok(())
}

/// Process histogram data message
pub fn process_histogram_data(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int()?;
  let n = parser.read_int()?;
  let mut items = Vec::with_capacity(n as usize);
  for _ in 0..n {
    let price = parser.read_double()?;
    let size = parser.read_decimal_max()?.unwrap_or(0.0); // Assuming size is decimal
    items.push((price, size));
  }
  log::debug!("Histogram Data: ReqID={}, Count={}", req_id, n);
  handler.histogram_data(req_id, &items);
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
  let time = crate::protocol_dec_parser::parse_tws_date_time(&date_str)
    .or_else(|_| {
      // Try parsing as epoch seconds if primary format fails
      date_str.parse::<i64>().map(|epoch| {
        Utc.timestamp_opt(epoch, 0).single().unwrap_or(Utc::now()) // Handle potential errors/ambiguity
      })
    })
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
  let tick_type_int = parser.read_int()?; // 1=Last, 2=AllLast, 3=BidAsk, 4=MidPoint
  let time = parser.read_i64()?; // Unix epoch seconds

  match tick_type_int {
    1 | 2 => { // Last or AllLast
      let price = parser.read_double()?;
      let size = parser.read_decimal_max()?.unwrap_or(0.0);
      let mask = parser.read_int()?;
      let tick_attrib_last = parse_tick_attrib_last(mask);
      let exchange = parser.read_string()?;
      let special_conditions = parser.read_string()?;
      log::trace!("Tick-By-Tick Last/AllLast: ID={}, Type={}, Time={}, Px={}, Sz={}, Exch={}, Cond={}", req_id, tick_type_int, time, price, size, exchange, special_conditions);
      handler.tick_by_tick_all_last(req_id, tick_type_int, time, price, size, tick_attrib_last, &exchange, &special_conditions);
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
      log::warn!("Unknown Tick-By-Tick Type: {}", tick_type_int);
      // Consume remaining fields based on expected structure if possible, or return error
      return Err(IBKRError::ParseError(format!("Unknown tick-by-tick type {}", tick_type_int)));
    }
  }
  Ok(())
}

/// Process tick generic message
pub fn process_tick_generic(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let tick_type_int = parser.read_int()?;
  let value = parser.read_double()?;

  let tick_type = TickType::try_from(tick_type_int)
    .unwrap_or_else(|_| {
      log::warn!("Received unknown TickType integer {} in process_tick_generic", tick_type_int);
      TickType::Unknown // Fallback to Unknown
    });

  log::trace!("Tick Generic: ID={}, Type={:?}({}), Value={}", req_id, tick_type, tick_type_int, value);
  handler.tick_generic(req_id, tick_type, value);
  Ok(())
}

/// Process tick string message
pub fn process_tick_string(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let tick_type_int = parser.read_int()?;
  let value = parser.read_string()?;

  let tick_type = TickType::try_from(tick_type_int)
    .unwrap_or_else(|_| {
      log::warn!("Received unknown TickType integer {} in process_tick_string", tick_type_int);
      TickType::Unknown // Fallback to Unknown
    });

  log::trace!("Tick String: ID={}, Type={:?}({}), Value='{}'", req_id, tick_type, tick_type_int, value);
  handler.tick_string(req_id, tick_type, &value);
  Ok(())
}

/// Process scanner parameters message (XML string)
// AI: Process the XML into a rustr struct, for example: <ScannerSubscription instrument="STK" locationCode="STK.US.MAJOR" scanCode="TOP_PERC_GAIN" numberOfRows="50" abovePrice="10" belowPrice="200" aboveVolume="1000000"/>
pub fn process_scanner_parameters(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?; // Version is typically 1
  let xml_data = parser.read_string()?;
  log::debug!("Scanner Parameters XML received (length: {})", xml_data.len());
  handler.scanner_parameters(&xml_data);
  Ok(())
}

/// Process scanner data message
pub fn process_scanner_data(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser, server_version: i32) -> Result<(),
                                                                                                                           IBKRError> {
  let _version = parser.read_int()?; // Version of the message format
  let req_id = parser.read_int()?;
  let num_elements = parser.read_int()?;

  log::debug!("Scanner Data: ReqID={}, NumElements={}", req_id, num_elements);

  for _ in 0..num_elements {
    let rank = parser.read_int()?;
    let mut contract_details = ContractDetails::default();

    // Populate Contract part of ContractDetails
    contract_details.contract.con_id = parser.read_int()?;
    contract_details.contract.symbol = parser.read_string()?;
    contract_details.contract.sec_type = SecType::from_str(&parser.read_string()?).map_err(|e| IBKRError::ParseError(e.to_string()))?;
    contract_details.contract.last_trade_date_or_contract_month = parse_opt_tws_date_or_month(parser.read_string_opt()?)?;
    contract_details.contract.strike = parser.read_double_max()?;
    let opt_right_str = parser.read_string()?;
    if !opt_right_str.is_empty() {
      contract_details.contract.right = Some(OptionRight::from_str(&opt_right_str)?);
    }
    contract_details.contract.exchange = parser.read_string()?;
    contract_details.contract.currency = parser.read_string()?;
    contract_details.contract.local_symbol = parser.read_string_opt()?;

    // Populate ContractDetails specific fields
    contract_details.market_name = parser.read_string()?;
    contract_details.contract.trading_class = parser.read_string_opt()?;

    let distance = parser.read_string()?;
    let benchmark = parser.read_string()?;
    let projection = parser.read_string()?;
    let mut legs_str = String::new();
    if server_version >= min_server_ver::SCANNER_GENERIC_OPTS {
      legs_str = parser.read_string()?;
    }

    handler.scanner_data(req_id, rank, &contract_details, &distance, &benchmark, &projection, Some(&legs_str));
  }

  // After all elements are processed, call scanner_data_end
  handler.scanner_data_end(req_id);
  Ok(())
}
