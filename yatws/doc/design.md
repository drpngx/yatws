# Implementation Tasks

The following table outlines the tasks required to implement the IBKR API design, along with priority levels, estimated effort, and dependencies.

| ID | Task | Description | Priority | Effort (days) | Dependencies |
|----|------|-------------|----------|---------------|--------------|
| 1 | Core Data Structures | Implement basic data structures for contracts, orders, account info | High | 3 | None |
| 2 | SQLite Schema | Design and implement the SQLite database schema for logging and replay | High | 2 | None |
| 3 | Message Protocol | Implement IBKR message protocol parsing and serialization | High | 5 | None |
| 4 | ConnectionManager | Implement TCP socket connection handling with reconnection logic | High | 4 | 3 |
| 5 | MessageQueue | Implement thread-safe message queuing system | High | 2 | None |
| 6 | RequestManager | Implement request tracking and response matching | High | 3 | 3, 5 |
| 7 | NotificationManager | Implement notification handling and observer pattern | Medium | 3 | 5 |
| 8 | ResponseRouter | Implement message routing between components | High | 4 | 3, 5, 6, 7 |
| 9 | MessageBroker | Integrate request/response/notification systems | High | 3 | 6, 7, 8 |
| 10 | OrderAPI | Implement order placement and tracking | High | 5 | 1, 9 |
| 11 | AccountAPI | Implement account information and position tracking | Medium | 5 | 1, 9 |
| 12 | DataAPI | Implement market data and historical data retrieval | Medium | 6 | 1, 9 |
| 13 | SQLite Logger | Implement database logging of all API interactions | Medium | 4 | 2, 4 |
| 14 | Replay System | Implement replay functionality from database logs | Medium | 5 | 2, 13 |
| 15 | Error Handling | Implement comprehensive error handling throughout the API | High | 3 | All |
| 16 | Rate Limiting | Implement request throttling to respect IBKR limits | Medium | 2 | 9 |
| 17 | Python Bindings | Create PyO3 bindings for all API functionality | High | 7 | 1-16 |
| 18 | Proxy Server | Implement proxy server for debugging and logging | Low | 4 | 4, 13 |
| 19 | Unit Tests | Create comprehensive unit tests for all components | High | 10 | All |
| 20 | Integration Tests | Create integration tests with mock TWS server | Medium | 7 | All |
| 21 | Documentation | Create detailed API documentation and examples | Medium | 5 | All |
| 22 | Performance Tuning | Optimize critical paths for performance | Low | 5 | All |

## Phased Implementation Plan

### Phase 1: Core Infrastructure (Weeks 1-3)
- Tasks 1-9: Implement the core messaging infrastructure
- Goal: Establish a stable foundation for the API components

### Phase 2: API Functionality (Weeks 4-6)
- Tasks 10-12: Implement the domain-specific APIs
- Task 15: Comprehensive error handling
- Task 16: Rate limiting
- Goal: Provide basic trading functionality

### Phase 3: Logging and Replay (Weeks 7-8)
- Tasks 13-14: Implement logging and replay functionality
- Task 18: Proxy server
- Goal: Enable debugging and testing capabilities

### Phase 4: Python Integration and Testing (Weeks 9-12)
- Task 17: Python bindings
- Tasks 19-21: Testing and documentation
- Task 22: Performance tuning
- Goal: Production-ready library with Python support

## Critical Path

The critical path for initial functionality is:
1. Core Data Structures (Task 1)
2. Message Protocol (Task 3)
3. ConnectionManager (Task 4)
4. MessageQueue (Task 5)
5. RequestManager (Task 6)
6. ResponseRouter (Task 8)
7. MessageBroker (Task 9)
8. OrderAPI (Task 10)
9. Python Bindings (Task 17)
10. Unit Tests (Task 19)

Focusing on these tasks first will enable basic order placement and management functionality in Python as quickly as possible, with other features added incrementally.

## Resource Allocation

The implementation requires:
- 1-2 Rust developers with experience in systems programming and network protocols
- 1 developer with experience in financial APIs, particularly IBKR
- 1 developer with experience in Python binding creation (PyO3)

Total estimated effort: Approximately 85 developer-days (17 weeks with a single developer, or 8-9 weeks with two developers working in parallel)# IBKR API Design Document

## 1. Introduction

This document outlines the design for a robust, easy-to-use interface to the Interactive Brokers (IBKR) API. The current synchronous API suffers from deadlock issues and poor multiplexing of the underlying asynchronous message protocol. The new design addresses these problems while maintaining a synchronous interface for Python compatibility.

## 2. Design Goals

- **Robustness**: Eliminate deadlocks and handle network errors gracefully
- **Simplicity**: Provide an intuitive API that abstracts the complexity of the IBKR message protocol
- **Observability**: Enable logging, replay, and debugging of all API interactions
- **Background Processing**: Handle notifications and state updates without blocking the main thread
- **Rate Limiting**: Implement proper throttling to respect IBKR API limits

## 3. System Architecture

### 3.1 High-Level Components

```
┌───────────▼───────────────────────────┐
│ IBKRClient (Main API Entry Point)      │
├───────────┬───────────────┬───────────┤
│ OrderAPI  │ AccountAPI    │ DataAPI   │
└───────────┴───────────────┴───────────┘
            │
┌───────────▼───────────────────────────┐
│ MessageBroker                          │
├─────────────────────────────────────┬─┤
│ RequestManager                      │ │
├─────────────────────────────────────┤ │
│ ResponseRouter                      │ │
├─────────────────────────────────────┤ │
│ NotificationManager                 │ │
└─────────────────────────────────────┴─┘
            │
┌───────────▼───────────────────────────┐
│ ConnectionManager                      │
└───────────▲───────────────────────────┘
            │
┌───────────▼───────────────────────────┐
│ IBKR TWS/Gateway                       │
└───────────────────────────────────────┘
```

### 3.2 Component Descriptions

#### 3.2.1 IBKRClient

The main entry point for applications. Provides access to specialized APIs for orders, account information, and market data. Manages connection lifecycle and high-level error handling.

#### 3.2.2 OrderAPI

Handles all order-related operations including placement, modification, and cancellation. Maintains an in-memory order book to track the status of all orders.

#### 3.2.3 AccountAPI

Provides access to account information, positions, and P&L data. Maintains a cache of account state that updates based on notifications.

#### 3.2.4 DataAPI

Handles requests for market data, historical data, and news. Implements rate limiting to avoid overwhelming the IBKR API.

#### 3.2.5 MessageBroker

Core component that manages the asynchronous message protocol while presenting a synchronous interface. Consists of:

- **RequestManager**: Handles outgoing requests, assigns request IDs, and tracks pending requests
- **ResponseRouter**: Routes incoming messages to the appropriate handlers
- **NotificationManager**: Processes unsolicited notifications and updates internal state

#### 3.2.6 ConnectionManager

Handles the low-level socket connection to IBKR TWS/Gateway. Manages reconnection logic and connection health checks.

## 4. Detailed Design

### 4.1 IBKRClient

