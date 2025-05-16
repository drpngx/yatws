# YATWS Rate Limiter Design

## Overview

This document outlines the design for a rate limiter system for the YATWS (Yet Another TWS API) library to enforce Interactive Brokers' (IBKR) API rate limits. The system will respect IBKR's documented limits while providing flexibility, configurability, and transparency to users.

## IBKR Rate Limits

The rate limiter will enforce three primary limits:

1. **Messages per Second**: Maximum 50 messages per second.
2. **Historical Data**: Maximum 50 simultaneous open historical data requests.
3. **Market Data Lines**: Default limit of 100 market data lines (configurable). Limits simultaneous market data streams including real-time data, market depth, and real-time bars.

## Design Principles

- **Optional**: Rate limiting is disabled by default.
- **Transparent**: Provides metrics on current usage.
- **Configurable**: Limits can be adjusted as needed.
- **Graceful**: Rate limiting should degrade gracefully (e.g., queuing, backpressure).
- **Modular**: Different limit types can be configured separately.
- **Automatic**: Tracks request completion via API end messages.

## Architecture

The rate limiter consists of three primary components:

1. **Token Bucket Rate Limiter**: Controls message frequency (msgs/sec)
2. **Counter Rate Limiter**: Tracks simultaneous open requests
3. **Rate Limiter Manager**: Coordinates different limiters and provides configuration interface

### Implementation Location

- **MessageBroker**: Add token bucket limiter for all outgoing messages.
- **DataMarketManager**: Add counter limiter for market data lines.
- **DataRefManager**: Add counter limiter for historical data requests.
- **IBKRClient**: Add configuration interface to manage all limiters.

## Public Interface

```rust
// Main configuration struct - Used to configure the rate limiters
pub struct RateLimiterConfig {
    // When enabled=false, rate limiting is bypassed
    pub enabled: bool,
    // Messages per second limit (default: 50)
    pub max_messages_per_second: u32,
    // Maximum burst size for message rate limiter (default: 100)
    pub max_message_burst: u32,
    // Maximum simultaneous historical data requests (default: 50)
    pub max_historical_requests: u32,
    // Maximum simultaneous market data lines (default: 100)
    pub max_market_data_lines: u32,
    // Maximum time to wait when rate limited before returning an error (default: 5s)
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

// Status struct - Reports current usage
pub struct RateLimiterStatus {
    pub enabled: bool,
    // Current message rate in messages per second
    pub current_message_rate: f64,
    // Current number of tokens in the bucket
    pub current_message_tokens: u32,
    // Number of active historical data requests
    pub active_historical_requests: u32,
    // Number of active market data lines
    pub active_market_data_lines: u32,
    // Counts of messages delayed due to rate limiting
    pub messages_delayed: u64,
    // Counts of requests rejected due to rate limiting
    pub requests_rejected: u64,
}

// Extension to IBKRClient
impl IBKRClient {
    /// Configure rate limiting
    pub fn configure_rate_limiter(&self, config: RateLimiterConfig) -> Result<(), IBKRError> {
        // Implementation will distribute config to various components
    }

    /// Get current rate limiter status
    pub fn get_rate_limiter_status(&self) -> RateLimiterStatus {
        // Implementation will gather status from various components
    }

    /// Enable rate limiting with default configuration
    pub fn enable_rate_limiting(&self) -> Result<(), IBKRError> {
        // Shortcut to enable with default settings
    }

    /// Disable rate limiting
    pub fn disable_rate_limiting(&self) -> Result<(), IBKRError> {
        // Shortcut to disable rate limiting
    }
}
```

## Internal Components

### Token Bucket Rate Limiter

```rust
struct TokenBucketRateLimiter {
    tokens: AtomicU32,
    max_tokens: u32,
    tokens_per_second: u32,
    last_refill: Mutex<Instant>,
    enabled: AtomicBool,
    messages_delayed: AtomicU64,
}

impl TokenBucketRateLimiter {
    fn new(max_tokens: u32, tokens_per_second: u32) -> Self { ... }

    fn try_acquire(&self) -> bool { ... }

    fn acquire(&self, timeout: Duration) -> bool { ... }

    fn status(&self) -> (f64, u32, u64) { ... }

    fn enabled(&self) -> bool { ... }

    fn set_enabled(&self, enabled: bool) { ... }

    fn configure(&self, max_tokens: u32, tokens_per_second: u32) { ... }
}
```

