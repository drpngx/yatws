use crate::base::IBKRError;
use crate::contract::{Contract, Bar, ScannerSubscription, ScanData, WhatToShow}; // Added WhatToShow
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt; // Add this
use std::str::FromStr; // Add this
use num_enum::TryFromPrimitive;


/// Enum representing the different types of market data ticks.
/// Based on `https://interactivebrokers.github.io/tws-api/tick_types.html`.
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

/// Enum representing the valid generic tick type IDs for market data requests.
/// These are typically numeric strings like "100", "101", "233", etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GenericTickType {
  OptionVolume = 100, // TickType 100: Option Volume (Vol. / Avg. Vol)
  OptionOpenInterest = 101, // TickType 101: Option Open Interest
  HistoricalVolatility = 104, // TickType 104: Historical Volatility (100%)
  AverageOptionVolume = 105, // TickType 105: Average Opt. Volume (30-day avg)
  OptionImpliedVolatility = 106, // TickType 106: Option Implied Vol. (100%)
  IndexFuturePremium = 162, // TickType 162: Index Future Premium
  MiscellaneousStats = 165, // TickType 165: Misc. Stats (High/Low/Avg Volume)
  MarketPrice = 221, // TickType 221: Mark Price (used in TWS P&L calculations)
  AuctionValues = 225, // TickType 225: Auction values (volume, price and imbalance)
  RtVolumeTimestamp = 233, // TickType 233: RTVolume - contains last trade, last volume, last price, total volume, VWAP, and single trade flag
  Shortable = 236, // TickType 236: Shortable
  FundamentalRatios = 258, // TickType 258: Fundamental Ratios
  NewsTick = 292, // TickType 292: News
  TradeCount = 293, // TickType 293: Trade Count for the day
  TradeRate = 294, // TickType 294: Trade Rate per minute
  VolumeRate = 295, // TickType 295: Volume Rate per minute
  LastRthTrade = 318, // TickType 318: Last RTH Trade
  RtHistoricalVolatility = 370, // TickType 370: Real-time Historical Volatility
  IbDividends = 377, // TickType 377: IB Dividends
  BondFactorMultiplier = 381, // TickType 381: Bond Factor Multiplier
  RegulatoryImbalance = 384, // TickType 384: Regulatory Imbalance
  NewsFeed = 387, // TickType 387: News Feed
  CompanyName = 388, // TickType 388: Company Name
  ShortTermVolume = 391, // TickType 391: Short Term Volume
  EtfNavX = 407, // TickType 407: ETF NAV X
  CreditmanMarkPrice = 411, // TickType 411: Creditman Mark Price
  CreditmanSlowMarkPrice = 414, // TickType 414: Creditman Slow Mark Price
  EtfNavLast = 514, // TickType 514: ETF NAV Last
  EtfNavFrozenLast = 515, // TickType 515: ETF NAV Frozen Last
  EtfNavHighLow = 516, // TickType 516: ETF NAV High/Low
  SocialMarketAnalytics = 517, // TickType 517: Social Market Analytics
  EstimatedIpoMidpoint = 518, // TickType 518: Estimated IPO Midpoint
  FinalIpoLast = 519, // TickType 519: Final IPO Last
  DelayedEtfNavLast = 523, // TickType 523: Delayed ETF NAV Last
  DelayedEtfNavFrozenLast = 524, // TickType 524: Delayed ETF NAV Frozen Last
  DelayedEtfNavHighLow = 525, // TickType 525: Delayed ETF NAV High/Low
  // Add more as needed from https://interactivebrokers.github.io/tws-api/generic_tick_types.html
}

impl fmt::Display for GenericTickType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", *self as i32)
  }
}

