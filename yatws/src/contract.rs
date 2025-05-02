// yatws/src/contract.rs
// Contract data structures for the IBKR API

use chrono::{DateTime, Utc};
use std::fmt;
use std::hash::{Hash, Hasher};
use crate::base::IBKRError;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecType {
  Stock,          // STK
  Option,         // OPT
  Future,         // FUT
  ContinuousFuture, // CONTFUT
  Forex,          // CASH
  Bond,           // BOND
  Cfd,            // CFD
  FutureOption,   // FOP
  Warrant,        // WAR
  IndexOption,    // IOPT
  Forward,        // FWD
  Combo,          // BAG
  Index,
  Bill,           // BILL
  Fund,           // FUND
  Fixed,          // FIXED
  Slb,            // SLB
  News,           // NEWS
  Commodity,      // CMDTY
  Basket,         // BSK
  Icu,            // ICU - Index-Linked Currency Unit?
  Ics,            // ICS - Index-Linked Contract?
  Crypto,         // CRYPTO
}

impl fmt::Display for SecType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let s = match self {
      SecType::Stock => "STK",
      SecType::Option => "OPT",
      SecType::Future => "FUT",
      SecType::ContinuousFuture => "CONTFUT",
      SecType::Forex => "CASH",
      SecType::Bond => "BOND",
      SecType::Cfd => "CFD",
      SecType::FutureOption => "FOP",
      SecType::Warrant => "WAR",
      SecType::IndexOption => "IOPT",
      SecType::Forward => "FWD",
      SecType::Combo => "BAG",
      SecType::Index => "IND",
      SecType::Bill => "BILL",
      SecType::Fund => "FUND",
      SecType::Fixed => "FIXED",
      SecType::Slb => "SLB",
      SecType::News => "NEWS",
      SecType::Commodity => "CMDTY",
      SecType::Basket => "BSK",
      SecType::Icu => "ICU",
      SecType::Ics => "ICS",
      SecType::Crypto => "CRYPTO",
    };
    write!(f, "{}", s)
  }
}

impl std::str::FromStr for SecType {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "STK" => Ok(SecType::Stock),
      "OPT" => Ok(SecType::Option),
      "FUT" => Ok(SecType::Future),
      "CONTFUT" => Ok(SecType::ContinuousFuture),
      "CASH" => Ok(SecType::Forex),
      "BOND" => Ok(SecType::Bond),
      "CFD" => Ok(SecType::Cfd),
      "FOP" => Ok(SecType::FutureOption),
      "WAR" => Ok(SecType::Warrant),
      "IOPT" => Ok(SecType::IndexOption),
      "FWD" => Ok(SecType::Forward),
      "BAG" => Ok(SecType::Combo),
      "IND" => Ok(SecType::Index),
      "BILL" => Ok(SecType::Bill),
      "FUND" => Ok(SecType::Fund),
      "FIXED" => Ok(SecType::Fixed),
      "SLB" => Ok(SecType::Slb),
      "NEWS" => Ok(SecType::News),
      "CMDTY" => Ok(SecType::Commodity),
      "BSK" => Ok(SecType::Basket),
      "ICU" => Ok(SecType::Icu),
      "ICS" => Ok(SecType::Ics),
      "CRYPTO" => Ok(SecType::Crypto),
      _ => Err(format!("Unknown security type: {}", s)),
    }
  }
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecIdType {
  Cusip,
  Sedol,
  Isin,
  Ric,
}

impl fmt::Display for SecIdType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let s = match self {
      SecIdType::Cusip => "CUSIP",
      SecIdType::Sedol => "SEDOL",
      SecIdType::Isin => "ISIN",
      SecIdType::Ric => "RIC",
    };
    write!(f, "{}", s)
  }
}

