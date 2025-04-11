// yatws/src/order.rs
// Order data structures for the IBKR API

use crate::contract::Contract;
use chrono::{DateTime, Utc};
use std::fmt;
use std::str::FromStr;
use crate::base::IBKRError;

#[derive(Debug, Clone)]
pub struct Order {
  /// The client-side order ID assigned when placing the order (string representation of i32).
  pub id: String,
  /// The permanent, unique order ID assigned by TWS/IBKR upon acceptance.
  /// Received via `orderStatus` message. Optional because it's not known initially.
  pub perm_id: Option<i32>,
  /// The client ID of the TWS API connection that placed the order.
  /// Received via `orderStatus` message. Optional.
  pub client_id: Option<i32>,
  /// The client-side order ID (`Order.id`) of the parent order for complex order types
  /// like Brackets, OCA groups (One-Cancels-All), etc. Set during creation if applicable.
  pub parent_id: Option<i64>, // TWS uses int/long, use i64 for safety
  // TWS Also sends parentPermId sometimes, could be added if needed.
  // pub parent_perm_id: Option<i64>,

  /// The financial instrument being ordered.
  pub contract: Contract,
  /// The parameters used to place or describe the order (e.g., quantity, type, price).
  pub request: OrderRequest,
  /// The current live state of the order as reported by TWS (e.g., status, filled qty).
  pub state: OrderState,

  /// Timestamp when the order object was created locally.
  pub created_at: DateTime<Utc>,
  /// Timestamp when the order object was last updated (locally).
  pub updated_at: DateTime<Utc>,
}