impl FromStr for GenericTickType {
  type Err = IBKRError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let val = s.parse::<i32>().map_err(|_| IBKRError::ParseError(format!("Invalid GenericTickType string: {}", s)))?;
    match val {
      100 => Ok(GenericTickType::OptionVolume),
      101 => Ok(GenericTickType::OptionOpenInterest),
      104 => Ok(GenericTickType::HistoricalVolatility),
      105 => Ok(GenericTickType::AverageOptionVolume),
      106 => Ok(GenericTickType::OptionImpliedVolatility),
      162 => Ok(GenericTickType::IndexFuturePremium),
      165 => Ok(GenericTickType::MiscellaneousStats),
      221 => Ok(GenericTickType::MarketPrice),
      225 => Ok(GenericTickType::AuctionValues),
      233 => Ok(GenericTickType::RtVolumeTimestamp),
      236 => Ok(GenericTickType::Shortable),
      258 => Ok(GenericTickType::FundamentalRatios),
      292 => Ok(GenericTickType::NewsTick),
      293 => Ok(GenericTickType::TradeCount),
      294 => Ok(GenericTickType::TradeRate),
      295 => Ok(GenericTickType::VolumeRate),
      318 => Ok(GenericTickType::LastRthTrade),
      370 => Ok(GenericTickType::RtHistoricalVolatility),
      377 => Ok(GenericTickType::IbDividends),
      381 => Ok(GenericTickType::BondFactorMultiplier),
      384 => Ok(GenericTickType::RegulatoryImbalance),
      387 => Ok(GenericTickType::NewsFeed),
      388 => Ok(GenericTickType::CompanyName),
      391 => Ok(GenericTickType::ShortTermVolume),
      407 => Ok(GenericTickType::EtfNavX),
      411 => Ok(GenericTickType::CreditmanMarkPrice),
      414 => Ok(GenericTickType::CreditmanSlowMarkPrice),
      514 => Ok(GenericTickType::EtfNavLast),
      515 => Ok(GenericTickType::EtfNavFrozenLast),
      516 => Ok(GenericTickType::EtfNavHighLow),
      517 => Ok(GenericTickType::SocialMarketAnalytics),
      518 => Ok(GenericTickType::EstimatedIpoMidpoint),
      519 => Ok(GenericTickType::FinalIpoLast),
      523 => Ok(GenericTickType::DelayedEtfNavLast),
      524 => Ok(GenericTickType::DelayedEtfNavFrozenLast),
      525 => Ok(GenericTickType::DelayedEtfNavHighLow),
      _ => Err(IBKRError::ParseError(format!("Unknown GenericTickType value: {}", val))),
    }
  }
}

/// Represents a duration for TWS API requests (e.g., historical data).
/// Use `DurationUnit::to_string()` to get the TWS-compatible string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DurationUnit {
  Second(i32),
  Day(i32),
  Week(i32),
  Month(i32),
  Year(i32),
}

impl fmt::Display for DurationUnit {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      DurationUnit::Second(n) => write!(f, "{} S", n),
      DurationUnit::Day(n) => write!(f, "{} D", n),
      DurationUnit::Week(n) => write!(f, "{} W", n),
      DurationUnit::Month(n) => write!(f, "{} M", n),
      DurationUnit::Year(n) => write!(f, "{} Y", n),
    }
  }
}

impl FromStr for DurationUnit {
  type Err = IBKRError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let parts: Vec<&str> = s.trim().split_whitespace().collect();
    if parts.len() != 2 {
      return Err(IBKRError::ParseError(format!("Invalid DurationUnit string format: '{}'", s)));
    }
    let value = parts[0].parse::<i32>().map_err(|_| IBKRError::ParseError(format!("Invalid number in DurationUnit string: '{}'", parts[0])))?;
    match parts[1].to_uppercase().as_str() {
      "S" => Ok(DurationUnit::Second(value)),
      "D" => Ok(DurationUnit::Day(value)),
      "W" => Ok(DurationUnit::Week(value)),
      "M" => Ok(DurationUnit::Month(value)),
      "Y" => Ok(DurationUnit::Year(value)),
      _ => Err(IBKRError::ParseError(format!("Invalid unit in DurationUnit string: '{}'", parts[1]))),
    }
  }
}

