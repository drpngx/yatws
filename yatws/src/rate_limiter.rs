// yatws/src/rate_limiter.rs
//! Rate limiter implementation for the YATWS library.
//!
//! This module provides rate limiting capabilities to respect Interactive Brokers'
//! API limitations:
//! - Maximum 50 messages per second
//! - Maximum 50 simultaneous historical data requests
//! - Maximum 100 market data lines (configurable)
//!
//! Rate limiting is disabled by default and can be configured through the IBKRClient.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::Mutex;
use log::{debug, info, warn};

/// Configuration for rate limiting in the YATWS library.
#[derive(Clone, Debug)]
pub struct RateLimiterConfig {
  /// When enabled=false, rate limiting is bypassed
  pub enabled: bool,
  /// Messages per second limit (default: 50)
  pub max_messages_per_second: u32,
  /// Maximum burst size for message rate limiter (default: 100)
  pub max_message_burst: u32,
  /// Maximum simultaneous historical data requests (default: 50)
  pub max_historical_requests: u32,
  /// Maximum simultaneous market data lines (default: 100)
  pub max_market_data_lines: u32,
  /// Maximum time to wait when rate limited before returning an error (default: 5s)
  pub rate_limit_wait_timeout: Duration,
}

impl Default for RateLimiterConfig {
  fn default() -> Self {
    Self {
      enabled: false, // Disabled by default
      max_messages_per_second: 50,
      max_message_burst: 100,
      max_historical_requests: 50,
      max_market_data_lines: 100,
      rate_limit_wait_timeout: Duration::from_secs(5),
    }
  }
}

/// Status of the rate limiter, showing current usage.
#[derive(Clone, Debug)]
pub struct RateLimiterStatus {
  pub enabled: bool,
  /// Current message rate in messages per second
  pub current_message_rate: f64,
  /// Current number of tokens in the bucket
  pub current_message_tokens: u32,
  /// Number of active historical data requests
  pub active_historical_requests: u32,
  /// Number of active market data lines
  pub active_market_data_lines: u32,
  /// Counts of messages delayed due to rate limiting
  pub messages_delayed: u64,
  /// Counts of requests rejected due to rate limiting
  pub requests_rejected: u64,
}

/// Token bucket implementation for rate limiting messages per second.
pub struct TokenBucketRateLimiter {
  tokens: AtomicU32,
  max_tokens: AtomicU32,
  tokens_per_second: AtomicU32,
  last_refill: Mutex<Instant>,
  enabled: AtomicBool,
  messages_delayed: AtomicU64,
}

impl TokenBucketRateLimiter {
  /// Create a new token bucket rate limiter.
  pub fn new(max_tokens: u32, tokens_per_second: u32) -> Self {
    Self {
      tokens: AtomicU32::new(max_tokens),
      max_tokens: AtomicU32::new(max_tokens),
      tokens_per_second: AtomicU32::new(tokens_per_second),
      last_refill: Mutex::new(Instant::now()),
      enabled: AtomicBool::new(false),
      messages_delayed: AtomicU64::new(0),
    }
  }

  /// Try to acquire a token without waiting.
  /// Returns true if token was acquired, false otherwise.
  pub fn try_acquire(&self) -> bool {
    if !self.enabled.load(Ordering::Relaxed) {
      return true;
    }

    self.refill_tokens();

    // Try to decrement token count
    let mut current = self.tokens.load(Ordering::Relaxed);
    loop {
      if current == 0 {
        return false;
      }

      match self.tokens.compare_exchange_weak(
        current,
        current - 1,
        Ordering::SeqCst,
        Ordering::Relaxed
      ) {
        Ok(_) => return true,
        Err(actual) => current = actual,
      }
    }
  }

