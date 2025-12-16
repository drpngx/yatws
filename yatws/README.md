# YATWS (Yet Another TWS API)

A comprehensive, thread-safe, type-safe, and ergonomic Rust interface to the Interactive Brokers TWS API.

## Overview

YATWS provides a modern Rust implementation for interacting with Interactive Brokers' Trader Workstation (TWS) and Gateway. The library's manager-based architecture and support for both synchronous and asynchronous patterns make it suitable for various trading applications, from simple data retrieval to complex automated trading systems.

This library was born out of the need to place orders in rapid succession in response to market events, which is not easily accomplished with existing Rust crates. It takes about 3ms to place an order with this library. While order management was the primary focus, other interfaces (market data, account information, etc.) have been implemented for completeness.

**Current Status**: Early stage but US equity trading is production-ready. The API has been used to trade 9 figures of dollars in volume.

## Features

- **Comprehensive API Coverage**: Access to orders, accounts, market data, fundamentals, news, and reference data
- **Book-keeping**: Keeps the portfolio with PNL and order book up-to-date
- **Multiple Programming Patterns**:
  - Synchronous blocking calls with timeouts
  - Asynchronous observer pattern
  - Subscription model
- **Options Strategy Builder**: Simplified creation of common options strategies
- **Strong Type Safety**: Leverages Rust's type system for safer API interactions
- **Domain-Specific Managers**: Organized access to different API functionalities
- **Rate limits**: pace the requests in accordance with IBKR rules
- **Session Recording/Replay**: Record TWS interactions for testing and debugging

## Architecture

YATWS follows a client-manager architecture:

1. **Core Client (`IBKRClient`)**: Central connection point to TWS/Gateway
2. **Specialized Managers**: Domain-specific functionality organized into managers:
   - `OrderManager`: Order placement, modification, and tracking
   - `AccountManager`: Account and portfolio data (summary, positions, P&L, executions, liquidation warnings)
   - `DataMarketManager`: Real-time and historical market data
   - `DataRefManager`: Reference data (contract details, etc.)
   - `DataNewsManager`: News headlines and articles
   - `DataFundamentalsManager`: Financial data
   - `FinancialAdvisorManager`: Financial Advisor configurations (groups, profiles, aliases)

## Usage Examples

### Establishing a Connection

```rust
// Connect to TWS/Gateway
let client = IBKRClient::new("127.0.0.1", 7497, 0, None)?;

// Alternative: Record session to SQLite database
let client = IBKRClient::new(
    "127.0.0.1",
    7497,
    0,
    Some(("sessions.db", "my_trading_session"))
)?;

// Replay a recorded session
let replay_client = IBKRClient::from_db("sessions.db", "my_trading_session")?;
```

### Account Information

```rust
// Subscribe to account updates
client.account().subscribe_account_updates()?;

// Get account summary
let info = client.account().get_account_info()?;
println!("Account Net Liq: {}", info.net_liquidation);

// Get a single specific value.
use yatws::account::AccountValueKey;
let buying_power_value = client.account().get_account_value(AccountValueKey::BuyingPower)?;
if let Some(bp) = buying_power_value {
    println!("Buying Power: {} {}", bp.value, bp.currency.unwrap_or_default());
}

// List positions
let positions = client.account().list_open_positions()?;

// Get today's executions
let executions = client.account().get_day_executions()?;

// Check for pre-liquidation warning
if client.account().has_received_pre_liquidation_warning() {
    println!("Warning: Pre-liquidation warning received this session.");
}
```

### Order Management

```rust
// Create and place an order using OrderBuilder
let (contract, order_request) = OrderBuilder::new(OrderSide::Buy, 100.0)
    .for_stock("AAPL")
    .with_exchange("SMART")
    .with_currency("USD")
    .limit(150.0)
    .with_tif(TimeInForce::Day)
    .build()?;

let order_id = client.orders().place_order(contract, order_request)?;

// Wait for order to be filled (with timeout)
let status = client.orders().try_wait_order_executed(
    &order_id,
    Duration::from_secs(30)
)?;
```

### Order Subscription Pattern

For tracking order lifecycle with real-time updates:

