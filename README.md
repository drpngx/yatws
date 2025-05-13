# YATWS (Yet Another TWS API)

A comprehensive, thread-safe, type-safe, and ergonomic Rust interface to the Interactive Brokers TWS API.

## Overview

YATWS provides a modern Rust implementation for interacting with Interactive Brokers' Trader Workstation (TWS) and Gateway. The library's manager-based architecture and support for both synchronous and asynchronous patterns make it suitable for various trading applications, from simple data retrieval to complex automated trading systems.

This library was born out of the need to place orders in rapid succession in response to market events, which is not easily accomplished with existing Rust crates. It takes about 3ms to place an order with this library. While order management was the primary focus, other interfaces (market data, account information, etc.) have been implemented for completeness.

**Current Status**: Early stage but production-ready. The API has been used to trade millions of dollars in volume.

## Features

- **Comprehensive API Coverage**: Access to orders, accounts, market data, fundamentals, news, and reference data
- **Multiple Programming Patterns**:
  - Synchronous blocking calls with timeouts
  - Asynchronous observer pattern
  - Subscription model
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
- `AccountManager`: Account and portfolio data (including liquidation warning status)
- `DataMarketManager`: Market data operations
- `DataRefManager`: Reference data
- `DataNewsManager`: News data
- `DataFundamentalsManager`: Financial data
- `FinancialAdvisorManager`: Financial Advisor configurations
- `OrderBuilder`: Fluent API for creating orders
- `OptionsStrategyBuilder`: Factory for common options strategies