/// Order request parameters
#[derive(Debug, Clone, PartialEq)]
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
  pub good_after_time: Option<DateTime<Utc>>,

  // Active times for GTC orders
  pub active_start_time: Option<String>,
  pub active_stop_time: Option<String>,

  // Order attributes
  pub all_or_none: bool,
  pub min_quantity: Option<i32>,
  pub outside_rth: bool,
  pub hidden: bool,
  pub transmit: bool,
  pub sweep_to_fill: bool,
  pub block_order: bool,
  pub not_held: bool,
  pub override_percentage_constraints: bool,
  pub display_size: Option<i32>,
  pub percent_offset: Option<f64>,
  pub trailing_percent: Option<f64>,
  pub trailing_stop_price: Option<f64>,

  // Order cancellation
  pub auto_cancel_date: Option<String>,
  pub auto_cancel_parent: bool,

  // Order reference and misc
  pub parent_id: Option<i64>,
  pub parent_perm_id: Option<i64>,
  pub order_ref: Option<String>,
  pub oca_group: Option<String>,
  pub oca_type: Option<i32>,
  pub rule_80a: Option<String>,
  pub trigger_method: Option<i32>,

  // Account information
  pub account: Option<String>,
  pub customer_account: Option<String>,
  pub settling_firm: Option<String>,
  pub clearing_account: Option<String>,
  pub clearing_intent: Option<String>,
  pub shareholder: Option<String>,

  // Professional/customer designation
  pub professional_customer: bool,
  pub open_close: Option<String>,
  pub origin: i32,  // 0=Customer, 1=Firm

  // Financial advisor fields
  pub fa_group: Option<String>,
  pub fa_method: Option<String>,
  pub fa_percentage: Option<String>,

  // Short sale fields
  pub short_sale_slot: Option<i32>,
  pub designated_location: Option<String>,
  pub exempt_code: Option<i32>,

  // Discretionary and smart routing
  pub discretionary_amt: Option<f64>,
  pub opt_out_smart_routing: bool,
  pub discretionary_up_to_limit_price: bool,

  // Algo parameters
  pub algo_strategy: Option<String>,
  pub algo_params: Vec<(String, String)>,
  pub algo_id: Option<String>,

  // Model code
  pub model_code: Option<String>,

  // Hedge parameters
  pub hedge_type: Option<String>,
  pub hedge_param: Option<String>,

  // Scale order parameters
  pub scale_init_level_size: Option<i32>,
  pub scale_subs_level_size: Option<i32>,
  pub scale_price_increment: Option<f64>,
  pub scale_price_adjust_value: Option<f64>,
  pub scale_price_adjust_interval: Option<i32>,
  pub scale_profit_offset: Option<f64>,
  pub scale_auto_reset: bool,
  pub scale_init_position: Option<i32>,
  pub scale_init_fill_qty: Option<i32>,
  pub scale_random_percent: bool,
  pub scale_table: Option<String>,

  // Auction orders
  pub auction_strategy: Option<i32>,
  pub starting_price: Option<f64>,
  pub stock_ref_price: Option<f64>,
  pub delta: Option<f64>,
  pub stock_range_lower: Option<f64>,
  pub stock_range_upper: Option<f64>,

  // Volatility orders
  pub volatility: Option<f64>,
  pub volatility_type: Option<i32>,
  pub continuous_update: Option<i32>,
  pub reference_price_type: Option<i32>,

  // Combination orders
  pub basis_points: Option<f64>,
  pub basis_points_type: Option<i32>,
  pub combo_legs_desc: Option<String>,
  pub combo_orders: Vec<(String, String)>, // Simplified representation for now

  // Delta-neutral orders
  pub delta_neutral_order_type: Option<String>,
  pub delta_neutral_aux_price: Option<f64>,
  pub delta_neutral_con_id: Option<i32>,
  pub delta_neutral_open_close: Option<String>,
  pub delta_neutral_short_sale: bool,
  pub delta_neutral_short_sale_slot: Option<i32>,
  pub delta_neutral_designated_location: Option<String>,
  pub delta_neutral_settling_firm: Option<String>,
  pub delta_neutral_clearing_account: Option<String>,
  pub delta_neutral_clearing_intent: Option<String>,

  // Pegged orders
  pub reference_contract_id: Option<i32>,
  pub pegged_change_amount: Option<f64>,
  pub is_pegged_change_amount_decrease: bool,
  pub reference_change_amount: Option<f64>,
  pub reference_exchange_id: Option<String>,
  pub adjusted_order_type: Option<OrderType>,
  pub trigger_price: Option<f64>,
  pub adjusted_stop_price: Option<f64>,
  pub adjusted_stop_limit_price: Option<f64>,
  pub adjusted_trailing_amount: Option<f64>,
  pub adjustable_trailing_unit: Option<i32>,
  pub lmt_price_offset: Option<f64>,

  // Order conditions
  pub conditions: Vec<String>, // Simplified for now, would need a proper enum structure
  pub conditions_cancel_order: bool,
  pub conditions_ignore_rth: bool,

  // MiFID II fields
  pub mifid2_decision_maker: Option<String>,
  pub mifid2_decision_algo: Option<String>,
  pub mifid2_execution_trader: Option<String>,
  pub mifid2_execution_algo: Option<String>,

  // Additional flags
  pub dont_use_auto_price_for_hedge: bool,
  pub is_oms_container: bool,
  pub use_price_mgmt_algo: Option<bool>,

  // Advanced trading parameters
  pub duration: Option<i32>,
  pub post_to_ats: Option<i32>,
  pub advanced_error_override: Option<String>,
  pub manual_order_time: Option<String>,
  pub min_trade_qty: Option<i32>,
  pub min_compete_size: Option<i32>,
  pub compete_against_best_offset: Option<f64>,
  pub mid_offset_at_whole: Option<f64>,
  pub mid_offset_at_half: Option<f64>,

  // Bond orders
  pub bond_accrued_interest: Option<String>,

  // Randomization
  pub randomize_size: bool,
  pub randomize_price: bool,

  // External identifiers
  pub external_user_id: Option<String>,
  pub manual_order_indicator: Option<i32>,
  pub ext_operator: Option<String>,

  // Misc trade parameters
  pub imbalance_only: bool,
  pub route_marketable_to_bbo: bool,
  pub soft_dollar_tier: Option<String>,
  pub cash_qty: Option<f64>,
  pub filled_quantity: Option<f64>,
  pub ref_futures_con_id: Option<i32>,
  pub solicited: bool,

  pub what_if: bool,
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
      good_after_time: None,
      active_start_time: None,
      active_stop_time: None,
      all_or_none: false,
      min_quantity: None,
      outside_rth: false,
      hidden: false,
      transmit: true,
      sweep_to_fill: false,
      block_order: false,
      not_held: false,
      override_percentage_constraints: false,
      display_size: None,
      percent_offset: None,
      trailing_percent: None,
      trailing_stop_price: None,
      auto_cancel_date: None,
      auto_cancel_parent: false,
      parent_id: None,
      parent_perm_id: None,
      order_ref: None,
      oca_group: None,
      oca_type: None,
      rule_80a: None,
      trigger_method: None,
      account: None,
      customer_account: None,
      settling_firm: None,
      clearing_account: None,
      clearing_intent: None,
      shareholder: None,
      professional_customer: false,
      open_close: None,
      origin: 0,
      fa_group: None,
      fa_method: None,
      fa_percentage: None,
      short_sale_slot: None,
      designated_location: None,
      exempt_code: None,
      discretionary_amt: None,
      opt_out_smart_routing: false,
      discretionary_up_to_limit_price: false,
      algo_strategy: None,
      algo_params: Vec::new(),
      algo_id: None,
      model_code: None,
      hedge_type: None,
      hedge_param: None,
      scale_init_level_size: None,
      scale_subs_level_size: None,
      scale_price_increment: None,
      scale_price_adjust_value: None,
      scale_price_adjust_interval: None,
      scale_profit_offset: None,
      scale_auto_reset: false,
      scale_init_position: None,
      scale_init_fill_qty: None,
      scale_random_percent: false,
      scale_table: None,
      auction_strategy: None,
      starting_price: None,
      stock_ref_price: None,
      delta: None,
      stock_range_lower: None,
      stock_range_upper: None,
      volatility: None,
      volatility_type: None,
      continuous_update: None,
      reference_price_type: None,
      basis_points: None,
      basis_points_type: None,
      combo_legs_desc: None,
      combo_orders: Vec::new(),
      delta_neutral_order_type: None,
      delta_neutral_aux_price: None,
      delta_neutral_con_id: None,
      delta_neutral_open_close: None,
      delta_neutral_short_sale: false,
      delta_neutral_short_sale_slot: None,
      delta_neutral_designated_location: None,
      delta_neutral_settling_firm: None,
      delta_neutral_clearing_account: None,
      delta_neutral_clearing_intent: None,
      reference_contract_id: None,
      pegged_change_amount: None,
      is_pegged_change_amount_decrease: false,
      reference_change_amount: None,
      reference_exchange_id: None,
      adjusted_order_type: None,
      trigger_price: None,
      adjusted_stop_price: None,
      adjusted_stop_limit_price: None,
      adjusted_trailing_amount: None,
      adjustable_trailing_unit: None,
      lmt_price_offset: None,
      conditions: Vec::new(),
      conditions_cancel_order: false,
      conditions_ignore_rth: false,
      mifid2_decision_maker: None,
      mifid2_decision_algo: None,
      mifid2_execution_trader: None,
      mifid2_execution_algo: None,
      dont_use_auto_price_for_hedge: false,
      is_oms_container: false,
      use_price_mgmt_algo: None,
      duration: None,
      post_to_ats: None,
      advanced_error_override: None,
      manual_order_time: None,
      min_trade_qty: None,
      min_compete_size: None,
      compete_against_best_offset: None,
      mid_offset_at_whole: None,
      mid_offset_at_half: None,
      bond_accrued_interest: None,
      randomize_size: false,
      randomize_price: false,
      external_user_id: None,
      manual_order_indicator: None,
      ext_operator: None,
      imbalance_only: false,
      route_marketable_to_bbo: false,
      soft_dollar_tier: None,
      cash_qty: None,
      filled_quantity: None,
      ref_futures_con_id: None,
      solicited: false,
      what_if: false,
    }
  }
}

