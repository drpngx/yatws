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

/// Filter criteria for requesting execution reports.
/// All fields are optional. If a field is `None`, the filter will not apply for that criterion.
#[derive(Debug, Clone, Default)]
pub struct ExecutionFilter {
  /// Filter by the API client which placed the order. 0 means all clients.
  pub client_id: Option<i32>,
  /// Filter by the account code. Empty string means all accounts accessible by this user.
  pub acct_code: Option<String>,
  /// Filter by time. Format: "YYYYMMDD HH:MM:SS". Note the single space.
  /// Only executions reported *after* this time will be returned.
  pub time: Option<String>,
  /// Filter by the instrument's symbol.
  pub symbol: Option<String>,
  /// Filter by the security type (e.g., "STK", "OPT", "FUT").
  pub sec_type: Option<String>,
  /// Filter by the exchange where the execution took place.
  pub exchange: Option<String>,
  /// Filter by the side of the execution ("BUY" or "SELL").
  pub side: Option<String>,
}

/// Trade execution information
#[derive(Debug, Clone)]
// Represents combined data primarily from execDetails and commissionReport messages
pub struct Execution {
  pub execution_id: String, // Unique identifier for the execution
  pub order_id: String,   // Corresponds to the API client's order ID
  pub perm_id: i32,       // Permanent TWS order ID
  pub client_id: i32,     // Client ID that placed the order
  pub symbol: String,     // Keep symbol for convenience alongside contract
  pub contract: Contract, // Include the full contract details
  pub side: OrderSide,
  pub quantity: f64,      // Also known as shares/size
  pub price: f64,         // Execution price (excluding commissions)
  pub time: DateTime<Utc>,// Parsed from execution time string
  pub avg_price: f64,     // Average price of the order containing this execution
  pub commission: Option<f64>, // Populated from CommissionReport
  pub commission_currency: Option<String>, // Populated from CommissionReport
  pub realized_pnl: Option<f64>, // Populated from CommissionReport
  pub exchange: String,    // Exchange where execution occurred
  pub account_id: String,  // Account ID associated with the execution
  /// Liquidation status: 0 = Not liquidation, 1 = Liquidation, 2 = Unknown. Stored as bool (true if 1 or 2).
  pub liquidation: bool,
  pub cum_qty: f64,        // Cumulative quantity filled for the order
  pub order_ref: Option<String>, // User-defined order reference
  /// Economic Value Rule name and optional argument (e.g., "aussieBond:YearsToExpiration=3"). Version >= 9
  pub ev_rule: Option<String>,
  /// Economic Value Rule multiplier. Version >= 9
  pub ev_multiplier: Option<f64>,
   /// Model code used for advisor models. Requires MIN_SERVER_VER_MODELS_SUPPORT.
  pub model_code: Option<String>,
  /// Liquidity type added indicator (1=Added, 2=Removed, 3=Rouded). Requires MIN_SERVER_VER_LAST_LIQUIDITY.
  pub last_liquidity: Option<i32>,
  /// Indicates if the price was revised before execution. Requires MIN_SERVER_VER_PENDING_PRICE_REVISION.
  pub pending_price_revision: Option<bool>,
  // Add other fields like evRule, evMultiplier, modelCode, lastLiquidity if needed
}

// Dummy struct for TickAttrib if not defined
#[derive(Debug, Clone, Default)]
pub struct TickAttrib {
  // Add fields as needed, e.g., can_auto_execute, past_limit, pre_open
}


/// Account observer trait for notifications
pub trait AccountObserver: Send + Sync {
  /// Called when core account values (like equity, cash, margin) are updated.
  fn on_account_update(&self, account_info: &AccountInfo);
  /// Called when a portfolio position is updated (quantity, market value, etc.).
  fn on_position_update(&self, position: &Position);
  /// Called when execution details are received or updated (e.g., with commission).
  /// The `Execution` object may be emitted initially without commission,
  /// and then emitted *again* later once commission details are merged.
  fn on_execution(&self, execution: &Execution);
  // Removed on_commission_report, as data is merged into on_execution
}
