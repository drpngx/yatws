# Enhanced Market Data Subscription Design

This document outlines an enhanced design for market data subscriptions in YATWS, making them more similar to the account subscription pattern with an iterator-based approach.

## Overview

Currently, market data requests in `DataMarketManager` follow two patterns:
1. Non-blocking methods (`request_*`) that start a stream and require observer callbacks
2. Blocking methods (`get_*`) that wait for completion conditions

The proposed enhancement adds a third pattern: subscriptions that perform an initial blocking fetch followed by an iterator-based interface for consuming updates.

## Key Design Goals

1. **Initial Blocking Fetch** - Similar to account subscriptions, perform an initial blocking fetch to ensure data is immediately available
2. **Iterator Interface** - Provide an iterator for consuming market data events as they arrive
3. **Flexible Consumption** - Support both polling and callback patterns
4. **Type Safety** - Each subscription type has its own interface
5. **Clean Cancellation** - Subscriptions are automatically cancelled when dropped

## Subscription Base Trait

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
    fn cancel(&self, manager: &DataMarketManager) -> Result<(), IBKRError>;
}
```

## MarketDataIterator Trait

```rust
/// Trait for iterators that produce market data events
pub trait MarketDataIterator<T>: Iterator<Item = T> {
    /// Set a timeout for the next call to `next()`
    fn with_timeout(self, timeout: Duration) -> Self;

    /// Try to get the next item with a timeout
    fn try_next(&mut self, timeout: Duration) -> Option<T>;

    /// Returns all currently available items without waiting
    fn collect_available(&mut self) -> Vec<T>;

    /// Cancel the underlying subscription
    fn cancel(self) -> Result<(), IBKRError>;
}
```

## Subscription Factory Pattern

The factory pattern is implemented in `DataMarketManager` to create subscription objects:

```rust
impl DataMarketManager {
    /// Create a new market data tick subscription
    pub fn subscribe_market_data(&self, contract: Contract) -> TickDataSubscription {
        TickDataSubscription::new(Arc::clone(&self), contract)
    }

    /// Create a new historical data subscription
    pub fn subscribe_historical_data(
        &self,
        contract: Contract,
        duration: DurationUnit,
        bar_size: BarSize,
        what_to_show: WhatToShow
    ) -> HistoricalDataSubscription {
        HistoricalDataSubscription::new(
            Arc::clone(&self),
            contract,
            duration,
            bar_size,
            what_to_show
        )
    }

    /// Create a new real-time bar subscription
    pub fn subscribe_real_time_bars(
        &self,
        contract: Contract,
        what_to_show: WhatToShow
    ) -> RealTimeBarSubscription {
        RealTimeBarSubscription::new(Arc::clone(&self), contract, what_to_show)
    }

    /// Create a new tick-by-tick data subscription
    pub fn subscribe_tick_by_tick_data(
        &self,
        contract: Contract,
        tick_type: TickByTickRequestType
    ) -> TickByTickDataSubscription {
        TickByTickDataSubscription::new(Arc::clone(&self), contract, tick_type)
    }

    /// Create a new market depth subscription
    pub fn subscribe_market_depth(
        &self,
        contract: Contract,
        num_rows: i32
    ) -> MarketDepthSubscription {
        MarketDepthSubscription::new(Arc::clone(&self), contract, num_rows)
    }

    /// Create a new market scanner subscription
    pub fn subscribe_scanner(
        &self,
        scanner_subscription: ScannerSubscription
    ) -> ScannerDataSubscription {
        ScannerDataSubscription::new(Arc::clone(&self), scanner_subscription)
    }

    /// Create a new histogram data subscription
    pub fn subscribe_histogram_data(
        &self,
        contract: Contract,
        time_period: TimePeriodUnit
    ) -> HistogramDataSubscription {
        HistogramDataSubscription::new(Arc::clone(&self), contract, time_period)
    }

    // Internal methods to handle subscription events...
}
```

## Example: TickDataSubscription

```rust
/// A subscription for market data ticks
pub struct TickDataSubscription {
    // Internal state
    state: Arc<Mutex<TickDataSubscriptionState>>,
    // Reference to the manager for cancellation
    manager: Arc<DataMarketManager>,
}

/// Events produced by a TickDataSubscription
pub enum TickDataEvent {
    Price(TickType, f64, TickAttrib),
    Size(TickType, f64),
    String(TickType, String),
    Generic(TickType, f64),
    SnapshotEnd,
}

