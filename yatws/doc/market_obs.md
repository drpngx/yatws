# Market Data Observer Pattern Implementation

This document outlines the implementation of observer traits and required changes to enable callback-based processing of market data in the `DataMarketManager`.

## Observer Traits

### 1. MarketDataObserver

For real-time market data ticks (price, size, etc.)

```rust
/// Observer for regular market data ticks (prices, sizes)
pub trait MarketDataObserver: Send + Sync {
    /// Called when a price tick is received
    fn on_tick_price(&self, req_id: i32, tick_type: TickType, price: f64, attrib: &TickAttrib);

    /// Called when a size tick is received
    fn on_tick_size(&self, req_id: i32, tick_type: TickType, size: f64);

    /// Called when a string tick is received
    fn on_tick_string(&self, req_id: i32, tick_type: TickType, value: &str);

    /// Called when a generic tick is received
    fn on_tick_generic(&self, req_id: i32, tick_type: TickType, value: f64);

    /// Called when an option computation tick is received
    fn on_tick_option_computation(&self, req_id: i32, data: &TickOptionComputationData);

    /// Called when a snapshot ends
    fn on_tick_snapshot_end(&self, req_id: i32);

    /// Called when market data type changes
    fn on_market_data_type(&self, req_id: i32, market_data_type: MarketDataType);
}
```

### 2. RealTimeBarObserver

For 5-second real-time bars.

```rust
/// Observer for real-time bars (5-second intervals)
pub trait RealTimeBarObserver: Send + Sync {
    /// Called when a real-time bar is received
    fn on_real_time_bar(&self, req_id: i32, time: i64, open: f64, high: f64,
                        low: f64, close: f64, volume: f64, wap: f64, count: i32);
}
```

### 3. TickByTickObserver

For detailed tick-by-tick data.

```rust
/// Observer for tick-by-tick data
pub trait TickByTickObserver: Send + Sync {
    /// Called when All Last or Last tick is received
    fn on_tick_by_tick_all_last(&self, req_id: i32, tick_type: i32, time: i64,
                               price: f64, size: f64,
                               tick_attrib_last: &TickAttribLast,
                               exchange: &str, special_conditions: &str);

    /// Called when BidAsk tick is received
    fn on_tick_by_tick_bid_ask(&self, req_id: i32, time: i64,
                              bid_price: f64, ask_price: f64,
                              bid_size: f64, ask_size: f64,
                              tick_attrib_bid_ask: &TickAttribBidAsk);

    /// Called when MidPoint tick is received
    fn on_tick_by_tick_mid_point(&self, req_id: i32, time: i64, mid_point: f64);
}
```

### 4. MarketDepthObserver

For Level II market depth data.

```rust
/// Observer for market depth (Level II) data
pub trait MarketDepthObserver: Send + Sync {
    /// Called when L1 market depth is updated
    fn on_update_mkt_depth(&self, req_id: i32, position: i32, operation: i32,
                          side: i32, price: f64, size: f64);

    /// Called when L2 market depth is updated
    fn on_update_mkt_depth_l2(&self, req_id: i32, position: i32,
                             market_maker: &str, operation: i32,
                             side: i32, price: f64, size: f64,
                             is_smart_depth: bool);
}
```

### 5. ScannerObserver

For market scanner results.

```rust
/// Observer for market scanner data
pub trait ScannerObserver: Send + Sync {
    /// Called when scanner data is received
    fn on_scanner_data(&self, req_id: i32, rank: i32,
                      contract_details: &ContractDetails,
                      distance: &str, benchmark: &str,
                      projection: &str, legs_str: Option<&str>);

    /// Called when scanner data ends
    fn on_scanner_data_end(&self, req_id: i32);

    /// Called when scanner parameters are received
    fn on_scanner_parameters(&self, xml: &str);
}
```

### 6. HistogramObserver

For price histogram data.

```rust
/// Observer for histogram data
pub trait HistogramObserver: Send + Sync {
    /// Called when histogram data is received
    fn on_histogram_data(&self, req_id: i32, items: &[HistogramEntry]);
}
```

### 7. HistoricalDataObserver

For historical bars.

```rust
/// Observer for historical bar data
pub trait HistoricalDataObserver: Send + Sync {
    /// Called when a historical data bar is received
    fn on_historical_data(&self, req_id: i32, bar: &Bar);

    /// Called when a historical data update bar is received (head bar)
    fn on_historical_data_update(&self, req_id: i32, bar: &Bar);

    /// Called when historical data ends
    fn on_historical_data_end(&self, req_id: i32, start_date: &str, end_date: &str);
}
```

### 8. HistoricalTicksObserver

For historical ticks.

```rust
/// Observer for historical ticks data
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

## Default Implementations

Base traits with default implementations for all methods:

```rust
/// Base trait with default implementations for MarketDataObserver
pub trait MarketDataObserverBase: Send + Sync {
    // Default empty implementations
    fn on_tick_price(&self, _req_id: i32, _tick_type: TickType, _price: f64,
                    _attrib: &TickAttrib) {}