```rust
pub struct IBKRClient {
    order_mgr: Arc<OrderManager>,
    account_mgr: Arc<AccountManager>,
    data_mgr: Arc<DataManager>,
    message_broker: Arc<MessageBroker>,
    connection_manager: Arc<ConnectionManager>,
    config: ClientConfig,
    logger: Option<Arc<dyn Logger + Send + Sync>>,
}

impl IBKRClient {
    pub fn new(host: &str, port: i32, client_id: i32, config: ClientConfig) -> Result<Self, IBKRError> {
        let connection_manager = Arc::new(ConnectionManager::new(host, port, client_id, ConnectionConfig::default())?);
        let message_broker = Arc::new(MessageBroker::new(connection_manager.clone())?);

        let order_mgr = Arc::new(OrderManager::new(message_broker.clone()));
        let account_mgr = Arc::new(AccountManager::new(message_broker.clone()));
        let data_mgr = Arc::new(DataManager::new(message_broker.clone()));

        Ok(IBKRClient {
            order_mgr,
            account_mgr,
            data_mgr,
            message_broker,
            connection_manager,
            config,
            logger: None,
        })
    }

    pub fn connect(&mut self) -> Result<(), IBKRError>;
    pub fn disconnect(&mut self);
    pub fn is_connected(&self) -> bool;

    pub fn orders(&self) -> Arc<OrderAPI> {
        self.order_api.clone()
    }

    pub fn account(&self) -> Arc<AccountAPI> {
        self.account_api.clone()
    }

    pub fn data(&self) -> Arc<DataAPI> {
        self.data_api.clone()
    }

    pub fn set_logging(&mut self, enabled: bool, db_path: Option<&str>) -> Result<(), IBKRError>;
    pub fn enable_replay_mode(&mut self, db_path: &str, session_id: i64) -> Result<(), IBKRError>;
}
```

### 4.2 Order API

```rust
pub struct OrderManager {
    message_broker: Arc<MessageBroker>,
    order_book: RwLock<HashMap<String, Arc<RwLock<Order>>>>,
    observers: RwLock<Vec<Box<dyn OrderObserver + Send + Sync>>>,
}

impl OrderManager {
    pub fn new(message_broker: Arc<MessageBroker>) -> Self {
        OrderManager {
            message_broker,
            order_book: RwLock::new(HashMap::new()),
            observers: RwLock::new(Vec::new()),
        }
    }

    // Order placement methods: return the order ID.
    pub fn place_market_order(&self, symbol: &str, quantity: f64, side: OrderSide) -> Result<String, IBKRError>;
    pub fn place_limit_order(&self, symbol: &str, quantity: f64, price: f64, side: OrderSide) -> Result<String, IBKRError>;
    pub fn place_stop_order(&self, symbol: &str, quantity: f64, stop_price: f64, side: OrderSide) -> Result<String, IBKRError>;
    pub fn place_custom_order(&self, contract: Contract, order: OrderRequest) -> Result<String, IBKRError>;

    // Order management methods
    pub fn cancel_order(&self, order_id: &str) -> Result<bool, IBKRError>;
    pub fn modify_order(&self, order_id: &str, updates: OrderUpdates) -> Result<Arc<RwLock<Order>>, IBKRError>;
    pub fn get_order(&self, order_id: &str) -> Option<Order>;
    pub fn get_all_orders(&self) -> Vec<Order>;
    pub fn get_open_orders(&self) -> Vec<Order>;

    // Subscription methods
    pub fn add_observer<T: OrderObserver + Send + Sync + 'static>(&self, observer: T) -> usize;
    pub fn remove_observer(&self, observer_id: usize) -> bool;

    // Utility methods
    pub fn refresh(&self) -> Result<(), IBKRError>;  // Force refresh of order book
}

// Structures
pub struct Order {
    pub id: String,
    pub contract: Contract,
    pub request: OrderRequest,
    pub state: OrderState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct OrderState {
    pub status: OrderStatus,
    pub filled_quantity: f64,
    pub remaining_quantity: f64,
    pub average_fill_price: f64,
    pub last_fill_price: Option<f64>,
    pub why_held: Option<String>,
    pub error: Option<IBKRError>,
}

pub enum OrderStatus {
    New,
    PendingSubmit,
    PendingCancel,
    PreSubmitted,
    Submitted,
    ApiPending,
    ApiCancelled,
    Cancelled,
    Filled,
    Inactive,
}

// Observer pattern
pub trait OrderObserver: Send + Sync {
    fn on_order_update(&self, order: &Order);
    fn on_order_error(&self, order_id: &str, error: &IBKRError);
}
```

### 4.3 Account API

```rust
pub struct AccountManager {
    message_broker: Arc<MessageBroker>,
    account_state: RwLock<AccountState>,
    positions: RwLock<HashMap<String, Arc<RwLock<Position>>>>,
    observers: RwLock<Vec<Box<dyn AccountObserver + Send + Sync>>>,
}

impl AccountManager {
    pub fn new(message_broker: Arc<MessageBroker>) -> Self {
        AccountManager {
            message_broker,
            account_state: RwLock::new(AccountState::default()),
            positions: RwLock::new(HashMap::new()),
            observers: RwLock::new(Vec::new()),
        }
    }

    // Account information methods
    pub fn get_account_info(&self) -> Result<AccountInfo, IBKRError>;
    pub fn get_buying_power(&self) -> Result<f64, IBKRError>;
    pub fn get_cash_balance(&self) -> Result<f64, IBKRError>;
    pub fn get_equity(&self) -> Result<f64, IBKRError>;

    // Position methods
    pub fn list_open_positions(&self) -> Result<Vec<Position>, IBKRError>;

    // PnL methods
    pub fn get_daily_pnl(&self) -> Result<f64, IBKRError>;
    pub fn get_unrealized_pnl(&self) -> Result<f64, IBKRError>;
    pub fn get_realized_pnl(&self) -> Result<f64, IBKRError>;

    // Trade confirmation methods
    pub fn get_executions(&self, from_date: Option<DateTime<Utc>>) -> Result<Vec<Execution>, IBKRError>;

    // Subscription methods
    pub fn add_observer<T: AccountObserver + Send + Sync + 'static>(&self, observer: T) -> usize;
    pub fn remove_observer(&self, observer_id: usize) -> bool;

    // Manual refresh
    pub fn refresh(&self) -> Result<(), IBKRError>;
}

// Structures
pub struct AccountInfo {
    pub account_id: String,
    pub account_type: String,
    pub base_currency: String,
    pub equity: f64,
    pub buying_power: f64,
    pub cash_balance: f64,
    pub day_trades_remaining: i32,
    pub leverage: f64,
    pub maintenance_margin: f64,
    pub initial_margin: f64,
    pub excess_liquidity: f64,
    pub updated_at: DateTime<Utc>,
}

pub struct Position {
    pub symbol: String,
    pub contract: Contract,
    pub quantity: f64,
    pub average_cost: f64,
    pub market_price: f64,
    pub market_value: f64,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
    pub updated_at: DateTime<Utc>,
}

pub struct Execution {
    pub execution_id: String,
    pub order_id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub quantity: f64,
    pub price: f64,
    pub time: DateTime<Utc>,
    pub commission: f64,
}

// Observer pattern
pub trait AccountObserver: Send + Sync {
    fn on_account_update(&self, account_info: &AccountInfo);
    fn on_position_update(&self, position: &Position);
    fn on_execution(&self, execution: &Execution);
}
```

### 4.4 Data API