/// Represents a time period for TWS API histogram requests.
/// Use `TimePeriodUnit::to_string()` to get the TWS-compatible string (e.g., "3 days").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimePeriodUnit {
  Day(i32),
  Week(i32),
  Month(i32),
  Year(i32),
}

impl fmt::Display for TimePeriodUnit {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      TimePeriodUnit::Day(n) => write!(f, "{} {}", n, if *n == 1 { "day" } else { "days" }),
      TimePeriodUnit::Week(n) => write!(f, "{} {}", n, if *n == 1 { "week" } else { "weeks" }),
      TimePeriodUnit::Month(n) => write!(f, "{} {}", n, if *n == 1 { "month" } else { "months" }),
      TimePeriodUnit::Year(n) => write!(f, "{} {}", n, if *n == 1 { "year" } else { "years" }),
    }
  }
}

impl FromStr for TimePeriodUnit {
  type Err = IBKRError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let parts: Vec<&str> = s.trim().split_whitespace().collect();
    if parts.len() != 2 {
      return Err(IBKRError::ParseError(format!("Invalid TimePeriodUnit string format: '{}'", s)));
    }
    let value = parts[0].parse::<i32>().map_err(|_| IBKRError::ParseError(format!("Invalid number in TimePeriodUnit string: '{}'", parts[0])))?;
    let unit_str = parts[1].to_lowercase();

    // Handle singular and plural forms
    match unit_str.as_str() {
      "day" | "days" => Ok(TimePeriodUnit::Day(value)),
      "week" | "weeks" => Ok(TimePeriodUnit::Week(value)),
      "month" | "months" => Ok(TimePeriodUnit::Month(value)),
      "year" | "years" => Ok(TimePeriodUnit::Year(value)),
      _ => Err(IBKRError::ParseError(format!("Invalid unit in TimePeriodUnit string: '{}'", parts[1]))),
    }
  }
}


/// Enum representing the type of tick-by-tick data being requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TickByTickRequestType {
  Last,
  AllLast,
  BidAsk,
  MidPoint,
}

impl fmt::Display for TickByTickRequestType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      TickByTickRequestType::Last => write!(f, "Last"),
      TickByTickRequestType::AllLast => write!(f, "AllLast"),
      TickByTickRequestType::BidAsk => write!(f, "BidAsk"),
      TickByTickRequestType::MidPoint => write!(f, "MidPoint"),
    }
  }
}

impl FromStr for TickByTickRequestType {
  type Err = IBKRError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "Last" => Ok(TickByTickRequestType::Last),
      "AllLast" => Ok(TickByTickRequestType::AllLast),
      "BidAsk" => Ok(TickByTickRequestType::BidAsk),
      "MidPoint" => Ok(TickByTickRequestType::MidPoint),
      _ => Err(IBKRError::ParseError(format!("Invalid TickByTickRequestType string: {}", s))),
    }
  }
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

