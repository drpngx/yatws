# Market Data Observer Design

## Overview

This document outlines a design for market data observers in the YATWS library. This design uses specialized observer traits for each market data type, allowing for more targeted and flexible subscription to specific data streams.

## Design Goals

1. **Type Safety**: Ensure observers only receive notifications for the specific data types they're interested in
2. **Reduced Complexity**: Simplify observer implementations by requiring only relevant methods
3. **Enhanced Flexibility**: Allow clients to register different observers for different data types
4. **Consistency**: Maintain a similar observer pattern to other managers (e.g., AccountManager)

## Observer Traits

### Market Data Observer

```rust
/// Observer for regular market data ticks (price, size, etc.)
pub trait MarketDataObserver: Send + Sync {
    /// Called when a price tick is received
    fn on_tick_price(&self, req_id: i32, tick_type: TickType, price: f64, attrib: TickAttrib);

    /// Called when a size tick is received
    fn on_tick_size(&self, req_id: i32, tick_type: TickType, size: f64);

    /// Called when a string tick is received
    fn on_tick_string(&self, req_id: i32, tick_type: TickType, value: &str);

    /// Called when a generic tick is received
    fn on_tick_generic(&self, req_id: i32, tick_type: TickType, value: f64);

    /// Called when a snapshot is completed
    fn on_tick_snapshot_end(&self, req_id: i32);

    /// Called when market data type changes
    fn on_market_data_type(&self, req_id: i32, market_data_type: MarketDataType);
}
```

### Other Specialized Observers

```rust
/// Observer for real-time bar data (streaming 5-second bars)
pub trait RealTimeBarsObserver: Send + Sync {
    /// Called when a new real-time bar is received
    fn on_bar_update(&self, req_id: i32, bar: &Bar);
}

/// Observer for tick-by-tick data (detailed trade and quote data)
pub trait TickByTickObserver: Send + Sync {
    /// Called when last or all last tick data is received
    fn on_tick_by_tick_all_last(&self, req_id: i32, tick_type: i32, time: i64, price: f64,
                               size: f64, tick_attrib_last: &TickAttribLast,
                               exchange: &str, special_conditions: &str);

    /// Called when bid/ask tick data is received
    fn on_tick_by_tick_bid_ask(&self, req_id: i32, time: i64, bid_price: f64, ask_price: f64,
                              bid_size: f64, ask_size: f64,
                              tick_attrib_bid_ask: &TickAttribBidAsk);

    /// Called when midpoint tick data is received
    fn on_tick_by_tick_mid_point(&self, req_id: i32, time: i64, mid_point: f64);
}

/// Observer for market depth (L2 order book) data
pub trait MarketDepthObserver: Send + Sync {
    /// Called when L1 market depth is updated
    fn on_update_mkt_depth(&self, req_id: i32, position: i32, operation: i32,
                          side: i32, price: f64, size: f64);

    /// Called when L2 market depth is updated
    fn on_update_mkt_depth_l2(&self, req_id: i32, position: i32, market_maker: &str,
                             operation: i32, side: i32, price: f64, size: f64,
                             is_smart_depth: bool);
}

/// Observer for historical bar data
pub trait HistoricalDataObserver: Send + Sync {
    /// Called when a historical data bar is received
    fn on_historical_data(&self, req_id: i32, bar: &Bar);

    /// Called when a historical data update bar is received
    fn on_historical_data_update(&self, req_id: i32, bar: &Bar);

    /// Called when historical data transmission is complete
    fn on_historical_data_end(&self, req_id: i32, start_date: &str, end_date: &str);
}

/// Observer for historical tick data
pub trait HistoricalTicksObserver: Send + Sync {
    /// Called when historical midpoint ticks are received
    fn on_historical_ticks(&self, req_id: i32, ticks: &[(i64, f64, f64)], done: bool);

    /// Called when historical bid/ask ticks are received
    fn on_historical_ticks_bid_ask(&self, req_id: i32,
                                  ticks: &[(i64, TickAttribBidAsk, f64, f64, f64, f64)],
                                  done: bool);

    /// Called when historical last ticks are received
    fn on_historical_ticks_last(&self, req_id: i32,
                               ticks: &[(i64, TickAttribLast, f64, f64, String, String)],
                               done: bool);
}
```

## Default Trait Implementations

To make it easier to implement observers, we'll provide default empty implementations for all methods:

```rust
impl Default for dyn MarketDataObserver {
    fn on_tick_price(&self, _req_id: i32, _tick_type: TickType, _price: f64, _attrib: TickAttrib) {}
    fn on_tick_size(&self, _req_id: i32, _tick_type: TickType, _size: f64) {}
    fn on_tick_string(&self, _req_id: i32, _tick_type: TickType, _value: &str) {}
    fn on_tick_generic(&self, _req_id: i32, _tick_type: TickType, _value: f64) {}
    fn on_tick_snapshot_end(&self, _req_id: i32) {}
    fn on_market_data_type(&self, _req_id: i32, _market_data_type: MarketDataType) {}
}
```

Similar default implementations should be provided for all other observer traits.

## DataMarketManager Modifications