```rust
pub struct DataManager {
    message_broker: Arc<MessageBroker>,
    rate_limiter: RwLock<RateLimiter>,
    market_data_subscriptions: RwLock<HashMap<String, Arc<RwLock<MarketDataSubscription>>>>,
}

impl DataManager {
    pub fn new(message_broker: Arc<MessageBroker>) -> Self {
        DataManager {
            message_broker,
            rate_limiter: RwLock::new(RateLimiter::new(50, Duration::from_secs(1))), // Default 50 requests per second
            market_data_subscriptions: RwLock::new(HashMap::new()),
        }
    }

    // Market data methods
    pub fn subscribe_market_data(&self, contract: &Contract, data_types: &[MarketDataType])
        -> Result<Arc<RwLock<MarketDataSubscription>>, IBKRError>;
    pub fn unsubscribe_market_data(&self, subscription_id: &str) -> Result<(), IBKRError>;

    // Historical data methods
    pub fn get_historical_data(
        &self,
        contract: &Contract,
        end_date_time: Option<DateTime<Utc>>,
        duration: Duration,
        bar_size: BarSize,
        what_to_show: WhatToShow,
        use_rth: bool
    ) -> Result<Vec<Bar>, IBKRError>;

    // Contract details
    pub fn get_contract_details(&self, contract: &Contract) -> Result<Vec<ContractDetails>, IBKRError>;

    // News
    pub fn subscribe_news(&self, sources: &[&str]) -> Result<NewsSubscription, IBKRError>;
    pub fn get_historical_news(
        &self,
        contract_id: i32,
        provider_codes: &[&str],
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        max_results: usize
    ) -> Result<Vec<NewsArticle>, IBKRError>;

    // Utility methods
    pub fn set_rate_limit(&self, requests_per_second: usize) {
        let mut rate_limiter = self.rate_limiter.write().unwrap();
        *rate_limiter = RateLimiter::new(requests_per_second, Duration::from_secs(1));
    }
}

// Structures
pub struct MarketDataSubscription {
    pub id: String,
    pub contract: Contract,
    pub data_types: Vec<MarketDataType>,
    pub last_price: Option<f64>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub bid_size: Option<i32>,
    pub ask_size: Option<i32>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub volume: Option<i64>,
    pub observers: RwLock<Vec<Box<dyn MarketDataObserver + Send + Sync>>>,
}

pub enum MarketDataType {
    BidAsk,
    LastPrice,
    HighLow,
    Volume,
    HistoricalVolatility,
    ImpliedVolatility,
    OptionChain,
    // ...
}

pub struct Bar {
    pub time: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: i64,
    pub wap: f64,
    pub count: i32,
}

pub struct NewsArticle {
    pub id: String,
    pub time: DateTime<Utc>,
    pub provider_code: String,
    pub article_id: String,
    pub headline: String,
    pub summary: Option<String>,
    pub content: Option<String>,
}

pub struct NewsSubscription {
    pub id: String,
    pub sources: Vec<String>,
    pub observers: Vec<Box<dyn NewsObserver>>,
}

// Observer patterns
pub trait MarketDataObserver: Send + Sync {
    fn on_price_update(&self, subscription: &MarketDataSubscription);
}

pub trait NewsObserver: Send + Sync {
    fn on_news_article(&self, article: &NewsArticle);
}
```

### 4.5 MessageBroker