  /// Try to acquire a token, waiting up to the timeout if needed.
  /// Returns true if token was acquired, false if timeout occurred.
  pub fn acquire(&self, timeout: Duration) -> bool {
    if !self.enabled.load(Ordering::Relaxed) {
      return true;
    }

    let start = Instant::now();

    // First quick check
    if self.try_acquire() {
      return true;
    }

    // Increment delay counter
    self.messages_delayed.fetch_add(1, Ordering::Relaxed);

    // Loop until timeout
    let mut sleep_duration = Duration::from_millis(1);

    while start.elapsed() < timeout {
      // Adaptive backoff
      std::thread::sleep(sleep_duration);
      sleep_duration = std::cmp::min(sleep_duration * 2, Duration::from_millis(100));

      // Refill and try again
      self.refill_tokens();

      if self.try_acquire() {
        return true;
      }
    }

    debug!("Rate limit timeout exceeded after {:?}", timeout);
    false
  }

  /// Get the current status of this rate limiter.
  /// Returns (current_rate, token_count, messages_delayed, enabled)
  pub fn status(&self) -> (f64, u32, u64, bool) {
    let tokens = self.tokens.load(Ordering::Relaxed);
    let rate = self.calculate_current_rate();
    let delayed = self.messages_delayed.load(Ordering::Relaxed);
    let enabled = self.enabled.load(Ordering::Relaxed);

    (rate, tokens, delayed, enabled)
  }

  /// Check if the rate limiter is enabled.
  pub fn enabled(&self) -> bool {
    self.enabled.load(Ordering::Relaxed)
  }

  /// Enable or disable the rate limiter.
  pub fn set_enabled(&self, enabled: bool) {
    self.enabled.store(enabled, Ordering::Relaxed);
  }

  /// Configure the rate limiter with new settings.
  pub fn configure(&self, max_tokens: u32, tokens_per_second: u32) {
    self.max_tokens.store(max_tokens, Ordering::Relaxed);
    self.tokens_per_second.store(tokens_per_second, Ordering::Relaxed);

    // If increasing max_tokens, also increase current tokens
    let current_tokens = self.tokens.load(Ordering::Relaxed);
    if max_tokens > current_tokens {
      self.tokens.store(max_tokens, Ordering::Relaxed);
    }
  }

  // Helper method to refill tokens based on elapsed time
  fn refill_tokens(&self) {
    let mut last_refill = self.last_refill.lock();
    let now = Instant::now();
    let elapsed = now.duration_since(*last_refill);

    // Convert to fractional seconds
    let seconds = elapsed.as_secs_f64();
    let tokens_to_add = (seconds * self.tokens_per_second.load(Ordering::Relaxed) as f64) as u32;

    if tokens_to_add > 0 {
      // Add tokens, but don't exceed max_tokens
      let max = self.max_tokens.load(Ordering::Relaxed);
      let current = self.tokens.load(Ordering::Relaxed);
      let new_value = std::cmp::min(current + tokens_to_add, max);
      self.tokens.store(new_value, Ordering::Relaxed);

      // Update last refill time
      *last_refill = now;
    }
  }

  // Helper to calculate the current rate
  fn calculate_current_rate(&self) -> f64 {
    let max_tokens = self.max_tokens.load(Ordering::Relaxed) as f64;
    let current_tokens = self.tokens.load(Ordering::Relaxed) as f64;
    let tokens_per_second = self.tokens_per_second.load(Ordering::Relaxed) as f64;

    if max_tokens == current_tokens {
      // If bucket is full, rate is likely 0
      0.0
    } else {
      // Estimate current rate based on missing tokens
      tokens_per_second * (1.0 - (current_tokens / max_tokens))
    }
  }
}

/// Counter-based rate limiter for tracking simultaneous requests.
pub struct CounterRateLimiter {
  counter: AtomicU32,
  max_count: AtomicU32,
  enabled: AtomicBool,
  requests_rejected: AtomicU64,
  // Map of active request IDs to facilitate automatic tracking
  active_requests: Arc<Mutex<HashMap<i32, Instant>>>,
}

