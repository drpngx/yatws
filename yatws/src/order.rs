// yatws/src/order.rs
// Order data structures for the IBKR API

use crate::contract::Contract;
use chrono::{DateTime, Utc};
use std::fmt;

/// An order
#[derive(Debug, Clone)]
pub struct Order {
  pub id: String,
  pub contract: Contract,
  pub request: OrderRequest,
  pub state: OrderState,
  pub created_at: DateTime<Utc>,
  pub updated_at: DateTime<Utc>,
}

/// Order request parameters
#[derive(Debug, Clone)]
pub struct OrderRequest {
  pub order_type: OrderType,
  pub side: OrderSide,
  pub quantity: f64,

  // Price fields (used depending on order type)
  pub limit_price: Option<f64>,
  pub aux_price: Option<f64>,  // Stop price for stop orders

  // Time in force
  pub time_in_force: TimeInForce,
  pub good_till_date: Option<DateTime<Utc>>,

  // Order attributes
  pub all_or_none: bool,
  pub min_quantity: Option<i32>,
  pub outside_rth: bool,
  pub hidden: bool,
  pub transmit: bool,

  // Order reference and misc
  pub parent_id: Option<String>,
  pub order_ref: Option<String>,
  pub account: Option<String>,
  pub fa_group: Option<String>,
  pub fa_method: Option<String>,
  pub fa_percentage: Option<String>,

  // Algo parameters
  pub algo_strategy: Option<String>,
  pub algo_params: Vec<(String, String)>,
}

impl Default for OrderRequest {
  fn default() -> Self {
    OrderRequest {
      order_type: OrderType::Market,
      side: OrderSide::Buy,
      quantity: 0.0,
      limit_price: None,
      aux_price: None,
      time_in_force: TimeInForce::Day,
      good_till_date: None,
      all_or_none: false,
      min_quantity: None,
      outside_rth: false,
      hidden: false,
      transmit: true,
      parent_id: None,
      order_ref: None,
      account: None,
      fa_group: None,
      fa_method: None,
      fa_percentage: None,
      algo_strategy: None,
      algo_params: Vec::new(),
    }
  }
}

/// Order state
#[derive(Debug, Clone)]
pub struct OrderState {
  pub status: OrderStatus,
  pub filled_quantity: f64,
  pub remaining_quantity: f64,
  pub average_fill_price: f64,
  pub last_fill_price: Option<f64>,
  pub why_held: Option<String>,
  pub error: Option<crate::base::IBKRError>,
}

impl Default for OrderState {
  fn default() -> Self {
    OrderState {
      status: OrderStatus::New,
      filled_quantity: 0.0,
      remaining_quantity: 0.0,
      average_fill_price: 0.0,
      last_fill_price: None,
      why_held: None,
      error: None,
    }
  }
}

/// Order updates (for modifying existing orders)
#[derive(Debug, Clone, Default)]
pub struct OrderUpdates {
  pub quantity: Option<f64>,
  pub limit_price: Option<f64>,
  pub aux_price: Option<f64>,
  pub time_in_force: Option<TimeInForce>,
  pub good_till_date: Option<DateTime<Utc>>,
  pub all_or_none: Option<bool>,
  pub min_quantity: Option<i32>,
  pub outside_rth: Option<bool>,
}

/// Order side (buy or sell)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSide {
  Buy,
  Sell,
  SellShort,
}

impl fmt::Display for OrderSide {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      OrderSide::Buy => write!(f, "BUY"),
      OrderSide::Sell => write!(f, "SELL"),
      OrderSide::SellShort => write!(f, "SSHORT"),
    }
  }
}

/// Order type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderType {
  Market,
  Limit,
  Stop,
  StopLimit,
  MarketIfTouched,
  LimitIfTouched,
  TrailingStop,
  TrailingStopLimit,
  PeggedToMarket,
  PeggedToMidpoint,
  MarketToLimit,
  Relative,
  BoxTop,
  LimitOnClose,
  MarketOnClose,
  LimitOnOpen,
  MarketOnOpen,
  Volatility,
  None,
}

impl fmt::Display for OrderType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let s = match self {
      OrderType::Market => "MKT",
      OrderType::Limit => "LMT",
      OrderType::Stop => "STP",
      OrderType::StopLimit => "STP LMT",
      OrderType::MarketIfTouched => "MIT",
      OrderType::LimitIfTouched => "LIT",
      OrderType::TrailingStop => "TRAIL",
      OrderType::TrailingStopLimit => "TRAIL LIMIT",
      OrderType::PeggedToMarket => "PEG MKT",
      OrderType::PeggedToMidpoint => "PEG MID",
      OrderType::MarketToLimit => "MTL",
      OrderType::Relative => "REL",
      OrderType::BoxTop => "BOX TOP",
      OrderType::LimitOnClose => "LOC",
      OrderType::MarketOnClose => "MOC",
      OrderType::LimitOnOpen => "LOO",
      OrderType::MarketOnOpen => "MOO",
      OrderType::Volatility => "VOL",
      OrderType::None => "NONE",
    };
    write!(f, "{}", s)
  }
}

/// Time in force for orders
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeInForce {
  Day,
  GoodTillCancelled,
  FillOrKill,
  ImmediateOrCancel,
  GoodTillDate,
}

impl fmt::Display for TimeInForce {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let s = match self {
      TimeInForce::Day => "DAY",
      TimeInForce::GoodTillCancelled => "GTC",
      TimeInForce::FillOrKill => "FOK",
      TimeInForce::ImmediateOrCancel => "IOC",
      TimeInForce::GoodTillDate => "GTD",
    };
    write!(f, "{}", s)
  }
}

/// Order status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
  New,
  PendingSubmit,
  PendingCancel,
  PreSubmitted,
  Submitted,
  ApiPending,
  ApiCancelled,
  Cancelled,
  Filled,
  Inactive,
}

impl fmt::Display for OrderStatus {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let s = match self {
      OrderStatus::New => "New",
      OrderStatus::PendingSubmit => "PendingSubmit",
      OrderStatus::PendingCancel => "PendingCancel",
      OrderStatus::PreSubmitted => "PreSubmitted",
      OrderStatus::Submitted => "Submitted",
      OrderStatus::ApiPending => "ApiPending",
      OrderStatus::ApiCancelled => "ApiCancelled",
      OrderStatus::Cancelled => "Cancelled",
      OrderStatus::Filled => "Filled",
      OrderStatus::Inactive => "Inactive",
    };
    write!(f, "{}", s)
  }
}

/// Trade execution information
#[derive(Debug, Clone)]
pub struct Execution {
  pub execution_id: String,
  pub order_id: String,
  pub symbol: String,
  pub side: OrderSide,
  pub quantity: f64,
  pub price: f64,
  pub time: DateTime<Utc>,
  pub commission: f64,
  pub exchange: String,
  pub account: String,
}

/// Observer trait for order notifications
pub trait OrderObserver: Send + Sync {
  fn on_order_update(&self, order: &Order);
  fn on_order_error(&self, order_id: &str, error: &crate::base::IBKRError);
}
