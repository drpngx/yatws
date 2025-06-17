#![allow(unused_variables)] // For default implementations

use crate::contract::Bar;
use crate::data::{
  TickType, TickAttrib, TickAttribLast, TickAttribBidAsk, MarketDataType,
};

use chrono::{DateTime, Utc};

/// Unique identifier for a registered observer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObserverId(pub usize);

/// Observer for regular market data ticks (price, size, etc.)
pub trait MarketDataObserver: Send + Sync {
  /// Called when a price tick is received.
  fn on_tick_price(&self, req_id: i32, tick_type: TickType, price: f64, attrib: TickAttrib) {}
  /// Called when a size tick is received.
  fn on_tick_size(&self, req_id: i32, tick_type: TickType, size: f64) {}
  /// Called when a string tick is received.
  fn on_tick_string(&self, req_id: i32, tick_type: TickType, value: &str) {}
  /// Called when a generic tick is received.
  fn on_tick_generic(&self, req_id: i32, tick_type: TickType, value: f64) {}
  /// Called when a snapshot is completed.
  fn on_tick_snapshot_end(&self, req_id: i32) {}
  /// Called when market data type changes.
  fn on_market_data_type(&self, req_id: i32, market_data_type: MarketDataType) {}
  /// Called when an error occurs related to this observer's data type.
  fn on_error(&self, req_id: i32, error_code: i32, error_message: &str) {}
}

/// Observer for real-time bar data (streaming 5-second bars).
pub trait RealTimeBarsObserver: Send + Sync {
  /// Called when a new real-time bar is received.
  fn on_bar_update(&self, req_id: i32, bar: &Bar) {}
  /// Called when an error occurs related to this observer's data type.
  fn on_error(&self, req_id: i32, error_code: i32, error_message: &str) {}
}

/// Observer for tick-by-tick data (detailed trade and quote data).
pub trait TickByTickObserver: Send + Sync {
  /// Called when last or all last tick data is received.
  fn on_tick_by_tick_all_last(
    &self,
    req_id: i32,
    tick_type: i32, // 1 for Last, 2 for AllLast
    time: i64,
    price: f64,
    size: f64,
    tick_attrib_last: &TickAttribLast,
    exchange: &str,
    special_conditions: &str,
  ) {}

  /// Called when bid/ask tick data is received.
  fn on_tick_by_tick_bid_ask(
    &self,
    req_id: i32,
    time: i64,
    bid_price: f64,
    ask_price: f64,
    bid_size: f64,
    ask_size: f64,
    tick_attrib_bid_ask: &TickAttribBidAsk,
  ) {}

  /// Called when midpoint tick data is received.
  fn on_tick_by_tick_mid_point(&self, req_id: i32, time: i64, mid_point: f64) {}
  /// Called when an error occurs related to this observer's data type.
  fn on_error(&self, req_id: i32, error_code: i32, error_message: &str) {}
}

/// Observer for market depth (L2 order book) data.
pub trait MarketDepthObserver: Send + Sync {
  /// Called when L1 market depth is updated.
  fn on_update_mkt_depth(
    &self,
    req_id: i32,
    position: i32,
    operation: i32,
    side: i32,
    price: f64,
    size: f64,
  ) {}

  /// Called when L2 market depth is updated.
  fn on_update_mkt_depth_l2(
    &self,
    req_id: i32,
    position: i32,
    market_maker: &str,
    operation: i32,
    side: i32,
    price: f64,
    size: f64,
    is_smart_depth: bool,
  ) {}
  /// Called when an error occurs related to this observer's data type.
  fn on_error(&self, req_id: i32, error_code: i32, error_message: &str) {}
}

/// Observer for historical bar data.
pub trait HistoricalDataObserver: Send + Sync {
  /// Called when a historical data bar is received.
  fn on_historical_data(&self, req_id: i32, bar: &Bar) {}
  /// Called when a historical data update bar is received (if `keep_up_to_date` was true).
  fn on_historical_data_update(&self, req_id: i32, bar: &Bar) {}
  /// Called when historical data transmission is complete.
  fn on_historical_data_end(&self, req_id: i32, start_date: Option<DateTime<Utc>>, end_date: Option<DateTime<Utc>>) {}
  /// Called when an error occurs related to this observer's data type.
  fn on_error(&self, req_id: i32, error_code: i32, error_message: &str) {}
}

/// Observer for historical tick data.
pub trait HistoricalTicksObserver: Send + Sync {
  /// Called when historical midpoint ticks are received.
  /// `ticks` is a slice of (time, price, size).
  fn on_historical_ticks_midpoint(&self, req_id: i32, ticks: &[(i64, f64, f64)], done: bool) {}

  /// Called when historical bid/ask ticks are received.
  /// `ticks` is a slice of (time, TickAttribBidAsk, price_bid, price_ask, size_bid, size_ask).
  fn on_historical_ticks_bid_ask(
    &self,
    req_id: i32,
    ticks: &[(i64, TickAttribBidAsk, f64, f64, f64, f64)],
    done: bool,
  ) {}

  /// Called when historical last ticks are received.
  /// `ticks` is a slice of (time, TickAttribLast, price, size, exchange, special_conditions).
  fn on_historical_ticks_last(
    &self,
    req_id: i32,
    ticks: &[(i64, TickAttribLast, f64, f64, String, String)],
    done: bool,
  ) {}
  /// Called when an error occurs related to this observer's data type.
  fn on_error(&self, req_id: i32, error_code: i32, error_message: &str) {}
}