```rust
pub struct MessageBroker {
    request_manager: Arc<RequestManager>,
    response_router: Arc<ResponseRouter>,
    notification_manager: Arc<NotificationManager>,
    connection_manager: Arc<ConnectionManager>,
    message_queue: Arc<MessageQueue>,
    worker_thread: Mutex<Option<JoinHandle<()>>>,
}

impl MessageBroker {
    pub fn new(connection_manager: Arc<ConnectionManager>) -> Result<Self, IBKRError> {
        let message_queue = Arc::new(MessageQueue::new());
        let request_manager = Arc::new(RequestManager::new());
        let notification_manager = Arc::new(NotificationManager::new());

        let response_router = Arc::new(ResponseRouter::new(
            message_queue.clone(),
            request_manager.clone(),
            notification_manager.clone()
        ));

        Ok(MessageBroker {
            request_manager,
            response_router,
            notification_manager,
            connection_manager,
            message_queue,
            worker_thread: Mutex::new(None),
        })
    }

    pub fn start(&self) -> Result<(), IBKRError> {
        let mut worker_thread = self.worker_thread.lock().unwrap();
        if worker_thread.is_some() {
            return Err(IBKRError::AlreadyRunning("Message broker already running".to_string()));
        }

        // Start response router
        self.response_router.start()?;

        let response_router = self.response_router.clone();
        let message_queue = self.message_queue.clone();
        let connection_manager = self.connection_manager.clone();

        // Create worker thread
        let handle = std::thread::spawn(move || {
            loop {
                // Read messages from connection
                if let Ok(Some(message)) = connection_manager.receive_message(Duration::from_millis(100)) {
                    // Add to message queue
                    message_queue.push(message);
                }
            }
        });

        *worker_thread = Some(handle);

        Ok(())
    }

    pub fn stop(&self) {
        let mut worker_thread = self.worker_thread.lock().unwrap();
        if let Some(handle) = worker_thread.take() {
            // Clean shutdown would be better, but this works for now
            handle.thread().unpark();
        }

        self.response_router.stop();
    }

    // Main methods for sending requests and receiving responses
    pub fn send_request<T: Request + 'static>(&self, request: T) -> Result<T::Response, IBKRError> {
        let request_id = self.request_manager.get_next_request_id();

        // Create a channel for the response
        let (tx, rx) = std::sync::mpsc::channel();

        // Register the pending request
        let pending_request = PendingRequest {
            id: request_id,
            response_sender: Box::new(move |response| {
                let typed_response = response.downcast::<T::Response>().unwrap();
                let _ = tx.send(*typed_response);
            }),
            timeout: Instant::now() + Duration::from_secs(30), // 30 second timeout
        };

        self.request_manager.add_pending_request(request_id, pending_request)?;

        // Send the request
        let message = request.to_message(request_id);
        self.connection_manager.send_message(message)?;

        // Wait for the response
        match rx.recv_timeout(Duration::from_secs(30)) {
            Ok(response) => Ok(response),
            Err(_) => {
                self.request_manager.cancel_request(request_id)?;
                Err(IBKRError::Timeout("Request timed out".to_string()))
            }
        }
    }

    pub fn send_request_async<T: Request + 'static>(&self, request: T) -> RequestHandle<T::Response> {
        let request_id = self.request_manager.get_next_request_id();

        // Create a channel for the response
        let (tx, rx) = std::sync::mpsc::channel();

        // Register the pending request
        let pending_request = PendingRequest {
            id: request_id,
            response_sender: Box::new(move |response| {
                let typed_response = response.downcast::<T::Response>().unwrap();
                let _ = tx.send(*typed_response);
            }),
            timeout: Instant::now() + Duration::from_secs(30), // 30 second timeout
        };

        if let Err(e) = self.request_manager.add_pending_request(request_id, pending_request) {
            return RequestHandle::new_error(e);
        }

        // Send the request
        let message = request.to_message(request_id);
        if let Err(e) = self.connection_manager.send_message(message) {
            let _ = self.request_manager.cancel_request(request_id);
            return RequestHandle::new_error(e);
        }

        RequestHandle::new(rx)
    }

    // Registration for notification handlers
    pub fn register_notification_handler<T: Notification + 'static, H: NotificationHandler<T> + Send + Sync + 'static>(
        &self,
        handler: H
    ) -> usize {
        self.notification_manager.register_handler(handler)
    }

    pub fn unregister_notification_handler<T: Notification + 'static>(&self, handler_id: usize) -> bool {
        self.notification_manager.unregister_handler::<T>(handler_id)
    }
}

// Request handling
pub struct RequestManager {
    next_request_id: AtomicI32,
    pending_requests: RwLock<HashMap<i32, PendingRequest>>,
    request_timeout: Duration,
}

impl RequestManager {
    pub fn new() -> Self {
        RequestManager {
            next_request_id: AtomicI32::new(1),
            pending_requests: RwLock::new(HashMap::new()),
            request_timeout: Duration::from_secs(30),
        }
    }

    pub fn get_next_request_id(&self) -> i32 {
        self.next_request_id.fetch_add(1, Ordering::SeqCst)
    }

    pub fn add_pending_request(&self, id: i32, request: PendingRequest) -> Result<(), IBKRError> {
        let mut requests = self.pending_requests.write().unwrap();
        if requests.contains_key(&id) {
            return Err(IBKRError::DuplicateRequestId(id));
        }
        requests.insert(id, request);
        Ok(())
    }

    pub fn complete_request(&self, id: i32, response: Box<dyn Any + Send>) -> Result<(), IBKRError> {
        let mut requests = self.pending_requests.write().unwrap();
        if let Some(request) = requests.remove(&id) {
            (request.response_sender)(response);
            Ok(())
        } else {
            Err(IBKRError::UnknownRequestId(id))
        }
    }

    pub fn cancel_request(&self, id: i32) -> Result<(), IBKRError> {
        let mut requests = self.pending_requests.write().unwrap();
        if requests.remove(&id).is_some() {
            Ok(())
        } else {
            Err(IBKRError::UnknownRequestId(id))
        }
    }

    pub fn check_timeouts(&self) {
        let now = Instant::now();
        let mut timed_out = Vec::new();

        // Find timed out requests
        {
            let requests = self.pending_requests.read().unwrap();
            for (id, request) in requests.iter() {
                if now > request.timeout {
                    timed_out.push(*id);
                }
            }
        }

        // Cancel timed out requests
        if !timed_out.is_empty() {
            let mut requests = self.pending_requests.write().unwrap();
            for id in timed_out {
                requests.remove(&id);
            }
        }
    }
}

// Response routing
pub struct ResponseRouter {
    message_queue: Arc<MessageQueue>,
    request_manager: Arc<RequestManager>,
    notification_manager: Arc<NotificationManager>,
    worker_thread: Mutex<Option<JoinHandle<()>>>,
}

impl ResponseRouter {
    pub fn new(
        message_queue: Arc<MessageQueue>,
        request_manager: Arc<RequestManager>,
        notification_manager: Arc<NotificationManager>
    ) -> Self {
        ResponseRouter {
            message_queue,
            request_manager,
            notification_manager,
            worker_thread: Mutex::new(None),
        }
    }

    pub fn start(&self) -> Result<(), IBKRError> {
        let mut worker_thread = self.worker_thread.lock().unwrap();
        if worker_thread.is_some() {
            return Err(IBKRError::AlreadyRunning("Response router already running".to_string()));
        }

        let message_queue = self.message_queue.clone();
        let request_manager = self.request_manager.clone();
        let notification_manager = self.notification_manager.clone();

        let handle = std::thread::spawn(move || {
            loop {
                // Get messages from queue
                if let Some(message) = message_queue.pop(Duration::from_millis(100)) {
                    // Process message
                    Self::process_message_static(
                        &message,
                        &request_manager,
                        &notification_manager
                    );
                }

                // Check for timed out requests
                request_manager.check_timeouts();
            }
        });

        *worker_thread = Some(handle);

        Ok(())
    }

    pub fn stop(&self) {
        let mut worker_thread = self.worker_thread.lock().unwrap();
        if let Some(handle) = worker_thread.take() {
            // Clean shutdown would be better
            handle.thread().unpark();
        }
    }

    fn process_message_static(
        message: &IBKRRawMessage,
        request_manager: &Arc<RequestManager>,
        notification_manager: &Arc<NotificationManager>
    ) {
        // Process the message based on its type
        match message.message_type() {
            // If it's a response to a request
            MessageType::Response => {
                if let Some(request_id) = message.request_id() {
                    // Parse the response based on the message content
                    if let Some(response) = Self::parse_response(message) {
                        // Complete the request
                        let _ = request_manager.complete_request(request_id, response);
                    }
                }
            }

            // If it's a notification
            MessageType::Notification => {
                // Parse the notification based on the message content
                if let Some(notification) = Self::parse_notification(message) {
                    // Process the notification
                    notification_manager.process_notification(notification);
                }
            }

            // Ignore other message types
            _ => {}
        }
    }

    fn parse_response(message: &IBKRRawMessage) -> Option<Box<dyn Any + Send>> {
        // Implementation depends on the message format
        // This is a simplified example
        None
    }

    fn parse_notification(message: &IBKRRawMessage) -> Option<Box<dyn Any + Send>> {
        // Implementation depends on the message format
        // This is a simplified example
        None
    }
}

// Notification handling
pub struct NotificationManager {
    handlers: RwLock<HashMap<TypeId, Vec<Box<dyn AnyNotificationHandler + Send + Sync>>>>,
}

impl NotificationManager {
    pub fn new() -> Self {
        NotificationManager {
            handlers: RwLock::new(HashMap::new()),
        }
    }

    pub fn register_handler<T: Notification + 'static, H: NotificationHandler<T> + Send + Sync + 'static>(
        &self,
        handler: H
    ) -> usize {
        let type_id = TypeId::of::<T>();
        let mut handlers = self.handlers.write().unwrap();

        let entry = handlers.entry(type_id).or_insert_with(Vec::new);
        let handler_id = entry.len();

        // Wrap the handler in a type-erased version
        let wrapper = Box::new(NotificationHandlerWrapper {
            handler: Box::new(handler),
            id: handler_id,
        }) as Box<dyn AnyNotificationHandler + Send + Sync>;

        entry.push(wrapper);

        handler_id
    }

    pub fn unregister_handler<T: Notification + 'static>(&self, handler_id: usize) -> bool {
        let type_id = TypeId::of::<T>();
        let mut handlers = self.handlers.write().unwrap();

        if let Some(type_handlers) = handlers.get_mut(&type_id) {
            if handler_id < type_handlers.len() {
                // Mark as removed (we don't actually remove to maintain IDs)
                type_handlers[handler_id] = Box::new(RemovedHandler {}) as Box<dyn AnyNotificationHandler + Send + Sync>;
                return true;
            }
        }

        false
    }

    pub fn process_notification(&self, notification: Box<dyn Any + Send>) {
        // Determine the notification type
        let type_id = (*notification).type_id();

        // Get handlers for this notification type
        let handlers = self.handlers.read().unwrap();
        if let Some(type_handlers) = handlers.get(&type_id) {
            for handler in type_handlers {
                handler.handle_notification(&*notification);
            }
        }
    }
}

/// Type-erased notification handler
trait AnyNotificationHandler: Send + Sync {
    fn handle_notification(&self, notification: &dyn Any);
    fn get_id(&self) -> usize;
}

/// Type-specific notification handler
pub trait NotificationHandler<T: Notification>: Send + Sync {
    fn on_notification(&self, notification: &T);
}

/// Wrapper for type-specific handlers
struct NotificationHandlerWrapper<T: Notification + 'static> {
    handler: Box<dyn NotificationHandler<T> + Send + Sync>,
    id: usize,
}

impl<T: Notification + 'static> AnyNotificationHandler for NotificationHandlerWrapper<T> {
    fn handle_notification(&self, notification: &dyn Any) {
        if let Some(typed_notification) = notification.downcast_ref::<T>() {
            self.handler.on_notification(typed_notification);
        }
    }

    fn get_id(&self) -> usize {
        self.id
    }
}

/// Placeholder for removed handlers
struct RemovedHandler {}

impl AnyNotificationHandler for RemovedHandler {
    fn handle_notification(&self, _notification: &dyn Any) {
        // Do nothing
    }

    fn get_id(&self) -> usize {
        usize::MAX
    }
}

// Message queue for internal communication
pub struct MessageQueue {
    queue: Mutex<VecDeque<IBKRRawMessage>>,
    condition: Condvar,
}

impl MessageQueue {
    pub fn new() -> Self {
        MessageQueue {
            queue: Mutex::new(VecDeque::new()),
            condition: Condvar::new(),
        }
    }

    pub fn push(&self, message: IBKRRawMessage) {
        let mut queue = self.queue.lock().unwrap();
        queue.push_back(message);
        self.condition.notify_one();
    }

    pub fn pop(&self, timeout: Duration) -> Option<IBKRRawMessage> {
        let mut queue = self.queue.lock().unwrap();

        if queue.is_empty() {
            // Wait for a notification or timeout
            let (new_queue, _) = self.condition.wait_timeout(queue, timeout).unwrap();
            queue = new_queue;
        }

        queue.pop_front()
    }

    pub fn pop_all(&self) -> Vec<IBKRRawMessage> {
        let mut queue = self.queue.lock().unwrap();
        let messages: Vec<IBKRRawMessage> = queue.drain(..).collect();
        messages
    }

    pub fn len(&self) -> usize {
        let queue = self.queue.lock().unwrap();
        queue.len()
    }

    pub fn is_empty(&self) -> bool {
        let queue = self.queue.lock().unwrap();
        queue.is_empty()
    }
}
```

