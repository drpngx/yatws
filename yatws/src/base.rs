// yatws/src/base.rs
// Base types and error definitions for the TWS API

use thiserror::Error;

/// Errors that can occur in the IBKR API
#[derive(Error, Debug, Clone)]
pub enum IBKRError {
  #[error("Configuration error: {0}")]
  ConfigurationError(String),

  #[error("Connection failed: {0}")]
  ConnectionFailed(String),

  #[error("Not connected to IBKR")]
  NotConnected,

  #[error("Already connected to IBKR")]
  AlreadyConnected,

  #[error("Service already running: {0}")]
  AlreadyRunning(String),

  #[error("Socket error: {0}")]
  SocketError(String),

  #[error("Message parse error: {0}")]
  ParseError(String),

  #[error("Request timeout: {0}")]
  Timeout(String),

  #[error("Duplicate request ID: {0}")]
  DuplicateRequestId(i32),

  #[error("Unknown request ID: {0}")]
  UnknownRequestId(i32),

  #[error("Order rejected: {0}")]
  OrderRejected(String),

  #[error("Invalid parameter: {0}")]
  InvalidParameter(String),

  #[error("Logging error: {0}")]
  LoggingError(String),

  #[error("Replay error: {0}")]
  ReplayError(String),

  #[error("Rate limit exceeded")]
  RateLimitExceeded,

  #[error("Internal error: {0}")]
  InternalError(String),

  #[error("Update TWS: {0}")]
  UpdateTws(String),

  #[error("Invalid contract: {0}")]
  InvalidContract(String),

  #[error("Invalid order: {0}")]
  InvalidOrder(String),

  #[error("Invalid account: {0}")]
  InvalidAccount(String),

  #[error("Rate Limit exceeded: {0}")]
  RateLimited(String),

  #[error("Unsupported: {0}")]
  Unsupported(String),

  #[error("API error: code={0}, msg={1}")]
  ApiError(i32, String),
}

// Implement From trait for quick_xml::Error to allow `?` operator conversion
impl From<quick_xml::Error> for IBKRError {
  fn from(err: quick_xml::Error) -> Self {
    IBKRError::ParseError(format!("XML parsing error: {}", err))
  }
}
