# Market Data Subscription Implementation

This document outlines the implementation of a subscription-based pattern for market data in YATWS. It enhances the existing observer-based approach with an iterator-based interface, automatic resource management, and improved ergonomics.

## Overview

The subscription pattern provides three key advantages over the existing API:

1. **Initial blocking fetch** - Data is immediately available after a subscription is created
2. **Iterator interface** - Events can be consumed using Rust's iterator pattern
3. **Automatic cleanup** - Subscriptions are cancelled when dropped (RAII)

Additionally, the design supports combining multiple subscriptions for processing events from different sources in a unified way.

## Core Design

### Base Traits

```rust
/// Trait for all market data subscription types
pub trait MarketDataSubscription: Send + Sync + 'static {
    /// The type of update event this subscription produces
    type Event;

    /// Returns the request ID for this subscription
    fn request_id(&self) -> i32;

    /// Returns a reference to the contract for this subscription
    fn contract(&self) -> &Contract;

    /// Checks if the subscription has completed (for one-time requests)
    fn is_completed(&self) -> bool;

    /// Checks if the subscription has encountered an error
    fn has_error(&self) -> bool;

    /// Get the error if any
    fn get_error(&self) -> Option<IBKRError>;

    /// Cancel this subscription
    fn cancel(&self) -> Result<(), IBKRError>;
}

/// Trait for iterators that produce market data events
pub trait MarketDataIterator<T>: Iterator<Item = T> {
    /// Set a timeout for the next call to `next()`
    fn with_timeout(self, timeout: Duration) -> Self;

    /// Try to get the next item with a timeout
    fn try_next(&mut self, timeout: Duration) -> Option<T>;

    /// Returns all currently available items without waiting
    fn collect_available(&mut self) -> Vec<T>;
}
```

### Subscription State

```rust
/// Internal state for all subscription types
struct SubscriptionState<E> {
    // Request parameters and metadata
    req_id: i32,
    contract: Contract,
    completed: AtomicBool,
    active: AtomicBool,
    error: Mutex<Option<IBKRError>>,

    // Event queue for iterator consumption
    events: Mutex<VecDeque<E>>,
    cond: Condvar,
}
```

## Subscription Types

Each subscription type will follow the same pattern but with type-specific events and configuration options:

### TickDataSubscription

```rust
/// Market data tick subscription
pub struct TickDataSubscription {
    manager: Weak<DataMarketManager>,
    state: Arc<SubscriptionState<TickDataEvent>>,
    observer_id: Mutex<Option<ObserverId>>,
    params: Mutex<TickDataParams>,
}

/// Events produced by a TickDataSubscription
#[derive(Debug, Clone)]
pub enum TickDataEvent {
    Price(TickType, f64, TickAttrib),
    Size(TickType, f64),
    String(TickType, String),
    Generic(TickType, f64),
    SnapshotEnd,
}
```

### RealTimeBarSubscription

```rust
/// Real-time bar subscription
pub struct RealTimeBarSubscription {
    manager: Weak<DataMarketManager>,
    state: Arc<SubscriptionState<Bar>>,
    observer_id: Mutex<Option<ObserverId>>,
    params: Mutex<RealTimeBarParams>,
}
```

### TickByTickSubscription

```rust
/// Tick-by-tick data subscription
pub struct TickByTickSubscription {
    manager: Weak<DataMarketManager>,
    state: Arc<SubscriptionState<TickByTickEvent>>,
    observer_id: Mutex<Option<ObserverId>>,
    params: Mutex<TickByTickParams>,
}

/// Events produced by a TickByTickSubscription
#[derive(Debug, Clone)]
pub enum TickByTickEvent {
    LastTrade {
        time: i64,
        price: f64,
        size: f64,
        exchange: String,
        conditions: String,
    },
    BidAsk {
        time: i64,
        bid_price: f64,
        ask_price: f64,
        bid_size: f64,
        ask_size: f64,
    },
    MidPoint {
        time: i64,
        price: f64,
    },
}
```

### MarketDepthSubscription