```rust
use yatws::{OrderBuilder, OrderSide, TimeInForce};
use yatws::order_manager::OrderEvent;
use std::time::Duration;

// Create order specification
let (contract, order_request) = OrderBuilder::new(OrderSide::Buy, 100.0)
    .for_stock("AAPL")
    .market()
    .with_tif(TimeInForce::Day)
    .build()?;

// Subscribe to order lifecycle events
let order_subscription = client.orders().subscribe_new_order(contract, order_request)?;
let mut order_events = order_subscription.events();

// Process order events until completion or error
while let Some(event) = order_events.try_next(Duration::from_secs(1)) {
    match event {
        OrderEvent::Update(order) => {
            println!("Order {} status: {:?}, filled: {}",
                     order.id, order.state.status, order.state.filled_quantity);

            if order.state.status.is_terminal() {
                println!("Order completed with status: {:?}", order.state.status);
                break;
            }
        }
        OrderEvent::Error(error) => {
            eprintln!("Order error: {:?}", error);
            break;
        }
    }
}
```

### Market Data

```rust
// Get a simple quote
let (bid, ask, last) = client.data_market().get_quote(
    &contract,
    None,
    Duration::from_secs(5)
)?;

// Get historical data
use yatws::data::DurationUnit;
let bars = client.data_market().get_historical_data(
    &contract,
    None,                 // End time (None = now)
    DurationUnit::Day(1),
    yatws::contract::BarSize::Hour1,
    yatws::contract::WhatToShow::Trades,
    true,                 // Use RTH
    1,                    // Date format
    false,                // Keep up to date?
    None,                 // Market data type
    &[]                   // Chart options
)?;
```

### Options Strategies

```rust
// Create a bull call spread
let builder = OptionsStrategyBuilder::new(
    client.data_ref(),
    "AAPL",
    150.0,     // Current price
    10.0,      // Quantity (10 spreads)
    SecType::Stock
)?;

let (contract, order) = builder
    .bull_call_spread(
        NaiveDate::from_ymd_opt(2025, 12, 19).unwrap(),
        150.0,  // Lower strike
        160.0   // Higher strike
    )?
    .with_limit_price(3.50)  // Debit of $3.50 per spread
    .build()?;

let order_id = client.orders().place_order(contract, order)?;
```

### Financial Advisor Configuration

```rust
// Access the FinancialAdvisorManager
let fa_manager = client.financial_advisor();

// Request FA Groups data
fa_manager.request_fa_data(yatws::FADataType::Groups)?;

// Get the current FA configuration
let fa_config = fa_manager.get_config();
println!("FA Groups: {:?}", fa_config.groups);
```

## Asynchronous Event Handling

YATWS supports an observer pattern for handling asynchronous events. This is a push programming model where events are pushed to you. You initiate the request and implement `on_event` methods. This is the closest to the official IBKR TWS API, except you have observers split by functionality rather than a single `EWrapper`.

This is best suited for event-driven trading, such as reacting to news, order fills and buying power depletion.

### Market Data Observer Example

```rust
use yatws::{IBKRClient, IBKRError, contract::Contract, data::{MarketDataType, TickType, TickAttrib}, data_observer::MarketDataObserver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug)]
struct MyMarketObserver {
    name: String,
    tick_count: Arc<Mutex<usize>>,
}

impl MarketDataObserver for MyMarketObserver {
    fn on_tick_price(&self, req_id: i32, tick_type: TickType, price: f64, attrib: TickAttrib) {
        println!("[{}] TickPrice: ReqID={}, Type={:?}, Price={}, Attrib={:?}", self.name, req_id, tick_type, price, attrib);
        *self.tick_count.lock().unwrap() += 1;
    }
    // Implement other MarketDataObserver methods as needed (on_tick_size, on_error, etc.)
    fn on_error(&self, req_id: i32, error_code: i32, error_message: &str) {
        eprintln!("[{}] Error: ReqID={}, Code={}, Msg='{}'", self.name, req_id, error_code, error_message);
    }
}

fn main() -> Result<(), IBKRError> {
    let client = IBKRClient::new("127.0.0.1", 7497, 2, None)?;
    let market_mgr = client.data_market();
    let contract = Contract::stock("AAPL");

    let observer = MyMarketObserver {
        name: "AAPL-Observer".to_string(),
        tick_count: Arc::new(Mutex::new(0)),
    };

    // Request streaming data and associate it with the observer
    let (req_id, _observer_id) = market_mgr.request_observe_market_data(
        &contract,
        &[GenericTickType::MiscellaneousStats], // Empty generic_tick_list for default ticks
        false, // snapshot = false for streaming
        false, // regulatory_snapshot
        &[],   // mkt_data_options
        Some(MarketDataType::Delayed), // Or RealTime if subscribed
        observer,
    )?;
    println!("Observing market data for AAPL with ReqID: {}", req_id);

    thread::sleep(Duration::from_secs(15)); // Observe for 15 seconds

    market_mgr.cancel_market_data(req_id)?; // Cancel the stream
    println!("Cancelled market data for AAPL.");

    Ok(())
}
```

