use crate::contract::{Contract, Bar, PriceIncrement};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Market data subscription request details and live state.
#[derive(Debug, Clone)]
pub struct MarketDataSubscription {
  pub req_id: i32, // TWS request ID
  pub contract: Contract,
  pub generic_tick_list: String,
  pub snapshot: bool,
  pub regulatory_snapshot: bool,
  pub mkt_data_options: Vec<(String, String)>, // Placeholder for TagValue

  // --- Live Data Fields ---
  // Tick Price related
  pub bid_price: Option<f64>,
  pub ask_price: Option<f64>,
  pub last_price: Option<f64>,
  pub high_price: Option<f64>,
  pub low_price: Option<f64>,
  pub close_price: Option<f64>,
  pub open_price: Option<f64>,
  // Tick Size related
  pub bid_size: Option<f64>, // Using f64 for Decimal
  pub ask_size: Option<f64>, // Using f64 for Decimal
  pub last_size: Option<f64>, // Using f64 for Decimal
  pub volume: Option<f64>,    // Using f64 for Decimal
  // Other Ticks
  pub halted: Option<bool>, // from TickType.HALTED
  pub last_timestamp: Option<i64>, // Unix timestamp from TickType.LAST_TIMESTAMP
  pub shortable_shares: Option<f64>, // from TickType.SHORTABLE_SHARES
  pub trade_count: Option<i64>,
  pub trade_rate: Option<f64>,
  pub volume_rate: Option<f64>,
  pub last_exchange: Option<String>,
  pub last_reg_time: Option<String>,
  pub futures_open_interest: Option<f64>,
  pub avg_volume: Option<f64>, // from TickType.AVG_VOLUME_10_DAY etc. needs mapping
  pub call_open_interest: Option<f64>,
  pub put_open_interest: Option<f64>,
  pub call_volume: Option<f64>,
  pub put_volume: Option<f64>,
  pub short_term_volume_3_min: Option<f64>, // Needs mapping
  pub short_term_volume_5_min: Option<f64>, // Needs mapping
  pub short_term_volume_10_min: Option<f64>,// Needs mapping
  // Tick Option Computation
  pub option_computation: Option<TickOptionComputationData>,
  // Tick News
  pub latest_news_time: Option<i64>, // Unix timestamp
  // Snapshot specific
  pub snapshot_permissions: Option<i32>,
  pub snapshot_end_received: bool, // Flag for snapshot completion
  // General
  pub market_data_type: Option<MarketDataTypeEnum>, // From message 58
  pub error_code: Option<i32>,
  pub error_message: Option<String>,
}

impl MarketDataSubscription {
  pub fn new(
    req_id: i32,
    contract: Contract,
    generic_tick_list: String,
    snapshot: bool,
    regulatory_snapshot: bool,
    mkt_data_options: Vec<(String, String)>,
  ) -> Self {
    MarketDataSubscription {
      req_id,
      contract,
      generic_tick_list,
      snapshot,
      regulatory_snapshot,
      mkt_data_options,
      bid_price: None,
      ask_price: None,
      last_price: None,
      high_price: None,
      low_price: None,
      close_price: None,
      open_price: None,
      bid_size: None,
      ask_size: None,
      last_size: None,
      volume: None,
      halted: None,
      last_timestamp: None,
      shortable_shares: None,
      trade_count: None,
      trade_rate: None,
      volume_rate: None,
      last_exchange: None,
      last_reg_time: None,
      futures_open_interest: None,
      avg_volume: None,
      call_open_interest: None,
      put_open_interest: None,
      call_volume: None,
      put_volume: None,
      short_term_volume_3_min: None,
      short_term_volume_5_min: None,
      short_term_volume_10_min: None,
      option_computation: None,
      latest_news_time: None,
      snapshot_permissions: None,
      snapshot_end_received: false,
      market_data_type: None,
      error_code: None,
      error_message: None,
    }
  }
}

/// Real-time bar subscription state
#[derive(Debug, Clone)]
pub struct RealTimeBarSubscription {
  pub req_id: i32,
  pub contract: Contract,
  pub bar_size: i32, // e.g., 5 for 5 seconds
  pub what_to_show: String,
  pub use_rth: bool,
  pub rt_bar_options: Vec<(String, String)>, // Placeholder for TagValue
  // --- Live Data ---
  pub latest_bar: Option<Bar>, // Use contract::Bar
  pub error_code: Option<i32>,
  pub error_message: Option<String>,
}

/// Tick-by-tick data subscription state
#[derive(Debug, Clone)]
pub struct TickByTickSubscription {
  pub req_id: i32,
  pub contract: Contract,
  pub tick_type: String, // "Last", "AllLast", "BidAsk", "MidPoint"
  pub number_of_ticks: i32,
  pub ignore_size: bool,
  // --- Live Data ---
  pub latest_tick: Option<TickByTickData>,
  pub error_code: Option<i32>,
  pub error_message: Option<String>,
}

/// Holds the different types of tick-by-tick data
#[derive(Debug, Clone)]
pub enum TickByTickData {
  None,
  Last {
    time: i64,
    price: f64,
    size: f64, // Using f64 for Decimal
    tick_attrib_last: TickAttribLast,
    exchange: String,
    special_conditions: String,
  },
  AllLast { // Same structure as Last, but type indicates source
    time: i64,
    price: f64,
    size: f64, // Using f64 for Decimal
    tick_attrib_last: TickAttribLast,
    exchange: String,
    special_conditions: String,
  },
  BidAsk {
    time: i64,
    bid_price: f64,
    ask_price: f64,
    bid_size: f64, // Using f64 for Decimal
    ask_size: f64, // Using f64 for Decimal
    tick_attrib_bid_ask: TickAttribBidAsk,
  },
  MidPoint {
    time: i64,
    mid_point: f64,
  },
}