### 4.6 ConnectionManager

```rust
pub struct ConnectionManager {
    host: String,
    port: i32,
    client_id: i32,
    socket: Mutex<Option<TcpStream>>,
    reader_thread: Mutex<Option<JoinHandle<()>>>,
    writer_thread: Mutex<Option<JoinHandle<()>>>,
    send_queue: Arc<MessageQueue>,
    receive_queue: Arc<MessageQueue>,
    connection_state: AtomicU8, // 0=disconnected, 1=connecting, 2=connected, 3=error
    reconnect_config: ReconnectConfig,
    logger: Mutex<Option<Arc<dyn Logger + Send + Sync>>>,
}

impl ConnectionManager {
    pub fn new(host: &str, port: i32, client_id: i32, config: ConnectionConfig) -> Result<Self, IBKRError> {
        Ok(ConnectionManager {
            host: host.to_string(),
            port,
            client_id,
            socket: Mutex::new(None),
            reader_thread: Mutex::new(None),
            writer_thread: Mutex::new(None),
            send_queue: Arc::new(MessageQueue::new()),
            receive_queue: Arc::new(MessageQueue::new()),
            connection_state: AtomicU8::new(0), // disconnected
            reconnect_config: config.reconnect_config,
            logger: Mutex::new(None),
        })
    }

    pub fn connect(&self) -> Result<(), IBKRError> {
        // Ensure we're not already connected
        if self.connection_state.load(Ordering::SeqCst) == 2 {
            return Err(IBKRError::AlreadyConnected);
        }

        // Set state to connecting
        self.connection_state.store(1, Ordering::SeqCst);

        // Create socket connection
        let socket = match TcpStream::connect(format!("{}:{}", self.host, self.port)) {
            Ok(socket) => socket,
            Err(e) => {
                self.connection_state.store(3, Ordering::SeqCst);
                return Err(IBKRError::ConnectionFailed(e.to_string()));
            }
        };

        // Configure socket
        socket.set_nodelay(true).map_err(|e| IBKRError::SocketError(e.to_string()))?;
        socket.set_read_timeout(Some(Duration::from_secs(1))).map_err(|e| IBKRError::SocketError(e.to_string()))?;
        socket.set_write_timeout(Some(Duration::from_secs(1))).map_err(|e| IBKRError::SocketError(e.to_string()))?;

        // Store socket
        let mut socket_guard = self.socket.lock().unwrap();
        *socket_guard = Some(socket);
        drop(socket_guard);

        // Start reader thread
        let reader_socket = self.socket.lock().unwrap().as_ref().unwrap().try_clone()
            .map_err(|e| IBKRError::SocketError(e.to_string()))?;
        let receive_queue = self.receive_queue.clone();
        let connection_state = Arc::new(self.connection_state.clone());
        let logger = self.logger.lock().unwrap().clone();

        let reader_handle = std::thread::spawn(move || {
            Self::socket_reader(reader_socket, receive_queue, connection_state, logger);
        });

        let mut reader_thread = self.reader_thread.lock().unwrap();
        *reader_thread = Some(reader_handle);
        drop(reader_thread);

        // Start writer thread
        let writer_socket = self.socket.lock().unwrap().as_ref().unwrap().try_clone()
            .map_err(|e| IBKRError::SocketError(e.to_string()))?;
        let send_queue = self.send_queue.clone();
        let connection_state = Arc::new(self.connection_state.clone());
        let logger = self.logger.lock().unwrap().clone();

        let writer_handle = std::thread::spawn(move || {
            Self::socket_writer(writer_socket, send_queue, connection_state, logger);
        });

        let mut writer_thread = self.writer_thread.lock().unwrap();
        *writer_thread = Some(writer_handle);
        drop(writer_thread);

        // Set state to connected
        self.connection_state.store(2, Ordering::SeqCst);

        // Send initial handshake message
        self.send_handshake()?;

        Ok(())
    }

    pub fn disconnect(&self) {
        // Set state to disconnecting
        self.connection_state.store(0, Ordering::SeqCst);

        // Close socket
        let mut socket_guard = self.socket.lock().unwrap();
        *socket_guard = None;
        drop(socket_guard);

        // Wait for reader thread to exit
        let mut reader_thread = self.reader_thread.lock().unwrap();
        if let Some(handle) = reader_thread.take() {
            let _ = handle.join();
        }
        drop(reader_thread);

        // Wait for writer thread to exit
        let mut writer_thread = self.writer_thread.lock().unwrap();
        if let Some(handle) = writer_thread.take() {
            let _ = handle.join();
        }
        drop(writer_thread);
    }

    pub fn is_connected(&self) -> bool {
        self.connection_state.load(Ordering::SeqCst) == 2
    }

    pub fn send_message(&self, message: IBKRRawMessage) -> Result<(), IBKRError> {
        if !self.is_connected() {
            return Err(IBKRError::NotConnected);
        }

        // Log outgoing message
        if let Some(logger) = self.logger.lock().unwrap().as_ref() {
            logger.log_message(MessageDirection::Outgoing, &message);
        }

        // Add to send queue
        self.send_queue.push(message);

        Ok(())
    }

    pub fn receive_message(&self, timeout: Duration) -> Result<Option<IBKRRawMessage>, IBKRError> {
        if !self.is_connected() {
            return Err(IBKRError::NotConnected);
        }

        // Get message from receive queue
        let message = self.receive_queue.pop(timeout);

        // Log incoming message
        if let Some(msg) = &message {
            if let Some(logger) = self.logger.lock().unwrap().as_ref() {
                logger.log_message(MessageDirection::Incoming, msg);
            }
        }

        Ok(message)
    }

    pub fn set_logger(&self, logger: Option<Arc<dyn Logger + Send + Sync>>) {
        let mut logger_guard = self.logger.lock().unwrap();
        *logger_guard = logger;
    }

    fn send_handshake(&self) -> Result<(), IBKRError> {
        // Construct and send client version
        let client_version = format!("v100..{}", self.client_id);
        let handshake = IBKRRawMessage::new_client_version(client_version);
        self.send_message(handshake)
    }

    fn socket_reader(
        mut socket: TcpStream,
        queue: Arc<MessageQueue>,
        connection_state: Arc<AtomicU8>,
        logger: Option<Arc<dyn Logger + Send + Sync>>
    ) {
        let mut buffer = [0u8; 4096];
        let mut message_buffer = Vec::new();

        while connection_state.load(Ordering::SeqCst) == 2 {
            match socket.read(&mut buffer) {
                Ok(0) => {
                    // Connection closed
                    connection_state.store(0, Ordering::SeqCst);
                    break;
                },
                Ok(n) => {
                    // Append to message buffer
                    message_buffer.extend_from_slice(&buffer[0..n]);

                    // Process complete messages
                    while let Some(pos) = message_buffer.iter().position(|&b| b == b'\0') {
                        if pos > 0 {
                            // Extract message
                            let message_data = message_buffer.drain(0..pos).collect::<Vec<u8>>();

                            // Skip null terminator
                            message_buffer.drain(0..1);

                            // Parse message
                            match IBKRRawMessage::parse(&message_data) {
                                Ok(message) => {
                                    // Log message
                                    if let Some(ref logger) = logger {
                                        logger.log_message(MessageDirection::Incoming, &message);
                                    }

                                    // Add to queue
                                    queue.push(message);
                                },
                                Err(e) => {
                                    if let Some(ref logger) = logger {
                                        logger.log_error(&IBKRError::ParseError(e.to_string()));
                                    }
                                }
                            }
                        } else {
                            // Empty message, skip null terminator
                            message_buffer.drain(0..1);
                        }
                    }
                },
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // Socket timeout, continue
                    continue;
                },
                Err(e) => {
                    // Socket error
                    if let Some(ref logger) = logger {
                        logger.log_error(&IBKRError::SocketError(e.to_string()));
                    }

                    connection_state.store(3, Ordering::SeqCst);
                    break;
                }
            }
        }
    }

    fn socket_writer(
        mut socket: TcpStream,
        queue: Arc<MessageQueue>,
        connection_state: Arc<AtomicU8>,
        logger: Option<Arc<dyn Logger + Send + Sync>>
    ) {
        while connection_state.load(Ordering::SeqCst) == 2 {
            // Get message from queue
            if let Some(message) = queue.pop(Duration::from_millis(100)) {
                // Serialize message
                let mut data = message.serialize();
                data.push(0); // Null terminator

                // Send message
                match socket.write_all(&data) {
                    Ok(_) => {
                        // Flush socket
                        if let Err(e) = socket.flush() {
                            if let Some(ref logger) = logger {
                                logger.log_error(&IBKRError::SocketError(e.to_string()));
                            }
                        }
                    },
                    Err(e) => {
                        // Socket error
                        if let Some(ref logger) = logger {
                            logger.log_error(&IBKRError::SocketError(e.to_string()));
                        }

                        connection_state.store(3, Ordering::SeqCst);
                        break;
                    }
                }
            }
        }
    }
}
```