/// Order state - reflects the live status received from TWS messages like `openOrder` and `orderStatus`.
/// Note that fields like filled quantity, remaining quantity, avg fill price, last fill price,
/// and why held primarily come from the `orderStatus` message, while margin values,
/// commission details, and warning text primarily come from the `openOrder` message.
#[derive(Debug, Clone)]
pub struct OrderState {
  /// The current status of the order (e.g., Submitted, Filled, Cancelled).
  /// This is updated by both `openOrder` and `orderStatus` messages. The `orderStatus`
  /// message usually provides the most up-to-date status.
  pub status: OrderStatus,

  // --- Fields primarily updated by OrderStatus message ---
  /// Quantity of the order that has been filled.
  pub filled_quantity: f64,
  /// Quantity of the order that is still outstanding.
  pub remaining_quantity: f64,
  /// Average price at which the order has been filled.
  pub average_fill_price: f64,
  /// Price of the last partial fill (requires TWS v973+ and orderStatus msg v4+).
  pub last_fill_price: Option<f64>,
  /// If the order is held by IBKR (e.g., waiting for stock locate), this field provides the reason.
  /// (Requires orderStatus msg v6+).
  pub why_held: Option<String>,
  /// Market cap price for orders benefiting from price improvement (requires TWS v973+).
  pub market_cap_price: Option<f64>,

  // --- Fields primarily updated by OpenOrder message ---
  /// Estimated commission cost for the order.
  pub commission: Option<f64>,
  /// Minimum estimated commission.
  pub min_commission: Option<f64>,
  /// Maximum estimated commission.
  pub max_commission: Option<f64>,
  /// Currency of the commission.
  pub commission_currency: Option<String>,
  /// Warning text returned by TWS related to the order.
  pub warning_text: Option<String>,