### Counter Rate Limiter

```rust
struct CounterRateLimiter {
    counter: AtomicU32,
    max_count: AtomicU32,
    enabled: AtomicBool,
    requests_rejected: AtomicU64,
    // Map of active request IDs to facilitate automatic tracking
    active_requests: Arc<Mutex<HashMap<i32, Instant>>>,
}

impl CounterRateLimiter {
    fn new(max_count: u32) -> Self { ... }

    // Try to acquire a slot for a new request
    fn try_acquire(&self) -> bool { ... }

    // Register a request ID for automatic tracking
    fn register_request(&self, req_id: i32) { ... }

    // Explicitly release a request (for cancellations)
    fn release(&self, req_id: i32) { ... }

    // Mark a request as completed (for end messages)
    fn mark_completed(&self, req_id: i32) { ... }

    // Get current status
    fn status(&self) -> (u32, u32, u64) { ... }

    fn enabled(&self) -> bool { ... }

    fn set_enabled(&self, enabled: bool) { ... }

    fn configure(&self, max_count: u32) { ... }

    // Periodic cleanup of stale requests (optional, for safety)
    fn cleanup_stale_requests(&self, older_than: Duration) -> u32 { ... }
}
```

## MessageBroker Extensions

```rust
impl MessageBroker {
    // Add rate limiter to constructor
    pub(crate) fn new(connection: Box<dyn Connection>, logger: Option<ConnectionLogger>, rate_limiter: Option<TokenBucketRateLimiter>) -> Self { ... }

    // Modify send_message to respect rate limiting
    pub fn send_message(&self, message_body: &[u8]) -> Result<(), IBKRError> {
        if let Some(limiter) = &self.rate_limiter {
            if limiter.enabled() {
                if !limiter.acquire(self.rate_limit_config.rate_limit_wait_timeout) {
                    return Err(IBKRError::RateLimited("Message rate limit exceeded".to_string()));
                }
            }
        }
        // Existing send logic...
    }

    // Add configuration method
    pub(crate) fn configure_rate_limiter(&self, config: &RateLimiterConfig) { ... }

    // Add status method
    pub(crate) fn get_rate_limiter_status(&self) -> Option<(f64, u32, u64, bool)> { ... }
}
```

## Manager Extensions

For both `DataMarketManager` and `DataRefManager`, we'll add counter rate limiters to track and limit simultaneous requests, with automatic tracking of request completion through end messages.

```rust
impl DataMarketManager {
    // Add counter limiter to track market data lines

    // Modify request methods to use the limiter
    pub fn request_market_data(...) -> Result<i32, IBKRError> {
        if self.market_data_limiter.enabled() && !self.market_data_limiter.try_acquire() {
            return Err(IBKRError::RateLimited("Market data line limit exceeded".to_string()));
        }

        // Existing request logic...

        // If request successful, register for tracking
        self.market_data_limiter.register_request(req_id);
        Ok(req_id)
    }

    // Modify cancel methods to explicitly release the counter
    pub fn cancel_market_data(req_id: i32) -> Result<(), IBKRError> {
        // Existing cancel logic...
        self.market_data_limiter.release(req_id);
        Ok(())
    }

    // Extend handler methods to mark completions
    fn tick_snapshot_end(&self, req_id: i32) {
        // Existing handler logic...

        // Mark snapshot request as completed in rate limiter
        self.market_data_limiter.mark_completed(req_id);
    }

    // Add configuration method
    pub(crate) fn configure_rate_limiter(&self, config: &RateLimiterConfig) { ... }

    // Add status method
    pub(crate) fn get_rate_limiter_status(&self) -> (u32, u32, u64, bool) { ... }
}
```

Similar extensions for `DataRefManager` to track historical data requests:

```rust
impl DataRefManager {
    // Modify handler method for historical_data_end
    fn historical_data_end(&self, req_id: i32, start_date: &str, end_date: &str) {
        // Existing handler logic...

        // Mark historical data request as complete in rate limiter
        self.historical_data_limiter.mark_completed(req_id);
    }

    // Other similar extensions for request tracking...
}
```

The following completion events will be tracked automatically:

