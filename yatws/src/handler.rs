// yatws/src/handler.rs
// Handlers for events parsed from the server.
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use crate::order::{OrderRequest, OrderStatus, OrderState};
use crate::account::Execution;
use crate::contract::{
  Bar, Contract, ContractDetails, SoftDollarTier, FamilyCode, ContractDescription,
  DepthMktDataDescription, SmartComponent, MarketRule, HistoricalSession, PriceIncrement
};
use crate::data::{
  TickAttrib, TickAttribLast, TickAttribBidAsk, TickOptionComputationData,
  MarketDataTypeEnum, TickNewsData, MarketDepthRow
};


/// Meta messages such as errors and time.
pub trait ClientHandler: Send + Sync {
  fn error(&self, id: i32, error_code: i32, error_msg: &str);
  fn connection_closed(&self);
  fn current_time(&self, time_unix: i64);
  fn verify_message_api(&self, _api_data: &str);
  fn verify_completed(&self, is_successful: bool, error_text: &str);
  fn verify_and_auth_message_api(&self, _api_data: &str, _xyz_challenge: &str);
  fn verify_and_auth_completed(&self, is_successful: bool, error_text: &str);
  fn display_group_list(&self, req_id: i32, groups: &str);
  fn display_group_updated(&self, req_id: i32, contract_info: &str);
  fn head_timestamp(&self, req_id: i32, timestamp_str: &str);
  fn user_info(&self, _req_id: i32, white_branding_id: &str);
}

/// Order processing.
pub trait OrderHandler: Send + Sync {
  /// Provides the next available order ID. Sent automatically on connection.
  fn next_valid_id(&self, order_id: i32);

  /// Provides the status of an order.
  /// Called automatically by TWS after placing/modifying/canceling orders.
  /// Note: The status string needs to be parsed into OrderStatus by the implementation.
  /// The mkt_cap_price field is relatively new.
  fn order_status(
    &self,
    order_id: i32,
    order_status: OrderStatus,
    filled: f64,
    remaining: f64,
    avg_fill_price: f64,
    perm_id: i32, // Permanent ID assigned by TWS, 0 if not set
    parent_id: i32, // Parent order ID, 0 if not part of a complex order
    last_fill_price: f64, // Price of the last partial fill
    client_id: i32, // ID of the client that placed the order
    why_held: &str, // Reason order is held (e.g., locate pending)
    mkt_cap_price: Option<f64>, // Market cap price for orders benefiting from price improvement
  );

  /// Provides order details when requesting open orders or receiving updates.
  /// This combines Contract, OrderRequest details, and OrderState.
  fn open_order(
    &self,
    order_id: i32,
    contract: &Contract,
    order_request: &OrderRequest, // The details of the order as submitted/known by TWS
    order_state: &OrderState,   // The current live state of the order
  );

  /// Indicates the end of the initial burst of open orders after connection
  /// or the end of a reqOpenOrders/reqAllOpenOrders response.
  fn open_order_end(&self);

  /// Callback for errors specific to an order ID (e.g., placing invalid order).
  /// Should be called from ClientHandler::error when id > 0.
  fn order_error(&self, order_id: i32, error_code: i32, error_msg: &str);

  /// Message sent when TWS binds an API order ID to a TWS order ID.
  /// Primarily useful for tracking orders submitted through other means or sessions.
  fn order_bound(&self, order_id: i64, api_client_id: i32, api_order_id: i32);

  /// Provides details of a completed order (filled or cancelled).
  fn completed_order(&self, contract: &Contract, order_request: &OrderRequest, order_state: &OrderState);

  /// Indicates the end of the completed orders transmission.
  fn completed_orders_end(&self);
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

  fn execution_details(&self, req_id: i32, _contract: &Contract, execution: &Execution);
  fn execution_details_end(&self, req_id: i32);
  fn commission_report(&self, exec_id: &str, commission: f64, currency: &str, yld: Option<f64>, yield_redemption: Option<f64>);