  // Margin fields (usually strings, represent values before/after hypothetical order placement/fill)
  // These typically come from the `openOrder` message when initially placed or checked (WhatIf).
  pub initial_margin_before: Option<String>,
  pub maintenance_margin_before: Option<String>,
  pub equity_with_loan_before: Option<String>,
  pub initial_margin_change: Option<String>,
  pub maintenance_margin_change: Option<String>,
  pub equity_with_loan_change: Option<String>,
  pub initial_margin_after: Option<String>,
  pub maintenance_margin_after: Option<String>,
  pub equity_with_loan_after: Option<String>,

  // --- Fields primarily updated by CompletedOrder message ---
  /// Time the order was completed (Filled or Cancelled), in "YYYYMMDD hh:mm:ss (zzz)" format.
  pub completed_time: Option<String>,
  /// Status string associated with order completion (e.g., "Filled", "Cancelled").
  pub completed_status: Option<String>,

  // --- Field set by OrderManager on error ---
  /// If an API error occurred related to this order, it's stored here.
  pub error: Option<IBKRError>, // Use your specific IBKRError type
}

impl Default for OrderState {
  /// Creates a default OrderState, typically representing an order not yet sent or acknowledged.
  fn default() -> Self {
    OrderState {
      status: OrderStatus::New, // Initial status before sending or confirmation
      // Fields from orderStatus default to zero/None initially
      filled_quantity: 0.0,
      remaining_quantity: 0.0, // Should be initialized from OrderRequest.quantity when Order is created
      average_fill_price: 0.0,
      last_fill_price: None,
      why_held: None,
      market_cap_price: None,
      // Fields from openOrder default to None
      commission: None,
      min_commission: None,
      max_commission: None,
      commission_currency: None,
      warning_text: None,
      initial_margin_before: None,
      maintenance_margin_before: None,
      equity_with_loan_before: None,
      initial_margin_change: None,
      maintenance_margin_change: None,
      equity_with_loan_change: None,
      initial_margin_after: None,
      maintenance_margin_after: None,
      equity_with_loan_after: None,
      // Fields from completedOrder default to None
      completed_time: None,
      completed_status: None,
      // Error state
      error: None,
    }
  }
}

impl OrderState { pub fn is_terminal(&self) -> bool { self.status.is_terminal() } }

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
  Market,              // MKT
  Limit,               // LMT
  Stop,                // STP
  StopLimit,           // STP LMT
  MarketIfTouched,     // MIT
  LimitIfTouched,      // LIT
  TrailingStop,        // TRAIL
  TrailingStopLimit,   // TRAIL LIMIT
  PeggedToMarket,      // PEG MKT
  PeggedToMidpoint,    // PEG MID
  MarketToLimit,       // MTL
  Relative,            // REL / RELATIVE
  BoxTop,              // BOX TOP (no longer supported?)
  LimitOnClose,        // LOC
  MarketOnClose,       // MOC
  LimitOnOpen,         // LOO (via TIF OPG + LMT type)
  MarketOnOpen,        // MOO (via TIF OPG + MKT type)
  Volatility,          // VOL
  PeggedToBenchmark,   // PEG BENCH
  PeggedBest,          // PEG BEST
  PeggedPrimary,       // PEG PRIM
  Auction,             // AUCTION
  // GAT,                 // GAT (Guaranteed Average Trade?) - Often represented by TIF GAT
  VWAP,                // VWAP (Algo order type)
  Scale,               // SCALE (Algo order type)
  None,                // Used internally or for errors
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
      OrderType::Relative => "REL", // Prefer short form
      OrderType::BoxTop => "BOX TOP",
      OrderType::LimitOnClose => "LOC",
      OrderType::MarketOnClose => "MOC",
      OrderType::LimitOnOpen => "LMT", // Type is LMT, TIF is OPG
      OrderType::MarketOnOpen => "MKT", // Type is MKT, TIF is OPG
      OrderType::Volatility => "VOL",
      OrderType::PeggedToBenchmark => "PEG BENCH",
      OrderType::PeggedBest => "PEG BEST",
      OrderType::PeggedPrimary => "PEG PRIM",
      OrderType::Auction => "AUCTION",
      // OrderType::GAT => "GAT",
      OrderType::VWAP => "VWAP",
      OrderType::Scale => "SCALE",
      OrderType::None => "None", // Internal representation
    };
    write!(f, "{}", s)
  }
}