## Subscriptions
Simimlar to the `ibapi` crate, you can use a subscription programming model. This is in-between the sync and async models desribed above. This is best suited for receiving a stream of data. This is a pull model where you receive an iterator of events.

### Account Subscription Example

```rust
use yatws::{IBKRClient, IBKRError};
use yatws::account_subscription::{AccountSubscription, AccountEvent};
use std::time::Duration;
use std::thread;

fn main() -> Result<(), IBKRError> {
    let client = IBKRClient::new("127.0.0.1", 7497, 1, None)?; // Use a unique client_id
    let account_manager = client.account();

    let mut account_sub = AccountSubscription::new(account_manager)?;
    println!("AccountSubscription created for account: {}", account_sub.account_id());

    // Example: Print initial summary
    if let Some(summary) = account_sub.last_known_summary() {
        println!("Initial Net Liquidation: {}", summary.net_liquidation);
    }

    // Spawn a thread to listen for events
    let event_receiver = account_sub.events(); // Get a receiver clone
    let event_thread = thread::spawn(move || {
        for event in event_receiver { // Iterates until channel is disconnected
            match event {
                AccountEvent::SummaryUpdate { info, .. } => {
                    println!("[Thread] Account Summary Update: NetLiq = {}", info.net_liquidation);
                }
                AccountEvent::PositionUpdate { position, .. } => {
                    println!("[Thread] Position Update: {} {} @ {}",
                             position.symbol, position.quantity, position.market_price);
                }
                AccountEvent::ExecutionUpdate { execution, .. } => {
                    println!("[Thread] Execution: {} {} {} @ {}",
                             execution.side, execution.quantity, execution.symbol, execution.price);
                }
                AccountEvent::Error { error, .. } => {
                    eprintln!("[Thread] AccountSubscription Error: {:?}", error);
                }
                AccountEvent::Closed { account_id, .. } => {
                    println!("[Thread] AccountSubscription for {} closed.", account_id);
                    break; // Exit loop
                }
            }
        }
    });

    // Let the subscription run for a bit
    thread::sleep(Duration::from_secs(60));

    // Explicitly close the subscription
    println!("Main thread: Closing AccountSubscription...");
    account_sub.close()?; // This will send AccountEvent::Closed and disconnect the channel

    // Wait for the event thread to finish
    event_thread.join().expect("Event thread panicked");
    println!("Main thread: Done.");

    Ok(())
}
```

### News Subscription Example