/// Market data request details and live state.
#[derive(Debug, Clone)]
pub struct MarketDataInfo {
  pub req_id: i32, // TWS request ID
  pub contract: Contract,
  pub generic_tick_list: Vec<GenericTickType>, // Changed from String
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

impl MarketDataInfo {
  pub fn new(
    req_id: i32,
    contract: Contract,
    generic_tick_list: Vec<GenericTickType>, // Changed from String
    snapshot: bool,
    regulatory_snapshot: bool,
    mkt_data_options: Vec<(String, String)>,
  ) -> Self {
    MarketDataInfo {
      req_id,
      contract,
      generic_tick_list, // Store the Vec
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
  pub what_to_show: WhatToShow, // Changed from String
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
  pub tick_type: TickByTickRequestType, // Changed from String
  pub number_of_ticks: i32,
  pub ignore_size: bool,
  // --- Live Data ---
  pub latest_tick: Option<TickByTickData>,
  pub ticks: Vec<TickByTickData>, // History for blocking requests
  pub completed: bool, // Flag for completion
  pub error_code: Option<i32>,
  pub error_message: Option<String>,
}

/// State for a market scanner subscription.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ScannerSubscriptionState {
  pub req_id: i32,
  pub subscription: ScannerSubscription,
  pub results: Vec<ScanData>,
  pub completed: bool,
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

// --- Financial Report Structures ---

/// Represents the type of fundamental report being parsed or requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FundamentalReportType {
  /// Company overview, ratios, estimates. TWS String: "ReportSnapshot"
  ReportSnapshot,
  /// Financial summary. TWS String: "ReportsFinSummary"
  ReportsFinSummary,
  /// Detailed financial statements (Income, Balance Sheet, Cash Flow). TWS String: "ReportsFinStatements"
  ReportsFinStatements,
  /// Analyst estimates. TWS String: "RESC"
  RESC,
  /// Corporate calendar events (deprecated by TWS in favor of WSH). TWS String: "CalendarReport"
  CalendarReport,
  // Consider adding ReportsOwnership if needed.
}

impl FundamentalReportType {
  /// Returns the string representation required by the TWS API for this report type.
  pub fn as_tws_str(&self) -> &'static str {
    match self {
      FundamentalReportType::ReportSnapshot => "ReportSnapshot",
      FundamentalReportType::ReportsFinSummary => "ReportsFinSummary",
      FundamentalReportType::ReportsFinStatements => "ReportsFinStatements",
      FundamentalReportType::RESC => "RESC",
      FundamentalReportType::CalendarReport => "CalendarReport",
    }
  }
}

/// General company identification information, aggregated from various XML parts.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CompanyInformation {
  pub con_id: Option<i64>,
  pub ticker: Option<String>,
  pub company_name: Option<String>,
  pub cik: Option<String>,
  pub exchange_code: Option<String>, // e.g., "NASDAQ" from <Exchange Code="NASDAQ">
  pub exchange_name: Option<String>, // e.g., "NASDAQ" from <Exchange>NASDAQ</Exchange>
  pub irs_number: Option<String>,
  pub sic_code: Option<String>,
  pub business_description: Option<String>,
  pub country: Option<String>, // From <CoAddress Country="United States">
  pub web_url: Option<String>, // From <WebURL>
  pub last_split_date: Option<chrono::NaiveDate>,
  pub last_split_ratio: Option<String>, // e.g., "1-for-2"
}

