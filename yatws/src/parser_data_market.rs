// yatws/src/parser_data_market.rs
use std::sync::Arc;
use chrono::{DateTime,NaiveDateTime, Utc};

use crate::handler::MarketDataHandler;

use crate::base::IBKRError;
use crate::protocol_dec_parser::FieldParser;
use crate::contract::{ContractDetails, OptionRight, Bar, SecType};


/// Process tick price message
pub fn process_tick_price(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let ticker_id = parser.read_int()?;
  let tick_type = parser.read_int()?;
  let price = parser.read_double()?;

  log::debug!("Tick Price: ID={}, Type={}, Price={}", ticker_id, tick_type, price);

  Ok(())
}

/// Process tick size message
pub fn process_tick_size(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let ticker_id = parser.read_int()?;
  let tick_type = parser.read_int()?;
  let size = parser.read_int()?;

  log::debug!("Tick Size: ID={}, Type={}, Size={}", ticker_id, tick_type, size);

  Ok(())
}

pub fn process_historical_data(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;

  let req_id = parser.read_int()?;

  // Parse start and end dates if version >= 2
  let start_date_str = parser.read_string()?;
  let end_date_str = parser.read_string()?;

  let item_count = parser.read_int()?;
  let mut bars = Vec::with_capacity(item_count as usize);

  for _ in 0..item_count {
    let date = parser.read_string()?;
    let open = parser.read_double()?;
    let high = parser.read_double()?;
    let low = parser.read_double()?;
    let close = parser.read_double()?;
    let volume = parser.read_int()? as i64;
    let wap = parser.read_double()?;

    // Skip has_gaps field
    parser.read_string()?;

    let bar_count = parser.read_int()?;

    let time = if date.contains(':') {
      // Time format
      match NaiveDateTime::parse_from_str(&date, "%Y%m%d %H:%M:%S") {
        Ok(ndt) => DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc),
        Err(_) => Utc::now(), // Fallback
      }
    } else {
      // Date format
      match NaiveDateTime::parse_from_str(&format!("{} 00:00:00", date), "%Y%m%d %H:%M:%S") {
        Ok(ndt) => DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc),
        Err(_) => Utc::now(), // Fallback
      }
    };

    let bar = Bar {
      time,
      open,
      high,
      low,
      close,
      volume,
      wap,
      count: bar_count,
    };

    bars.push(bar);
  }

  log::debug!("Historical Data: ReqID={}, Bars={}", req_id, bars.len());

  Ok(())
}

/// Process market depth message
pub fn process_market_depth(handler: &Arc<dyn MarketDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse market depth data
  Ok(())
}

/// Process market depth L2 message
pub fn process_market_depth_l2(handler: &Arc<dyn MarketDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse L2 market depth data
  Ok(())
}

/// Process scanner parameters message
pub fn process_scanner_parameters(handler: &Arc<dyn MarketDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse scanner parameters
  Ok(())
}

/// Process scanner data message
pub fn process_scanner_data(handler: &Arc<dyn MarketDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse scanner data
  Ok(())
}

/// Process tick EFP message
pub fn process_tick_efp(handler: &Arc<dyn MarketDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse EFP tick data
  Ok(())
}

/// Process real-time bars message
pub fn process_real_time_bars(handler: &Arc<dyn MarketDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse real-time bar data
  Ok(())
}

/// Process delta neutral validation message
pub fn process_delta_neutral_validation(handler: &Arc<dyn MarketDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse delta neutral validation data
  Ok(())
}

/// Process tick snapshot end message
pub fn process_tick_snapshot_end(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;

  log::debug!("Tick Snapshot End: {}", req_id);

  Ok(())
}

/// Process market data type message
pub fn process_market_data_type(handler: &Arc<dyn MarketDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let market_data_type = parser.read_int()?;

  log::debug!("Market Data Type: ReqId={}, Type={}", req_id, market_data_type);

  Ok(())
}

/// Process market depth exchanges message
pub fn process_mkt_depth_exchanges(handler: &Arc<dyn MarketDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse market depth exchanges message
  Ok(())
}

/// Process tick req params message
pub fn process_tick_req_params(handler: &Arc<dyn MarketDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse tick req params message
  Ok(())
}

/// Process histogram data message
pub fn process_histogram_data(handler: &Arc<dyn MarketDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse histogram data message
  Ok(())
}

/// Process historical data update message
pub fn process_historical_data_update(handler: &Arc<dyn MarketDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse historical data update message
  Ok(())
}

/// Process reroute market data request message
pub fn process_reroute_mkt_data_req(handler: &Arc<dyn MarketDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse reroute market data request message
  Ok(())
}

/// Process reroute market depth request message
pub fn process_reroute_mkt_depth_req(handler: &Arc<dyn MarketDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse reroute market depth request message
  Ok(())
}

/// Process historical ticks message
pub fn process_historical_ticks(handler: &Arc<dyn MarketDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse historical ticks message
  Ok(())
}

/// Process historical ticks bid ask message
pub fn process_historical_ticks_bid_ask(handler: &Arc<dyn MarketDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse historical ticks bid ask message
  Ok(())
}

/// Process historical ticks last message
pub fn process_historical_ticks_last(handler: &Arc<dyn MarketDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse historical ticks last message
  Ok(())
}

/// Process tick by tick message
pub fn process_tick_by_tick(handler: &Arc<dyn MarketDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse tick by tick message
  Ok(())
}

/// Process tick generic message
pub fn process_tick_generic(handler: &Arc<dyn MarketDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse generic tick data
  Ok(())
}

/// Process tick string message
pub fn process_tick_string(handler: &Arc<dyn MarketDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse string tick data
  Ok(())
}