The `DataMarketManager` will be updated to include specialized observer collections and registration methods:

### Internal State

```rust
pub struct DataMarketManager {
    // Existing fields...

    // Observer collections
    market_data_observers: RwLock<HashMap<usize, Box<dyn MarketDataObserver + Send + Sync>>>,
    realtime_bars_observers: RwLock<HashMap<usize, Box<dyn RealTimeBarsObserver + Send + Sync>>>,
    tick_by_tick_observers: RwLock<HashMap<usize, Box<dyn TickByTickObserver + Send + Sync>>>,
    market_depth_observers: RwLock<HashMap<usize, Box<dyn MarketDepthObserver + Send + Sync>>>,
    historical_data_observers: RwLock<HashMap<usize, Box<dyn HistoricalDataObserver + Send + Sync>>>,
    historical_ticks_observers: RwLock<HashMap<usize, Box<dyn HistoricalTicksObserver + Send + Sync>>>,

    next_observer_id: AtomicUsize,
}
```

### Observer Registration Methods

```rust
impl DataMarketManager {
    /// Register an observer for market data tick updates
    pub fn observe_market_data<T: MarketDataObserver + Send + Sync + 'static>(&self, observer: T) -> usize {
        let observer_id = self.next_observer_id.fetch_add(1, Ordering::SeqCst);
        let mut observers = self.market_data_observers.write();
        observers.insert(observer_id, Box::new(observer));
        debug!("Added market data observer with ID: {}", observer_id);
        observer_id
    }

    /// Unregister a market data observer
    pub fn remove_market_data_observer(&self, observer_id: usize) -> bool {
        let mut observers = self.market_data_observers.write();
        let removed = observers.remove(&observer_id).is_some();
        if removed {
            debug!("Removed market data observer with ID: {}", observer_id);
        } else {
            warn!("Attempted to remove non-existent market data observer ID: {}", observer_id);
        }
        removed
    }

    /// Register an observer for real-time bar updates
    pub fn observe_realtime_bars<T: RealTimeBarsObserver + Send + Sync + 'static>(&self, observer: T) -> usize {
        let observer_id = self.next_observer_id.fetch_add(1, Ordering::SeqCst);
        let mut observers = self.realtime_bars_observers.write();
        observers.insert(observer_id, Box::new(observer));
        debug!("Added real-time bars observer with ID: {}", observer_id);
        observer_id
    }

    /// Unregister a real-time bars observer
    pub fn remove_realtime_bars_observer(&self, observer_id: usize) -> bool {
        let mut observers = self.realtime_bars_observers.write();
        let removed = observers.remove(&observer_id).is_some();
        if removed {
            debug!("Removed real-time bars observer with ID: {}", observer_id);
        } else {
            warn!("Attempted to remove non-existent real-time bars observer ID: {}", observer_id);
        }
        removed
    }

    // Similar methods for other observer types...
    pub fn observe_tick_by_tick<T: TickByTickObserver + Send + Sync + 'static>(&self, observer: T) -> usize;
    pub fn remove_tick_by_tick_observer(&self, observer_id: usize) -> bool;

    pub fn observe_market_depth<T: MarketDepthObserver + Send + Sync + 'static>(&self, observer: T) -> usize;
    pub fn remove_market_depth_observer(&self, observer_id: usize) -> bool;

    pub fn observe_historical_data<T: HistoricalDataObserver + Send + Sync + 'static>(&self, observer: T) -> usize;
    pub fn remove_historical_data_observer(&self, observer_id: usize) -> bool;

    pub fn observe_historical_ticks<T: HistoricalTicksObserver + Send + Sync + 'static>(&self, observer: T) -> usize;
    pub fn remove_historical_ticks_observer(&self, observer_id: usize) -> bool;
}
```

### MarketDataHandler Implementation Updates

```rust
impl MarketDataHandler for DataMarketManager {
    fn tick_price(&self, req_id: i32, tick_type: TickType, price: f64, attrib: TickAttrib) {
        // Existing implementation to update internal state...

        // Notify observers
        let observers = self.market_data_observers.read();
        for observer in observers.values() {
            observer.on_tick_price(req_id, tick_type, price, attrib.clone());
        }
    }

    fn tick_size(&self, req_id: i32, tick_type: TickType, size: f64) {
        // Existing implementation...

        // Notify observers
        let observers = self.market_data_observers.read();
        for observer in observers.values() {
            observer.on_tick_size(req_id, tick_type, size);
        }
    }

    fn tick_string(&self, req_id: i32, tick_type: TickType, value: &str) {
        // Existing implementation...

        // Notify observers
        let observers = self.market_data_observers.read();
        for observer in observers.values() {
            observer.on_tick_string(req_id, tick_type, value);
        }
    }

    fn tick_generic(&self, req_id: i32, tick_type: TickType, value: f64) {
        // Existing implementation...

        // Notify observers
        let observers = self.market_data_observers.read();
        for observer in observers.values() {
            observer.on_tick_generic(req_id, tick_type, value);
        }
    }

    fn tick_snapshot_end(&self, req_id: i32) {
        // Existing implementation...

        // Notify observers
        let observers = self.market_data_observers.read();
        for observer in observers.values() {
            observer.on_tick_snapshot_end(req_id);
        }
    }

    fn market_data_type(&self, req_id: i32, market_data_type: MarketDataType) {
        // Existing implementation...

        // Notify observers
        let observers = self.market_data_observers.read();
        for observer in observers.values() {
            observer.on_market_data_type(req_id, market_data_type);
        }
    }

    // Similar updates for other handler methods...
}
```