impl CounterRateLimiter {
  /// Create a new counter rate limiter.
  pub fn new(max_count: u32) -> Self {
    Self {
      counter: AtomicU32::new(0),
      max_count: AtomicU32::new(max_count),
      enabled: AtomicBool::new(false),
      requests_rejected: AtomicU64::new(0),
      active_requests: Arc::new(Mutex::new(HashMap::new())),
    }
  }

  /// Try to acquire a slot for a new request.
  /// Returns true if slot was acquired, false otherwise.
  pub fn try_acquire(&self) -> bool {
    if !self.enabled.load(Ordering::Relaxed) {
      return true;
    }

    let max = self.max_count.load(Ordering::Relaxed);
    let mut current = self.counter.load(Ordering::Relaxed);

    loop {
      if current >= max {
        self.requests_rejected.fetch_add(1, Ordering::Relaxed);
        return false;
      }

      match self.counter.compare_exchange_weak(
        current,
        current + 1,
        Ordering::SeqCst,
        Ordering::Relaxed
      ) {
        Ok(_) => return true,
        Err(actual) => current = actual,
      }
    }
  }

  /// Register a request ID for automatic tracking.
  pub fn register_request(&self, req_id: i32) {
    if !self.enabled.load(Ordering::Relaxed) {
      return;
    }

    let mut active = self.active_requests.lock();
    active.insert(req_id, Instant::now());
  }

  /// Explicitly release a request (for cancellations).
  pub fn release(&self, req_id: i32) {
    if !self.enabled.load(Ordering::Relaxed) {
      return;
    }

    let mut active = self.active_requests.lock();
    if active.remove(&req_id).is_some() {
      // Decrease counter, but don't go below 0
      let current = self.counter.load(Ordering::Relaxed);
      if current > 0 {
        self.counter.fetch_sub(1, Ordering::SeqCst);
      }
    }
  }

  /// Mark a request as completed (for end messages).
  pub fn mark_completed(&self, req_id: i32) {
    self.release(req_id); // Same implementation as release
  }

  /// Get current status of this rate limiter.
  /// Returns (active_count, max_count, requests_rejected, enabled)
  pub fn status(&self) -> (u32, u32, u64, bool) {
    let count = self.counter.load(Ordering::Relaxed);
    let max = self.max_count.load(Ordering::Relaxed);
    let rejected = self.requests_rejected.load(Ordering::Relaxed);
    let enabled = self.enabled.load(Ordering::Relaxed);

    (count, max, rejected, enabled)
  }

  /// Check if the rate limiter is enabled.
  pub fn enabled(&self) -> bool {
    self.enabled.load(Ordering::Relaxed)
  }

  /// Enable or disable the rate limiter.
  pub fn set_enabled(&self, enabled: bool) {
    self.enabled.store(enabled, Ordering::Relaxed);
  }

  /// Configure the rate limiter with a new maximum count.
  pub fn configure(&self, max_count: u32) {
    self.max_count.store(max_count, Ordering::Relaxed);
  }

  /// Periodic cleanup of stale requests (optional, for safety).
  /// Returns the number of cleaned up requests.
  pub fn cleanup_stale_requests(&self, older_than: Duration) -> u32 {
    if !self.enabled.load(Ordering::Relaxed) {
      return 0;
    }

    let now = Instant::now();
    let mut cleaned = 0;

    let mut active = self.active_requests.lock();
    let stale_keys: Vec<i32> = active
      .iter()
      .filter(|(_, time)| now.duration_since(**time) > older_than)
      .map(|(&k, _)| k)
      .collect();

    for key in stale_keys {
      active.remove(&key);
      cleaned += 1;
    }

    if cleaned > 0 {
      // Adjust counter, but don't go below 0
      let current = self.counter.load(Ordering::Relaxed);
      if current >= cleaned {
        self.counter.fetch_sub(cleaned, Ordering::SeqCst);
      } else {
        self.counter.store(0, Ordering::SeqCst);
      }

      warn!("Cleaned up {} stale rate limit requests", cleaned);
    }

    cleaned
  }
}

