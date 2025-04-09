// yatws/src/contract.rs
// Contract data structures for the IBKR API

use chrono::{DateTime, Utc};
use std::fmt;

/// A financial instrument contract
#[derive(Debug, Clone, PartialEq)]
pub struct Contract {
  pub con_id: i32,               // Contract ID
  pub symbol: String,            // Symbol
  pub sec_type: SecurityType,    // Security type
  pub exchange: String,          // Exchange
  pub currency: String,          // Currency
  pub local_symbol: Option<String>, // Local symbol
  pub primary_exchange: Option<String>, // Primary exchange
  pub include_expired: bool,     // Include expired contracts

  // Optional fields for specific security types
  pub strike: Option<f64>,       // Option strike price
  pub expiry: Option<DateTime<Utc>>, // Option/Future expiry
  pub right: Option<OptionRight>, // Option right
  pub multiplier: Option<String>, // Contract multiplier

  // Bond specific fields
  pub issuer: Option<String>,    // Bond issuer
  pub cusip: Option<String>,     // Bond CUSIP

  // Combo specific fields
  pub combo_legs: Vec<ComboLeg>, // Combo legs
}

impl Contract {
  /// Create a new stock contract
  pub fn stock(symbol: &str) -> Self {
    Self::stock_with_exchange(symbol, "SMART", "USD")
  }

  /// Create a new stock contract with specified exchange and currency
  pub fn stock_with_exchange(symbol: &str, exchange: &str, currency: &str) -> Self {
    Contract {
      con_id: 0,
      symbol: symbol.to_string(),
      sec_type: SecurityType::Stock,
      exchange: exchange.to_string(),
      currency: currency.to_string(),
      local_symbol: None,
      primary_exchange: None,
      include_expired: false,
      strike: None,
      expiry: None,
      right: None,
      multiplier: None,
      issuer: None,
      cusip: None,
      combo_legs: Vec::new(),
    }
  }

  /// Create a new option contract
  pub fn option(
    symbol: &str,
    expiry: DateTime<Utc>,
    strike: f64,
    right: OptionRight,
    exchange: &str,
    currency: &str,
  ) -> Self {
    Contract {
      con_id: 0,
      symbol: symbol.to_string(),
      sec_type: SecurityType::Option,
      exchange: exchange.to_string(),
      currency: currency.to_string(),
      local_symbol: None,
      primary_exchange: None,
      include_expired: false,
      strike: Some(strike),
      expiry: Some(expiry),
      right: Some(right),
      multiplier: Some("100".to_string()), // Default for US options
      issuer: None,
      cusip: None,
      combo_legs: Vec::new(),
    }
  }

  /// Create a new future contract
  pub fn future(
    symbol: &str,
    expiry: DateTime<Utc>,
    exchange: &str,
    currency: &str,
  ) -> Self {
    Contract {
      con_id: 0,
      symbol: symbol.to_string(),
      sec_type: SecurityType::Future,
      exchange: exchange.to_string(),
      currency: currency.to_string(),
      local_symbol: None,
      primary_exchange: None,
      include_expired: false,
      strike: None,
      expiry: Some(expiry),
      right: None,
      multiplier: None,
      issuer: None,
      cusip: None,
      combo_legs: Vec::new(),
    }
  }

  /// Create a new forex contract
  pub fn forex(symbol_pair: &str) -> Self {
    let currency_pair: Vec<&str> = symbol_pair.split('/').collect();
    let base = currency_pair.get(0).cloned().unwrap_or("USD");
    let quote = currency_pair.get(1).cloned().unwrap_or("USD");

    Contract {
      con_id: 0,
      symbol: base.to_string(),
      sec_type: SecurityType::Forex,
      exchange: "IDEALPRO".to_string(),
      currency: quote.to_string(),
      local_symbol: None,
      primary_exchange: None,
      include_expired: false,
      strike: None,
      expiry: None,
      right: None,
      multiplier: None,
      issuer: None,
      cusip: None,
      combo_legs: Vec::new(),
    }
  }
}

/// Security type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityType {
  Stock,
  Option,
  Future,
  Index,
  FutureOption,
  Forex,
  Commodity,
  Bond,
  Warrant,
  Fund,
  Combo,
}