/// Internal state for TickDataSubscription
struct TickDataSubscriptionState {
    // Request parameters
    req_id: i32,
    contract: Contract,
    generic_tick_list: Vec<GenericTickType>,
    snapshot: bool,
    regulatory_snapshot: bool,
    market_data_type: MarketDataType,

    // Subscription state
    is_active: bool,
    error: Option<IBKRError>,

    // Data storage
    tick_prices: HashMap<TickType, (f64, TickAttrib)>,
    tick_sizes: HashMap<TickType, f64>,

    // Event queue for iterator
    events: VecDeque<TickDataEvent>,

    // Condition variable for waiting
    cond: Condvar,
}

impl TickDataSubscription {
    /// Create a new market data subscription
    pub fn new(manager: Arc<DataMarketManager>, contract: Contract) -> Self {
        let req_id = manager.next_request_id();
        let state = Arc::new(Mutex::new(TickDataSubscriptionState {
            req_id,
            contract,
            generic_tick_list: Vec::new(),
            snapshot: false,
            regulatory_snapshot: false,
            market_data_type: MarketDataType::RealTime,
            is_active: false,
            error: None,
            tick_prices: HashMap::new(),
            tick_sizes: HashMap::new(),
            events: VecDeque::new(),
            cond: Condvar::new(),
        }));

        Self { state, manager }
    }

    /// Get the request ID for this subscription
    pub fn request_id(&self) -> i32 {
        self.state.lock().req_id
    }

    /// Get a reference to the contract for this subscription
    pub fn contract(&self) -> &Contract {
        &self.state.lock().contract
    }

    /// Start this subscription with real-time data
    pub fn with_real_time(self) -> Self {
        self.with_market_data_type(MarketDataType::RealTime)
    }

    /// Start this subscription with delayed data
    pub fn with_delayed(self) -> Self {
        self.with_market_data_type(MarketDataType::Delayed)
    }

    /// Start this subscription with frozen data
    pub fn with_frozen(self) -> Self {
        self.with_market_data_type(MarketDataType::Frozen)
    }

    /// Start this subscription with the specified market data type
    pub fn with_market_data_type(mut self, mkt_data_type: MarketDataType) -> Self {
        let mut state = self.state.lock();
        state.market_data_type = mkt_data_type;
        drop(state);
        self
    }

    /// Request specific generic tick types
    pub fn with_generic_ticks(mut self, generic_ticks: &[GenericTickType]) -> Self {
        let mut state = self.state.lock();
        state.generic_tick_list = generic_ticks.to_vec();
        drop(state);
        self
    }

    /// Request a snapshot instead of continuous updates
    pub fn with_snapshot(mut self, snapshot: bool) -> Self {
        let mut state = self.state.lock();
        state.snapshot = snapshot;
        drop(state);
        self
    }

    /// Submit the subscription and perform initial blocking fetch
    pub fn submit(self, timeout: Duration) -> Result<Self, IBKRError> {
        // Implementation would:
        // 1. Extract parameters from state
        // 2. Call manager to send the market data request
        // 3. Wait for initial data (e.g., bid/ask prices)
        // 4. Return self on success, or error

        // For demonstration:
        Ok(self)
    }

    /// Get the current bid price, if available
    pub fn bid_price(&self) -> Option<f64> {
        let state = self.state.lock();
        state.tick_prices.get(&TickType::BidPrice).map(|(price, _)| *price)
    }

    /// Get the current ask price, if available
    pub fn ask_price(&self) -> Option<f64> {
        let state = self.state.lock();
        state.tick_prices.get(&TickType::AskPrice).map(|(price, _)| *price)
    }

    /// Get the current last price, if available
    pub fn last_price(&self) -> Option<f64> {
        let state = self.state.lock();
        state.tick_prices.get(&TickType::LastPrice).map(|(price, _)| *price)
    }

    /// Get an iterator that produces tick events as they arrive
    pub fn events(&self) -> TickDataIterator {
        TickDataIterator {
            state: self.state.clone(),
            timeout: None,
        }
    }
}

/// Iterator for TickDataEvents
pub struct TickDataIterator {
    state: Arc<Mutex<TickDataSubscriptionState>>,
    timeout: Option<Duration>,
}

