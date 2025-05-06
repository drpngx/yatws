// yatws/src/lib.rs
// Main entry point for the IBKR API library

//! # YATWS - Yet Another TWS API
//!
//! A modern Rust wrapper for the Interactive Brokers TWS API that provides:
//!
//! - Robust connection handling with automatic reconnection
//! - Thread-safe API with proper synchronization
//! - Comprehensive error handling
//! - High-level abstractions for orders, accounts, and market data
//! - Logging and replay functionality

mod account_manager;
mod base;
mod client;
mod conn;
mod conn_log;
mod conn_mock;
mod data_fin_manager;
mod data_market_manager;
mod data_news_manager;
mod data_ref_manager;
mod financial_report_parser;
mod handler;
mod message_parser;
mod min_server_ver;
mod news;
mod options_strategy_builder;
mod order_builder;
mod order_manager;
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

// Data structures.
pub mod account;
pub mod data;
pub mod contract;
pub mod order;
pub mod data_wsh;

pub use order_builder::OrderBuilder;
pub use options_strategy_builder::OptionsStrategyBuilder;
pub use base::IBKRError;
pub use financial_report_parser::parse_fundamental_xml;
pub use client::IBKRClient;
pub use account_manager::AccountManager;
pub use order_manager::OrderManager;
pub use data_ref_manager::DataRefManager;
pub use data_market_manager::DataMarketManager;
pub use data_news_manager::DataNewsManager;
pub use data_fin_manager::DataFundamentalsManager;

// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const API_VERSION: &str = "10.30"; // IBKR API version supported