## Request-specific observers

In many cases, an observer might only be interested in data for a specific request. To support this, we can provide helper methods for filtering and mapping observer registrations:

```rust
impl DataMarketManager {
    /// Request market data and events from that request to the observer.
    pub fn request_observe_market_data<T: MarketDataObserver + Send + Sync + 'static>(
        &self,
        contract: &Contract,
        generic_tick_list: &[GenericTickType],
        snapshot: bool,
        regulatory_snapshot: bool,
        mkt_data_options: &[(String, String)],
        market_data_type: Option<MarketDataType>,
        observer: T
    ) -> usize {
        // Create a wrapper that only forwards events for the specified req_id
        struct FilteredObserver<T: MarketDataObserver + Send + Sync> {
            inner: T,
            req_id: i32,
        }

        impl<T: MarketDataObserver + Send + Sync> MarketDataObserver for FilteredObserver<T> {
            fn on_tick_price(&self, req_id: i32, tick_type: TickType, price: f64, attrib: TickAttrib) {
                if req_id == self.req_id {
                    self.inner.on_tick_price(req_id, tick_type, price, attrib);
                }
            }

            // Similar implementations for other methods...
            fn on_tick_size(&self, req_id: i32, tick_type: TickType, size: f64) {
                if req_id == self.req_id {
                    self.inner.on_tick_size(req_id, tick_type, size);
                }
            }

            fn on_tick_string(&self, req_id: i32, tick_type: TickType, value: &str) {
                if req_id == self.req_id {
                    self.inner.on_tick_string(req_id, tick_type, value);
                }
            }

            fn on_tick_generic(&self, req_id: i32, tick_type: TickType, value: f64) {
                if req_id == self.req_id {
                    self.inner.on_tick_generic(req_id, tick_type, value);
                }
            }

            fn on_tick_snapshot_end(&self, req_id: i32) {
                if req_id == self.req_id {
                    self.inner.on_tick_snapshot_end(req_id);
                }
            }

            fn on_market_data_type(&self, req_id: i32, market_data_type: MarketDataType) {
                if req_id == self.req_id {
                    self.inner.on_market_data_type(req_id, market_data_type);
                }
            }
        }
        let req_id = self.message_broker.next_request_id();
        // Register the observer now before we submit any request.
        self.observe_market_data(FilteredObserver { inner: observer, req_id })
        // Call internal request market data which is like the present one but with a given request id.
        self.internal_request_market_data(req_id, contract, generic_tick_list, snapshot, regulatory_snapshot, mkt_data_options, market_data_type)?;
    }

    // Similar methods for other observer types...
}
```

## Example Usage

```rust
// Example: Streaming market data with observer
struct MyMarketDataObserver;

impl MarketDataObserver for MyMarketDataObserver {
    fn on_tick_price(&self, req_id: i32, tick_type: TickType, price: f64, attrib: TickAttrib) {
        println!("Price update for request {}: Type={:?}, Price={}", req_id, tick_type, price);
    }

    fn on_tick_size(&self, req_id: i32, tick_type: TickType, size: f64) {
        println!("Size update for request {}: Type={:?}, Size={}", req_id, tick_type, size);
    }

    // Implement other methods as needed...
}

// In application code:
let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
let market_data_mgr = client.data_market();

// Register observer
let observer_id = market_data_mgr.observe_market_data(MyMarketDataObserver);

// Request streaming market data
let contract = Contract::stock("AAPL");
let req_id = market_data_mgr.request_market_data(
    &contract, &[], false, false, &[], None
)?;

// Later, unregister observer if needed
market_data_mgr.remove_market_data_observer(observer_id);
```

## Example with Requeste-Observe

```rust
// Register observer only for a specific request ID
let contract = Contract::stock("AAPL");
let obs = MyMarketDataObserver::new(...);
let observer_id = market_data_mgr.request_observe_market_data(
    &contract, &[], false, false, &[], None, obs
)?;
// Later, unregister observer if needed
market_data_mgr.remove_market_data_observer(observer_id);
```

## Implementation Plan

1. Define the specialized observer traits with default method implementations
2. Add observer collections to `DataMarketManager`
3. Implement observer registration and removal methods
4. Update `MarketDataHandler` implementation to call observer methods
5. Add filtered observer registration helpers
6. Update API documentation
7. Create usage examples

## Summary

This design replaces a potential global `MarketDataObserver` trait with specialized traits for different market data types. It provides a more type-safe and flexible approach to observing market data, allowing clients to register specific observers for the data types they're interested in.

The design for market data observers is particularly granular, with separate callback methods for different tick types, allowing fine-grained control over which data the observer receives.

The implementation follows a similar pattern to the `AccountManager` observer system for consistency across the library.