  // --- Methods for multi-account updates (optional to implement fully initially) ---
  // fn position_multi(&self, req_id: i32, account: &str, model_code: &str, contract: &Contract, pos: f64, avg_cost: f64);
  // fn position_multi_end(&self, req_id: i32);
  // fn account_update_multi(&self, req_id: i32, account: &str, model_code: &str, key: &str, value: &str, currency: &str);
  // fn account_update_multi_end(&self, req_id: i32);
}

pub trait ReferenceDataHandler: Send + Sync {
  /// Provides details for a contract requested via `reqContractDetails`.
  fn contract_details(&self, req_id: i32, contract_details: &ContractDetails);

  /// Provides details for a bond contract requested via `reqContractDetails`.
  fn bond_contract_details(&self, req_id: i32, contract_details: &ContractDetails);

  /// Indicates the end of a contract details request.
  fn contract_details_end(&self, req_id: i32);

  /// Provides option parameters (expirations and strikes) for a requested underlying.
  fn security_definition_option_parameter(
    &self,
    req_id: i32,
    exchange: &str,
    underlying_con_id: i32,
    trading_class: &str,
    multiplier: &str,
    expirations: &[String], // Using slice for borrowing
    strikes: &[f64],         // Using slice for borrowing
  );

  /// Indicates the end of a security definition option parameter request.
  fn security_definition_option_parameter_end(&self, req_id: i32);

  /// Provides the tiers for soft dollar commissions.
  fn soft_dollar_tiers(&self, req_id: i32, tiers: &[SoftDollarTier]);

  /// Provides the family codes used in linked accounts.
  fn family_codes(&self, family_codes: &[FamilyCode]);

  /// Provides contracts matching a requested symbol pattern.
  fn symbol_samples(&self, req_id: i32, contract_descriptions: &[ContractDescription]);

  /// Provides exchanges offering market depth for a requested contract.
  fn mkt_depth_exchanges(&self, descriptions: &[DepthMktDataDescription]);

  /// Provides the components of a SMART routing destination.
  fn smart_components(&self, req_id: i32, components: &HashMap<i32, (String, char)>);

  /// Provides details about a specific market rule.
  fn market_rule(&self, market_rule_id: i32, price_increments: &[PriceIncrement]);

  /// Provides the trading schedule for a contract over a requested time period.
  fn historical_schedule(
    &self,
    req_id: i32,
    start_date_time: &str,
    end_date_time: &str,
    time_zone: &str,
    sessions: &[HistoricalSession],
  );

  // Add other reference data methods as needed (e.g., scanner parameters)
}

/// Micro-structure: quotes etc.
pub trait MarketDataHandler: Send + Sync {
  // --- Tick Data ---
  fn tick_price(&self, req_id: i32, tick_type: i32, price: f64, attrib: TickAttrib);
  fn tick_size(&self, req_id: i32, tick_type: i32, size: f64); // Use f64 for Decimal size
  fn tick_string(&self, req_id: i32, tick_type: i32, value: &str);
  fn tick_generic(&self, req_id: i32, tick_type: i32, value: f64);
  fn tick_efp(&self, req_id: i32, tick_type: i32, basis_points: f64, formatted_basis_points: &str,
              implied_futures_price: f64, hold_days: i32, future_last_trade_date: &str,
              dividend_impact: f64, dividends_to_last_trade_date: f64);
  fn tick_option_computation(&self, req_id: i32, data: TickOptionComputationData);
  fn tick_snapshot_end(&self, req_id: i32);
  fn market_data_type(&self, req_id: i32, market_data_type: MarketDataTypeEnum);
  fn tick_req_params(&self, req_id: i32, min_tick: f64, bbo_exchange: &str, snapshot_permissions: i32);

  // --- Real Time Bars ---
  fn real_time_bar(&self, req_id: i32, time: i64, open: f64, high: f64, low: f64, close: f64,
                   volume: f64, wap: f64, count: i32); // Use f64 for Decimal volume/wap

  // --- Historical Data ---
  fn historical_data(&self, req_id: i32, bar: &Bar);
  fn historical_data_update(&self, req_id: i32, bar: &Bar);
  fn historical_data_end(&self, req_id: i32, start_date: &str, end_date: &str);
  fn historical_ticks(&self, req_id: i32, ticks: &[(i64, f64, f64)], done: bool); // time, price, size
  fn historical_ticks_bid_ask(&self, req_id: i32, ticks: &[(i64, TickAttribBidAsk, f64, f64, f64, f64)], done: bool); // time, attrib, priceBid, priceAsk, sizeBid, sizeAsk
  fn historical_ticks_last(&self, req_id: i32, ticks: &[(i64, TickAttribLast, f64, f64, String, String)], done: bool); // time, attrib, price, size, exchange, specialConditions