### 4.7 SQLite Logging and Replay

```rust
// Database schema for message logging
/*
CREATE TABLE connection_sessions (
    session_id INTEGER PRIMARY KEY AUTOINCREMENT,
    start_time TIMESTAMP NOT NULL,
    end_time TIMESTAMP,
    host TEXT NOT NULL,
    port INTEGER NOT NULL,
    client_id INTEGER NOT NULL,
    status TEXT NOT NULL  -- "active", "closed", "error"
);

CREATE TABLE messages (
    message_id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER NOT NULL,
    timestamp TIMESTAMP NOT NULL,
    direction TEXT NOT NULL,  -- "outgoing" or "incoming"
    request_id INTEGER,       -- Can be NULL for some messages
    message_type TEXT NOT NULL,
    message_content BLOB NOT NULL,
    FOREIGN KEY (session_id) REFERENCES connection_sessions(session_id)
);

CREATE TABLE orders (
    order_id TEXT PRIMARY KEY,
    session_id INTEGER NOT NULL,
    contract_symbol TEXT NOT NULL,
    action TEXT NOT NULL,
    quantity REAL NOT NULL,
    order_type TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL,
    limit_price REAL,
    FOREIGN KEY (session_id) REFERENCES connection_sessions(session_id)
);

CREATE TABLE order_status (
    status_id INTEGER PRIMARY KEY AUTOINCREMENT,
    order_id TEXT NOT NULL,
    status TEXT NOT NULL,
    filled_quantity REAL NOT NULL,
    remaining_quantity REAL NOT NULL,
    average_price REAL,
    last_price REAL,
    timestamp TIMESTAMP NOT NULL,
    FOREIGN KEY (order_id) REFERENCES orders(order_id)
);

CREATE TABLE executions (
    execution_id TEXT PRIMARY KEY,
    order_id TEXT NOT NULL,
    time TIMESTAMP NOT NULL,
    symbol TEXT NOT NULL,
    side TEXT NOT NULL,
    quantity REAL NOT NULL,
    price REAL NOT NULL,
    commission REAL,
    FOREIGN KEY (order_id) REFERENCES orders(order_id)
);
*/

pub trait Logger: Send + Sync {
    fn log_message(&self, direction: MessageDirection, message: &IBKRRawMessage);
    fn log_error(&self, error: &IBKRError);
    fn log_info(&self, message: &str);
}

pub enum MessageDirection {
    Outgoing,
    Incoming,
}

pub struct SqliteLogger {
    connection: Arc<Mutex<rusqlite::Connection>>,
    session_id: i64,
}

impl SqliteLogger {
    pub fn new(db_path: &str, host: &str, port: i32, client_id: i32) -> Result<Self, IBKRError> {
        // Create or open the database
        let connection = rusqlite::Connection::open(db_path)
            .map_err(|e| IBKRError::LoggingError(format!("Failed to open SQLite database: {}", e)))?;

        // Initialize schema if needed
        Self::initialize_schema(&connection)?;

        // Create a new session
        let session_id = Self::create_session(&connection, host, port, client_id)?;

        Ok(SqliteLogger {
            connection: Arc::new(Mutex::new(connection)),
            session_id,
        })
    }

    fn initialize_schema(conn: &rusqlite::Connection) -> Result<(), IBKRError> {
        // Execute the CREATE TABLE statements (schema defined above)
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS connection_sessions (
                session_id INTEGER PRIMARY KEY AUTOINCREMENT,
                start_time TIMESTAMP NOT NULL,
                end_time TIMESTAMP,
                host TEXT NOT NULL,
                port INTEGER NOT NULL,
                client_id INTEGER NOT NULL,
                status TEXT NOT NULL
            );
            -- Create other tables and indexes
            -- ..."
        ).map_err(|e| IBKRError::LoggingError(format!("Failed to initialize database schema: {}", e)))?;

        Ok(())
    }

    fn create_session(conn: &rusqlite::Connection, host: &str, port: i32, client_id: i32) -> Result<i64, IBKRError> {
        conn.execute(
            "INSERT INTO connection_sessions (start_time, host, port, client_id, status) VALUES (datetime('now'), ?, ?, ?, 'active')",
            rusqlite::params![host, port, client_id],
        ).map_err(|e| IBKRError::LoggingError(format!("Failed to create session: {}", e)))?;

        Ok(conn.last_insert_rowid())
    }

    pub fn close_session(&self) -> Result<(), IBKRError> {
        let conn = self.connection.lock().unwrap();
        conn.execute(
            "UPDATE connection_sessions SET end_time = datetime('now'), status = 'closed' WHERE session_id = ?",
            [self.session_id],
        ).map_err(|e| IBKRError::LoggingError(format!("Failed to close session: {}", e)))?;

        Ok(())
    }

    pub fn log_message(&self, direction: MessageDirection, message: &IBKRRawMessage) -> Result<(), IBKRError> {
        let conn = self.connection.lock().unwrap();

        // Extract request_id and message_type from the message
        let (request_id, message_type) = self.extract_message_info(message);

        // Serialize the message content
        let message_content = bincode::serialize(message)
            .map_err(|e| IBKRError::LoggingError(format!("Failed to serialize message: {}", e)))?;

        conn.execute(
            "INSERT INTO messages (session_id, timestamp, direction, request_id, message_type, message_content)
             VALUES (?, datetime('now'), ?, ?, ?, ?)",
            rusqlite::params![
                self.session_id,
                direction.to_string(),
                request_id,
                message_type,
                message_content,
            ],
        ).map_err(|e| IBKRError::LoggingError(format!("Failed to log message: {}", e)))?;

        Ok(())
    }

    // Other logging methods for orders, executions, etc.
}

impl Logger for SqliteLogger {
    fn log_message(&self, direction: MessageDirection, message: &IBKRRawMessage) {
        if let Err(e) = self.log_message(direction, message) {
            eprintln!("Error logging message: {}", e);
        }
    }

    fn log_error(&self, error: &IBKRError) {
        eprintln!("IBKR Error: {}", error);
    }

    fn log_info(&self, message: &str) {
        println!("IBKR Info: {}", message);
    }
}

pub struct SqliteReplayConnection {
    db_connection: rusqlite::Connection,
    session_id: i64,
    message_queue: Arc<MessageQueue>,
    replay_thread: Option<JoinHandle<()>>,
    replay_speed: f64,
    paused: AtomicBool,
    current_timestamp: Mutex<Option<DateTime<Utc>>>,
}

impl SqliteReplayConnection {
    pub fn new(db_path: &str, session_id: i64, replay_speed: f64) -> Result<Self, IBKRError> {
        let db_connection = rusqlite::Connection::open(db_path)
            .map_err(|e| IBKRError::ReplayError(format!("Failed to open SQLite database: {}", e)))?;

        // Verify the session exists
        let mut stmt = db_connection.prepare("SELECT COUNT(*) FROM connection_sessions WHERE session_id = ?")
            .map_err(|e| IBKRError::ReplayError(format!("Failed to prepare query: {}", e)))?;

        let count: i64 = stmt.query_row([session_id], |row| row.get(0))
            .map_err(|e| IBKRError::ReplayError(format!("Failed to query session: {}", e)))?;

        if count == 0 {
            return Err(IBKRError::ReplayError(format!("Session {} not found", session_id)));
        }

        Ok(SqliteReplayConnection {
            db_connection,
            session_id,
            message_queue: Arc::new(MessageQueue::new()),
            replay_thread: None,
            replay_speed,
            paused: AtomicBool::new(false),
            current_timestamp: Mutex::new(None),
        })
    }

    pub fn start_replay(&mut self) -> Result<(), IBKRError> {
        if self.replay_thread.is_some() {
            return Err(IBKRError::ReplayError("Replay already running".to_string()));
        }

        let message_queue = self.message_queue.clone();
        let session_id = self.session_id;
        let db_path = self.db_connection.path().unwrap().to_string_lossy().to_string();
        let replay_speed = self.replay_speed;
        let paused = self.paused.clone();
        let current_timestamp = self.current_timestamp.clone();

        let thread = std::thread::spawn(move || {
            let db_conn = rusqlite::Connection::open(db_path).unwrap();

            // Query all messages in timestamp order
            let mut stmt = db_conn.prepare(
                "SELECT timestamp, direction, request_id, message_type, message_content
                 FROM messages
                 WHERE session_id = ?
                 ORDER BY timestamp ASC"
            ).unwrap();

            let message_iter = stmt.query_map([session_id], |row| {
                let timestamp: String = row.get(0)?;
                let direction: String = row.get(1)?;
                let request_id: Option<i32> = row.get(2)?;
                let message_type: String = row.get(3)?;
                let content: Vec<u8> = row.get(4)?;

                Ok((timestamp, direction, request_id, message_type, content))
            }).unwrap();

            let mut previous_time: Option<DateTime<Utc>> = None;

            for message_result in message_iter {
                // Check if replay is paused
                while paused.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(100));
                }

                let (timestamp_str, direction, request_id, message_type, content) = message_result.unwrap();

                // Parse timestamp
                let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
                    .unwrap()
                    .with_timezone(&Utc);

                *current_timestamp.lock().unwrap() = Some(timestamp);

                // Calculate delay if we have a previous timestamp
                if let Some(prev) = previous_time {
                    // Calculate delay based on real time between messages and replay speed
                    let real_delay = (timestamp - prev).to_std().unwrap();
                    let replay_delay = Duration::from_secs_f64(real_delay.as_secs_f64() / replay_speed);

                    // Wait for the appropriate delay
                    std::thread::sleep(replay_delay);
                }

                // Deserialize the message
                let raw_message: IBKRRawMessage = bincode::deserialize(&content).unwrap();

                // Only enqueue incoming messages to simulate server responses
                if direction == "incoming" {
                    message_queue.push(raw_message);
                }

                previous_time = Some(timestamp);
            }
        });

        self.replay_thread = Some(thread);
        Ok(())
    }

    pub fn pause_replay(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume_replay(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn set_replay_speed(&mut self, speed: f64) {
        self.replay_speed = speed;
    }

    pub fn stop_replay(&mut self) {
        if let Some(thread) = self.replay_thread.take() {
            // Signal thread to stop
            self.paused.store(true, Ordering::SeqCst);
            // Wait for a bit to allow thread to clean up
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    // Methods to retrieve logged data for analysis
    pub fn get_session_info(&self) -> Result<SessionInfo, IBKRError>;
    pub fn list_orders(&self) -> Result<Vec<Order>, IBKRError>;
    pub fn list_executions(&self, order_id: Option<&str>) -> Result<Vec<Execution>, IBKRError>;
}

// Connection manager that uses the replay connection
pub struct ReplayConnectionManager {
    replay_connection: SqliteReplayConnection,
    host: String,
    port: i32,
    client_id: i32,
}

impl ReplayConnectionManager {
    pub fn new(replay_connection: SqliteReplayConnection, host: &str, port: i32, client_id: i32) -> Self {
        ReplayConnectionManager {
            replay_connection,
            host: host.to_string(),
            port,
            client_id,
        }
    }

    pub fn start_replay(&mut self) -> Result<(), IBKRError> {
        self.replay_connection.start_replay()
    }

    pub fn pause_replay(&self) {
        self.replay_connection.pause_replay();
    }

    pub fn resume_replay(&self) {
        self.replay_connection.resume_replay();
    }

    pub fn set_replay_speed(&mut self, speed: f64) {
        self.replay_connection.set_replay_speed(speed);
    }
}

// Connection manager decorator that enables proxying and recording
pub struct ProxyConnectionManager {
    inner: Arc<ConnectionManager>,
    logger: Arc<dyn Logger + Send + Sync>,
    proxy_server: Option<ProxyServer>,
}

impl ProxyConnectionManager {
    pub fn new(inner: Arc<ConnectionManager>, logger: Arc<dyn Logger + Send + Sync>) -> Self;
    pub fn start_proxy_server(&mut self, bind_address: &str, port: u16) -> Result<(), IBKRError>;
    pub fn stop_proxy_server(&mut self);
}

struct ProxyServer {
    listener: TcpListener,
    clients: Vec<TcpStream>,
    worker_thread: JoinHandle<()>,
}
```

