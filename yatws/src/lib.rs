// yatws/src/lib.rs
// Main entry point for the IBKR API library

//! # YATWS (Yet Another TWS API) Module Documentation
//!
//! ## Overview
//!
//! YATWS provides a comprehensive, thread-safe, type-safe, and ergonomic Rust interface to the Interactive Brokers TWS API. Its manager-based architecture and support for both synchronous and asynchronous patterns make it suitable for various trading application needs, from simple data retrieval to complex automated trading systems.
//!
//! This is a very early stage of the API and it may change at any time.
//!
//! This library was born out of my need to place orders in rapid succession in response to market events. That is not easily done with existing rust crates. It takes about 3ms to send an order with this library. This was the primary goal and other interfaces (market data, etc) have been implemented for the sake of completeness. This library is in production and has traded millions of dollar volume.
//!
//! ## Features
//!
//! - **Comprehensive API Coverage**: Access to orders, accounts, market data, fundamentals, news, and reference data
//! - **Book-keeping**: Keeps the portfolio with PNL and order book up-to-date
//! - **Multiple Programming Patterns**:
//!   - Synchronous blocking calls with timeouts
//!   - Asynchronous observer pattern
//!   - Subscription model
//! - **Options Strategy Builder**: Simplified creation of common options strategies
//! - **Strong Type Safety**: Leverages Rust's type system for safer API interactions
//! - **Domain-Specific Managers**: Organized access to different API functionalities
//! - **Rate limits**: pace the requests in accordance with IBKR rules
//! - **Session Recording/Replay**: Record TWS interactions for testing and debugging
//!
//! ## Comparison with Alternative Libraries
//!
//! ### YATWS vs. `rust-ibapi`
//! For many people, the `ibapi` crate is a fine choice. The subscription model works fine for fetching market data.
//! The yatws crate allows different programming models if you want the flexibility.
//!
//! **Pros of YATWS:**
//! - Structured manager-based architecture for logical organization
//! - Ergonomic synchronous functions with built-in timeouts
//! - Observer pattern support for asynchronous event handling
//! - Built-in support for common options strategies via the `OptionsStrategyBuilder`
//! - Support for session recording and replay for testing/debugging
//! - More comprehensive use of Rust enums and types instead of strings
//!
//! **Cons of YATWS:**
//! - Newer library with potentially fewer community examples
//! - Partial `Subscription` implementation
//! - Partial testing and API interface still fluid
//! - Order placement is my primary use case and the only one that is battle-tested
//!
//! ### YATWS vs. ib_insync (Python)
//!
//! **Pros of YATWS:**
//! - Native Rust performance advantages
//! - Thread safety through Rust's ownership model
//!
//! **Cons of YATWS:**
//! - Less mature than ib_insync
//! - Rust's learning curve vs. Python's accessibility
//! - Smaller ecosystem compared to Python's data science tools
//! - Lacks ib_insync's integration with asyncio and notebooks
//!
//! ## Architecture and Interaction Model
//!
//! ### Connection Model
//!
//! YATWS follows a client-manager architecture:
//!
//! 1. **Core Client**: `IBKRClient` is the central connection point to TWS/Gateway
//! 2. **Specialized Managers**: Domain-specific functionality is organized into managers
//!
//! ```rust
//! // Establishing a connection to TWS/Gateway
//! let client = IBKRClient::new("127.0.0.1", 7497, 0, None)?;
//!
//! // Alternative: Replay a recorded session for testing
//! let replay_client = IBKRClient::from_db("sessions.db", "backtest_session")?;
//! ```
//!
//! ### Manager Access
//!
//! Each functional area is accessible through specialized manager objects:
//!
//! ```rust
//! // Access managers through the client
//! let order_manager = client.orders();           // Order operations
//! let account_manager = client.account();        // Account data
//! let market_data = client.data_market();        // Market data
//! let reference_data = client.data_ref();        // Contract details, etc.
//! let news_manager = client.data_news();         // News feeds
//! let fundamentals = client.data_financials();   // Financial data
//! let fin_adv = client.financial_advisor();      // Financial advisor
//! ```
//!
//! ## Programming Patterns
//!
//! YATWS supports multiple interaction patterns to suit different use cases:
//!
//! ### 1. Synchronous (Blocking) Operations
//!
//! Most operations offer a synchronous version that blocks until completion or timeout:
//!
//! ```rust
//! // Blocking call with timeout
//! let quote = client.data_market().get_quote(
//!     &contract,
//!     None,  // Default market data type
//!     Duration::from_secs(5)
//! )?;
//!
//! // Blocking call to fetch positions
//! let positions = client.account().list_open_positions()?;
//! ```
//!
//! ### 2. Asynchronous Request/Cancel Pattern
//!
//! For operations needing continuous data flow, YATWS uses a request/cancel pattern:
//!
//! ```rust
//! // Start a streaming request
//! let req_id = client.data_market().request_market_data(
//!     &contract,
//!     &[],  // Tick types
//!     false,         // Not a snapshot
//!     false,         // Not regulatory
//!     &[],           // No options
//!     None           // Default market data type
//! )?;
//!
//! // Do other operations...
//!
//! // Cancel the streaming request when done
//! client.data_market().cancel_market_data(req_id)?;
//! ```
//!
//! ### 3. Generic Observer Pattern
//!
//! The observer pattern is similar to the IBKR TWS API programming model. You launch a
//! request and the API will notify of results with callbacks. This is best for reacting
//! to events. For instance, one may have a dashboard showing pending orders and current
//! pnl that automatically updates.
//!
//! For handling asynchronous events like order status updates, position changes, etc.:
//!
//! ```rust
//! // Implement the observer trait
//! struct MyOrderObserver;
//!
//! impl OrderObserver for MyOrderObserver {
//!     fn on_order_update(&self, order: &Order) {
//!         println!("Order update: {:?}", order);
//!     }
//!
//!     fn on_order_error(&self, order_id: &str, error: &IBKRError) {
//!         println!("Order error: {} - {:?}", order_id, error);
//!     }
//! }
//!
//! // Register the observer
//! let observer = Box::new(MyOrderObserver);
//! client.orders().add_observer(observer);
//! ```
//!
//! ### 3. Request-specific Observer Pattern
//!
//! The API will route events to observers if needed. This is useful for cases when multiple
//! requests of the same type are in flight, for instance. For instance, you may request real-time
//! ticks for multiple stocks. Each stock has its own observer. This is accomplished with the
//! `request_observe` pattern.
//!
//! ```rust
//! struct TestMarketObserver {
//!   name: String,
//! }
//! impl MarketDataObserver for TestMarketObserver {
//!   fn on_tick_price(&self, req_id: i32, tick_type: TickType, price: f64, attrib: yatws::data::TickAttrib) {
//!     info!("[{}] TickPrice: ReqID={}, Type={:?}, Price={}, Attrib={:?}", self.name, req_id, tick_type, price, attrib);
//!   }
//!   ...
//! }
//! let observer = TestMarketObserver { name: "MSFT-Observer".to_string(), error_occurred: Default::default() };
//! let generic_tick_list: &[GenericTickType] = &[];
//! let snapshot = false; // Streaming
//! let mkt_data_options = &[];
//!  info!("Requesting observed market data for {} (Generic Ticks: '{}')...",
//!       contract.symbol,
//!       generic_tick_list.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(","));
//!  let (req_id, observer_id) = data_mgr.request_observe_market_data(
//!   &contract,
//!   generic_tick_list,
//!   snapshot,
//!   false, // regulatory_snapshot
//!   mkt_data_options,
//!   Some(MarketDataType::Delayed),
//!   observer,
//! ).context("Failed to request observed market data")?;
//!
//! ```
//!
//! ### 4. Subscription Pattern
//!
//! Used in the `ibapi` crate, this pattern uses a pull model to receive notifications
//! in the current control flow. This is most useful for cases when you want to have
//! streaming results and follow up in the current process. For example, you may be trading
//! some technical pattern for a particular stock. You would collect the real-time bars
//! and place orders if some technical indicator triggers.
//!
//! ```rust
//!    let subscription = data_mgr.subscribe_market_data(&contract)
//!      .with_snapshot(false) // Streaming
//!      .with_market_data_type(MarketDataType::RealTime)
//!      .submit()
//!      .context("Failed to submit TickDataSubscription")?;
//!
//!    let iteration_timeout = Duration::from_secs(2);
//!    let total_wait_duration = Duration::from_secs(600);
//!    let start_time = std::time::Instant::now();
//!    let mut iter = subscription.events(); // This is the stream of events
//!
//!    while start_time.elapsed() < total_wait_duration {
//!      match iter.try_next(iteration_timeout) {
//!        Some(event) => {
//!          let should_buy = compute_technical_signal(event);
//!          if should_buy {
//!            ... buy ...
//!            break;
//!          }
//!        }
//!        None => { // Timeout
//!        }
//!      }
//!    }
//! ```
//!
//! ### News Subscription Example
//!
//! ```rust
//! use yatws::{IBKRClient, IBKRError, news_subscription::NewsEvent};
//! use std::time::Duration;
//! use std::thread;
//!
//! # fn main() -> Result<(), IBKRError> {
//! # let client = IBKRClient::new("127.0.0.1", 7497, 103, None)?; // Use a unique client_id
//! let news_manager = client.data_news();
//!
//! // Subscribe to new news bulletins only
//! let mut news_sub = news_manager.subscribe_news_bulletins_stream(false).submit()?;
//! println!("Subscribed to news bulletins (ReqID: {}).", news_sub.request_id());
//!
//! let event_receiver = news_sub.events();
//! let news_thread = thread::spawn(move || {
//!     for event in event_receiver { // Iterates until channel is disconnected
//!         match event {
//!             NewsEvent::Bulletin { article, .. } => {
//!                 println!("[Thread] News Bulletin: Provider={}, ID={}, Headline='{}'",
//!                          article.provider_code, article.article_id, article.headline);
//!             }
//!             NewsEvent::Error(e) => {
//!                 eprintln!("[Thread] NewsSubscription Error: {:?}", e);
//!             }
//!             NewsEvent::Closed { .. } => {
//!                 println!("[Thread] NewsSubscription closed.");
//!                 break; // Exit loop
//!             }
//!         }
//!     }
//! });
//!
//! // Let the subscription run for a bit
//! thread::sleep(Duration::from_secs(30));
//!
//! // Explicitly close the subscription
//! println!("Main thread: Closing NewsSubscription...");
//! news_sub.cancel()?; // This will send NewsEvent::Closed and disconnect the channel
//!
//! // Wait for the event thread to finish
//! news_thread.join().expect("News thread panicked");
//! println!("Main thread: Done with news.");
//! # Ok(())
//! # }
//! ```
//!
//! ## Key Functional Areas
//!
//! ### 1. Account Information
//!
//! ```rust
//! // Subscribe to account updates
//! client.account().subscribe_account_updates()?;
//!
//! // Get account summary
//! let equity = client.account().get_net_liquidation()?;
//! let buying_power = client.account().get_buying_power()?;
//! let positions = client.account().list_open_positions()?;
//!
//! // Get executions for current day
//! let executions = client.account().get_day_executions()?;
//! ```
//!
//! ### 2. Order Management
//!
//! ```rust
//! // Create and place an order using OrderBuilder
//! let (contract, order_request) = OrderBuilder::new(OrderSide::Buy, 100.0)
//!     .for_stock("AAPL")
//!     .with_exchange("SMART")
//!     .with_currency("USD")
//!     .limit(150.0)
//!     .with_tif(TimeInForce::Day)
//!     .build()?;
//!
//! let order_id = client.orders().place_order(contract, order_request)?;
//!
//! // Wait for order to be filled (with timeout)
//! let status = client.orders().try_wait_order_executed(
//!     &order_id,
//!     Duration::from_secs(30)
//! )?;
//! ```
//!
//! ### Order Subscription Pattern
//!
//! For more interactive order management, use the subscription pattern to track order lifecycle:
//!
//! ```rust
//! use yatws::{OrderBuilder, OrderSide, TimeInForce};
//! use yatws::contract::Contract;
//! use yatws::order_manager::OrderEvent;
//! use std::time::Duration;
//!
//! // Create order specification
//! let (contract, order_request) = OrderBuilder::new(OrderSide::Buy, 100.0)
//!     .for_stock("AAPL")
//!     .market()
//!     .with_tif(TimeInForce::Day)
//!     .build()?;
//!
//! // Subscribe to order lifecycle events
//! let order_subscription = client.orders().subscribe_new_order(contract, order_request)?;
//! let mut order_events = order_subscription.events();
//!
//! // Process order events until completion or error
//! while let Some(event) = order_events.try_next(Duration::from_secs(1)) {
//!     match event {
//!         OrderEvent::Update(order) => {
//!             println!("Order {} status: {:?}, filled: {}",
//!                      order.id, order.state.status, order.state.filled_quantity);
//!
//!             if order.state.status.is_terminal() {
//!                 println!("Order completed with status: {:?}", order.state.status);
//!                 break;
//!             }
//!         }
//!         OrderEvent::Error(error) => {
//!             eprintln!("Order error: {:?}", error);
//!             break;
//!         }
//!     }
//! }
//! ```
//!
//! ### 3. Market Data
//!
//! ```rust
//! // Get a simple quote
//! let (bid, ask, last) = client.data_market().get_quote(
//!     &contract,
//!     None,
//!     Duration::from_secs(5)
//! )?;
//!
//! // Get shortability status
//! let shortability = client.data_market().get_shortability(
//!     &contract,
//!     None,
//!     Duration::from_secs(10)
//! )?;
//! println!("Shortability: {:?}", shortability);
//!
//! // Get short inventory data (requires shortinv feature)
//! #[cfg(feature = "shortinv")]
//! {
//!     use yatws::data_market_manager::ShortMarket;
//!     let inventory = client.data_market().get_short_inventory(ShortMarket::USA)?;
//!     println!("Found {} shortable stocks", inventory.len());
//!
//!     // Get margin requirements
//!     use yatws::data_market_manager::MarginMarket;
//!     let margin_data = client.data_market().get_short_margin(MarginMarket::US)?;
//!     println!("Found {} margin entries", margin_data.len());
//! }
//!
//! // Get historical data
//! use yatws::data::DurationUnit;
//! use yatws::contract::{BarSize, WhatToShow};
//! use yatws::data::MarketDataType;
//!
//! let bars = client.data_market().get_historical_data(
//!     &contract,
//!     None,                               // End time (None = now)
//!     DurationUnit::Day(1),               // Duration
//!     BarSize::Hour1,                     // Bar size
//!     WhatToShow::Trades,                 // What to show
//!     true,                               // Use RTH
//!     1,                                  // Date format (1 for yyyyMMdd HH:mm:ss)
//!     false,                              // Keep up to date?
//!     Some(MarketDataType::Delayed),      // Market data type
//!     &[]                                 // Chart options
//! )?;
//! ```
//!
//! ### 4. Options Strategies
//!
//! ```rust
//! // Create a bull call spread
//! let builder = OptionsStrategyBuilder::new(
//!     client.data_ref(),
//!     "AAPL",
//!     150.0,     // Current price
//!     10.0,      // Quantity (10 spreads)
//!     SecType::Stock
//! )?;
//!
//! let (contract, order) = builder
//!     .bull_call_spread(
//!         NaiveDate::from_ymd_opt(2025, 12, 19).unwrap(),
//!         150.0,  // Lower strike
//!         160.0   // Higher strike
//!     )?
//!     .with_limit_price(3.50)  // Debit of $3.50 per spread
//!     .build()?;
//!
//! let order_id = client.orders().place_order(contract, order)?;
//! ```
//!
//! ## 5. Financial Instrument Reference
//!
//! The DataRefManager provides comprehensive access to reference data about financial instruments, exchanges, and market parameters:
//!
//! ```rust
//! // Get contract details
//! let contract_details = client.data_ref().get_contract_details(&contract)?;
//!
//! // Get option chain parameters for an underlying
//! let option_params = client.data_ref().get_option_chain_params(
//!     "AAPL",
//!     "",                // Future/FOP exchange (empty for stocks)
//!     SecType::Stock,
//!     0                  // Underlying contract ID (0 if unknown)
//! )?;
//!
//! // Find contracts matching a pattern
//! let matching_symbols = client.data_ref().get_matching_symbols("APPLE")?;
//! ```
//!
//! ## 6. Market News and Information
//!
//! The DataNewsManager provides access to news headlines, articles, and bulletins from various providers:
//!
//! ```rust
//! // Get available news providers
//! let providers = client.data_news().get_news_providers()?;
//!
//! // Get a specific news article
//! let article = client.data_news().get_news_article(
//!     "BZ",              // Provider code
//!     "BZ-1234567",      // Article ID
//!     &[]                // No options
//! )?;
//!
//! // Get historical news for a contract
//! let news = client.data_news().get_historical_news(
//!     12345,             // Contract ID
//!     "BZ",              // Provider code
//!     None,              // Start time (None = default)
//!     None,              // End time (None = now)
//!     10,                // Max results
//!     &[]                // No options
//! )?;
//!
//! // Subscribe to live news bulletins
//! client.data_news().request_news_bulletins(true)?; // true = all messages
//! ```
//!
//! ## 7. Financial Fundamentals and Corporate Events
//!
//! The DataFundamentalsManager provides access to company financial data and corporate events:
//!
//! ```rust
//! // Get fundamental data (returns XML)
//! let fundamental_data = client.data_financials().get_fundamental_data(
//!     &contract,
//!     yatws::data::FundamentalReportType::ReportsFinSummary,  // Report type using enum
//!     &[]                   // No options
//! )?;
//!
//! // Parse the XML data into structured format
//! let parsed_data = parse_fundamental_xml(&fundamental_data, yatws::data::FundamentalReportType::ReportsFinSummary)?;
//!
//! // Get Wall Street Horizon events
//! let wsh_request = WshEventDataRequest {
//!     con_id: 12345,        // Contract ID
//!     count: 10,            // Number of events per page
//!     // Other filter parameters...
//!     total_limit: Some(50) // Total events to retrieve
//! };
//!
//! let wsh_events = client.data_financials().get_wsh_events(
//!     &wsh_request,
//!     Duration::from_secs(30)
//! )?;
//! ```
//!
//! ## Advanced Features
//!
//! ### 1. Session Recording and Replay
//!
//! YATWS supports recording TWS interactions to a SQLite database for later replay:
//!
//! ```rust
//! // Enable session recording
//! let client = IBKRClient::new(
//!     "127.0.0.1",
//!     7497,
//!     0,
//!     Some(("sessions.db", "my_trading_session"))
//! )?;
//!
//! // Later, replay the session
//! let replay_client = IBKRClient::from_db("sessions.db", "my_trading_session")?;
//! ```
//!
//! ### 2. Order Conditions
//!
//! ```rust
//! // Create a price-conditional order
//! let (contract, order) = OrderBuilder::new(OrderSide::Buy, 100.0)
//!     .for_stock("AAPL")
//!     .limit(150.0)
//!     .add_price_condition(
//!         265598,             // SPY con_id
//!         "ISLAND",           // Exchange
//!         400.0,              // Price
//!         TriggerMethod::Last, // Trigger method
//!         false               // Is less than 400
//!     )
//!     .build()?;
//! ```
//!
//! ### 3. Rate Limiting
//!
//! YATWS includes a configurable rate limiter to ensure compliance with Interactive Brokers' API limits:
//!
//! ```rust
//! // Enable rate limiting with default settings
//! client.enable_rate_limiting()?;
//!
//! // Configure custom rate limiting
//! let mut config = RateLimiterConfig::default();
//! config.enabled = true;
//! config.max_messages_per_second = 40; // More conservative than default
//! config.max_historical_requests = 30; // Fewer simultaneous historical requests
//! client.configure_rate_limiter(config)?;
//!
//! // Monitor rate limiter status
//! if let Some(status) = client.get_rate_limiter_status() {
//!     println!("Active historical requests: {}/{}",
//!              status.active_historical_requests,
//!              config.max_historical_requests);
//! }
//!
//! // Clean up stale requests (for long-running applications)
//! let (hist_cleaned, mkt_cleaned) = client.cleanup_stale_rate_limiter_requests(
//!     Duration::from_secs(300)  // Clean up requests older than 5 minutes
//! )?;
//! ```
//!
//! The rate limiter enforces:
//! - Maximum messages per second (default: 50)
//! - Maximum simultaneous historical data requests (default: 50)
//! - Maximum market data lines (default: 100)
//!
//! Rate limiting is disabled by default but can be enabled with a single method call.
//!
//! ## Error Handling
//!
//! YATWS uses Rust's Result pattern consistently, with a custom `IBKRError` type:
//!
//! ```rust
//! match client.account().get_net_liquidation() {
//!     Ok(net_liq_value) => println!("Account Net Liquidation Value: ${}", net_liq_value),
//!     Err(IBKRError::Timeout) => println!("Operation timed out"),
//!     Err(IBKRError::ApiError(code, msg)) => println!("API error {}: {}", code, msg),
//!     Err(e) => println!("Other error: {:?}", e),
//! }
//! ```