// --- Structures for "ReportSnapshot" ---

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Ratio {
  pub group_name: String, // From parent <Group Name="...">
  pub field_name: String, // From <Ratio FieldName="...">
  pub raw_value: Option<String>, // The value as a string from the XML
  pub period_type: Option<String>, // e.g., "TTM", "3YAVG", "Latest", "PENEPS"
  // You might add an `as_f64: Option<f64>` field if you parse `raw_value`
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Issue {
  pub id: Option<String>,
  pub issue_type: Option<String>, // "C" for Common, "P" for Preferred
  pub description: Option<String>,
  pub order: Option<i32>,
  pub name: Option<String>,
  pub ticker: Option<String>,
  pub ric: Option<String>,
  pub display_ric: Option<String>,
  pub instrument_pi: Option<String>,
  pub quote_pi: Option<String>,
  pub instrument_perm_id: Option<String>,
  pub quote_perm_id: Option<String>,
  pub exchange_code: Option<String>,
  pub exchange_country: Option<String>,
  pub exchange_name: Option<String>,
  pub global_listing_type: Option<String>,
  pub most_recent_split_date: Option<chrono::NaiveDate>,
  pub most_recent_split_factor: Option<f64>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CoGeneralInfo {
  pub co_status_code: Option<i32>,
  pub co_status_desc: Option<String>,
  pub co_type_code: Option<String>,
  pub co_type_desc: Option<String>,
  pub last_modified: Option<chrono::NaiveDate>,
  pub latest_available_annual: Option<chrono::NaiveDate>,
  pub latest_available_interim: Option<chrono::NaiveDate>,
  pub employees: Option<i64>,
  pub employees_last_updated: Option<chrono::NaiveDate>,
  pub shares_out: Option<f64>,
  pub shares_out_date: Option<chrono::NaiveDate>,
  pub total_float: Option<f64>,
  pub reporting_currency_code: Option<String>,
  pub reporting_currency_name: Option<String>,
  pub most_recent_exchange_rate: Option<f64>,
  pub most_recent_exchange_rate_date: Option<chrono::NaiveDate>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TextInfoItem {
  pub text_type: Option<String>, // "Business Summary", "Financial Summary"
  pub last_modified: Option<chrono::DateTime<chrono::Utc>>, // Note: Includes time
  pub text: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ContactInfo {
  pub last_updated: Option<chrono::DateTime<chrono::Utc>>,
  pub address_line1: Option<String>,
  pub address_line2: Option<String>,
  pub address_line3: Option<String>,
  pub city: Option<String>,
  pub state_region: Option<String>,
  pub postal_code: Option<String>,
  pub country_code: Option<String>,
  pub country_name: Option<String>,
  pub contact_name: Option<String>,
  pub contact_title: Option<String>,
  pub main_phone_country_code: Option<String>,
  pub main_phone_area_code: Option<String>,
  pub main_phone_number: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct WebLink {
  pub last_updated: Option<chrono::DateTime<chrono::Utc>>,
  pub home_page: Option<String>,
  pub email: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Industry {
  pub industry_type: Option<String>, // "TRBC", "NAICS", "SIC"
  pub order: Option<i32>,
  pub reported: Option<bool>,
  pub code: Option<String>,
  pub mnemonic: Option<String>,
  pub description: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PeerInfo {
  pub last_updated: Option<chrono::DateTime<chrono::Utc>>,
  pub industries: Vec<Industry>,
  pub index_constituents: Vec<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Officer {
  pub rank: Option<i32>,
  pub since: Option<String>, // Could parse to date if format is consistent
  pub first_name: Option<String>,
  pub middle_initial: Option<String>,
  pub last_name: Option<String>,
  pub age: Option<String>, // Keep as string due to trailing space?
  pub title_start_year: Option<i32>,
  pub title_start_month: Option<i32>,
  pub title_start_day: Option<i32>,
  pub title_id1: Option<String>,
  pub title_abbr1: Option<String>,
  pub title_id2: Option<String>,
  pub title_abbr2: Option<String>,
  pub title_full: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RatioGroup {
  pub id: Option<String>, // e.g., "Price and Volume"
  pub ratios: Vec<Ratio>, // Re-use the existing Ratio struct
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ForecastDataItem {
  pub field_name: String,
  pub value_type: Option<String>, // "N" for numeric, "D" for date?
  pub period_type: Option<String>, // "CURR"
  pub value: Option<String>, // Keep as string, parse later if needed
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ForecastData {
  pub consensus_type: Option<String>,
  pub cur_fiscal_year: Option<i32>,
  pub cur_fiscal_year_end_month: Option<i32>,
  pub cur_interim_end_cal_year: Option<i32>,
  pub cur_interim_end_month: Option<i32>,
  pub earnings_basis: Option<String>,
  pub items: Vec<ForecastDataItem>,
}


#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ReportSnapshot {
  // Top-level CoIDs might exist, merge with Issue CoIDs?
  pub top_level_coids: HashMap<String, String>, // e.g., RepNo, CompanyName, IRSNo, CIKNo
  pub issues: Vec<Issue>,
  pub co_general_info: Option<CoGeneralInfo>,
  pub text_info: Vec<TextInfoItem>,
  pub contact_info: Option<ContactInfo>,
  pub web_links: Option<WebLink>,
  pub peer_info: Option<PeerInfo>,
  pub officers: Vec<Officer>,
  // Ratios section attributes
  pub ratios_price_currency: Option<String>,
  pub ratios_reporting_currency: Option<String>,
  pub ratios_exchange_rate: Option<f64>,
  pub ratios_latest_available_date: Option<chrono::NaiveDate>,
  pub ratio_groups: Vec<RatioGroup>,
  pub forecast_data: Option<ForecastData>,
  // Add CompanyInformation if needed, populated from CoIDs/Issues
  pub company_info: Option<CompanyInformation>,
}

// --- Structures for "ReportsFinSummary" ---

/// Represents an EPS (Earnings Per Share) record from FinancialSummary.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct EPSRecord {
  pub as_of_date: Option<chrono::NaiveDate>,
  pub report_type: Option<String>, // e.g., "R", "TTM", "A", "P"
  pub period: Option<String>,      // e.g., "12M", "3M"
  pub value: Option<f64>,
  pub currency: Option<String>, // From parent <EPSs currency="USD">
}

/// Represents a Dividend Per Share record from FinancialSummary.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DividendPerShareRecord {
  pub as_of_date: Option<chrono::NaiveDate>,
  pub report_type: Option<String>,
  pub period: Option<String>,
  pub value: Option<f64>,
  pub currency: Option<String>,
}

/// Represents a Total Revenue record from FinancialSummary.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TotalRevenueRecord {
  pub as_of_date: Option<chrono::NaiveDate>,
  pub report_type: Option<String>,
  pub period: Option<String>,
  pub value: Option<f64>,
  pub currency: Option<String>,
}

/// Represents an individual announced dividend from FinancialSummary.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AnnouncedDividendRecord {
  pub dividend_type: Option<String>, // e.g., "CD" (Cash Dividend)
  pub ex_date: Option<chrono::NaiveDate>,
  pub record_date: Option<chrono::NaiveDate>,
  pub pay_date: Option<chrono::NaiveDate>,
  pub declaration_date: Option<chrono::NaiveDate>,
  pub value: Option<f64>,
  pub currency: Option<String>,
}

// --- Structures for "ReportsFinStatements" (DIFFERENT from ReportsFinSummary) ---
// This section remains for the other report type that uses <ReportFinancialStatements>

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FinancialStatementType {
  INC, // IncomeStatement
  BAL, // BalanceSheet
  CAS, // CashFlow
}

impl Default for FinancialStatementType {
  fn default() -> Self {
    FinancialStatementType::INC // Or a specific 'Unknown' variant if you add one
  }
}

impl TryFrom<&str> for FinancialStatementType {
  type Error = IBKRError;

  fn try_from(value: &str) -> Result<Self, Self::Error> {
    match value {
      "INC" => Ok(FinancialStatementType::INC),
      "BAL" => Ok(FinancialStatementType::BAL),
      "CAS" => Ok(FinancialStatementType::CAS),
      _ => Err(IBKRError::ParseError(format!("Unknown FinancialStatementType: {}", value))),
    }
  }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PeriodType {
  Annual,
  Quarterly, // "Interim" in IBKR XML
}

impl Default for PeriodType {
  fn default() -> Self {
    PeriodType::Annual // Or a specific 'Unknown' variant if you add one
  }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct FinancialStatement {
  pub statement_type: FinancialStatementType,
  pub period_type: PeriodType, // Annual or Interim/Quarterly
  pub end_date: Option<chrono::NaiveDate>,
  pub fiscal_year: Option<i32>,
  pub period_length: Option<i32>, // e.g., 3 for quarter, 12 for annual (from FiscalPeriod/@FiscalPeriodNumber or calculated)
  pub currency: Option<String>, // From <Statement Currency="USD">
  // Using HashMap for flexibility with coaItem codes
  // Key: COAItem code (e.g., "SREV", "ATCA"), Value: the numeric value
  pub items: HashMap<String, Option<f64>>,
  // Raw items if needed for unconverted values or different types
  // pub raw_items: HashMap<String, Option<String>>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ReportsFinSummary {
  // CompanyInfo might be sourced from a higher level if this XML is part of a larger document.
  // For now, assuming it's not directly in this <FinancialSummary> snippet.
  // pub company_info: Option<CompanyInformation>,
  pub eps_records: Vec<EPSRecord>,
  pub dividend_per_share_records: Vec<DividendPerShareRecord>,
  pub total_revenue_records: Vec<TotalRevenueRecord>,
  pub announced_dividend_records: Vec<AnnouncedDividendRecord>,
}


/// Top-level enum for parsed fundamental report data.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
// #[serde(untagged)] // Consider if you want to deserialize without a type field, might make errors harder.
pub enum ParsedFundamentalData {
  Snapshot(ReportSnapshot),
  FinancialSummary(ReportsFinSummary),
  // Add other variants as more report types are supported
  // Error(String), // Could be useful for returning parsing errors directly
}

/// Represents a single data point in a histogram.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HistogramEntry {
  pub price: f64,
  pub size: f64, // Using f64 for Decimal size
}

/// State for a histogram data request.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct HistogramDataRequestState {
  pub req_id: i32,
  pub contract: Contract,
  pub use_rth: bool,
  pub time_period: TimePeriodUnit, // Changed from String
  pub items: Vec<HistogramEntry>, // Stores the received histogram data
  pub completed: bool, // Flag for completion (all data received or error)
  pub error_code: Option<i32>,
  pub error_message: Option<String>,
}

// Default implementation for HistogramDataRequestState can be added if needed,
// but it's usually constructed with specific request parameters.

/// Represents a single historical tick.
/// The structure varies depending on what was requested ("TRADES", "BID_ASK", "MIDPOINT").
#[derive(Debug, Clone)]
pub enum HistoricalTick {
  Trade {
    time: i64, // Unix timestamp
    price: f64,
    size: f64, // TWS API sends size as double for historical ticks
    tick_attrib_last: TickAttribLast,
    exchange: String,
    special_conditions: String,
  },
  BidAsk {
    time: i64, // Unix timestamp
    tick_attrib_bid_ask: TickAttribBidAsk,
    price_bid: f64,
    price_ask: f64,
    size_bid: f64,
    size_ask: f64,
  },
  MidPoint {
    time: i64, // Unix timestamp
    price: f64, // Midpoint price
    size: f64,  // Size associated with the midpoint tick (often 0)
  },
}

/// State for a historical ticks data request.
#[derive(Debug, Clone, Default)]
pub struct HistoricalTicksRequestState {
  pub req_id: i32,
  pub contract: Contract,
  // Store original request parameters for context
  pub start_date_time: Option<chrono::DateTime<chrono::Utc>>,
  pub end_date_time: Option<chrono::DateTime<chrono::Utc>>,
  pub number_of_ticks: i32,
  pub what_to_show: WhatToShow, // Changed from String
  pub use_rth: bool,
  pub ignore_size: bool,
  pub misc_options: Vec<(String, String)>,

  pub ticks: Vec<HistoricalTick>, // Stores the received ticks
  pub completed: bool, // True when 'done' is received from the historical_ticks_* callback
  pub error_code: Option<i32>,
  pub error_message: Option<String>,
}

impl HistoricalTicksRequestState {
  pub fn new(req_id: i32, contract: Contract, start_date_time: Option<chrono::DateTime<chrono::Utc>>, end_date_time: Option<chrono::DateTime<chrono::Utc>>, number_of_ticks: i32, what_to_show: WhatToShow, use_rth: bool, ignore_size: bool, misc_options: Vec<(String, String)>) -> Self { // Changed what_to_show type
    Self {
      req_id, contract, start_date_time, end_date_time, number_of_ticks, what_to_show, use_rth, ignore_size, misc_options,
      ticks: Vec::new(), completed: false, error_code: None, error_message: None,
    }
  }
}
