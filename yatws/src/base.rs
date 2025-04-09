// yatws/src/base.rs

use thiserror::Error;
use std::fmt;

/// Errors that can occur in the IBKR API
#[derive(Error, Debug, Clone)]
pub enum IBKRError {
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
}

/// Direction of a message (incoming or outgoing)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageDirection {
  Incoming,
  Outgoing,
}

impl fmt::Display for MessageDirection {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      MessageDirection::Incoming => write!(f, "incoming"),
      MessageDirection::Outgoing => write!(f, "outgoing"),
    }
  }
}

/// Type of a message
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
  Request,
  Response,
  Notification,
  Handshake,
}

/// A trait for request types
pub trait Request: Send + Sync {
  /// The type of response expected for this request
  type Response: Send + 'static;

  /// Convert the request to a raw message
  fn to_message(&self, request_id: i32) -> IBKRRawMessage;
}

/// A trait for notification types
pub trait Notification: Send + Sync + 'static {}

/// A raw message from/to the IBKR API
#[derive(Debug, Clone)]
pub struct IBKRRawMessage {
  fields: Vec<String>,
  #[allow(dead_code)]
  version: i32,
}

impl IBKRRawMessage {
  /// Create a new raw message
  pub fn new(fields: Vec<String>, version: i32) -> Self {
    IBKRRawMessage { fields, version }
  }

  /// Create a new client version handshake message
  pub fn new_client_version(version: String) -> Self {
    IBKRRawMessage {
      fields: vec![version],
      version: 1,
    }
  }

  /// Get the message type
  pub fn message_type(&self) -> MessageType {
    // In a real implementation, this would analyze the message structure
    // This is a placeholder implementation
    if self.fields.is_empty() {
      return MessageType::Notification;
    }

    match self.fields[0].as_str() {
      "CONNECT" => MessageType::Handshake,
      _ => {
        if self.request_id().is_some() {
          MessageType::Response
        } else {
          MessageType::Request
        }
      }
    }
  }

  /// Get the request ID if present
  pub fn request_id(&self) -> Option<i32> {
    // In a real implementation, this would extract the request ID from the message
    // This is a placeholder implementation
    if self.fields.len() > 1 {
      self.fields[1].parse::<i32>().ok()
    } else {
      None
    }
  }

  /// Parse a raw message from bytes
  pub fn parse(data: &[u8]) -> Result<Self, IBKRError> {
    // In a real implementation, this would parse the IBKR message format
    // This is a placeholder implementation
    let s = String::from_utf8_lossy(data);
    let fields: Vec<String> = s.split('\0').map(|s| s.to_string()).collect();

    Ok(IBKRRawMessage {
      fields,
      version: 1,
    })
  }

  /// Serialize the message to bytes
  pub fn serialize(&self) -> Vec<u8> {
    // In a real implementation, this would serialize to the IBKR message format
    // This is a placeholder implementation
    let mut result = Vec::new();
    for field in &self.fields {
      result.extend_from_slice(field.as_bytes());
      result.push(0); // Null terminator
    }
    result
  }
}

/// Configuration for an IBKR client
#[derive(Debug, Clone)]
pub struct ClientConfig {
  pub reconnect_enabled: bool,
  pub reconnect_max_attempts: u32,
  pub reconnect_backoff_ms: u64,
  pub request_timeout_ms: u64,
  pub background_update_interval_ms: u64,
}

impl Default for ClientConfig {
  fn default() -> Self {
    ClientConfig {
      reconnect_enabled: true,
      reconnect_max_attempts: 5,
      reconnect_backoff_ms: 1000,
      request_timeout_ms: 30000,
      background_update_interval_ms: 60000,
    }
  }
}

/// Configuration for connection manager
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
  pub reconnect_config: ReconnectConfig,
}

impl Default for ConnectionConfig {
  fn default() -> Self {
    ConnectionConfig {
      reconnect_config: ReconnectConfig::default(),
    }
  }
}

/// Configuration for reconnection logic
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
  pub enabled: bool,
  pub max_attempts: u32,
  pub backoff_ms: u64,
}

impl Default for ReconnectConfig {
  fn default() -> Self {
    ReconnectConfig {
      enabled: true,
      max_attempts: 5,
      backoff_ms: 1000,
    }
  }
}

/// Rate limiter for API requests
#[derive(Debug)]
pub struct RateLimiter {
  requests_per_period: usize,
  period: std::time::Duration,
  request_times: Vec<std::time::Instant>,
}

impl RateLimiter {
  /// Create a new rate limiter
  pub fn new(requests_per_period: usize, period: std::time::Duration) -> Self {
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
      std::thread::sleep(std::time::Duration::from_millis(10));
    }
  }
}
