use crate::contract::Contract;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

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