```rust
/// Market depth subscription
pub struct MarketDepthSubscription {
    manager: Weak<DataMarketManager>,
    state: Arc<SubscriptionState<MarketDepthEvent>>,
    observer_id: Mutex<Option<ObserverId>>,
    params: Mutex<MarketDepthParams>,
}

/// Events produced by a MarketDepthSubscription
#[derive(Debug, Clone)]
pub enum MarketDepthEvent {
    Update {
        position: i32,
        operation: i32,
        side: i32,
        price: f64,
        size: f64,
    },
    UpdateL2 {
        position: i32,
        market_maker: String,
        operation: i32,
        side: i32,
        price: f64,
        size: f64,
        is_smart_depth: bool,
    },
}
```

### HistoricalDataSubscription

```rust
/// Historical data subscription
pub struct HistoricalDataSubscription {
    manager: Weak<DataMarketManager>,
    state: Arc<SubscriptionState<HistoricalDataEvent>>,
    observer_id: Mutex<Option<ObserverId>>,
    params: Mutex<HistoricalDataParams>,
}

/// Events produced by a HistoricalDataSubscription
#[derive(Debug, Clone)]
pub enum HistoricalDataEvent {
    Bar(Bar),
    UpdateBar(Bar),
    Complete { start_date: String, end_date: String },
}
```

## Key Implementation Details

### Factory Methods in DataMarketManager

```rust
impl DataMarketManager {
    /// Create a market data tick subscription
    pub fn subscribe_market_data(&self, contract: Contract) -> TickDataSubscription {
        TickDataSubscription::new(Arc::downgrade(&Arc::clone(self)), contract)
    }

    /// Create a real-time bars subscription
    pub fn subscribe_real_time_bars(&self, contract: Contract, what_to_show: WhatToShow) -> RealTimeBarSubscription {
        RealTimeBarSubscription::new(Arc::downgrade(&Arc::clone(self)), contract, what_to_show)
    }

    /// Create a tick-by-tick data subscription
    pub fn subscribe_tick_by_tick(&self, contract: Contract, tick_type: TickByTickRequestType) -> TickByTickSubscription {
        TickByTickSubscription::new(Arc::downgrade(&Arc::clone(self)), contract, tick_type)
    }

    /// Create a market depth subscription
    pub fn subscribe_market_depth(&self, contract: Contract, num_rows: i32) -> MarketDepthSubscription {
        MarketDepthSubscription::new(Arc::downgrade(&Arc::clone(self)), contract, num_rows)
    }

    /// Create a historical data subscription
    pub fn subscribe_historical_data(&self, contract: Contract, duration: DurationUnit,
                                    bar_size: BarSize, what_to_show: WhatToShow) -> HistoricalDataSubscription {
        HistoricalDataSubscription::new(
            Arc::downgrade(&Arc::clone(self)),
            contract,
            duration,
            bar_size,
            what_to_show
        )
    }
}
```

### Observer Integration

Each subscription type will internally create an observer to handle TWS events:

```rust
/// Internal observer implementation for TickDataSubscription
struct TickDataObserver {
    state: Arc<SubscriptionState<TickDataEvent>>,
}

impl MarketDataObserver for TickDataObserver {
    fn on_tick_price(&self, req_id: i32, tick_type: TickType, price: f64, attrib: TickAttrib) {
        if req_id == self.state.req_id {
            self.state.push_event(TickDataEvent::Price(tick_type, price, attrib));
        }
    }

    // Other methods...
}
```

### Automatic Cleanup

```rust
/// Auto-cancel the subscription when dropped
impl Drop for TickDataSubscription {
    fn drop(&mut self) {
        // Only attempt to cancel if active
        if self.state.active.load(Ordering::SeqCst) {
            let _ = self.cancel(); // Ignore errors during drop
        }
    }
}
```

### Iterator Implementation

```rust
/// Iterator for TickDataEvents
pub struct TickDataIterator {
    state: Arc<SubscriptionState<TickDataEvent>>,
    timeout: Option<Duration>,
}

impl Iterator for TickDataIterator {
    type Item = TickDataEvent;

    fn next(&mut self) -> Option<Self::Item> {
        // If a timeout is set, use it, otherwise wait indefinitely
        if let Some(timeout) = self.timeout {
            self.try_next(timeout)
        } else {
            // Default to a long timeout
            self.try_next(Duration::from_secs(3600)) // 1 hour as "indefinite"
        }
    }
}

impl MarketDataIterator<TickDataEvent> for TickDataIterator {
    fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    fn try_next(&mut self, timeout: Duration) -> Option<TickDataEvent> {
        // Implementation...
    }

    fn collect_available(&mut self) -> Vec<TickDataEvent> {
        // Implementation...
    }
}
```