## 5. Usage Examples

### 5.1 Basic Connection and Order Placement

```rust
// Initialize client
let config = ClientConfig::default();
let mut client = IBKRClient::new("127.0.0.1", 7496, 0, config)?;

// Connect to TWS/Gateway
client.connect()?;

// Place a market order
let order = client.orders().place_market_order("AAPL", 100.0, OrderSide::Buy)?;
println!("Order placed: {}", order.id);

// Check order status later
let updated_order = client.orders().get_order(&order.id).expect("Order not found");
println!("Order status: {:?}", updated_order.state.status);
```

### 5.2 Account Information and Position Management

```rust
// Get account information
let account_info = client.account().get_account_info()?;
println!("Buying power: ${:.2}", account_info.buying_power);

// Get current positions
let positions = client.account().get_positions()?;
for position in positions {
    println!("{}: {} shares at ${:.2} (P&L: ${:.2})",
             position.symbol, position.quantity,
             position.average_cost, position.unrealized_pnl);
}

// Set up background updates for account information
client.account().start_background_updates(Duration::from_secs(60))?;

// Add an observer for position updates
struct MyPositionObserver;
impl AccountObserver for MyPositionObserver {
    fn on_account_update(&self, _account_info: &AccountInfo) {}
    fn on_position_update(&self, position: &Position) {
        println!("Position updated: {} - {} shares", position.symbol, position.quantity);
    }
    fn on_execution(&self, execution: &Execution) {
        println!("Execution: {} - {} shares at ${:.2}",
                execution.symbol, execution.quantity, execution.price);
    }
}

client.account().add_observer(Box::new(MyPositionObserver));
```