    fn on_tick_size(&self, _req_id: i32, _tick_type: TickType, _size: f64) {}

    // ... other methods with default implementations ...
}

// Then make the main trait extend it
pub trait MarketDataObserver: MarketDataObserverBase {}

// Do similar for other observer types
```

## Updates to DataMarketManager

### 1. Add observer storage to the struct

```rust
/// Manages requests for real-time and historical market data from TWS.
pub struct DataMarketManager {
    // Existing fields...

    // Observer collections
    market_data_observers: RwLock<HashMap<usize, Weak<dyn MarketDataObserver>>>,
    rtbar_observers: RwLock<HashMap<usize, Weak<dyn RealTimeBarObserver>>>,
    tick_by_tick_observers: RwLock<HashMap<usize, Weak<dyn TickByTickObserver>>>,
    market_depth_observers: RwLock<HashMap<usize, Weak<dyn MarketDepthObserver>>>,
    scanner_observers: RwLock<HashMap<usize, Weak<dyn ScannerObserver>>>,
    histogram_observers: RwLock<HashMap<usize, Weak<dyn HistogramObserver>>>,
    historical_data_observers: RwLock<HashMap<usize, Weak<dyn HistoricalDataObserver>>>,
    historical_ticks_observers: RwLock<HashMap<usize, Weak<dyn HistoricalTicksObserver>>>,
    next_observer_id: AtomicUsize,
}
```

### 2. Initialize the observer fields in constructor

```rust
impl DataMarketManager {
    pub(crate) fn new(message_broker: Arc<MessageBroker>) -> Arc<Self> {
        Arc::new(DataMarketManager {
            message_broker,
            subscriptions: Mutex::new(HashMap::new()),
            request_cond: Condvar::new(),
            current_market_data_type: Mutex::new(MarketDataType::RealTime),
            market_data_type_cond: Condvar::new(),
            scanner_parameters_xml: Mutex::new(None),
            scanner_parameters_cond: Condvar::new(),

            // Initialize observer collections
            market_data_observers: RwLock::new(HashMap::new()),
            rtbar_observers: RwLock::new(HashMap::new()),
            tick_by_tick_observers: RwLock::new(HashMap::new()),
            market_depth_observers: RwLock::new(HashMap::new()),
            scanner_observers: RwLock::new(HashMap::new()),
            histogram_observers: RwLock::new(HashMap::new()),
            historical_data_observers: RwLock::new(HashMap::new()),
            historical_ticks_observers: RwLock::new(HashMap::new()),
            next_observer_id: AtomicUsize::new(1),
        })
    }
}
```

### 3. Add methods to add/remove observers

```rust
impl DataMarketManager {
    /// Adds an observer for market data ticks
    ///
    /// # Returns
    /// A unique observer ID that can be used with `remove_market_data_observer`.
    pub fn add_market_data_observer<T>(&self, observer: Arc<T>) -> usize
    where
        T: MarketDataObserver + 'static
    {
        let id = self.next_observer_id.fetch_add(1, Ordering::SeqCst);
        let weak_ref = Arc::downgrade(&observer) as Weak<dyn MarketDataObserver>;
        self.market_data_observers.write().insert(id, weak_ref);
        id
    }

    /// Removes a previously added market data observer
    ///
    /// # Returns
    /// `true` if the observer was found and removed, `false` otherwise.
    pub fn remove_market_data_observer(&self, id: usize) -> bool {
        let mut guard = self.market_data_observers.write();
        let result = guard.remove(&id).is_some();

        // While we have the write lock, clean up any expired weak references
        self.clean_expired_observers(&mut guard);

        result
    }

    // Add similar methods for all other observer types...

    // Helper to clean up expired weak references
    fn clean_expired_observers<T: ?Sized>(&self, observers: &mut HashMap<usize, Weak<T>>) {
        observers.retain(|_, weak_ref| weak_ref.upgrade().is_some());
    }
}
```

### 4. Add notification methods

```rust
impl DataMarketManager {
    // Notification methods
    fn notify_tick_price(&self, req_id: i32, tick_type: TickType, price: f64, attrib: &TickAttrib) {
        let observers = self.market_data_observers.read();
        for (_, weak_observer) in observers.iter() {
            if let Some(observer) = weak_observer.upgrade() {
                observer.on_tick_price(req_id, tick_type, price, attrib);
            }
        }
    }

    fn notify_tick_size(&self, req_id: i32, tick_type: TickType, size: f64) {
        let observers = self.market_data_observers.read();
        for (_, weak_observer) in observers.iter() {
            if let Some(observer) = weak_observer.upgrade() {
                observer.on_tick_size(req_id, tick_type, size);
            }
        }
    }