## Multiple Subscription Support

### TaggedEvent Structure

```rust
/// A tagged market data event with source information
pub struct TaggedEvent<T> {
    /// The source subscription ID
    pub req_id: i32,
    /// The source contract
    pub contract: Contract,
    /// The actual event
    pub event: T,
}
```

### MultiSubscriptionIterator

```rust
/// An iterator that multiplexes events from multiple market data subscriptions
pub struct MultiSubscriptionIterator<T> {
    iterators: Vec<(i32, Contract, Box<dyn MarketDataIterator<T>>)>,
    timeout: Option<Duration>,
}

impl<T> MultiSubscriptionIterator<T> {
    /// Create a new iterator that multiplexes events from multiple sources
    pub fn new() -> Self {
        Self {
            iterators: Vec::new(),
            timeout: None,
        }
    }

    /// Add an iterator with source information to this multiplexer
    pub fn add(
        mut self,
        req_id: i32,
        contract: Contract,
        iterator: impl MarketDataIterator<T> + 'static
    ) -> Self {
        self.iterators.push((req_id, contract, Box::new(iterator)));
        self
    }

    /// Set a timeout for the next call to `next()`
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Try to get the next event with a timeout
    pub fn try_next(&mut self, timeout: Duration) -> Option<TaggedEvent<T>> {
        // Implementation...
    }

    /// Cancel all underlying subscriptions
    pub fn cancel_all(self) -> Result<(), IBKRError> {
        // Implementation...
    }
}

impl<T> Iterator for MultiSubscriptionIterator<T> {
    type Item = TaggedEvent<T>;

    fn next(&mut self) -> Option<Self::Item> {
        // Implementation...
    }
}
```

### Factory Methods for Multiple Subscriptions

```rust
impl DataMarketManager {
    /// Create a multiplexed iterator from multiple market data subscriptions
    pub fn combine_subscriptions<T>(&self) -> MultiSubscriptionBuilder<T> {
        MultiSubscriptionBuilder::new()
    }
}

/// Builder for creating MultiSubscriptionIterator
pub struct MultiSubscriptionBuilder<T> {
    iterators: Vec<(i32, Contract, Box<dyn MarketDataIterator<T>>)>,
}

impl<T> MultiSubscriptionBuilder<T> {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            iterators: Vec::new(),
        }
    }

    /// Add a market data subscription and its iterator
    pub fn add<S: MarketDataSubscription<Event = T>>(
        mut self,
        subscription: &S,
        iterator: impl MarketDataIterator<T> + 'static
    ) -> Self {
        self.iterators.push(
            (subscription.request_id(),
             subscription.contract().clone(),
             Box::new(iterator))
        );
        self
    }

    /// Build the MultiSubscriptionIterator
    pub fn build(self) -> MultiSubscriptionIterator<T> {
        MultiSubscriptionIterator {
            iterators: self.iterators,
            timeout: None,
        }
    }
}
```

## Example Usage

### Basic Subscription

```rust
fn main() -> Result<(), IBKRError> {
    let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
    let market_data_mgr = client.data_market();

    // Create and submit a market data subscription
    let aapl_subscription = market_data_mgr.subscribe_market_data(Contract::stock("AAPL"))
        .with_delayed()
        .submit(Duration::from_secs(5))?;

    // Print the current prices
    println!("AAPL Initial Prices:");
    println!("  Bid: {:?}", aapl_subscription.bid_price());
    println!("  Ask: {:?}", aapl_subscription.ask_price());
    println!("  Last: {:?}", aapl_subscription.last_price());

    // Process 10 market data events with a timeout
    println!("\nWaiting for market data events:");
    let mut events = aapl_subscription.events().with_timeout(Duration::from_secs(2));
    for _ in 0..10 {
        match events.next() {
            Some(TickDataEvent::Price(tick_type, price, _)) => {
                println!("  Price update: {:?} = {}", tick_type, price);
            }
            Some(TickDataEvent::Size(tick_type, size)) => {
                println!("  Size update: {:?} = {}", tick_type, size);
            }
            Some(event) => {
                println!("  Other event: {:?}", event);
            }
            None => {
                println!("  No event within timeout");
            }
        }
    }

    // Subscription is automatically cancelled when dropped
    Ok(())
}
```

