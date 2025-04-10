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
// yatws/src/order.rs (Updated OrderRequest struct)

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

/// Observer trait for order notifications
pub trait OrderObserver: Send + Sync {
  fn on_order_update(&self, order: &Order);
  fn on_order_error(&self, order_id: &str, error: &crate::base::IBKRError);
}
