// yatws/src/account.rs
// Account data structures for the IBKR API

use crate::contract::Contract;
use crate::order::OrderSide;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Account information - A consolidated view of key account metrics.
/// Populated primarily from `accountSummary` messages.
#[derive(Debug, Clone)]
pub struct AccountInfo {
  // --- Core Identifiers ---
  pub account_id: String,
  pub account_type: String,
  pub base_currency: String,

  // --- Key Balances & Equity ---
  /// Net Liquidation Value (often referred to as Equity)
  pub net_liquidation: f64,
  /// Total Cash Value
  pub total_cash_value: f64,
  /// Settled Cash
  pub settled_cash: f64,
  /// Accrued Cash (interest, dividends)
  pub accrued_cash: f64,
  /// Buying Power
  pub buying_power: f64,
  /// Equity with Loan Value
  pub equity_with_loan_value: f64,
  /// Previous Day's Equity with Loan Value
  pub previous_equity_with_loan_value: f64,
  /// Gross Position Value (sum of absolute values of positions)
  pub gross_position_value: f64,

  // --- Margin Requirements ---
  /// Reg T Equity
  pub reg_t_equity: f64,
  /// Reg T Margin
  pub reg_t_margin: f64,
  /// Special Memorandum Account (SMA)
  pub sma: f64,
  /// Initial Margin Requirement
  pub init_margin_req: f64,
  /// Maintenance Margin Requirement
  pub maint_margin_req: f64,
  /// Full Initial Margin Requirement
  pub full_init_margin_req: f64,
  /// Full Maintenance Margin Requirement
  pub full_maint_margin_req: f64,

  // --- Available Funds & Liquidity ---
  /// Available Funds
  pub available_funds: f64,
  /// Excess Liquidity
  pub excess_liquidity: f64,
  /// Cushion (Excess Liquidity as a percentage of Net Liquidation)
  pub cushion: f64,
  /// Full Available Funds
  pub full_available_funds: f64,
  /// Full Excess Liquidity
  pub full_excess_liquidity: f64,

  // --- Look Ahead Margin ---
  /// Time of next margin change (HHMMSS format)
  pub look_ahead_next_change: String, // Keep as string for HHMMSS
  /// Look Ahead Initial Margin Requirement
  pub look_ahead_init_margin_req: f64,
  /// Look Ahead Maintenance Margin Requirement
  pub look_ahead_maint_margin_req: f64,
  /// Look Ahead Available Funds
  pub look_ahead_available_funds: f64,
  /// Look Ahead Excess Liquidity
  pub look_ahead_excess_liquidity: f64,

  // --- Other ---
  /// Highest margin requirement severity level
  pub highest_severity: i32, // Or String? TWS sends integer
  /// Day Trades Remaining (Pattern Day Trader)
  pub day_trades_remaining: i32,
  /// Leverage (for specific products, often -S suffix)
  pub leverage_s: f64,
  /// Daily Profit and Loss
  pub daily_pnl: f64,
  /// Unrealized Profit and Loss
  pub unrealized_pnl: f64,
  /// Realized Profit and Loss
  pub realized_pnl: f64,

  // --- Timestamp ---
  /// Time of the last update received for any value in this summary
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

/// Enum representing the keys for account values requested from TWS.
/// Use `AccountValueKey::to_string()` to get the TWS tag name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AccountValueKey {
  AccountType,
  NetLiquidation,
  TotalCashValue,
  SettledCash,
  AccruedCash,
  BuyingPower,
  EquityWithLoanValue,
  PreviousEquityWithLoanValue,
  GrossPositionValue,
  ReqTEquity,
  ReqTMargin,
  SMA,
  InitMarginReq,
  MaintMarginReq,
  AvailableFunds,
  ExcessLiquidity,
  Cushion,
  FullInitMarginReq,
  FullMaintMarginReq,
  FullAvailableFunds,
  FullExcessLiquidity,
  LookAheadNextChange,
  LookAheadInitMarginReq,
  LookAheadMaintMarginReq,
  LookAheadAvailableFunds,
  LookAheadExcessLiquidity,
  HighestSeverity,
  DayTradesRemaining,
  LeverageS, // Note: TWS uses "Leverage-S"
  Currency,
  DailyPnL,
  UnrealizedPnL,
  RealizedPnL,
  // Add other keys as needed
  Other(String), // For keys not explicitly listed
}

impl std::fmt::Display for AccountValueKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccountValueKey::AccountType => write!(f, "AccountType"),
            AccountValueKey::NetLiquidation => write!(f, "NetLiquidation"),
            AccountValueKey::TotalCashValue => write!(f, "TotalCashValue"),
            AccountValueKey::SettledCash => write!(f, "SettledCash"),
            AccountValueKey::AccruedCash => write!(f, "AccruedCash"),
            AccountValueKey::BuyingPower => write!(f, "BuyingPower"),
            AccountValueKey::EquityWithLoanValue => write!(f, "EquityWithLoanValue"),
            AccountValueKey::PreviousEquityWithLoanValue => write!(f, "PreviousEquityWithLoanValue"),
            AccountValueKey::GrossPositionValue => write!(f, "GrossPositionValue"),
            AccountValueKey::ReqTEquity => write!(f, "ReqTEquity"),
            AccountValueKey::ReqTMargin => write!(f, "ReqTMargin"),
            AccountValueKey::SMA => write!(f, "SMA"),
            AccountValueKey::InitMarginReq => write!(f, "InitMarginReq"),
            AccountValueKey::MaintMarginReq => write!(f, "MaintMarginReq"),
            AccountValueKey::AvailableFunds => write!(f, "AvailableFunds"),
            AccountValueKey::ExcessLiquidity => write!(f, "ExcessLiquidity"),
            AccountValueKey::Cushion => write!(f, "Cushion"),
            AccountValueKey::FullInitMarginReq => write!(f, "FullInitMarginReq"),
            AccountValueKey::FullMaintMarginReq => write!(f, "FullMaintMarginReq"),
            AccountValueKey::FullAvailableFunds => write!(f, "FullAvailableFunds"),
            AccountValueKey::FullExcessLiquidity => write!(f, "FullExcessLiquidity"),
            AccountValueKey::LookAheadNextChange => write!(f, "LookAheadNextChange"),
            AccountValueKey::LookAheadInitMarginReq => write!(f, "LookAheadInitMarginReq"),
            AccountValueKey::LookAheadMaintMarginReq => write!(f, "LookAheadMaintMarginReq"),
            AccountValueKey::LookAheadAvailableFunds => write!(f, "LookAheadAvailableFunds"),
            AccountValueKey::LookAheadExcessLiquidity => write!(f, "LookAheadExcessLiquidity"),
            AccountValueKey::HighestSeverity => write!(f, "HighestSeverity"),
            AccountValueKey::DayTradesRemaining => write!(f, "DayTradesRemaining"),
            AccountValueKey::LeverageS => write!(f, "Leverage-S"), // Special case for hyphen
            AccountValueKey::Currency => write!(f, "Currency"),
            AccountValueKey::DailyPnL => write!(f, "DailyPnL"),
            AccountValueKey::UnrealizedPnL => write!(f, "UnrealizedPnL"),
            AccountValueKey::RealizedPnL => write!(f, "RealizedPnL"),
            AccountValueKey::Other(s) => write!(f, "{}", s),
        }
    }
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
  pub position_daily_pnl: Option<f64>, // P&L for this specific position from pnl_single
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
