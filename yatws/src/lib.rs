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

mod base;
mod contract;
mod order;
mod account;
mod protocol;
mod protocol_encoder;
mod protocol_decoder;
mod protocol_dec_parser;
mod min_server_ver;
mod message_parser;
mod parser_client;
mod parser_order;
mod parser_account;
mod parser_fin_adv;
mod parser_data_ref;
mod parser_data_market;
mod parser_data_fin;
mod parser_data_news;
pub mod handler;
pub mod conn;

// Re-export the core data structures
// pub use base::*;
// pub use contract::*;
// pub use order::*;
// pub use account::*;

// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const API_VERSION: &str = "10.30"; // IBKR API version supported