/// Market depth subscription state
#[derive(Debug, Clone)]
pub struct MarketDepthSubscription {
  pub req_id: i32,
  pub contract: Contract,
  pub num_rows: i32,
  pub is_smart_depth: bool,
  pub mkt_depth_options: Vec<(String, String)>, // Placeholder for TagValue
  // --- Live Data ---
  // L1 (Top of Book)
  pub bid_price: Option<f64>,
  pub ask_price: Option<f64>,
  pub bid_size: Option<f64>,
  pub ask_size: Option<f64>,
  // L2 (Depth - Vec to store rows, bool indicates bid/ask)
  pub depth_bids: Vec<MarketDepthRow>,
  pub depth_asks: Vec<MarketDepthRow>,
  pub error_code: Option<i32>,
  pub error_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MarketDepthRow {
  pub position: i32,
  pub market_maker: String,
  pub operation: i32, // 0=insert, 1=update, 2=delete
  pub side: i32, // 0=ask, 1=bid
  pub price: f64,
  pub size: f64, // Using f64 for Decimal
  pub is_smart_depth: Option<bool>, // Only for L2
}

/// Historical data request state
#[derive(Debug, Clone, Default)]
pub struct HistoricalDataRequestState {
  pub req_id: i32,
  pub contract: Contract,
  pub bars: Vec<Bar>,
  pub start_date: String, // Received in end message
  pub end_date: String,   // Received in end message
  pub end_received: bool,
  pub update_received: bool, // Flag if real-time updates come after initial load
  pub error_code: Option<i32>,
  pub error_message: Option<String>,
}


/// Enum for Market Data Type Message (ID 58)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketDataTypeEnum {
  Unknown = 0,
  RealTime = 1,
  Frozen = 2,
  Delayed = 3,
  DelayedFrozen = 4,
}

impl From<i32> for MarketDataTypeEnum {
  fn from(v: i32) -> Self {
    match v {
      1 => MarketDataTypeEnum::RealTime,
      2 => MarketDataTypeEnum::Frozen,
      3 => MarketDataTypeEnum::Delayed,
      4 => MarketDataTypeEnum::DelayedFrozen,
      _ => MarketDataTypeEnum::Unknown,
    }
  }
}

/// Tick attributes for price ticks
#[derive(Debug, Clone, Default)]
pub struct TickAttrib {
  pub can_auto_execute: bool,
  pub past_limit: bool,
  pub pre_open: bool,
}

/// Tick attributes specific to Last ticks in Tick-By-Tick data
#[derive(Debug, Clone, Default)]
pub struct TickAttribLast {
  pub past_limit: bool,
  pub unreported: bool,
}

/// Tick attributes specific to BidAsk ticks in Tick-By-Tick data
#[derive(Debug, Clone, Default)]
pub struct TickAttribBidAsk {
  pub bid_past_low: bool,
  pub ask_past_high: bool,
}

/// Data for Tick Option Computation Message (ID 21)
#[derive(Debug, Clone, Default)]
pub struct TickOptionComputationData {
  pub tick_type: i32,
  pub tick_attrib: i32, // Specific attributes for option computation ticks
  pub implied_vol: Option<f64>,
  pub delta: Option<f64>,
  pub opt_price: Option<f64>,
  pub pv_dividend: Option<f64>,
  pub gamma: Option<f64>,
  pub vega: Option<f64>,
  pub theta: Option<f64>,
  pub und_price: Option<f64>,
}

/// News Tick Data (ID 84)
#[derive(Debug, Clone)]
pub struct TickNewsData {
  pub time_stamp: i64, // Unix timestamp
  pub provider_code: String,
  pub article_id: String,
  pub headline: String,
  pub extra_data: String,
}


/// Market data type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketDataType {
  BidAsk,
  LastPrice,
  HighLow,
  Volume,
  HistoricalVolatility,
  ImpliedVolatility,
  OptionChain, // Note: Option Chain data usually comes via SecDefOptParams
  PutCallRatio,
  DividendSchedule,
  Fundamentals,
  News,
  RealTimeBars, // Added
  TickByTick,  // Added
  MarketDepth, // Added
}

/// Market data observer trait (Potentially enhance later)
pub trait MarketDataObserver: Send + Sync {
  fn on_tick_update(&self, subscription: &MarketDataSubscription);
  fn on_realtime_bar_update(&self, subscription: &RealTimeBarSubscription);
  // Add other update types as needed (tick-by-tick, depth, etc.)
}

// --- News Related Data Structures ---
/// Represents an available news provider.
#[derive(Debug, Clone)]
pub struct NewsProvider {
  pub code: String, // e.g., "BZ", "FLY"
  pub name: String, // e.g., "Benzinga", "Fly on the Wall"
}

/// Holds the data for a requested news article.
#[derive(Debug, Clone)]
pub struct NewsArticleData {
  pub req_id: i32,
  pub article_type: i32, // 0 for plain text, 1 for HTML
  pub article_text: String,
}

/// Represents a single historical news headline.
#[derive(Debug, Clone)]
pub struct HistoricalNews {
  // req_id is handled by the manager state
  pub time: String, // YYYY-MM-DD HH:MM:SS.fff format
  pub provider_code: String,
  pub article_id: String,
  pub headline: String,
}

/// Represents a news bulletin message (Incoming ID 14)
#[derive(Debug, Clone)]
pub struct NewsBulletin {
  pub msg_id: i32,
  pub msg_type: i32, // 1 = Regular, 2 = Exchange no longer available, 3 = Exchange is available
  pub message: String,
  pub orig_exchange: String,
}