impl std::str::FromStr for SecIdType {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "CUSIP" => Ok(SecIdType::Cusip),
      "SEDOL" => Ok(SecIdType::Sedol),
      "ISIN" => Ok(SecIdType::Isin),
      "RIC" => Ok(SecIdType::Ric),
      _ => Err(format!("Unknown security ID type: {}", s)),
    }
  }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComboLeg {
  pub con_id: i32,
  pub ratio: i32,
  pub action: String,
  pub exchange: String,
  pub open_close: i32, // 0=Same, 1=Open, 2=Close, 3=Unknown
  pub short_sale_slot: i32, // 1=Retail, 2=Institution
  pub designated_location: String,
  pub exempt_code: i32,
  pub price: Option<f64>, // Added optional price per leg
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeltaNeutralContract {
  pub con_id: i32,
  pub delta: f64,
  pub price: f64,
}

#[derive(Debug, Clone)]
pub struct Contract {
  pub con_id: i32,
  pub symbol: String,
  pub sec_type: SecType,
  pub last_trade_date_or_contract_month: Option<String>,
  pub last_trade_date: Option<String>,
  pub strike: Option<f64>,
  pub right: Option<OptionRight>,
  pub multiplier: Option<String>,
  pub exchange: String,
  pub primary_exchange: Option<String>,
  pub currency: String,
  pub local_symbol: Option<String>,
  pub trading_class: Option<String>,
  pub sec_id_type: Option<SecIdType>,
  pub sec_id: Option<String>,
  pub description: Option<String>,
  pub issuer_id: Option<String>,
  pub delta_neutral_contract: Option<DeltaNeutralContract>,
  pub include_expired: bool,
  pub combo_legs_descrip: Option<String>,
  pub combo_legs: Vec<ComboLeg>,
}

impl Contract {
  pub fn new() -> Self {
    Self::default()
  }
  /// Create a new stock contract
  pub fn stock(symbol: &str) -> Self {
    Self::stock_with_exchange(symbol, "SMART", "USD")
  }

  /// Create a new stock contract with specified exchange and currency
  pub fn stock_with_exchange(symbol: &str, exchange: &str, currency: &str) -> Self {
    Contract {
      con_id: 0,
      symbol: symbol.to_string(),
      sec_type: SecType::Stock,
      exchange: exchange.to_string(),
      currency: currency.to_string(),
      last_trade_date_or_contract_month: None,
      last_trade_date: None,
      strike: None,
      right: None,
      multiplier: None,
      primary_exchange: None,
      local_symbol: None,
      trading_class: None,
      sec_id_type: None,
      sec_id: None,
      description: None,
      issuer_id: None,
      delta_neutral_contract: None,
      include_expired: false,
      combo_legs_descrip: None,
      combo_legs: Vec::new(),
    }
  }

  /// Create a new option contract
  pub fn option(
    symbol: &str,
    expiry: &str,
    strike: f64,
    right: OptionRight,
    exchange: &str,
    currency: &str,
  ) -> Self {
    Contract {
      con_id: 0,
      symbol: symbol.to_string(),
      sec_type: SecType::Option,
      last_trade_date_or_contract_month: Some(expiry.to_string()),
      exchange: exchange.to_string(),
      currency: currency.to_string(),
      strike: Some(strike),
      right: Some(right),
      multiplier: Some("100".to_string()), // Default for US options
      last_trade_date: None,
      primary_exchange: None,
      local_symbol: None,
      trading_class: None,
      sec_id_type: None,
      sec_id: None,
      description: None,
      issuer_id: None,
      delta_neutral_contract: None,
      include_expired: false,
      combo_legs_descrip: None,
      combo_legs: Vec::new(),
    }
  }

  /// Is this a combo contract?
  pub fn is_combo(&self) -> bool {
    !self.combo_legs.is_empty()
  }

  /// Get a text description that can be used for display
  pub fn text_description(&self) -> String {
    let mut sb = String::new();

    if self.is_combo() {
      for (i, leg) in self.combo_legs.iter().enumerate() {
        if i > 0 {
          sb.push_str("/");
        }
        sb.push_str(&format!("{}", leg.con_id));
      }
    } else {
      sb.push_str(&self.symbol);
      app(&mut sb, &self.sec_type.to_string());
      app(&mut sb, &self.exchange);

      if self.exchange == "SMART" && self.primary_exchange.is_some() {
        app(&mut sb, &self.primary_exchange.as_ref().unwrap());
      }

      if let Some(date) = &self.last_trade_date_or_contract_month {
        app(&mut sb, date);
      }

      if let Some(date) = &self.last_trade_date {
        app(&mut sb, date);
      }

      if let Some(strike) = &self.strike {
        app(&mut sb, &strike.to_string());
      }

      if let Some(right) = &self.right {
        app(&mut sb, &right.to_string());
      }

      app(&mut sb, &self.currency);
    }

    sb
  }
}

impl Default for Contract {
  fn default() -> Contract {
    Self {
      con_id: 0,
      symbol: String::new(),
      sec_type: SecType::Stock,
      last_trade_date_or_contract_month: None,
      last_trade_date: None,
      strike: None,
      right: None,
      multiplier: None,
      exchange: String::new(),
      primary_exchange: None,
      currency: String::new(),
      local_symbol: None,
      trading_class: None,
      sec_id_type: None,
      sec_id: None,
      description: None,
      issuer_id: None,
      delta_neutral_contract: None,
      include_expired: false,
      combo_legs_descrip: None,
      combo_legs: Vec::new(),
    }
  }
}

impl PartialEq for Contract {
  fn eq(&self, other: &Self) -> bool {
    // Start with basic fields
    if self.con_id != other.con_id {
      return false;
    }

    if self.sec_type != other.sec_type {
      return false;
    }

    if self.symbol != other.symbol ||
      self.exchange != other.exchange ||
      self.primary_exchange != other.primary_exchange ||
      self.currency != other.currency {
        return false;
      }

    // Skip additional fields for BOND
    if self.sec_type != SecType::Bond {
      if self.strike != other.strike {
        return false;
      }

      if self.last_trade_date_or_contract_month != other.last_trade_date_or_contract_month ||
        self.last_trade_date != other.last_trade_date ||
        self.right != other.right ||
        self.multiplier != other.multiplier ||
        self.local_symbol != other.local_symbol ||
        self.trading_class != other.trading_class {
          return false;
        }
    }

    if self.sec_id_type != other.sec_id_type {
      return false;
    }

    if self.sec_id != other.sec_id {
      return false;
    }

    // Compare combo legs
    if self.combo_legs.len() != other.combo_legs.len() {
      return false;
    }

    // Unordered comparison of combo legs
    for leg in &self.combo_legs {
      if !other.combo_legs.iter().any(|other_leg| other_leg == leg) {
        return false;
      }
    }

    // Compare delta neutral contract
    match (&self.delta_neutral_contract, &other.delta_neutral_contract) {
      (Some(a), Some(b)) => if a != b { return false; },
      (None, None) => {},
      _ => return false,
    }

    if self.description != other.description {
      return false;
    }

    if self.issuer_id != other.issuer_id {
      return false;
    }

    true
  }
}

impl Eq for Contract {}

impl Hash for Contract {
  fn hash<H: Hasher>(&self, state: &mut H) {
    // Use a few key fields for hashing
    self.con_id.hash(state);
    if !self.symbol.is_empty() {
      self.symbol.hash(state);
    }
    if let Some(strike) = self.strike {
      strike.to_bits().hash(state);
    }
  }
}

// Helper function for text_description
fn app(buf: &mut String, obj: &str) {
  if !obj.is_empty() {
    buf.push(' ');
    buf.push_str(obj);
  }
}


/// Option right enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)] // Added Hash
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

impl FromStr for OptionRight {
  type Err = IBKRError;
  fn from_str(s: &str) -> Result<Self, IBKRError> {
    // Match case-insensitively for robustness
    match s.trim().to_uppercase().as_str() {
      "C" | "CALL" => Ok(OptionRight::Call),
      "P" | "PUT" => Ok(OptionRight::Put),
      _ => Err(IBKRError::ParseError(s.to_string())),
    }
  }
}

/// Detailed contract information
#[derive(Debug, Clone)]
pub struct ContractDetails {
  pub contract: Contract,
  pub stock_type: String,   // ETF, ADR, etc
  pub market_name: String,
  pub min_tick: f64,
  pub price_magnifier: i32,
  pub order_types: String,
  pub valid_exchanges: String,
  pub underlying_con_id: i32,
  pub underlying_symbol: String,
  pub underlying_sec_type: Option<SecType>,
  pub real_expiration_date: String,
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
  pub sec_id_list: Vec<TagValue>, // List of (sec_id_type, sec_id)
  pub min_size: f64,
  pub size_increment: f64,
  pub suggested_size_increment: f64,
  pub agg_group: Option<i32>,
  pub market_rule_ids: String,
  pub fund_name: Option<String>,
  pub fund_family: Option<String>,
  pub fund_type: Option<String>,
  pub ineligibility_reason_list: Vec<(String, String)>,
  pub bond_details: Option<BondDetails>,
}

/// Bond-specific details
#[derive(Debug, Clone)]
pub struct BondDetails {
  pub cusip: String,
  pub maturity: String,
  pub issue_date: String,
  pub coupon: f64,
  pub next_option_date: Option<DateTime<Utc>>,
  pub next_option_type: Option<String>,
  pub next_option_partial: bool,
  pub callable: bool,
  pub puttable: bool,
  pub convertible: bool,
  pub ratings: String,
  pub desc_append: String,
  pub bond_type: String,
  pub coupon_type: String,
  pub duration: f64,
  pub notes: String,
}

impl Default for ContractDetails {
  fn default() -> Self {
    ContractDetails {
      contract: Contract::new(),
      stock_type: String::new(),
      market_name: String::new(),
      min_tick: 0.0,
      price_magnifier: 1,
      order_types: String::new(),
      valid_exchanges: String::new(),
      underlying_con_id: 0,
      underlying_symbol: String::new(),
      underlying_sec_type: None,
      real_expiration_date: String::new(),
      long_name: String::new(),
      contract_month: String::new(),
      industry: String::new(),
      category: String::new(),
      subcategory: String::new(),
      time_zone_id: String::new(),
      trading_hours: String::new(),
      liquid_hours: String::new(),
      ev_rule: String::new(),
      ev_multiplier: 0.0,
      sec_id_list: Vec::new(),
      bond_details: None,
      min_size: 0.0,
      size_increment: 0.0,
      suggested_size_increment: 0.0,
      agg_group: None,
      market_rule_ids: String::new(),
      fund_name: None,
      fund_family: None,
      fund_type: None,
      ineligibility_reason_list: Vec::new(),
    }
  }
}

impl Default for BondDetails {
  fn default() -> Self {
    BondDetails {
      cusip: String::new(),
      maturity: String::new(),
      issue_date: String::new(),
      coupon: 0.0,
      next_option_date: None,
      next_option_type: None,
      next_option_partial: false,
      callable: false,
      puttable: false,
      convertible: false,
      ratings: String::new(),
      desc_append: String::new(),
      bond_type: String::new(),
      coupon_type: String::new(),
      duration: 0.0,
      notes: String::new(),
    }
  }
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

// ----- Reference data:
/// Represents a soft dollar tier details.
#[derive(Debug, Clone, Default)]
pub struct SoftDollarTier {
  pub name: String,
  pub value: String,
  pub display_name: String,
}

/// Represents a family code.
#[derive(Debug, Clone, Default)]
pub struct FamilyCode {
  pub account_id: String,
  pub family_code_str: String,
}

/// Describes a contract for symbol matching results.
#[derive(Debug, Clone)]
pub struct ContractDescription {
  pub contract: Contract,
  pub derivative_sec_types: Vec<String>,
}

/// Represents an entry in a market depth exchange description.
#[derive(Debug, Clone, Default)]
pub struct DepthMktDataDescription {
  pub exchange: String,
  pub sec_type: String,
  pub listing_exch: String, // Added based on Java EDecoder
  pub service_data_type: String,
  pub agg_group: Option<i32>, // Optional based on server version
}

/// Represents a component of a SMART route.
#[derive(Debug, Clone)]
pub struct SmartComponent {
  pub bit_number: i32,
  pub exchange: String,
  pub exchange_letter: char,
}

/// Represents a price increment rule.
#[derive(Debug, Clone, Copy, Default)]
pub struct PriceIncrement {
  pub low_edge: f64,
  pub increment: f64,
}

/// Represents a market rule ID and its associated price increments.
#[derive(Debug, Clone, Default)]
pub struct MarketRule {
  pub market_rule_id: i32,
  pub price_increments: Vec<PriceIncrement>,
}

/// Represents a session in the historical data schedule.
#[derive(Debug, Clone, Default)]
pub struct HistoricalSession {
  pub start_date_time: String, // Consider parsing to DateTime<Tz> if needed
  pub end_date_time: String,   // Consider parsing to DateTime<Tz> if needed
  pub ref_date: String,        // Consider parsing to Date if needed
}

#[derive(Debug, Clone, Default)]
pub struct TagValue {
  pub tag: String,
  pub value: String,
}