/// Manager for all rate limiters in the system.
pub struct RateLimiterManager {
  config: Mutex<RateLimiterConfig>,
  message_limiter: Arc<TokenBucketRateLimiter>,
  historical_limiter: Arc<CounterRateLimiter>,
  market_data_limiter: Arc<CounterRateLimiter>,
}

impl RateLimiterManager {
  /// Create a new rate limiter manager with the specified configuration.
  pub fn new(config: RateLimiterConfig) -> Self {
    let message_limiter = Arc::new(TokenBucketRateLimiter::new(
      config.max_message_burst,
      config.max_messages_per_second,
    ));

    let historical_limiter = Arc::new(CounterRateLimiter::new(
      config.max_historical_requests,
    ));

    let market_data_limiter = Arc::new(CounterRateLimiter::new(
      config.max_market_data_lines,
    ));

    // Set enabled status
    message_limiter.set_enabled(config.enabled);
    historical_limiter.set_enabled(config.enabled);
    market_data_limiter.set_enabled(config.enabled);

    info!("Created rate limiter manager (enabled: {})", config.enabled);

    Self {
      config: Mutex::new(config),
      message_limiter,
      historical_limiter,
      market_data_limiter,
    }
  }

  /// Configure all rate limiters with new settings.
  pub fn configure(&self, config: RateLimiterConfig) {
    info!("Configuring rate limiters (enabled: {}, msg/sec: {}, burst: {}, hist: {}, mkt: {})",
          config.enabled, config.max_messages_per_second, config.max_message_burst,
          config.max_historical_requests, config.max_market_data_lines);

    // Store the config
    *self.config.lock() = config.clone();

    // Configure individual limiters
    self.message_limiter.configure(
      config.max_message_burst,
      config.max_messages_per_second,
    );
    self.historical_limiter.configure(
      config.max_historical_requests,
    );
    self.market_data_limiter.configure(
      config.max_market_data_lines,
    );

    // Set enabled status
    self.message_limiter.set_enabled(config.enabled);
    self.historical_limiter.set_enabled(config.enabled);
    self.market_data_limiter.set_enabled(config.enabled);
  }

  /// Get the current status of all rate limiters.
  pub fn get_status(&self) -> RateLimiterStatus {
    let (msg_rate, msg_tokens, msg_delayed, _) = self.message_limiter.status();
    let (hist_count, _, hist_rejected, _) = self.historical_limiter.status();
    let (mkt_count, _, mkt_rejected, _) = self.market_data_limiter.status();

    let config = self.config.lock();

    RateLimiterStatus {
      enabled: config.enabled,
      current_message_rate: msg_rate,
      current_message_tokens: msg_tokens,
      active_historical_requests: hist_count,
      active_market_data_lines: mkt_count,
      messages_delayed: msg_delayed,
      requests_rejected: hist_rejected + mkt_rejected,
    }
  }

  /// Get a reference to the message rate limiter.
  pub fn get_message_limiter(&self) -> Arc<TokenBucketRateLimiter> {
    Arc::clone(&self.message_limiter)
  }

  /// Get a reference to the historical data rate limiter.
  pub fn get_historical_limiter(&self) -> Arc<CounterRateLimiter> {
    Arc::clone(&self.historical_limiter)
  }

  /// Get a reference to the market data rate limiter.
  pub fn get_market_data_limiter(&self) -> Arc<CounterRateLimiter> {
    Arc::clone(&self.market_data_limiter)
  }

  /// Get the wait timeout for rate limiting.
  pub fn get_wait_timeout(&self) -> Duration {
    self.config.lock().rate_limit_wait_timeout
  }

  /// Run periodic cleanup of stale requests.
  /// This should be called occasionally to clean up any requests that
  /// never received completion messages.
  pub fn cleanup_stale_requests(&self, older_than: Duration) -> (u32, u32) {
    let hist_cleaned = self.historical_limiter.cleanup_stale_requests(older_than);
    let mkt_cleaned = self.market_data_limiter.cleanup_stale_requests(older_than);

    (hist_cleaned, mkt_cleaned)
  }
}
