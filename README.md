# YATWS (Yet Another TWS API)

A comprehensive, thread-safe, type-safe, and ergonomic Rust interface to the Interactive Brokers TWS API.

## Overview

YATWS provides a modern Rust implementation for interacting with Interactive Brokers' Trader Workstation (TWS) and Gateway. The library's manager-based architecture and support for both synchronous and asynchronous patterns make it suitable for various trading applications, from simple data retrieval to complex automated trading systems.

This library was born out of the need to place orders in rapid succession in response to market events, which is not easily accomplished with existing Rust crates. While order management was the primary focus, other interfaces (market data, account information, etc.) have been implemented for completeness.

**Current Status**: Early stage but production-ready. The API has been used to trade millions of dollars in volume.

## Features

- **Comprehensive API Coverage**: Access to orders, accounts, market data, fundamentals, news, and reference data
- **Multiple Programming Patterns**:
  - Synchronous blocking calls with timeouts
  - Asynchronous request/cancel pattern
  - Observer pattern for event handling
- **Options Strategy Builder**: Simplified creation of common options strategies
- **Strong Type Safety**: Leverages Rust's type system for safer API interactions
- **Session Recording/Replay**: Record TWS interactions for testing and debugging
- **Domain-Specific Managers**: Organized access to different API functionalities

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
yatws = "0.1.0"  # Replace with actual version
```

## Architecture

YATWS follows a client-manager architecture:

1. **Core Client (`IBKRClient`)**: Central connection point to TWS/Gateway
2. **Specialized Managers**: Domain-specific functionality organized into managers:
   - `OrderManager`: Order placement, modification, and tracking
   - `AccountManager`: Account and portfolio data
   - `DataMarketManager`: Real-time and historical market data
   - `DataRefManager`: Reference data (contract details, etc.)
   - `DataNewsManager`: News headlines and articles
   - `DataFundamentalsManager`: Financial data

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
let equity = client.account().get_equity()?;
let buying_power = client.account().get_buying_power()?;
let positions = client.account().list_open_positions()?;

// Get today's executions
let executions = client.account().get_day_executions()?;
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

### Market Data

```rust
// Get a simple quote
let (bid, ask, last) = client.data_market().get_quote(
    &contract,
    None,
    Duration::from_secs(5)
)?;

// Get historical data
let bars = client.data_market().get_historical_data(
    &contract,
    None,                 // End time (None = now)
    "1 D",                // Duration string
    "1 hour",             // Bar size
    "TRADES",             // What to show
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

### Asynchronous Event Handling

```rust
// Implement the observer trait
struct MyOrderObserver;

impl OrderObserver for MyOrderObserver {
    fn on_order_update(&self, order: &Order) {
        println!("Order update: {:?}", order);
    }

    fn on_order_error(&self, order_id: &str, error: &IBKRError) {
        println!("Order error: {} - {:?}", order_id, error);
    }
}

// Register the observer
let observer = Box::new(MyOrderObserver);
client.orders().add_observer(observer);
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

## Error Handling

YATWS uses Rust's Result pattern consistently, with a custom `IBKRError` type:

```rust
match client.account().get_equity() {
    Ok(equity) => println!("Account equity: ${}", equity),
    Err(IBKRError::Timeout) => println!("Operation timed out"),
    Err(IBKRError::ApiError(code, msg)) => println!("API error {}: {}", code, msg),
    Err(e) => println!("Other error: {:?}", e),
}
```

## API Reference

For detailed information about the available functions, structs, and traits, please refer to the [API documentation](https://docs.rs/yatws). The reference is availalbe [on github](https://github.com/drpngx/yatws/blob/main/yatws/doc/api.md).

Key components include:

- `IBKRClient`: Primary client for interacting with the TWS API
- `OrderManager`: Order-related operations
- `AccountManager`: Account and portfolio data
- `DataMarketManager`: Market data operations
- `DataRefManager`: Reference data
- `DataNewsManager`: News data
- `DataFundamentalsManager`: Financial data
- `OrderBuilder`: Fluent API for creating orders
- `OptionsStrategyBuilder`: Factory for common options strategies