### DataMarketManager
- `historical_data_end`: Marks completion of historical data requests
- `tick_snapshot_end`: Marks completion of market data snapshot requests
- `scanner_data_end`: Marks completion of scanner requests
- `histogram_data`: Marks completion of histogram data requests
- `historical_ticks`, `historical_ticks_bid_ask`, `historical_ticks_last` with `done=true`: Marks completion of historical ticks requests

### DataRefManager
- `contract_details_end`: Marks completion of contract details requests
- `security_definition_option_parameter_end`: Marks completion of option chain parameters requests
- Other specific "end" messages for various data types

Streaming data requests like real-time bars, continuous market data, and market depth typically run until explicitly cancelled, so they will remain counted against the limit until cancellation.

## Implementation Plan

1. **Phase 1: Core Rate Limiter Classes**
   - Implement `TokenBucketRateLimiter` and `CounterRateLimiter`
   - Add unit tests to verify behavior
   - Develop tracking system for request IDs and completion

2. **Phase 2: MessageBroker Integration**
   - Add rate limiter to MessageBroker
   - Modify send_message to respect rate limiting
   - Add configuration and status methods

3. **Phase 3: Manager Integration**
   - Add counter rate limiters to DataMarketManager and DataRefManager
   - Modify request methods to use rate limiters
   - Identify and hook into all "end" message handlers to track request completion:
     - `tick_snapshot_end`, `historical_data_end`, `contract_details_end`, etc.
   - Add configuration and status methods

4. **Phase 4: Client Interface**
   - Add rate limiter configuration methods to IBKRClient
   - Implement status gathering and reporting
   - Add comprehensive documentation

5. **Phase 5: Testing and Refinement**
   - Test with actual IBKR API
   - Verify automatic tracking of request completion
   - Add telemetry and visualizations
   - Refine based on real-world usage
   - Implement periodic cleanup for safety (handling requests that never complete)

## Extensions for Future Consideration

1. **Queue-based Rate Limiting**: Instead of blocking or rejecting, queue messages/requests and process them as limits allow
2. **Priority-based Rate Limiting**: Allow certain request types to have higher priority (e.g., order operations over data requests)
3. **Adaptive Rate Limiting**: Adjust limits based on observed IBKR behavior or error responses
4. **Rate Limit Estimation**: Automatically detect IBKR's current limits based on error responses (especially for market data lines)
5. **Automatic Cleanup**: Periodically check for and clean up leaked requests that never received completion messages
6. **Circuit Breaker**: Temporarily disable requests when seeing a pattern of errors from IBKR
7. **Adaptive Backoff**: When rate-limited, implement exponential backoff strategies
8. **Request Scheduling**: Schedule non-urgent requests for low-traffic periods

## Overview

IBKR imposes various rate limits on API requests. To properly track and respect these limits, we need to know when a request has completed so we can release the used "slot" from the rate limiter counter.

The majority of TWS API requests have explicit completion indicators (usually named with "End" suffix), but some requests either:
- Return a single response message without an explicit "end" indicator
- Continue streaming indefinitely until cancelled
- Complete after a specific condition is met

## Explicit End Messages

These messages explicitly signal the completion of a specific request:

| Message ID | Message Name | Handler Method | Description |
|------------|--------------|----------------|-------------|
| 52 | Contract Data End | `contract_details_end` | Marks completion of contract details requests |
| 57 | Tick Snapshot End | `tick_snapshot_end` | Marks completion of market data snapshot requests |
| 17* | Historical Data | `historical_data_end` | Historical data requests include an end flag in the message |
| 20* | Scanner Data | `scanner_data_end` | Scanner data includes an end flag in final message |
| 76 | Security Definition Option Parameter End | `security_definition_option_parameter_end` | Marks completion of option chain parameters request |
| 87 | Historical News End | `historical_news_end` | Marks completion of historical news requests |
| 96* | Historical Ticks | `historical_ticks` with done=true | Marks completion of historical midpoint ticks |
| 97* | Historical Ticks Bid Ask | `historical_ticks_bid_ask` with done=true | Marks completion of historical bid/ask ticks |
| 98* | Historical Ticks Last | `historical_ticks_last` with done=true | Marks completion of historical trade ticks |
| 62 | Position End | `position_end` | Marks completion of position requests |
| 64 | Account Summary End | `account_summary_end` | Marks completion of account summary requests |
| 53 | Open Order End | `open_order_end` | Marks completion of open orders retrieval |
| 55 | Execution Data End | `execution_data_end` | Marks completion of execution history requests |
| 72 | Position Multi End | `position_multi_end` | Marks completion of multi-account position requests |
| 74 | Account Update Multi End | `account_update_multi_end` | Marks completion of multi-account updates |
| 54 | Account Download End | `account_download_end` | Marks completion of account data download |
| 103 | Replace FA End | `replace_fa_end` | Marks completion of FA configuration replacement |
| 102 | Completed Orders End | `completed_orders_end` | Marks completion of completed orders request |