    // Add similar notification methods for all callbacks...
}
```

### 5. Update MarketDataHandler implementation to notify observers

```rust
impl MarketDataHandler for DataMarketManager {
    fn tick_price(&self, req_id: i32, tick_type: TickType, price: f64, attrib: TickAttrib) {
        trace!("Handler: Tick Price: ID={}, Type={:?}, Price={}, Attrib={:?}", req_id, tick_type, price, attrib);

        // Existing implementation...
        let mut subs = self.subscriptions.lock();
        if let Some(MarketSubscription::TickData(state)) = subs.get_mut(&req_id) {
            // Existing logic to update state...
        }

        // Notify observers
        self.notify_tick_price(req_id, tick_type, price, &attrib);
    }

    // Update all other handler methods similarly...
}
```

## Convenience Methods

### 1. Filter Wrapper

```rust
/// A wrapper that filters market data by request ID
pub struct RequestIdFilter<T> {
    observer: Arc<T>,
    req_ids: HashSet<i32>, // Only notify for these request IDs
}

impl<T: MarketDataObserver> MarketDataObserver for RequestIdFilter<T> {
    fn on_tick_price(&self, req_id: i32, tick_type: TickType, price: f64, attrib: &TickAttrib) {
        if self.req_ids.contains(&req_id) {
            self.observer.on_tick_price(req_id, tick_type, price, attrib);
        }
    }

    // Implement other methods similarly...
}

// Create similar wrappers for other observer types
```

### 2. `observe` convenience methods

```rust
impl DataMarketManager {
    /// Request market data and register an observer in one step
    pub fn observe_market_data<T>(
        &self,
        contract: &Contract,
        generic_tick_list: &[GenericTickType],
        snapshot: bool,
        regulatory_snapshot: bool,
        mkt_data_options: &[(String, String)],
        market_data_type: Option<MarketDataType>,
        observer: Arc<T>
    ) -> Result<(i32, usize), IBKRError>
    where
        T: MarketDataObserver + 'static
    {
        // Request the market data
        let req_id = self.request_market_data(
            contract,
            generic_tick_list,
            snapshot,
            regulatory_snapshot,
            mkt_data_options,
            market_data_type
        )?;

        // Add the observer
        let observer_id = self.add_market_data_observer(observer);

        Ok((req_id, observer_id))
    }

    // Add similar methods for other subscription types...
}
```

## Default Observer Base Implementations

```rust
/// Default implementation of MarketDataObserver that does nothing
pub struct DefaultMarketDataObserver;

impl MarketDataObserver for DefaultMarketDataObserver {}
impl MarketDataObserverBase for DefaultMarketDataObserver {}

// Create similar defaults for other observer types
```

## Example Usage

```rust
use std::sync::Arc;

// Create a custom observer
struct MyMarketDataObserver;

impl MarketDataObserverBase for MyMarketDataObserver {
    // Override only the methods we care about
    fn on_tick_price(&self, req_id: i32, tick_type: TickType, price: f64, attrib: &TickAttrib) {
        println!("Received price: req_id={}, type={:?}, price={}", req_id, tick_type, price);
    }
}

impl MarketDataObserver for MyMarketDataObserver {}

// Usage in application
fn main() -> Result<(), IBKRError> {
    let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
    let market_data_mgr = client.data_market();

    let observer = Arc::new(MyMarketDataObserver);
    let observer_id = market_data_mgr.add_market_data_observer(observer);

    let contract = Contract::stock("AAPL");
    let req_id = market_data_mgr.request_market_data(
        &contract,
        &[],
        false,
        false,
        &[],
        Some(MarketDataType::Delayed)
    )?;

    // Let it run for a while...
    std::thread::sleep(std::time::Duration::from_secs(60));

    // Clean up
    market_data_mgr.cancel_market_data(req_id)?;
    market_data_mgr.remove_market_data_observer(observer_id);

    Ok(())
}
```

## Combined Observer Trait

For convenience, a combined observer trait:

```rust
/// Observer that combines all market data observer traits
pub trait MarketDataCombinedObserver:
    MarketDataObserver +
    RealTimeBarObserver +
    TickByTickObserver +
    MarketDepthObserver +
    ScannerObserver +
    HistogramObserver +
    HistoricalDataObserver +
    HistoricalTicksObserver {
    // No additional methods
}

/// Default implementation of the combined observer
pub struct DefaultCombinedObserver;

impl MarketDataObserverBase for DefaultCombinedObserver {}
impl MarketDataObserver for DefaultCombinedObserver {}
// Implement all other base traits
```

## Implementation Note

This observer pattern can coexist with the existing blocking methods. The existing blocking methods will continue to function as before, working via condition variables, while the observer pattern provides an event-driven alternative.

The patterns are complementary:
- Use **blocking methods** for simple one-shot queries where you need the result immediately and then continue.
- Use the **observer pattern** for continuous streaming data or complex applications that need to react to events asynchronously.
