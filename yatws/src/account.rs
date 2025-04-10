// yatws/src/account.rs
// Account data structures for the IBKR API

use crate::contract::Contract;
use crate::order::OrderSide;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Account information
#[derive(Debug, Clone)]
pub struct AccountInfo {
  pub account_id: String,
  pub account_type: String,
  pub base_currency: String,
  pub equity: f64,
  pub buying_power: f64,
  pub cash_balance: f64,
  pub day_trades_remaining: i32,
  pub leverage: f64,
  pub maintenance_margin: f64,
  pub initial_margin: f64,
  pub excess_liquidity: f64,
  pub updated_at: DateTime<Utc>,
}

/// Account state for internal state tracking
#[derive(Debug, Clone, Default)]
pub struct AccountState {
  pub account_id: String,
  pub values: HashMap<String, AccountValue>,
  pub portfolio: HashMap<String, Position>,
  pub last_updated: Option<DateTime<Utc>>,
}

/// Account value for specific metrics
#[derive(Debug, Clone)]
pub struct AccountValue {
  pub key: String,
  pub value: String,
  pub currency: Option<String>,
  pub account_id: String,
}

/// Portfolio position
#[derive(Debug, Clone)]
pub struct Position {
  pub symbol: String,
  pub contract: Contract,
  pub quantity: f64,
  pub average_cost: f64,
  pub market_price: f64,
  pub market_value: f64,
  pub unrealized_pnl: f64,
  pub realized_pnl: f64,
  pub updated_at: DateTime<Utc>,
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

// Dummy struct for TickAttrib if not defined
#[derive(Debug, Clone, Default)]
pub struct TickAttrib {
    // Add fields as needed, e.g., can_auto_execute, past_limit, pre_open
}

// Dummy struct for CommissionReport if not defined
#[derive(Debug, Clone, Default)]
pub struct CommissionReport {
    pub exec_id: String,
    pub commission: f64,
    pub currency: String,
    pub realized_pnl: Option<f64>,
    pub yield_amount: Option<f64>,
    pub yield_redemption_date: Option<i32>, // Format?
}


/// Account observer trait for notifications
pub trait AccountObserver: Send + Sync {
  fn on_account_update(&self, account_info: &AccountInfo);
  fn on_position_update(&self, position: &Position);
  fn on_execution(&self, execution: &Execution);
}
