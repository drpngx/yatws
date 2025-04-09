// yatws/src/account.rs
// Account data structures for the IBKR API

use crate::contract::Contract;
use crate::order::Execution;
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

/// Account observer trait for notifications
pub trait AccountObserver: Send + Sync {
  fn on_account_update(&self, account_info: &AccountInfo);
  fn on_position_update(&self, position: &Position);
  fn on_execution(&self, execution: &Execution);
}

/// News article
#[derive(Debug, Clone)]
pub struct NewsArticle {
  pub id: String,
  pub time: DateTime<Utc>,
  pub provider_code: String,
  pub article_id: String,
  pub headline: String,
  pub summary: Option<String>,
  pub content: Option<String>,
}

/// News subscription
#[derive(Debug, Clone)]
pub struct NewsSubscription {
  pub id: String,
  pub sources: Vec<String>,
}

/// News observer trait
pub trait NewsObserver: Send + Sync {
  fn on_news_article(&self, article: &NewsArticle);
}

/// Market data subscription
#[derive(Debug, Clone)]
pub struct MarketDataSubscription {
  pub id: String,
  pub contract: Contract,
  pub data_types: Vec<MarketDataType>,
  pub last_price: Option<f64>,
  pub bid: Option<f64>,
  pub ask: Option<f64>,
  pub bid_size: Option<i32>,
  pub ask_size: Option<i32>,
  pub high: Option<f64>,
  pub low: Option<f64>,
  pub volume: Option<i64>,
  pub last_timestamp: Option<DateTime<Utc>>,
  pub halted: bool,
}

/// Market data type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketDataType {
  BidAsk,
  LastPrice,
  HighLow,
  Volume,
  HistoricalVolatility,
  ImpliedVolatility,
  OptionChain,
  PutCallRatio,
  DividendSchedule,
  Fundamentals,
  News,
}

/// Market data observer trait
pub trait MarketDataObserver: Send + Sync {
  fn on_price_update(&self, subscription: &MarketDataSubscription);
}

/// Request handle for async requests
pub struct RequestHandle<T> {
  receiver: std::sync::mpsc::Receiver<T>,
  error: Option<crate::base::IBKRError>,
}

impl<T> RequestHandle<T> {
  /// Create a new request handle
  pub fn new(receiver: std::sync::mpsc::Receiver<T>) -> Self {
    RequestHandle {
      receiver,
      error: None,
    }
  }

  /// Create a new request handle with an error
  pub fn new_error(error: crate::base::IBKRError) -> Self {
    RequestHandle {
      receiver: std::sync::mpsc::channel().1,
      error: Some(error),
    }
  }

  /// Wait for the response with a timeout
  pub fn wait_for_response(&self, timeout: std::time::Duration) -> Result<T, crate::base::IBKRError> {
    if let Some(ref error) = self.error {
      return Err(error.clone());
    }

    match self.receiver.recv_timeout(timeout) {
      Ok(response) => Ok(response),
      Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
        Err(crate::base::IBKRError::Timeout("Request timed out".to_string()))
      }
      Err(_) => Err(crate::base::IBKRError::InternalError(
        "Channel error".to_string(),
      )),
    }
  }

  /// Check if the response is ready
  pub fn is_ready(&self) -> bool {
    self.error.is_some() || self.receiver.try_recv().is_ok()
  }
}
