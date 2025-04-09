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


// Re-export the core data structures
pub use base::*;
pub use contract::*;
pub use order::*;
pub use account::*;

// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const API_VERSION: &str = "10.30"; // IBKR API version supported