```rust
use yatws::{IBKRClient, IBKRError, news_subscription::NewsEvent};
use std::time::Duration;
use std::thread;

fn main() -> Result<(), IBKRError> {
    let client = IBKRClient::new("127.0.0.1", 7497, 3, None)?;
    let news_manager = client.data_news();

    // Subscribe to new news bulletins
    let mut news_sub = news_manager.subscribe_news_bulletins_stream(false).submit()?;
    println!("Subscribed to news bulletins (ReqID: {}).", news_sub.request_id());

    let event_receiver = news_sub.events();
    let news_thread = thread::spawn(move || {
        for event in event_receiver {
            match event {
                NewsEvent::Bulletin { article, .. } => {
                    println!("[Thread] News Bulletin: Provider={}, ID={}, Headline='{}'",
                             article.provider_code, article.article_id, article.headline);
                }
                NewsEvent::Error(e) => {
                    eprintln!("[Thread] NewsSubscription Error: {:?}", e);
                }
                NewsEvent::Closed { .. } => {
                    println!("[Thread] NewsSubscription closed.");
                    break;
                }
            }
        }
    });

    thread::sleep(Duration::from_secs(60)); // Listen for news for 60 seconds

    println!("Main thread: Closing NewsSubscription...");
    news_sub.cancel()?; // Explicitly cancel the subscription

    news_thread.join().expect("News thread panicked");
    println!("Main thread: Done with news.");

    Ok(())
}
```

## Advanced Features

### Session Recording and Replay

```rust
// Enable session recording
let client = IBKRClient::new(
    "127.0.0.1",
    7497,
    0,
    Some(("sessions.db", "my_trading_session"))
)?;

// Later, replay the session
let replay_client = IBKRClient::from_db("sessions.db", "my_trading_session")?;
```

### Order Conditions

```rust
// Create a price-conditional order
let (contract, order) = OrderBuilder::new(OrderSide::Buy, 100.0)
    .for_stock("AAPL")
    .limit(150.0)
    .add_price_condition(
        265598,             // SPY con_id
        "ISLAND",           // Exchange
        400.0,              // Price
        TriggerMethod::Last, // Trigger method
        false               // Is less than 400
    )
    .build()?;
```

### Rate Limiting

YATWS provides built-in rate limiting to ensure compliance with Interactive Brokers' API limits:

```rust
// Enable rate limiting with default settings (50 msgs/sec, 50 historical requests, 100 market data lines)
client.enable_rate_limiting()?;

// Configure custom rate limiting settings
let mut config = RateLimiterConfig::default();
config.enabled = true;
config.max_messages_per_second = 40;  // Be more conservative
config.max_historical_requests = 30;  // Lower than default 50
config.rate_limit_wait_timeout = Duration::from_secs(10);  // Longer timeout
client.configure_rate_limiter(config)?;

// Check current rate limiter status
if let Some(status) = client.get_rate_limiter_status() {
    println!("Rate limiting enabled: {}", status.enabled);
    println!("Current message rate: {:.2} msgs/sec", status.current_message_rate);
    println!("Active historical requests: {}/{}",
             status.active_historical_requests,
             config.max_historical_requests);
}

// Disable rate limiting when needed
client.disable_rate_limiting()?;

// Clean up stale requests (useful for long-running applications)
let (hist_cleaned, mkt_cleaned) = client.cleanup_stale_rate_limiter_requests(
    Duration::from_secs(300)  // Clean up requests older than 5 minutes
)?;
println!("Cleaned up {} historical and {} market data requests", hist_cleaned, mkt_cleaned);
```

## Error Handling

YATWS uses Rust's Result pattern consistently, with a custom `IBKRError` type:

```rust
match client.account().get_net_liquidation() {
    Ok(net_liq_value) => println!("Account Net Liquidation Value: ${}", net_liq_value),
    Err(IBKRError::Timeout) => println!("Operation timed out"),
    Err(IBKRError::ApiError(code, msg)) => println!("API error {}: {}", code, msg),
    Err(e) => println!("Other error: {:?}", e),
}
```

## API Reference

For detailed information about the available functions, structs, and traits, please refer to the [API documentation on docs.rs](https://docs.rs/yatws) or the [API reference on GitHub](https://github.com/drpngx/yatws/blob/main/yatws/doc/api.md).

Key components include:

- `IBKRClient`: Primary client for interacting with the TWS API
- `OrderManager`: Order-related operations
- `AccountManager`: Account and portfolio data (including liquidation warning status)
- `DataMarketManager`: Market data operations
- `DataRefManager`: Reference data
- `DataNewsManager`: News data
- `DataFundamentalsManager`: Financial data
- `FinancialAdvisorManager`: Financial Advisor configurations
- `OrderBuilder`: Fluent API for creating orders
- `OptionsStrategyBuilder`: Factory for common options strategies