impl fmt::Display for SecurityType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let s = match self {
      SecurityType::Stock => "STK",
      SecurityType::Option => "OPT",
      SecurityType::Future => "FUT",
      SecurityType::Index => "IND",
      SecurityType::FutureOption => "FOP",
      SecurityType::Forex => "CASH",
      SecurityType::Commodity => "CMDTY",
      SecurityType::Bond => "BOND",
      SecurityType::Warrant => "WAR",
      SecurityType::Fund => "FUND",
      SecurityType::Combo => "BAG",
    };
    write!(f, "{}", s)
  }
}

/// Option right enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionRight {
  Call,
  Put,
}

impl fmt::Display for OptionRight {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      OptionRight::Call => write!(f, "C"),
      OptionRight::Put => write!(f, "P"),
    }
  }
}

/// A combo leg for a combo contract
#[derive(Debug, Clone, PartialEq)]
pub struct ComboLeg {
  pub con_id: i32,
  pub ratio: i32,
  pub action: String,  // "BUY" or "SELL"
  pub exchange: String,
}

/// Detailed contract information
#[derive(Debug, Clone)]
pub struct ContractDetails {
  pub contract: Contract,
  pub market_name: String,
  pub min_tick: f64,
  pub price_magnifier: i32,
  pub order_types: String,
  pub valid_exchanges: String,
  pub underlying_con_id: i32,
  pub long_name: String,
  pub contract_month: String,
  pub industry: String,
  pub category: String,
  pub subcategory: String,
  pub time_zone_id: String,
  pub trading_hours: String,
  pub liquid_hours: String,
  pub ev_rule: String,
  pub ev_multiplier: f64,
  pub sec_id_list: Vec<(String, String)>, // List of (sec_id_type, sec_id)
  pub bond_details: Option<BondDetails>,
}

/// Bond-specific details
#[derive(Debug, Clone)]
pub struct BondDetails {
  pub cusip: String,
  pub maturity: DateTime<Utc>,
  pub issue_date: DateTime<Utc>,
  pub coupon: f64,
  pub next_option_date: Option<DateTime<Utc>>,
  pub next_option_type: Option<String>,
  pub callable: bool,
  pub puttable: bool,
  pub convertible: bool,
  pub ratings: String,
  pub desc_append: String,
  pub bond_type: String,
  pub coupon_type: String,
  pub duration: f64,
}

/// Bar data for historical data requests
#[derive(Debug, Clone)]
pub struct Bar {
  pub time: DateTime<Utc>,
  pub open: f64,
  pub high: f64,
  pub low: f64,
  pub close: f64,
  pub volume: i64,
  pub wap: f64,
  pub count: i32,
}

/// Bar size for historical data requests
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarSize {
  OneSecond,
  FiveSeconds,
  FifteenSeconds,
  ThirtySeconds,
  OneMinute,
  TwoMinutes,
  ThreeMinutes,
  FiveMinutes,
  FifteenMinutes,
  ThirtyMinutes,
  OneHour,
  FourHours,
  OneDay,
  OneWeek,
  OneMonth,
}

impl fmt::Display for BarSize {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let s = match self {
      BarSize::OneSecond => "1 secs",
      BarSize::FiveSeconds => "5 secs",
      BarSize::FifteenSeconds => "15 secs",
      BarSize::ThirtySeconds => "30 secs",
      BarSize::OneMinute => "1 min",
      BarSize::TwoMinutes => "2 mins",
      BarSize::ThreeMinutes => "3 mins",
      BarSize::FiveMinutes => "5 mins",
      BarSize::FifteenMinutes => "15 mins",
      BarSize::ThirtyMinutes => "30 mins",
      BarSize::OneHour => "1 hour",
      BarSize::FourHours => "4 hours",
      BarSize::OneDay => "1 day",
      BarSize::OneWeek => "1 week",
      BarSize::OneMonth => "1 month",
    };
    write!(f, "{}", s)
  }
}

/// What to show for historical data requests
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhatToShow {
  Trades,
  Midpoint,
  Bid,
  Ask,
  BidAsk,
  HistoricalVolatility,
  ImpliedVolatility,
  OptionVolume,
}

impl fmt::Display for WhatToShow {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let s = match self {
      WhatToShow::Trades => "TRADES",
      WhatToShow::Midpoint => "MIDPOINT",
      WhatToShow::Bid => "BID",
      WhatToShow::Ask => "ASK",
      WhatToShow::BidAsk => "BID_ASK",
      WhatToShow::HistoricalVolatility => "HISTORICAL_VOLATILITY",
      WhatToShow::ImpliedVolatility => "OPTION_IMPLIED_VOLATILITY",
      WhatToShow::OptionVolume => "OPTION_VOLUME",
    };
    write!(f, "{}", s)
  }
}