\* These messages include the completion status as a parameter in the message itself, not as a separate message type.

## One-Time Response Messages

These messages represent one-time responses without explicit "end" messages. Receipt of the message indicates the request is complete:

| Message ID | Message Name | Handler Method | Description |
|------------|--------------|----------------|-------------|
| 89 | Histogram Data | `histogram_data` | Complete response to histogram data request |
| 106 | Historical Schedule | `historical_schedule` | Complete response to historical schedule request |
| 77 | Soft Dollar Tiers | `soft_dollar_tiers` | Complete response to soft dollar tiers request |
| 78 | Family Codes | `family_codes` | Complete response to family codes request |
| 79 | Symbol Samples | `symbol_samples` | Complete response to matching symbols request |
| 80 | Market Depth Exchanges | `mkt_depth_exchanges` | Complete response to market depth exchanges request |
| 82 | Smart Components | `smart_components` | Complete response to smart components request |
| 93 | Market Rule | `market_rule` | Complete response to market rule request |
| 19 | Scanner Parameters | `scanner_parameters` | Complete response to scanner parameters request |
| 85 | News Providers | `news_providers` | Complete response to news providers request |
| 104 | WSH Meta Data | `wsh_meta_data` | Complete response to WSH metadata request |
| 88 | Head Timestamp | `head_timestamp` | Complete response to head timestamp request |
| 83 | News Article | `news_article` | Complete response to news article request |
| 21 | Option Computation | `tick_option_computation` | For option calculation requests |

## Special Cases

### WSH Event Data (105)

The `wsh_event_data` message (ID 105) should be marked as complete when:
- The request had a `total_limit` parameter and that number of events has been received
- After a reasonable timeout (IBKR documentation doesn't specify; 30-60 seconds is typical)

### Current Time (49)

The `current_time` message (ID 49) is a single response and should be treated as immediately completed when received.

### Verify/Authentication Messages

Messages related to verification and authentication (65, 66, 69, 70) are single responses or pairs that should be tracked individually.

## Streaming Requests Without End Messages

These requests start streams that continue until explicitly cancelled and should be tracked from request until cancellation:

| Request Type | Start Method | Cancel Method | Notes |
|--------------|--------------|---------------|-------|
| Market Data (continuous) | `request_market_data` | `cancel_market_data` | Non-snapshot market data |
| Real-Time Bars | `request_real_time_bars` | `cancel_real_time_bars` | 5-second bars |
| Market Depth | `request_market_depth` | `cancel_market_depth` | Level II book data |
| News Bulletins | `request_news_bulletins` | `cancel_news_bulletins` | System-wide news |
| Tick-By-Tick Data | `request_tick_by_tick_data` | `cancel_tick_by_tick_data` | When not historical (number_of_ticks=0) |
| PnL Updates | `request_pnl` | `cancel_pnl` | Account P&L |
| PnL Single Updates | `request_pnl_single` | `cancel_pnl_single` | Position P&L |

## Error Handling

When an error occurs (message ID 4), the rate limiter should:
- Check if the error is associated with a specific request ID
- If so, mark that request as completed to free up the rate limit slot
- Some errors (like "historical data request pacing violation") might require special handling

## Implementation Guidelines

When implementing rate limiting:

1. **Request Registration**:
   - Register each request in the rate limiter when it's initiated
   - Store the request ID, type, and start time

2. **Completion Tracking**:
   - Mark requests as completed when:
     - An explicit end message is received
     - A one-time response message is received
     - The request is explicitly cancelled
     - An error specific to that request is received