mod base;
mod client;
mod conn;
mod conn_log;
mod conn_mock;
mod financial_report_parser;
mod handler;
mod message_parser;
mod min_server_ver;
mod options_strategy_builder;
mod parser_account;
mod parser_client;
mod parser_data_fin;
mod parser_data_market;
mod parser_data_news;
mod parser_data_ref;
mod parser_fin_adv;
mod parser_order;
mod protocol_dec_parser;
mod protocol_decoder;
mod protocol_encoder;
mod order_builder;

pub mod order_build_types {
  use super::order_builder;
  pub use order_builder::TriggerMethod;
  pub use order_builder::ConditionConjunction;
}

// Data structures:
pub mod account;
pub mod data;
pub mod contract;
pub mod order;
pub mod data_wsh;
pub mod news;
pub mod financial_advisor;

// API:
pub mod account_subscription;
pub mod data_subscription;
pub mod news_subscription;
pub mod data_observer;

// Managers:
pub mod account_manager;
pub mod data_fin_manager;
pub mod data_market_manager;
pub mod data_news_manager;
pub mod data_ref_manager;
pub mod order_manager;
pub mod scan_parameters;
pub mod financial_advisor_manager;

pub mod rate_limiter;

pub use order_builder::OrderBuilder;
pub use options_strategy_builder::OptionsStrategyBuilder;
pub use base::IBKRError;
pub use financial_report_parser::parse_fundamental_xml;
pub use client::IBKRClient;
pub use client::client_manager;
pub use order_manager::{OrderEvent, OrderSubscription, OrderSubscriptionIterator};

// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const API_VERSION: &str = "10.30"; // IBKR API version supported