impl FromStr for OrderType {
  type Err = IBKRError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_uppercase().as_str() {
      "MKT" => Ok(OrderType::Market),
      "LMT" => Ok(OrderType::Limit),
      "STP" => Ok(OrderType::Stop),
      "STP LMT" => Ok(OrderType::StopLimit),
      "MIT" => Ok(OrderType::MarketIfTouched),
      "LIT" => Ok(OrderType::LimitIfTouched),
      "TRAIL" => Ok(OrderType::TrailingStop),
      "TRAIL LIMIT" => Ok(OrderType::TrailingStopLimit),
      "PEG MKT" => Ok(OrderType::PeggedToMarket),
      "PEG MID" => Ok(OrderType::PeggedToMidpoint),
      "MTL" => Ok(OrderType::MarketToLimit),
      "REL" | "RELATIVE" => Ok(OrderType::Relative),
      "BOX TOP" => Ok(OrderType::BoxTop),
      "LOC" => Ok(OrderType::LimitOnClose),
      "MOC" => Ok(OrderType::MarketOnClose),
      "LOO" => Ok(OrderType::LimitOnOpen), // Represented by LMT + OPG TIF
      "MOO" => Ok(OrderType::MarketOnOpen), // Represented by MKT + OPG TIF
      "VOL" => Ok(OrderType::Volatility),
      "PEG BENCH" => Ok(OrderType::PeggedToBenchmark),
      "PEG BEST" => Ok(OrderType::PeggedBest),
      "PEG PRIM" => Ok(OrderType::PeggedPrimary),
      "AUCTION" => Ok(OrderType::Auction),
      // "GAT" => Ok(OrderType::GAT), // GAT is usually a TIF
      "VWAP" => Ok(OrderType::VWAP),
      "SCALE" => Ok(OrderType::Scale),
      "NONE" => Ok(OrderType::None),
      _ => Err(IBKRError::ParseError(format!("Unknown OrderType string: {}", s))),
    }
  }
}

/// Time in force for orders
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeInForce {
  Day,               // DAY
  GoodTillCancelled, // GTC
  FillOrKill,        // FOK
  ImmediateOrCancel, // IOC
  GoodTillDate,      // GTD
  MarketOnOpen,      // OPG (TIF used for MOO/LOO orders)
  GoodTillExtended,  // GTX (requires outsideRth=true)
  AuctionMatch,      // AUC (Participate in auction match)
  GoodAfterTime,     // GAT (Good after time - often combined with DAY or GTC)
  Duration,          // DUR (Duration in seconds - for VWAP/TWAP algos)
  // Note: TWS API can be inconsistent; sometimes GAT is a TIF, sometimes an attribute.
}

impl fmt::Display for TimeInForce {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let s = match self {
      TimeInForce::Day => "DAY",
      TimeInForce::GoodTillCancelled => "GTC",
      TimeInForce::FillOrKill => "FOK",
      TimeInForce::ImmediateOrCancel => "IOC",
      TimeInForce::GoodTillDate => "GTD",
      TimeInForce::MarketOnOpen => "OPG",
      TimeInForce::GoodTillExtended => "GTX",
      TimeInForce::AuctionMatch => "AUC",
      TimeInForce::GoodAfterTime => "GAT",
      TimeInForce::Duration => "DUR",
    };
    write!(f, "{}", s)
  }
}

impl FromStr for TimeInForce {
  type Err = IBKRError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_uppercase().as_str() {
      "DAY" => Ok(TimeInForce::Day),
      "GTC" => Ok(TimeInForce::GoodTillCancelled),
      "FOK" => Ok(TimeInForce::FillOrKill),
      "IOC" => Ok(TimeInForce::ImmediateOrCancel),
      "GTD" => Ok(TimeInForce::GoodTillDate),
      "OPG" => Ok(TimeInForce::MarketOnOpen),
      "GTX" => Ok(TimeInForce::GoodTillExtended),
      "AUC" => Ok(TimeInForce::AuctionMatch),
      "GAT" => Ok(TimeInForce::GoodAfterTime),
      "DUR" => Ok(TimeInForce::Duration),
      _ => Err(IBKRError::ParseError(format!("Unknown TimeInForce string: {}", s))),
    }
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

impl OrderStatus {
  pub fn is_terminal(&self) -> bool {
    matches!(self, OrderStatus::Filled | OrderStatus::Cancelled | OrderStatus::Inactive | OrderStatus::ApiCancelled)
  }
}