impl Iterator for TickDataIterator {
    type Item = TickDataEvent;

    fn next(&mut self) -> Option<Self::Item> {
        let mut state = self.state.lock();

        // Return immediately if there's an available event
        if let Some(event) = state.events.pop_front() {
            return Some(event);
        }

        // If subscription is no longer active, return None
        if !state.is_active {
            return None;
        }

        // If a timeout is set, wait for that duration
        if let Some(timeout) = self.timeout {
            let result = state.cond.wait_for(&mut state, timeout);
            if result.timed_out() {
                return None;
            }
        } else {
            // Wait indefinitely
            state.cond = state.cond.wait(&mut state);
        }

        // After waking, check events again
        state.events.pop_front()
    }
}

impl MarketDataIterator<TickDataEvent> for TickDataIterator {
    fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    fn try_next(&mut self, timeout: Duration) -> Option<TickDataEvent> {
        let mut state = self.state.lock();

        // Return immediately if there's an available event
        if let Some(event) = state.events.pop_front() {
            return Some(event);
        }

        // If subscription is no longer active, return None
        if !state.is_active {
            return None;
        }

        // Wait with the provided timeout
        let result = state.cond.wait_for(&mut state, timeout);
        if result.timed_out() {
            return None;
        }

        // After waking, check events again
        state.events.pop_front()
    }

    fn collect_available(&mut self) -> Vec<TickDataEvent> {
        let mut state = self.state.lock();
        let mut events = Vec::new();

        while let Some(event) = state.events.pop_front() {
            events.push(event);
        }

        events
    }

    fn cancel(self) -> Result<(), IBKRError> {
        let req_id = self.state.lock().req_id;

        // Implementation would call cancel on the manager
        // For demonstration:
        Ok(())
    }
}

// Implement Drop to automatically cancel the subscription
impl Drop for TickDataSubscription {
    fn drop(&mut self) {
        let state = self.state.lock();
        if state.is_active {
            let req_id = state.req_id;
            // Best-effort cancel, ignore errors
            let _ = self.manager.cancel_market_data(req_id);
        }
    }
}
```

## Multiple Subscription Handling

### Identifying the Source of Data Events

When combining events from multiple sources, it's important to identify which subscription an event came from. This is solved by introducing a tagged event structure:

```rust
/// A tagged market data event with source information
pub struct TaggedMarketDataEvent<T> {
    /// The source subscription ID
    pub req_id: i32,
    /// The source contract
    pub contract: Contract,
    /// The actual event
    pub event: T,
}
```

### MultiMarketDataIterator

The `MultiMarketDataIterator` produces tagged events that include source information:

```rust
/// An iterator that multiplexes events from multiple market data subscriptions
pub struct MultiMarketDataIterator<T> {
    iterators: Vec<(i32, Contract, Box<dyn MarketDataIterator<T>>)>,
    timeout: Option<Duration>,
}

impl<T> MultiMarketDataIterator<T> {
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
    pub fn try_next(&mut self, timeout: Duration) -> Option<TaggedMarketDataEvent<T>> {
        let start = std::time::Instant::now();
        let poll_interval = Duration::from_millis(10);

        while start.elapsed() < timeout {
            // Check each iterator for an available event
            for (idx, (req_id, contract, iterator)) in self.iterators.iter_mut().enumerate() {
                if let Some(event) = iterator.try_next(poll_interval) {
                    return Some(TaggedMarketDataEvent {
                        req_id: *req_id,
                        contract: contract.clone(),
                        event,
                    });
                }
            }

            // Small sleep to avoid tight loop
            std::thread::sleep(poll_interval);
        }

        None
    }

    /// Cancel all underlying subscriptions
    pub fn cancel_all(self) -> Result<(), IBKRError> {
        let mut errors = Vec::new();

        for (_, _, iterator) in self.iterators {
            if let Err(e) = iterator.cancel() {
                errors.push(e);
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(IBKRError::CompositeError(format!("{} cancellation errors", errors.len())))
        }
    }
}

impl<T> Iterator for MultiMarketDataIterator<T> {
    type Item = TaggedMarketDataEvent<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(timeout) = self.timeout {
            self.try_next(timeout)
        } else {
            // If no timeout is set, we need to implement a polling strategy
            // for indefinite waiting
            self.try_next(Duration::from_secs(3600)) // 1 hour as a practical "indefinite"
        }
    }
}
```

### Factory Method for Multiple Subscriptions

To integrate the multiple subscription pattern with the factory pattern, the `DataMarketManager` can provide factory methods for creating `MultiMarketDataIterator` objects:

```rust
impl DataMarketManager {
    /// Create a multiplexed iterator from multiple market data subscriptions
    pub fn combine_subscriptions<T>(&self) -> MultiMarketDataBuilder<T> {
        MultiMarketDataBuilder::new()
    }
}

