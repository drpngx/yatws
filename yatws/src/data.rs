use crate::base::IBKRError;
use crate::contract::{Contract, Bar};
use std::collections::HashMap;
use std::convert::TryFrom;
use num_enum::TryFromPrimitive;


/// Enum representing the different types of market data ticks.
/// Based on https://interactivebrokers.github.io/tws-api/tick_types.html
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, TryFromPrimitive)]
#[repr(i32)]
pub enum TickType {
    BidSize = 0,
    BidPrice = 1,
    AskPrice = 2,
    AskSize = 3,
    LastPrice = 4,
    LastSize = 5,
    High = 6,
    Low = 7,
    Volume = 8,
    ClosePrice = 9,
    BidOptionComputation = 10,
    AskOptionComputation = 11,
    LastOptionComputation = 12,
    ModelOptionComputation = 13,
    OpenTick = 14,
    Low13Weeks = 15,
    High13Weeks = 16,
    Low26Weeks = 17,
    High26Weeks = 18,
    Low52Weeks = 19,
    High52Weeks = 20,
    AverageVolume = 21,
    // OpenInterest = 22, // Deprecated
    OptionHistoricalVolatility = 23,
    OptionImpliedVolatility = 24,
    // OptionBidExchange = 25, // Not Used
    // OptionAskExchange = 26, // Not Used
    OptionCallOpenInterest = 27,
    OptionPutOpenInterest = 28,
    OptionCallVolume = 29,
    OptionPutVolume = 30,
    IndexFuturePremium = 31,
    BidExchange = 32,
    AskExchange = 33,
    AuctionVolume = 34,
    AuctionPrice = 35,
    AuctionImbalance = 36,
    MarkPrice = 37,
    BidEfpComputation = 38,
    AskEfpComputation = 39,
    LastEfpComputation = 40,
    OpenEfpComputation = 41,
    HighEfpComputation = 42,
    LowEfpComputation = 43,
    CloseEfpComputation = 44,
    LastTimestamp = 45,
    Shortable = 46,
    // FundamentalRatios = 47, // Not listed as a tick type, but generic tick 258
    RtVolume = 48, // Time & Sales
    Halted = 49,
    BidYield = 50,
    AskYield = 51,
    LastYield = 52,
    CustomOptionComputation = 53,
    TradeCount = 54,
    TradeRate = 55,
    VolumeRate = 56,
    LastRthTrade = 57,
    RtHistoricalVolatility = 58,
    IbDividends = 59,
    BondFactorMultiplier = 60,
    RegulatoryImbalance = 61,
    News = 62,
    ShortTermVolume3Minutes = 63,
    ShortTermVolume5Minutes = 64,
    ShortTermVolume10Minutes = 65,
    DelayedBid = 66,
    DelayedAsk = 67,
    DelayedLast = 68,
    DelayedBidSize = 69,
    DelayedAskSize = 70,
    DelayedLastSize = 71,
    DelayedHighPrice = 72,
    DelayedLowPrice = 73,
    DelayedVolume = 74,
    DelayedClose = 75,
    DelayedOpen = 76,
    RtTradeVolume = 77, // RT Trd Vol
    CreditmanMarkPrice = 78,
    CreditmanSlowMarkPrice = 79,
    DelayedBidOption = 80,
    DelayedAskOption = 81,
    DelayedLastOption = 82,
    DelayedModelOption = 83,
    LastExchange = 84,
    LastRegulatoryTime = 85,
    FuturesOpenInterest = 86,
    AverageOptionVolume = 87,
    DelayedLastTimestamp = 88,
    ShortableShares = 89,
    // 90, 91 unknown
    EtfNavClose = 92,
    EtfNavPriorClose = 93,
    EtfNavBid = 94,
    EtfNavAsk = 95,
    EtfNavLast = 96,
    EtfNavFrozenLast = 97,
    EtfNavHigh = 98,
    EtfNavLow = 99,
    // 100 unknown? (Maybe Option Put Volume again?)
    EstimatedIpoMidpoint = 101, // Note: Doc says 101, but also lists GenericTick101. Assuming direct ID.
    FinalIpoPrice = 102, // Note: Doc says 102, but also lists GenericTick102. Assuming direct ID.
    // Add generic tick IDs if needed, or handle them separately
    // GenericTick100 = 100, // Option Volume
    // GenericTick101 = 101, // Option Open Interest
    // GenericTick104 = 104, // Historical Volatility
    // GenericTick105 = 105, // Average Option Volume
    // GenericTick106 = 106, // Option Implied Volatility
    // GenericTick162 = 162, // Index Future Premium
    // GenericTick165 = 165, // 13, 26, 52 week high/low, Avg Volume
    // GenericTick225 = 225, // Auction data
    // GenericTick232 = 232, // Mark Price
    // GenericTick233 = 233, // RT Volume Timestamp
    // GenericTick236 = 236, // Shortable
    // GenericTick258 = 258, // Fundamental Ratios
    // GenericTick292 = 292, // News
    // GenericTick293 = 293, // Trade Count
    // GenericTick294 = 294, // Trade Rate
    // GenericTick295 = 295, // Volume Rate
    // GenericTick318 = 318, // Last RTH Trade
    // GenericTick375 = 375, // RT Trade Volume
    // GenericTick411 = 411, // RT Historical Volatility
    // GenericTick456 = 456, // IB Dividends
    // GenericTick460 = 460, // Bond Factor Multiplier
    // GenericTick576 = 576, // ETF NAV Bid/Ask
    // GenericTick577 = 577, // ETF NAV Last
    // GenericTick578 = 578, // ETF NAV Close/Prior Close
    // GenericTick586 = 586, // IPO Prices
    // GenericTick588 = 588, // Futures Open Interest
    // GenericTick595 = 595, // Short Term Volume
    // GenericTick614 = 614, // ETF NAV High/Low
    // GenericTick619 = 619, // Creditman Slow Mark Price
    // GenericTick623 = 623, // ETF NAV Frozen Last
    Unknown = -1, // For unhandled cases
}

