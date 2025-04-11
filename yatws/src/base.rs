// yatws/src/base.rs
// Base types and error definitions for the TWS API

use thiserror::Error;
use std::fmt;
use std::io::{Read, Write};
use std::time::Duration;


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

  #[error("Unsupported: {0}")]
  Unsupported(String),

  #[error("API error: code={0}, msg={1}")]
  ApiError(i32, String),
}

/// Rate limiter for API requests
#[derive(Debug)]
pub struct RateLimiter {
  requests_per_period: usize,
  period: Duration,
  request_times: Vec<std::time::Instant>,
}

impl RateLimiter {
  /// Create a new rate limiter
  pub fn new(requests_per_period: usize, period: Duration) -> Self {
    RateLimiter {
      requests_per_period,
      period,
      request_times: Vec::with_capacity(requests_per_period),
    }
  }

  /// Check if a request can be made
  pub fn check(&mut self) -> bool {
    let now = std::time::Instant::now();

    // Remove expired timestamps
    let cutoff = now - self.period;
    self.request_times.retain(|&t| t > cutoff);

    // Check if we're under the limit
    if self.request_times.len() < self.requests_per_period {
      self.request_times.push(now);
      true
    } else {
      false
    }
  }

  /// Wait until a request can be made
  pub fn wait(&mut self) {
    while !self.check() {
      std::thread::sleep(Duration::from_millis(10));
    }
  }
}