/// Builder for creating MultiMarketDataIterator
pub struct MultiMarketDataBuilder<T> {
    iterators: Vec<(i32, Contract, Box<dyn MarketDataIterator<T>>)>,
}

impl<T> MultiMarketDataBuilder<T> {
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

    /// Build the MultiMarketDataIterator
    pub fn build(self) -> MultiMarketDataIterator<T> {
        MultiMarketDataIterator {
            iterators: self.iterators,
            timeout: None,
        }
    }
}
```

## Usage Example: Multiple Subscriptions

Here's how to combine multiple subscriptions using the factory pattern:

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

    // Create a histogram data subscription for TSLA
    let tsla_histogram = market_data_mgr.subscribe_histogram_data(
            Contract::stock("TSLA"),
            TimePeriodUnit::Day(5)
        )
        .submit(Duration::from_secs(5))?;

    // Combine tick events from AAPL and MSFT (same event type)
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

    // Note: Cannot combine different event types this way
    // You would need to handle the different iterators separately

    // Process TSLA histogram events
    println!("Processing TSLA histogram data...");
    let mut histogram_events = tsla_histogram.events();
    for _ in 0..5 {
        if let Some(event) = histogram_events.try_next(Duration::from_secs(3)) {
            println!("Histogram entry: price = {}, size = {}", event.price, event.count);
        } else {
            println!("No histogram events within timeout");
        }
    }

    // Subscriptions are automatically cancelled when dropped
    Ok(())
}
```

## Implementation in DataMarketManager

```rust
impl DataMarketManager {
    /// Internal method to process a tick price message for a subscription
    fn process_tick_price_for_subscription(&self, req_id: i32, tick_type: TickType, price: f64, attrib: TickAttrib) {
        // Find the subscription in a registry of active subscriptions
        // Update its state
        // Create a TickDataEvent
        // Add it to the event queue
        // Notify any waiting iterators
    }

    // Similar methods for other message types...

    /// Internal registry of subscriptions
    fn register_subscription<S: MarketDataSubscription + 'static>(&self, subscription: Arc<S>) {
        // Store in a type-erased registry for event routing
    }

    /// Get the next available request ID
    fn next_request_id(&self) -> i32 {
        self.message_broker.next_request_id()
    }
}
```

## Modified MarketDataHandler Implementation

```rust
impl MarketDataHandler for DataMarketManager {
    fn tick_price(&self, req_id: i32, tick_type: TickType, price: f64, attrib: TickAttrib) {
        // Process for legacy subscriptions (existing implementation)

        // Process for new iterator-based subscriptions
        self.process_tick_price_for_subscription(req_id, tick_type, price, attrib);
    }

    // Similar implementations for other handler methods...
}
```

## Benefits Over Current Design

1. **More Intuitive API** - Provides a natural way to consume events through iterators
2. **Initial Blocking Fetch** - Ensures data is immediately available after subscription
3. **No Callback Hell** - Avoids complex observer registration and callback management
4. **Flexible Consumption Patterns** - Supports both polling and streaming models
5. **Resource Cleanup** - Automatic cancellation when subscriptions go out of scope
6. **Type Safety** - Each subscription type has its own specialized interface
7. **Easy Composition** - Iterator-based design allows for easy composition with the `MultiMarketDataIterator`

## Migration Strategy

1. Implement the `MarketDataSubscription` and `MarketDataIterator` traits
2. Implement the concrete subscription types with their iterators
3. Add factory methods to `DataMarketManager`
4. Update `MarketDataHandler` implementation to handle both old and new subscriptions
5. Implement `MultiMarketDataIterator` for handling multiple subscriptions
6. Add documentation and examples
7. Gradually migrate existing code to the new pattern
8. Mark old methods as deprecated when ready

The existing methods (`request_*` and `get_*`) can remain for backward compatibility while their implementation gradually migrates to use the new subscription model internally.