### 5.3 Market Data and Historical Data

```rust
// Create a contract for AAPL
let contract = Contract::stock("AAPL", "SMART", "USD");

// Subscribe to market data
let subscription = client.data().subscribe_market_data(
    &contract,
    &[MarketDataType::BidAsk, MarketDataType::LastPrice]
)?;

// Add observer for price updates
struct MyPriceObserver;
impl MarketDataObserver for MyPriceObserver {
    fn on_price_update(&self, subscription: &MarketDataSubscription) {
        if let (Some(bid), Some(ask)) = (subscription.bid, subscription.ask) {
            println!("{}: Bid: ${:.2}, Ask: ${:.2}",
                    subscription.contract.symbol, bid, ask);
        }
        if let Some(last) = subscription.last_price {
            println!("{}: Last: ${:.2}", subscription.contract.symbol, last);
        }
    }
}

// Get historical data
let bars = client.data().get_historical_data(
    &contract,
    None, // End date time (now)
    Duration::days(5),
    BarSize::OneHour,
    WhatToShow::Trades,
    true // Use regular trading hours
)?;

println!("Retrieved {} bars of historical data", bars.len());
```

### 5.4 Logging and Replay

```rust
// Enable logging
let mut client = IBKRClient::new("127.0.0.1", 7496, 0, ClientConfig::default())?;
client.set_logging(true, Some("/path/to/log/file.log"));

// Connect and perform operations
client.connect()?;
// ... perform operations ...

// Disconnect
client.disconnect();

// Later, replay the logged session
let mut replay_client = IBKRClient::new("", 0, 0, ClientConfig::default())?;
replay_client.enable_replay_mode("/path/to/log/file.log")?;

// The replay client behaves like a real connection
let account_info = replay_client.account().get_account_info()?;
```

## 6. Implementation Considerations

### 6.1 Thread Safety

All components must be thread-safe as they will be accessed from multiple threads:
- Main application thread (synchronous API calls)
- Background worker threads (handling messages from TWS)
- Notification threads (pushing updates to observers)

Use of Arc, Mutex, and RwLock patterns will ensure thread safety while maintaining performance.

### 6.2 Error Handling

The API should provide clear, detailed error information:
- Connection errors (network issues, authentication problems)
- Request errors (invalid parameters, rate limiting)
- Order errors (rejected orders, execution issues)
- System errors (internal failures)

Errors should be propagated to the caller with context and appropriate suggestions for resolution.

### 6.3 Performance Considerations

- Minimize lock contention by using fine-grained locks
- Use read-write locks where appropriate to allow concurrent reads
- Implement efficient buffer management for message processing
- Use thread pools for background processing to avoid thread creation overhead


## 7. Testing Strategy

### 7.1 Unit Tests

- Test each component in isolation with mocked dependencies
- Verify thread safety under concurrent access patterns
- Test error handling and edge cases

### 7.2 Integration Tests

- Test interaction between components
- Verify correct message routing and response handling
- Test reconnection logic and resilience to failures

### 7.3 Replay-Based Testing

- Capture real-world API sessions for testing
- Replay sessions to verify behavior consistency
- Use replay for regression testing and benchmarking

### 7.4 Mock TWS Server

- Implement a mock TWS server for testing without a real TWS instance
- Simulate various response patterns and error conditions
- Test rate limiting and throttling logic

## 8. Deployment and Configuration

### 8.1 Configuration Options

- Connection parameters (host, port, client_id)
- Reconnection settings (enabled, max attempts, backoff)
- Logging settings (enabled, level, path)
- Rate limiting thresholds
- Request timeouts
- Background update intervals

### 8.2 Environment Variables

Support for configuration via environment variables:

```
IBKR_HOST=127.0.0.1
IBKR_PORT=7496
IBKR_CLIENT_ID=0
IBKR_RECONNECT_ENABLED=true
IBKR_LOG_PATH=/path/to/logs
...
```

## 9. Conclusion

This API design addresses the core issues with the current synchronous IBKR API:

1. **Eliminates deadlocks** by properly managing threads and messaging
2. **Simplifies usage** with a clean, domain-specific API
3. **Improves robustness** with proper error handling and reconnection logic
4. **Enhances observability** with comprehensive logging and replay
5. **Manages complexity** by abstracting the underlying message protocol
