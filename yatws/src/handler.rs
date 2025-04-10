// yatws/src/handler.rs
// Handlers for events parsed from the server.
use crate::contract::Contract;
use chrono::{DateTime, Utc};
use std::collections::HashMap;


/// Meta messages such as errors and time.
pub trait ClientHandler: Send + Sync {
  // fn error(&mut self, id: i32, error_code: i32, error_msg: &str);
  //  fn connection_closed(&mut self);
  // ... other methods ...
}

/// Order processing.
pub trait OrderHandler: Send + Sync {
    // fn next_valid_id(&mut self, order_id: i32);
    // // ... Add Send + Sync if needed ...
    // fn open_order(&mut self, order_id: i32, contract: &Contract, order: &Order, order_state: &OrderState);
    // fn open_order_end(&mut self);
    // fn order_status(&mut self, order_id: i32, status: &str, filled: f64, remaining: f64, avg_fill_price: f64, perm_id: i32, parent_id: i32, last_fill_price: f64, client_id: i32, why_held: &str, mkt_cap_price: f64);
    // fn execution_details(&mut self, req_id: i32, contract: &Contract, execution: &Execution);
    // fn execution_details_end(&mut self, req_id: i32);
    // fn commission_report(&mut self, commission_report: &CommissionReport);
    // fn completed_order(&mut self, contract: &Contract, order: &Order, order_state: &OrderState);
    // fn completed_orders_end(&mut self);
}

/// Information about the account such as open positions and cash balance.
pub trait AccountHandler: Send + Sync {
  /// Update account value for a specific key.
  fn account_value(&self, key: &str, value: &str, currency: Option<&str>, account_name: &str);

  /// Update portfolio position details.
  fn portfolio_value(&self, contract: &Contract, position: f64, market_price: f64, market_value: f64, average_cost: f64, unrealized_pnl: f64, realized_pnl: f64, account_name: &str);

  /// Received timestamp of the last account update.
  fn account_update_time(&self, time_stamp: &str);

  /// Indicates the end of the account download stream.
  fn account_download_end(&self, account: &str);

  /// Provides the list of managed accounts. (Less critical for state)
  fn managed_accounts(&self, accounts_list: &str);

  /// Provides position details (alternative to portfolio_value, often used with reqPositions).
  fn position(&self, account: &str, contract: &Contract, position: f64, avg_cost: f64);

  /// Indicates the end of the position stream.
  fn position_end(&self);

  /// Provides a specific account summary value requested via reqAccountSummary.
  fn account_summary(&self, req_id: i32, account: &str, tag: &str, value: &str, currency: &str);

  /// Indicates the end of an account summary request.
  fn account_summary_end(&self, req_id: i32);

  /// Update PnL for the entire account.
  fn pnl(&self, req_id: i32, daily_pnl: f64, unrealized_pnl: Option<f64>, realized_pnl: Option<f64>);

  /// Update PnL for a single position.
  fn pnl_single(&self, req_id: i32, pos: i32, daily_pnl: f64, unrealized_pnl: Option<f64>, realized_pnl: Option<f64>, value: f64);

  // --- Methods for multi-account updates (optional to implement fully initially) ---
  // fn position_multi(&self, req_id: i32, account: &str, model_code: &str, contract: &Contract, pos: f64, avg_cost: f64);
  // fn position_multi_end(&self, req_id: i32);
  // fn account_update_multi(&self, req_id: i32, account: &str, model_code: &str, key: &str, value: &str, currency: &str);
  // fn account_update_multi_end(&self, req_id: i32);
}

/// Contract types etc.
pub trait ReferenceDataHandler: Send + Sync {
    // fn contract_details(&mut self, req_id: i32, contract_details: &ContractDetails);
    // fn contract_details_end(&mut self, req_id: i32);
    // fn bond_contract_details(&mut self, req_id: i32, contract_details: &ContractDetails);
    // // ... other methods ...
}

/// Micro-structure: quotes etc.
pub trait MarketDataHandler: Send + Sync {
}

/// Fundamentals.
pub trait FinancialDataHandler: Send + Sync {
}

pub trait NewsDataHandler: Send + Sync {
}

pub trait FinancialAdvisorHandler: Send + Sync {
}

pub struct MessageHandler {
  pub client: Box<dyn ClientHandler>,
  pub order: Box<dyn OrderHandler>,
  pub account: Box<dyn AccountHandler>,
  pub fin_adv: Box<dyn FinancialAdvisorHandler>,
  pub data_ref: Box<dyn ReferenceDataHandler>,
  pub data_market: Box<dyn MarketDataHandler>,
  pub data_news: Box<dyn NewsDataHandler>,
  pub data_fin: Box<dyn FinancialDataHandler>,
}

impl MessageHandler {
  pub fn connection_closed(&mut self) {
  }
}