impl TickType {
    /// Returns the corresponding size tick type for a given price tick type, if applicable.
    pub fn get_corresponding_size_tick(&self) -> Option<TickType> {
        match self {
            TickType::BidPrice => Some(TickType::BidSize),
            TickType::AskPrice => Some(TickType::AskSize),
            TickType::LastPrice => Some(TickType::LastSize),
            TickType::DelayedBid => Some(TickType::DelayedBidSize),
            TickType::DelayedAsk => Some(TickType::DelayedAskSize),
            TickType::DelayedLast => Some(TickType::DelayedLastSize),
            _ => None,
        }
    }
}

impl Default for TickType {
  fn default() -> Self {
    TickType::Unknown
  }
}

/// Market data subscription request details and live state.
#[derive(Debug, Clone)]
#[allow(dead_code)]
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
  // Internal flags for blocking requests
  pub is_blocking_quote_request: bool, // Is this a request from get_quote?
  pub quote_received: bool, // Flag set by tick_snapshot_end or when all required ticks arrive
  pub completed: bool, // General completion flag for flexible blocking requests
  // History for flexible blocking requests
  pub ticks: HashMap<TickType, Vec<(f64, TickAttrib)>>, // Stores price ticks: tick_type -> Vec<(price, attrib)>
  pub sizes: HashMap<TickType, Vec<f64>>, // Stores size ticks: tick_type -> Vec<size>
  // General
  pub market_data_type: Option<MarketDataType>, // From message 58
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
      is_blocking_quote_request: false, // Default to false
      quote_received: false, // Default to false
      completed: false, // Default to false
      ticks: HashMap::new(), // Initialize empty history
      sizes: HashMap::new(), // Initialize empty history
      market_data_type: None,
      error_code: None,
      error_message: None,
    }
  }
}

/// Real-time bar subscription state
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RealTimeBarSubscription {
  pub req_id: i32,
  pub contract: Contract,
  pub bar_size: i32, // e.g., 5 for 5 seconds
  pub what_to_show: String,
  pub use_rth: bool,
  pub rt_bar_options: Vec<(String, String)>, // Placeholder for TagValue
  // --- Live Data ---
  pub latest_bar: Option<Bar>, // Most recent bar received
  pub bars: Vec<Bar>, // All bars collected for this request
  pub target_bar_count: Option<usize>, // For blocking requests
  pub completed: bool, // Flag for completion (count reached or error)
  pub error_code: Option<i32>,
  pub error_message: Option<String>,
}

/// Tick-by-tick data subscription state
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TickByTickSubscription {
  pub req_id: i32,
  pub contract: Contract,
  pub tick_type: String, // "Last", "AllLast", "BidAsk", "MidPoint"
  pub number_of_ticks: i32,
  pub ignore_size: bool,
  // --- Live Data ---
  pub latest_tick: Option<TickByTickData>,
  pub ticks: Vec<TickByTickData>, // History for blocking requests
  pub completed: bool, // Flag for completion
  pub error_code: Option<i32>,
  pub error_message: Option<String>,
}

/// Holds the different types of tick-by-tick data
#[derive(Debug, Clone)]
#[allow(dead_code)]
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
#[allow(dead_code)]
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
  pub completed: bool, // Flag for completion
  pub error_code: Option<i32>,
  pub error_message: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
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
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct HistoricalDataRequestState {
  pub req_id: i32,
  pub contract: Contract,
  pub bars: Vec<Bar>,
  pub start_date: String, // Received in end message
  pub end_date: String,   // Received in end message
  pub end_received: bool,
  pub update_received: bool, // Flag if real-time updates come after initial load
  pub requested_market_data_type: MarketDataType, // Track the type requested
  pub error_code: Option<i32>,
  pub error_message: Option<String>,
}

impl Default for HistoricalDataRequestState {
  fn default() -> Self {
    Self {
      req_id: 0,
      contract: Contract::default(),
      bars: Vec::new(),
      start_date: String::new(),
      end_date: String::new(),
      end_received: false,
      update_received: false,
      requested_market_data_type: MarketDataType::RealTime, // Default
      error_code: None,
      error_message: None,
    }
  }
}


/// Enum for Market Data Type Message (ID 58)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketDataType {
  Unknown = 0, // Should not happen in practice
  RealTime = 1, // Default
  Frozen = 2,
  Delayed = 3,
  DelayedFrozen = 4,
}

impl From<i32> for MarketDataType {
  fn from(v: i32) -> Self {
    match v {
      1 => MarketDataType::RealTime,
      2 => MarketDataType::Frozen,
      3 => MarketDataType::Delayed,
      4 => MarketDataType::DelayedFrozen,
      _ => MarketDataType::Unknown,
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

/// Data for Tick Option Computation Message
#[derive(Debug, Clone, Default)]
pub struct TickOptionComputationData {
  pub tick_type: TickType, // Use the enum
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
#[allow(dead_code)]
pub struct TickNewsData {
  pub time_stamp: i64, // Unix timestamp
  pub provider_code: String,
  pub article_id: String,
  pub headline: String,
  pub extra_data: String,
}