### Multiple Subscriptions

```rust
fn main() -> Result<(), IBKRError> {
    let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
    let market_data_mgr = client.data_market();

    // Create tick data subscription for AAPL
    let aapl_subscription = market_data_mgr.subscribe_market_data(Contract::stock("AAPL"))
        .with_delayed()
        .submit(Duration::from_secs(5))?;

    // Create tick data subscription for MSFT
    let msft_subscription = market_data_mgr.subscribe_market_data(Contract::stock("MSFT"))
        .with_delayed()
        .submit(Duration::from_secs(5))?;

    // Combine tick events from AAPL and MSFT
    let mut combined_ticks = market_data_mgr.combine_subscriptions::<TickDataEvent>()
        .add(&aapl_subscription, aapl_subscription.events())
        .add(&msft_subscription, msft_subscription.events())
        .build()
        .with_timeout(Duration::from_secs(30));

    // Process combined events
    println!("Waiting for tick events from either AAPL or MSFT...");
    for _ in 0..10 {
        // Try to get the next event with a 5-second timeout
        if let Some(tagged_event) = combined_ticks.try_next(Duration::from_secs(5)) {
            let symbol = &tagged_event.contract.symbol;

            match tagged_event.event {
                TickDataEvent::Price(tick_type, price, _) => {
                    println!("{} price update: {:?} = {}", symbol, tick_type, price);
                }
                TickDataEvent::Size(tick_type, size) => {
                    println!("{} size update: {:?} = {}", symbol, tick_type, size);
                }
                // Handle other event types...
                _ => println!("{} other tick event", symbol),
            }
        } else {
            println!("No events received for 5 seconds");
        }
    }

    // Subscriptions are automatically cancelled when dropped
    Ok(())
}
```

### Historical Data

```rust
fn main() -> Result<(), IBKRError> {
    let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
    let market_data_mgr = client.data_market();

    // Create historical data subscription for AAPL
    let aapl_history = market_data_mgr.subscribe_historical_data(
            Contract::stock("AAPL"),
            DurationUnit::Day(5),
            BarSize::Hour1,
            WhatToShow::Trades
        )
        .with_delayed()
        .submit(Duration::from_secs(10))?;

    // Process historical data events
    println!("AAPL Historical Data:");
    let mut events = aapl_history.events();
    while let Some(event) = events.next() {
        match event {
            HistoricalDataEvent::Bar(bar) => {
                println!("  Bar: Time={}, Close={}", bar.time, bar.close);
            }
            HistoricalDataEvent::Complete { start_date, end_date } => {
                println!("  Historical data complete: {} to {}", start_date, end_date);
                break;
            }
            _ => {}
        }
    }

    // Historical data subscription is completed at this point
    println!("Is completed: {}", aapl_history.is_completed());

    Ok(())
}
```

## Implementation Strategy

The implementation should be rolled out in phases:

1. **Phase 1: Core Infrastructure**
   - Basic traits (`MarketDataSubscription`, `MarketDataIterator`)
   - Subscription state and event queue management
   - Base iterator implementation

2. **Phase 2: TickDataSubscription**
   - First complete subscription type
   - Observer integration
   - Factory methods

3. **Phase 3: Additional Subscription Types**
   - RealTimeBarSubscription
   - TickByTickSubscription
   - MarketDepthSubscription
   - HistoricalDataSubscription
   - ScannerSubscription
   - HistogramSubscription

4. **Phase 4: Multiple Subscription Support**
   - TaggedEvent structure
   - MultiSubscriptionIterator
   - Combination factory methods

5. **Phase 5: Documentation and Testing**
   - Comprehensive docs
   - Usage examples
   - Integration tests

## Benefits Over Current Design

1. **More Intuitive API** - Events can be consumed through iterators
2. **Initial Blocking Fetch** - Data is immediately available after subscription
3. **No Callback Hell** - Avoids complex observer registration and callback management
4. **Flexible Consumption Patterns** - Supports both polling and streaming models
5. **Resource Cleanup** - Automatic cancellation when subscriptions go out of scope
6. **Type Safety** - Each subscription type has its own specialized interface
7. **Easy Composition** - Multiple subscriptions can be combined in a unified way

## Backward Compatibility

The existing methods (`request_*` and `get_*`) will remain for backward compatibility. Their implementation will remain unchanged.