  // --- Tick By Tick ---
  fn tick_by_tick_all_last(&self, req_id: i32, tick_type: i32, time: i64, price: f64, size: f64,
                           tick_attrib_last: TickAttribLast, exchange: &str, special_conditions: &str);
  fn tick_by_tick_bid_ask(&self, req_id: i32, time: i64, bid_price: f64, ask_price: f64, bid_size: f64,
                          ask_size: f64, tick_attrib_bid_ask: TickAttribBidAsk);
  fn tick_by_tick_mid_point(&self, req_id: i32, time: i64, mid_point: f64);

  // --- Market Depth ---
  fn update_mkt_depth(&self, req_id: i32, position: i32, operation: i32, side: i32, price: f64, size: f64);
  fn update_mkt_depth_l2(&self, req_id: i32, position: i32, market_maker: &str, operation: i32,
                         side: i32, price: f64, size: f64, is_smart_depth: bool);

  // --- Other Market Data ---
  fn delta_neutral_validation(&self, req_id: i32, /* con_id: i32, delta: f64, price: f64 */); // Add DeltaNeutralContract if needed
  fn histogram_data(&self, req_id: i32, /* items: &[(f64, f64)] */); // Add HistogramEntry if needed
  fn scanner_parameters(&self, xml: &str);
  fn scanner_data(&self, req_id: i32, rank: i32, contract_details: &ContractDetails, distance: &str,
                  benchmark: &str, projection: &str, legs_str: Option<&str>);
  fn scanner_data_end(&self, req_id: i32);

  // --- Errors ---
  fn market_data_error(&self, req_id: i32, error_code: i32, error_msg: &str); // Specific error callback

  // --- Rerouting ---
  fn reroute_mkt_data_req(&self, req_id: i32, con_id: i32, exchange: &str);
  fn reroute_mkt_depth_req(&self, req_id: i32, con_id: i32, exchange: &str);
}

/// Fundamentals.
pub trait FinancialDataHandler: Send + Sync {
}

pub trait NewsDataHandler: Send + Sync {
  // fn tick_news(&self, req_id: i32, time_stamp: i64, provider_code: &str, article_id: &str,
  //              headline: &str, extra_data: &str);
}

pub trait FinancialAdvisorHandler: Send + Sync {
}

struct Dummy {}
impl FinancialAdvisorHandler for Dummy {}
impl NewsDataHandler for Dummy {}
impl FinancialDataHandler for Dummy {}

pub struct MessageHandler {
  server_version: i32,
  pub client: Arc<dyn ClientHandler>,
  pub order: Arc<dyn OrderHandler>,
  pub account: Arc<dyn AccountHandler>,
  pub fin_adv: Arc<dyn FinancialAdvisorHandler>,
  pub data_ref: Arc<dyn ReferenceDataHandler>,
  pub data_market: Arc<dyn MarketDataHandler>,
  pub data_news: Arc<dyn NewsDataHandler>,
  pub data_fin: Arc<dyn FinancialDataHandler>,
}

impl MessageHandler {
  pub fn new(server_version: i32,
             client: Arc<dyn ClientHandler>,
             account: Arc<dyn AccountHandler>,
             order: Arc<dyn OrderHandler>,
             data_ref: Arc<dyn ReferenceDataHandler>,
             data_market: Arc<dyn MarketDataHandler>) -> Self {
    let dummy = Arc::new(Dummy {});
    MessageHandler {
      server_version,
      client,
      account,
      order,
      data_ref,
      data_market,
      fin_adv: dummy.clone(),
      data_news: dummy.clone(),
      data_fin: dummy.clone(),
    }
  }
  // TODO: move this to ClientManager.
  pub fn connection_closed(&mut self) {
  }

  pub fn get_server_version(&self) -> i32 { self.server_version }
}
