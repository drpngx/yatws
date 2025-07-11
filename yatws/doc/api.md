# YATWS API Reference

This document provides an overview of the public API for YATWS (Yet Another TWS API).

## Table of Contents

- [IBKRClient](#ibkrclient)
- [ClientManager](#clientmanager)
- [AccountManager](#accountmanager)
  - [AccountObserver Trait](#accountobserver-trait)
- [OrderManager](#ordermanager)
  - [OrderObserver Trait](#orderobserver-trait)
- [OrderBuilder](#orderbuilder)
  - [TriggerMethod Enum](#triggermethod-enum)
- [OptionsStrategyBuilder](#optionsstrategybuilder)
- [DataRefManager](#datarefmanager)
  - [SecDefOptParamsResult Struct](#secdefoptparamsresult-struct)
- [DataMarketManager](#datamarketmanager)
  - [Quote Type Alias](#quote-type-alias)
  - [Observer Traits](#market-data-observer-traits)
  - [Subscription Types](#market-data-subscription-types)
- [DataNewsManager](#datanewsmanager)
  - [NewsObserver Trait](#newsobserver-trait)
  - [Subscription Types](#news-subscription-types)
- [FinancialAdvisorManager](#financialadvisormanager)
- [DataFundamentalsManager](#datafundamentalsmanager)
- [Financial Report Parser](#financial-report-parser)
- [IBKRAlgo Enum](#ibkralgo-enum)
- [Rate Limiter](#rate-limiter)
  - [RateLimiterConfig Struct](#ratelimiterconfig-struct)
  - [RateLimiterStatus Struct](#ratelimiterstatus-struct)

---

## IBKRClient

The primary client for interacting with the Interactive Brokers TWS API. It provides access to various specialized managers for
different API functionalities.

**File:** `yatws/src/client.rs`

### `IBKRClient::new(host: &str, port: u16, client_id: i32, log_config: Option<(String, String)>) -> Result<Self, IBKRError>`

Creates a new `IBKRClient` for a live connection to TWS/Gateway. This method establishes a connection, performs initial handshakes,
and waits for the `nextValidId` from the server before returning.

-   `host`: The hostname or IP address of the TWS/Gateway.
-   `port`: The port number TWS/Gateway is listening on.
-   `client_id`: A unique client ID for this connection.
-   `log_config`: Optional configuration for logging interactions to a SQLite database. If `Some((db_path, session_name))`,
interactions will be logged.
-   **Errors**: Returns `IBKRError` if connection fails, handshake is unsuccessful, or logger initialization issues.

### `IBKRClient::from_db(db_path: &str, session_name: &str) -> Result<IBKRClient, IBKRError>`

Creates a new `IBKRClient` for replaying a previously logged session from a database. Useful for testing and debugging.

-   `db_path`: Path to the SQLite database file.
-   `session_name`: The name of the session to replay.
-   **Errors**: Returns `IBKRError` if database cannot be opened, session not found, or replay initialization issues.

### `IBKRClient::client_id(&self) -> i32`

Returns the client ID used for this connection.

### `IBKRClient::client(&self) -> Arc<ClientManager>`

Provides access to the `ClientManager` for general client operations (server time, connection status, API verification).

### `IBKRClient::orders(&self) -> Arc<OrderManager>`

Provides access to the `OrderManager` for order-related operations (place, modify, cancel orders, query status).

### `IBKRClient::account(&self) -> Arc<AccountManager>`

Provides access to the `AccountManager` for account and portfolio data (summary, positions, P&L).

### `IBKRClient::data_ref(&self) -> Arc<DataRefManager>`

Provides access to the `DataRefManager` for reference data (contract details, exchange info).

### `IBKRClient::data_market(&self) -> Arc<DataMarketManager>`

Provides access to the `DataMarketManager` for market data (real-time and historical quotes, ticks, bars, depth).

### `IBKRClient::data_news(&self) -> Arc<DataNewsManager>`

Provides access to the `DataNewsManager` for news headlines and articles.

### `IBKRClient::data_financials(&self) -> Arc<DataFundamentalsManager>`

Provides access to the `DataFundamentalsManager` for financial data (company fundamentals, WSH events).

### `IBKRClient::financial_advisor(&self) -> Arc<FinancialAdvisorManager>`
Provides access to the `FinancialAdvisorManager` for Financial Advisor configurations (groups, profiles, aliases).

### `IBKRClient::configure_rate_limiter(&self, config: RateLimiterConfig) -> Result<(), IBKRError>`

Configures rate limiting for API requests with custom settings.
- `config`: The rate limiter configuration to apply.
- **Errors**: Returns `IBKRError` if configuration cannot be applied.

### `IBKRClient::get_rate_limiter_status(&self) -> Option<RateLimiterStatus>`

Returns the current rate limiter status, including usage statistics for each limiter.
- **Returns**: `Option<RateLimiterStatus>`, or `None` if rate limiting is not configured.

### `IBKRClient::enable_rate_limiting(&self) -> Result<(), IBKRError>`

Enables rate limiting with default configuration (50 msg/sec, 50 historical requests, 100 market data lines).
- **Errors**: Returns `IBKRError` if rate limiting cannot be enabled.

### `IBKRClient::disable_rate_limiting(&self) -> Result<(), IBKRError>`

Disables all rate limiting. Use with caution, as excessive API usage may cause IBKR to throttle or disconnect.
- **Errors**: Returns `IBKRError` if rate limiting cannot be disabled.

### `IBKRClient::cleanup_stale_rate_limiter_requests(&self, older_than: Duration) -> Result<(u32, u32), IBKRError>`

Cleans up stale rate limiter requests that never received completion messages.
- `older_than`: Duration threshold for considering a request stale.
- **Returns**: Tuple of (historical_cleaned, market_data_cleaned) counts.
- **Errors**: Returns `IBKRError` if cleanup fails.

---

## ClientManager

Manages client-level interactions and state with TWS, including connection status, server time requests, API verification messages,
and general error reporting. Accessed via `IBKRClient::client()`.

**File:** `yatws/src/client.rs` (within `mod mgr`)

### `ClientManager::is_connected(&self) -> bool`

Checks if the client believes it is currently connected to TWS/Gateway. Based on messages received and internal tracking.

### `ClientManager::get_last_error(&self) -> Option<(i32, i32, String)>`

Retrieves the last error message received. Returns `Option<(request_id, error_code, error_message)>`. `request_id` is -1 for general
errors.

### `ClientManager::get_server_time(&self) -> Option<DateTime<Utc>>`

Gets the last known server time from TWS. Updated by `request_current_time`.

### `ClientManager::request_current_time(&self) -> Result<DateTime<Utc>, IBKRError>`

Requests the current server time from TWS and blocks until response or timeout. Updates internal server time.
-   **Errors**: `IBKRError::Timeout` or other connection/encoding issues.

---

## AccountManager

Manages account-related data, including account summary, portfolio positions, and Profit & Loss (P&L). Allows subscribing to
continuous updates and fetching snapshots. Accessed via `IBKRClient::account()`.

**File:** `yatws/src/account_manager.rs`

### `AccountManager::get_account_value(&self, key: AccountValueKey) -> Result<Option<AccountValue>, IBKRError>`

Retrieves a specific account value by its `AccountValueKey` enum variant. Requires active subscription.
-   `key`: The `AccountValueKey` enum variant representing the desired account value.
-   **Returns**: `Ok(Some(AccountValue))` if found, `Ok(None)` if not, or `Err(IBKRError)`.

### `AccountManager::get_account_info(&self) -> Result<AccountInfo, IBKRError>`

Retrieves a consolidated summary of account information. This includes metrics like account ID, type, base currency, equity (net liquidation), buying power, cash balances, margin requirements (initial, maintenance, look-ahead), P&L figures, day trades remaining, and more. Requires active subscription.
-   **Errors**: If subscription fails, data not populated, or essential values missing/unparsable.

### `AccountManager::get_buying_power(&self) -> Result<f64, IBKRError>`

Retrieves current buying power. Requires active subscription.

### `AccountManager::get_cash_balance(&self) -> Result<f64, IBKRError>`

Retrieves total cash balance. Requires active subscription.

### `AccountManager::get_net_liquidation(&self) -> Result<f64, IBKRError>`

Retrieves the net liquidation value of the account. Requires active subscription.

### `AccountManager::list_open_positions(&self) -> Result<Vec<Position>, IBKRError>`

Retrieves a list of all open positions. Ensures subscription and requests a fresh snapshot, blocking until data is received. Filters
out zero quantity positions.
-   **Errors**: If subscription fails, refresh times out, or communication issues.

### `AccountManager::get_daily_pnl(&self) -> Result<f64, IBKRError>`

Retrieves daily Profit & Loss. Requires active subscription.

### `AccountManager::get_unrealized_pnl(&self) -> Result<f64, IBKRError>`

Retrieves unrealized Profit & Loss. Requires active subscription.

### `AccountManager::get_realized_pnl(&self) -> Result<f64, IBKRError>`

Retrieves realized Profit & Loss. Requires active subscription.

### `AccountManager::request_managed_accounts(&self) -> Result<(), IBKRError>`

Requests the list of managed accounts from TWS. The result is received asynchronously by the `managed_accounts` handler, which typically sets the primary account ID if it's not already known. This is often called automatically on connection, but can be called manually.
-   **Errors**: Returns `IBKRError` if there are issues encoding or sending the request.

### `AccountManager::subscribe_pnl(&self, con_id_filter: Option<Vec<i32>>) -> Result<(), IBKRError>`

Subscribes to real-time Profit and Loss (P&L) updates for individual positions.
-   `con_id_filter`: `None` for all open positions, or `Some(Vec<i32>)` to filter by contract IDs.
-   Adjusts subscriptions based on the filter and open positions. New positions matching the filter are auto-subscribed. Closed positions are auto-unsubscribed.
-   **Errors**: If underlying account subscription not established, communication issues, or account ID unavailable.

### `AccountManager::unsubscribe_pnl(&self) -> Result<(), IBKRError>`

Cancels all active single-position P&L subscriptions and clears the P&L filter.
-   **Errors**: Can return `IBKRError` for fundamental issues, individual cancellation errors logged as warnings.

### `AccountManager::get_day_executions(&self) -> Result<Vec<Execution>, IBKRError>`

Requests and returns execution details for the current trading day, attempting to merge commission data. Uses default
`ExecutionFilter`. Blocking call with grace period for commissions. Does *not* require prior `subscribe_account_updates()`.
-   **Errors**: If request times out or communication issues.

### `AccountManager::get_executions_filtered(&self, filter: &ExecutionFilter) -> Result<Vec<Execution>, IBKRError>`

Requests and returns execution details based on `filter`, merged with commission data. Blocking call with grace period. Does *not*
require prior `subscribe_account_updates()`.
-   `filter`: `ExecutionFilter` specifying criteria.
-   **Errors**: If request times out or communication issues.

### `AccountManager::add_observer<T: AccountObserver + Send + Sync + 'static>(&self, observer: T) -> usize`

Adds an observer to receive asynchronous notifications about account updates, position changes, and new executions.
-   `observer`: Boxed trait object implementing `AccountObserver`.
-   **Returns**: Unique `usize` ID for the observer.

### `AccountManager::remove_observer(&self, observer_id: usize) -> bool`

Removes a previously registered account observer.
-   `observer_id`: ID from `add_observer()`.
-   **Returns**: `true` if observer found and removed, `false` otherwise.

### `AccountManager::subscribe_account_updates(&self) -> Result<(), IBKRError>`

Subscribes to continuous updates for account summary and portfolio positions. Performs initial blocking fetch if not already
subscribed. Uses `reqAccountSummary` and `reqPositions`.
-   **Behavior**: Blocks on initial fetch until `accountSummaryEnd` and `positionEnd` or timeout. Returns immediately if already
subscribed.
-   **Errors**: If initial fetch times out, another fetch is in progress, or communication issues.

### `AccountManager::stop_account_updates(&self) -> Result<(), IBKRError>`

Stops receiving continuous updates for account summary and positions. Sends `cancelAccountSummary` and `cancelPositions`. Returns
`Ok(())` if not subscribed.
-   **Errors**: If cancellation messages fail to encode or send.

### `AccountManager::has_received_pre_liquidation_warning(&self) -> bool`

Checks if a pre-liquidation warning (TWS error code 2148) has been received during the current API session.
-   **Returns**: `true` if the warning was received in this session, `false` otherwise.
-   **Note**: This flag indicates receipt of the warning in the session, not the current account status. It may have been received in a previous sesion or may no longer be valid. Check `cushion` in account info.

---

## AccountSubscription

Provides a focused, resource-managed way to handle the lifecycle and stream of events for a specific trading account. It encapsulates the logic of subscribing to account updates via `AccountManager` and processing them into a clean event stream.

**File:** `yatws/src/account_subscription.rs`

### `AccountSubscription::new(account_manager: Arc<AccountManager>) -> Result<Self, IBKRError>`

Creates a new subscription to account events. Ensures the `AccountManager` is subscribed to TWS for updates, registers an internal observer, and attempts to populate initial summary and position data.
-   `account_manager`: An `Arc` to the `AccountManager`.
-   **Returns**: `Result<Self, IBKRError>`.
-   **Errors**: If `AccountManager::subscribe_account_updates()` fails or other setup issues occur.

### `AccountSubscription::account_id(&self) -> String`

Returns the account ID that this subscription is monitoring.

### `AccountSubscription::last_known_summary(&self) -> Option<AccountInfo>`

Returns a clone of the most recent `AccountInfo` summary received. `None` if no summary processed yet.

### `AccountSubscription::last_known_positions(&self) -> Vec<Position>`

Returns a clone of the list of most recent `Position` objects. Empty if no positions processed.

### `AccountSubscription::next_event(&self) -> Result<AccountEvent, crossbeam_channel::RecvError>`

Blocks until the next `AccountEvent` is available.
-   **Errors**: `RecvError` if the channel is disconnected (e.g., subscription closed).

### `AccountSubscription::try_next_event(&self) -> Result<AccountEvent, crossbeam_channel::TryRecvError>`

Attempts to receive the next `AccountEvent` without blocking.
-   **Errors**: `TryRecvError::Empty` if no event available, `TryRecvError::Disconnected` if channel disconnected.

### `AccountSubscription::events(&self) -> crossbeam_channel::Receiver<AccountEvent>`

Returns a clone of the `Receiver` for `AccountEvent`s, allowing flexible event consumption.

### `AccountSubscription::close(&mut self) -> Result<(), IBKRError>`

Explicitly closes the subscription. Unregisters its internal observer and sends `AccountEvent::Closed`.
-   **Returns**: `Ok(())` if successful.
-   **Errors**: If already closed or observer removal fails.

### AccountEvent Enum

**File:** `yatws/src/account_subscription.rs`

Represents different types of updates or events related to a subscribed account.

```rust
pub enum AccountEvent {
    SummaryUpdate { info: AccountInfo, timestamp: DateTime<Utc> },
    PositionUpdate { position: Position, timestamp: DateTime<Utc> },
    ExecutionUpdate { execution: Execution, timestamp: DateTime<Utc> },
    Error { error: IBKRError, timestamp: DateTime<Utc> },
    Closed { account_id: String, timestamp: DateTime<Utc> },
}
```

---

### AccountObserver Trait

**File:** `yatws/src/account.rs` (definition assumed, documented based on usage in `AccountManager`)

```rust
pub trait AccountObserver {
    fn on_account_update(&self, info: &AccountInfo);
    fn on_position_update(&self, position: &Position);
    fn on_execution(&self, execution: &Execution);
}
```
-   `on_account_update`: Called with updated `AccountInfo`.
-   `on_position_update`: Called with updated `Position` details.
-   `on_execution`: Called when a new `Execution` is received or updated (e.g., with commission).

---

## OrderManager

Manages order submission, status tracking, and cancellation. Accessed via `IBKRClient::orders()`.

**File:** `yatws/src/order_manager.rs`

### `OrderManager::add_observer(&self, observer: Box<dyn OrderObserver + Send + Sync>)`

Adds an observer to receive asynchronous notifications about order updates and errors.

### `OrderManager::peek_order_id(&self) -> Result<String, IBKRError>`

Peeks at the next available client order ID without consuming it.
-   **Errors**: `IBKRError::InternalError` if initial `nextValidId` not yet received.

### `OrderManager::place_order(&self, contract: Contract, request: OrderRequest) -> Result<String, IBKRError>`

Places an order with TWS. Returns client order ID immediately. Initial status `PendingSubmit`.
-   `contract`: The `Contract` to trade.
-   `request`: The `OrderRequest` detailing order parameters.
-   **Returns**: Client-assigned order ID (`String`).
-   **Errors**: If `nextValidId` unavailable, encoding issue, or send failure.

### `OrderManager::cancel_order(&self, order_id: &str) -> Result<bool, IBKRError>`

Requests cancellation of an existing order. Returns immediately.
-   `order_id`: Client order ID to cancel.
-   **Returns**: `Ok(true)` if request sent, `Ok(false)` if order already terminal.
-   **Errors**: Invalid ID format, encoding issue, or send failure.

### `OrderManager::modify_order(&self, order_id: &str, updates: OrderUpdates) -> Result<Arc<RwLock<Order>>, IBKRError>`

Modifies an existing open order. Sends new `placeOrder` with same client ID and updated parameters. Returns immediately.
-   `order_id`: Client order ID to modify.
-   `updates`: `OrderUpdates` struct with parameters to change.
-   **Returns**: `Arc<RwLock<Order>>` to the order's state in local book.
-   **Errors**: Order not found, terminal state, invalid ID, encoding/send issues.

### `OrderManager::replace_order(&self, order_id: &str, contract: Contract, request: OrderRequest) -> Result<Arc<RwLock<Order>>, IBKRError>`

Replaces an existing open order with a new contract and/or order request. This is a more direct replacement than `modify_order`. It sends a new `placeOrder` message with the same client ID but entirely new `Contract` and `OrderRequest` data. Returns immediately.
-   `order_id`: Client order ID to replace.
-   `contract`: The new `Contract` for the order.
-   `request`: The new `OrderRequest` for the order.
-   **Returns**: `Arc<RwLock<Order>>` to the order's state in local book.
-   **Errors**: Order not found, terminal state, invalid ID, encoding/send issues.

### `OrderManager::refresh_orderbook(&self, timeout: Duration) -> Result<(), IBKRError>`

Refreshes local order book by requesting all open orders from TWS. Blocking call, waits for `openOrderEnd` or timeout.
-   `timeout`: Maximum duration to wait.
-   **Errors**: `IBKRError::Timeout` or other message/send errors.

### `OrderManager::wait_order_submitted(&self, order_id: &str) -> Result<OrderStatus, IBKRError>`

Waits indefinitely until order status is `Submitted` or `Filled`, or order fails/terminates.

### `OrderManager::try_wait_order_submitted(&self, order_id: &str, timeout: Duration) -> Result<OrderStatus, IBKRError>`

Tries to wait for specified duration until order status is `Submitted` or `Filled`, or fails/terminates/times out.

### `OrderManager::wait_order_executed(&self, order_id: &str) -> Result<OrderStatus, IBKRError>`

Waits indefinitely until order status is `Filled`, or order fails/terminates.

### `OrderManager::try_wait_order_executed(&self, order_id: &str, timeout: Duration) -> Result<OrderStatus, IBKRError>`

Tries to wait for specified duration until order status is `Filled`, or fails/terminates/times out.

### `OrderManager::wait_order_canceled(&self, order_id: &str) -> Result<OrderStatus, IBKRError>`

Waits indefinitely until order status is `Cancelled`, `ApiCancelled`, or `Inactive`.

### `OrderManager::try_wait_order_canceled(&self, order_id: &str, timeout: Duration) -> Result<OrderStatus, IBKRError>`

Tries to wait for specified duration until order status is `Cancelled`, `ApiCancelled`, or `Inactive`, or fails/terminates/times out.

### `OrderManager::get_order(&self, order_id: &str) -> Option<Order>`

Retrieves a snapshot of an order's current state by client order ID. Returns `None` if not found.

### `OrderManager::get_all_orders(&self) -> Vec<Order>`

Retrieves a list of all orders known to the manager (any state).

### `OrderManager::get_open_orders(&self) -> Vec<Order>`

Retrieves a list of all open (non-terminal) orders.

### `OrderManager::cancel_all_orders_globally(&self) -> Result<(), IBKRError>`

Sends a global cancel request to TWS for all open orders for the current client ID. TWS sends `orderStatus` messages for affected orders. Returns immediately.
-   **Errors**: If server version doesn't support global cancel, or encoding/send issues.

### `OrderManager::exercise_option(&self, req_id: i32, contract: &Contract, action: crate::order::ExerciseAction, quantity: i32, account: &str, override_action: bool) -> Result<(), IBKRError>`

Exercises or lapses an option contract. Uses a `reqId` (ticker ID). Outcome observed via account/position updates or errors.
-   `req_id`: Unique request identifier (not an order ID).
-   `contract`: The option `Contract` (OPT or FOP).
-   `action`: `ExerciseAction::Exercise` or `ExerciseAction::Lapse`.
-   `quantity`: Number of contracts.
-   `account`: Account holding the option.
-   `override_action`: `true` to override TWS default exercise behavior.
-   **Errors**: If server version unsupported, contract not an option, or encoding/send issues.

### `OrderManager::check_what_if_order(&self, contract: &Contract, request: &OrderRequest, timeout: Duration) -> Result<OrderState, IBKRError>`

Checks margin and commission impact of a potential order without placing it (What-If). Blocks until results or timeout.
-   `contract`: The `Contract` for the instrument.
-   `request`: The `OrderRequest` describing the potential order.
-   `timeout`: Maximum duration to wait.
-   **Returns**: `OrderState` with margin/commission details.
-   **Errors**: `IBKRError::Timeout`, API errors, or if server version unsupported.

### `OrderManager::get_completed_orders(&self) -> Vec<Order>`

Retrieves a list of all orders currently known to the `OrderManager` that are in a terminal state (e.g., Filled, Cancelled).

### OrderObserver Trait

**File:** `yatws/src/order_manager.rs`

```rust
pub trait OrderObserver: Send + Sync {
  fn on_order_update(&self, order: &Order);
  fn on_order_error(&self, order_id: &str, error: &crate::base::IBKRError);
}
```
-   `on_order_update`: Called when an order's state is updated (by `orderStatus` or `openOrder`).
-   `on_order_error`: Called when an error related to a specific order is received.

---

## OrderBuilder

Builds an order request. Supports common order types and a `.with` pattern for customization.

**File:** `yatws/src/order_builder.rs`

### `OrderBuilder::new(action: OrderSide, quantity: f64) -> Self`

Starts building an order with essential action and quantity. Initializes default Stock contract and Market order.

### Contract Methods

-   `for_stock(symbol: &str) -> Self`
-   `for_option(symbol: &str, expiry: &DateOrMonth, strike: f64, right: OptionRight) -> Self`
-   `for_future(symbol: &str, expiry: &DateOrMonth) -> Self`
-   `for_forex(pair: &str) -> Self`
-   `for_continuous_future(symbol: &str) -> Self`
-   `for_bond(symbol: &str) -> Self`
-   `for_future_option(symbol: &str, expiry: &DateOrMonth, strike: f64, right: OptionRight) -> Self`
-   `for_warrant(symbol: &str) -> Self`
-   `for_index_option(symbol: &str, expiry: &DateOrMonth, strike: f64, right: OptionRight) -> Self`
-   `for_forward(symbol: &str, expiry: &DateOrMonth) -> Self`
-   `for_index(symbol: &str) -> Self`
-   `for_bill(symbol: &str) -> Self`
-   `for_fund(symbol: &str) -> Self`
-   `for_fixed(symbol: &str) -> Self`
-   `for_slb(symbol: &str) -> Self`
-   `for_commodity(symbol: &str) -> Self`
-   `for_basket(symbol: &str) -> Self`
-   `for_crypto(symbol: &str) -> Self`
-   `for_combo() -> Self`
-   `add_combo_leg(con_id: i32, ratio: i32, action: &str, exchange: &str) -> Self`
-   `with_combo_leg_price(leg_index: usize, price: f64) -> Self`
-   `with_con_id(con_id: i32) -> Self`
-   `with_exchange(exchange: &str) -> Self`
-   `with_primary_exchange(primary_exchange: &str) -> Self`
-   `with_currency(currency: &str) -> Self`
-   `with_local_symbol(local_symbol: &str) -> Self`
-   `with_trading_class(trading_class: &str) -> Self`

### OrderRequest Methods (Order Types)

-   `market() -> Self`
-   `limit(limit_price: f64) -> Self`
-   `stop(stop_price: f64) -> Self`
-   `stop_limit(stop_price: f64, limit_price: f64) -> Self`
-   `market_if_touched(trigger_price: f64) -> Self`
-   `limit_if_touched(trigger_price: f64, limit_price: f64) -> Self`
-   `trailing_stop_abs(trailing_amount: f64, trail_stop_price: Option<f64>) -> Self`
-   `trailing_stop_pct(trailing_percent: f64, trail_stop_price: Option<f64>) -> Self`
-   `trailing_stop_limit_abs(trailing_amount: f64, limit_offset: f64, trail_stop_price: Option<f64>) -> Self`
-   `trailing_stop_limit_pct(trailing_percent: f64, limit_offset: f64, trail_stop_price: Option<f64>) -> Self`

### OrderRequest Methods (General Parameters)

-   `with_tif(tif: TimeInForce) -> Self`
-   `with_good_till_date(date: DateTime<Utc>) -> Self`
-   `with_account(account: &str) -> Self`
-   `with_order_ref(order_ref: &str) -> Self`
-   `with_transmit(transmit: bool) -> Self`
-   `with_outside_rth(outside_rth: bool) -> Self`
-   `with_hidden(hidden: bool) -> Self`
-   `with_all_or_none(all_or_none: bool) -> Self`
-   `with_sweep_to_fill(sweep: bool) -> Self`
-   `with_block_order(block: bool) -> Self`
-   `with_not_held(not_held: bool) -> Self`
-   `with_parent_id(parent_id: i64) -> Self`
-   `with_oca_group(group: &str) -> Self`
-   `with_oca_type(oca_type: i32) -> Self` (1-3)

### Specific Order Type Configurations

-   `auction(price: f64) -> Self`
-   `auction_limit(limit_price: f64, auction_strategy: i32) -> Self`
-   `auction_pegged_stock(delta: f64, starting_price: f64) -> Self`
-   `auction_relative(offset: f64) -> Self`
-   `box_top() -> Self`
-   `with_discretionary_amount(amount: f64) -> Self`
-   `forex_cash_quantity(cash_quantity: f64, limit_price: Option<f64>) -> Self`
-   `limit_on_close(limit_price: f64) -> Self`
-   `limit_on_open(limit_price: f64) -> Self`
-   `market_on_close() -> Self`
-   `market_on_open() -> Self`
-   `market_to_limit() -> Self`
-   `market_with_protection() -> Self`
-   `passive_relative(offset: f64) -> Self`
-   `pegged_benchmark(reference_con_id: i32, starting_price: f64, pegged_change_amount: f64, reference_change_amount: f64,
is_decrease: bool) -> Self`
-   `pegged_market(market_offset: f64) -> Self`
-   `relative_pegged_primary(offset_amount: f64, price_cap: Option<f64>) -> Self`
-   `pegged_stock(delta: f64, stock_ref_price: f64, starting_price: f64) -> Self`
-   `stop_with_protection(stop_price: f64) -> Self`
-   `volatility(volatility_percent: f64, vol_type: i32) -> Self` (vol_type: 1=daily, 2=annual)
-   `midprice(price_cap: Option<f64>) -> Self`

### Adjustable Stop Methods

-   `with_trigger_price(price: f64) -> Self`
-   `adjust_to_stop(adjusted_stop_price: f64) -> Self`
-   `adjust_to_stop_limit(adjusted_stop_price: f64, adjusted_limit_price: f64) -> Self`
-   `adjust_to_trail_abs(adjusted_stop_price: f64, adjusted_trail_amount: f64) -> Self`
-   `adjust_to_trail_pct(adjusted_stop_price: f64, adjusted_trail_percent: f64) -> Self`

### Algo Methods

-   `with_ibkr_algo(algo: IBKRAlgo) -> Self`: Sets the IBKR Algo strategy and parameters using the `IBKRAlgo` enum. This replaces any previously set algo. See `IBKRAlgo` enum definition below for variants and parameters.

### Condition Methods

-   `with_next_condition_conjunction(conjunction: ConditionConjunction) -> Self`
-   `add_price_condition(con_id: i32, exchange: &str, price: f64, trigger_method: TriggerMethod, is_more: bool) -> Self`
-   `add_time_condition(time: DateTime<Utc>, is_more: bool) -> Self` (Time is UTC, formatted as "YYYYMMDD HH:MM:SS")
-   `add_margin_condition(percent: i32, is_more: bool) -> Self`
-   `add_execution_condition(symbol: &str, sec_type: &str, exchange: &str) -> Self`
-   `add_volume_condition(con_id: i32, exchange: &str, volume: i32, is_more: bool) -> Self` (Requires SMART routing)
-   `add_percent_change_condition(con_id: i32, exchange: &str, change_percent: f64, is_more: bool) -> Self`
-   `with_conditions_cancel_order(cancel: bool) -> Self`
-   `with_conditions_ignore_rth(ignore_rth: bool) -> Self`

### `OrderBuilder::build(self) -> Result<(Contract, OrderRequest), IBKRError>`

Finalizes and validates the order, returning the `Contract` and `OrderRequest`.
-   **Errors**: `IBKRError::InvalidOrder` if validation fails.

### TriggerMethod Enum

**File:** `yatws/src/order_builder.rs`

```rust
pub enum TriggerMethod {
  Default = 0,
  DoubleBidAsk = 1,
  Last = 2,
  DoubleLast = 3,
  BidAsk = 4,
  LastOrBidAsk = 7,
  Midpoint = 8,
}
```

---

## OptionsStrategyBuilder

Builds common options strategies as combo orders.

**File:** `yatws/src/options_strategy_builder.rs`

### `OptionsStrategyBuilder::new(data_ref_manager: Arc<DataRefManager>, underlying_symbol: &str, underlying_price: f64, quantity:
f64, underlying_sec_type: SecType) -> Result<Self, IBKRError>`

Starts building an options strategy.
-   `data_ref_manager`: For fetching contract details.
-   `underlying_symbol`: Symbol of the underlying asset.
-   `underlying_price`: Current price of the underlying.
-   `quantity`: Number of strategy units (e.g., 10 spreads). Must be positive.
-   `underlying_sec_type`: `SecType::Stock` or `SecType::Future`.
-   **Errors**: `IBKRError::InvalidParameter` for invalid quantity or sec_type.

### Strategy Definition Methods (Vertical Spreads)

-   `bull_call_spread(expiry: NaiveDate, target_strike1: f64, target_strike2: f64) -> Result<Self, IBKRError>`
-   `bear_call_spread(expiry: NaiveDate, target_strike1: f64, target_strike2: f64) -> Result<Self, IBKRError>`
-   `bull_put_spread(expiry: NaiveDate, target_strike1: f64, target_strike2: f64) -> Result<Self, IBKRError>`
-   `bear_put_spread(expiry: NaiveDate, target_strike1: f64, target_strike2: f64) -> Result<Self, IBKRError>`

### Strategy Definition Methods (Straddles / Strangles)

-   `long_straddle(expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError>`
-   `short_straddle(expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError>`
-   `long_strangle(expiry: NaiveDate, target_call_strike: f64, target_put_strike: f64) -> Result<Self, IBKRError>`
-   `short_strangle(expiry: NaiveDate, target_call_strike: f64, target_put_strike: f64) -> Result<Self, IBKRError>`

### Strategy Definition Methods (Box Spread)

-   `box_spread_nearest_expiry(target_expiry: NaiveDate, target_strike1: f64, target_strike2: f64) -> Result<Self, IBKRError>`

### Strategy Definition Methods (Single Leg Options)

-   `buy_call(expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError>`
-   `sell_call(expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError>`
-   `buy_put(expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError>`
-   `sell_put(expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError>`

### Strategy Definition Methods (Options Legs for Stock Strategies)

(Note: Stock leg must be handled separately by the caller)
-   `collar_options(expiry: NaiveDate, target_put_strike: f64, target_call_strike: f64) -> Result<Self, IBKRError>`
-   `covered_call_option(expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError>`
-   `covered_put_option(expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError>`
-   `protective_put_option(expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError>`
-   `stock_repair_options(expiry: NaiveDate, target_strike1: f64, target_strike2: f64) -> Result<Self, IBKRError>`

### Strategy Definition Methods (Ratio Spreads)

-   `long_ratio_call_spread(expiry: NaiveDate, target_strike1: f64, target_strike2: f64, buy_ratio: i32, sell_ratio: i32) ->
Result<Self, IBKRError>`
-   `long_ratio_put_spread(expiry: NaiveDate, target_strike1: f64, target_strike2: f64, buy_ratio: i32, sell_ratio: i32) ->
Result<Self, IBKRError>`
-   `short_ratio_put_spread(expiry: NaiveDate, target_strike1: f64, target_strike2: f64, sell_ratio: i32, buy_ratio: i32) ->
Result<Self, IBKRError>`

### Strategy Definition Methods (Butterflies)

-   `long_put_butterfly(expiry: NaiveDate, target_strike1: f64, target_strike2: f64, target_strike3: f64) -> Result<Self, IBKRError>`
-   `short_call_butterfly(expiry: NaiveDate, target_strike1: f64, target_strike2: f64, target_strike3: f64) -> Result<Self,
IBKRError>`
-   `long_iron_butterfly(expiry: NaiveDate, target_strike1: f64, target_strike2: f64, target_strike3: f64) -> Result<Self,
IBKRError>`

### Strategy Definition Methods (Condors)

-   `long_put_condor(expiry: NaiveDate, target_strike1: f64, target_strike2: f64, target_strike3: f64, target_strike4: f64) ->
Result<Self, IBKRError>`
-   `short_condor(expiry: NaiveDate, target_strike1: f64, target_strike2: f64, target_strike3: f64, target_strike4: f64) ->
Result<Self, IBKRError>`

### Strategy Definition Methods (Calendar Spreads)

-   `long_put_calendar_spread(target_strike: f64, near_expiry: NaiveDate, far_expiry: NaiveDate) -> Result<Self, IBKRError>`
-   `short_call_calendar_spread(target_strike: f64, near_expiry: NaiveDate, far_expiry: NaiveDate) -> Result<Self, IBKRError>`

### Strategy Definition Methods (Synthetics)

-   `synthetic_long_put_option(expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError>` (Option leg only)
-   `synthetic_long_stock(expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError>`
-   `synthetic_short_stock(expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError>`

### Order Parameter Methods (for the Combo Order)

-   `with_limit_price(price: f64) -> Self` (Net debit/credit for combo; positive for Debit, negative for Credit)
-   `with_order_type(order_type: OrderType) -> Self` (Default: `Limit`)
-   `with_tif(tif: TimeInForce) -> Self` (Default: `Day`)
-   `with_account(account: &str) -> Self`
-   `with_order_ref(order_ref: &str) -> Self`
-   `with_transmit(transmit: bool) -> Self` (Default: `true`)
-   `with_underlying_price(price: f64) -> Self` (Used for strike selection logic)

### `OptionsStrategyBuilder::build(mut self) -> Result<(Contract, OrderRequest), IBKRError>`

Finalizes the strategy, fetches contracts for each leg, and builds the combo `Contract` and `OrderRequest`.
-   **Errors**: `IBKRError::InvalidOrder` if no legs defined, `IBKRError::InvalidContract` or `IBKRError::InvalidParameter` if
contract fetching or validation fails.

---

## DataRefManager

Manages requests for reference data like contract details, option chains, etc. Accessed via `IBKRClient::data_ref()`.

**File:** `yatws/src/data_ref_manager.rs`

### `DataRefManager::get_contract_details(&self, contract: &Contract) -> Result<Vec<ContractDetails>, IBKRError>`

Requests and returns contract details for a given contract specification. Can return multiple matches. Blocks until
`contractDetailsEnd` or timeout.

### `DataRefManager::get_option_chain_params(&self, underlying_symbol: &str, fut_fop_exchange: &str, underlying_sec_type: SecType,
underlying_con_id: i32) -> Result<Vec<SecDefOptParamsResult>, IBKRError>`

Requests and returns option chain parameters (expirations, strikes) for an underlying. Blocks until
`securityDefinitionOptionParameterEnd` or timeout. Returns `Vec` in case multiple exchanges respond.
-   `fut_fop_exchange`: Typically `""` for stock options.

### `DataRefManager::get_soft_dollar_tiers(&self) -> Result<Vec<SoftDollarTier>, IBKRError>`

Requests and returns soft dollar tiers. Blocks until data received or timeout (no explicit end message).

### `DataRefManager::get_family_codes(&self) -> Result<Vec<FamilyCode>, IBKRError>`

Requests and returns family codes. Blocks until data received or timeout (no explicit end message).

### `DataRefManager::get_matching_symbols(&self, pattern: &str) -> Result<Vec<ContractDescription>, IBKRError>`

Requests and returns contracts matching a pattern. Blocks until data received or timeout (no explicit end message).

### `DataRefManager::get_mkt_depth_exchanges(&self) -> Result<Vec<DepthMktDataDescription>, IBKRError>`

Requests and returns exchanges offering market depth. Blocks until data received or timeout (no explicit end message).

### `DataRefManager::get_smart_components(&self, bbo_exchange: &str) -> Result<HashMap<i32, (String, char)>, IBKRError>`

Requests and returns SMART routing components for a BBO exchange. Blocks until data received or timeout (no explicit end message).

### `DataRefManager::get_market_rule(&self, market_rule_id: i32) -> Result<MarketRule, IBKRError>`

Requests and returns details for a specific market rule ID. Blocks until data received or timeout (no explicit end message).

### `DataRefManager::get_historical_schedule(&self, contract: &Contract, start_date: Option<chrono::DateTime<chrono::Utc>>, end_date: Option<chrono::DateTime<chrono::Utc>>, use_rth: bool, time_zone_id: &str) -> Result<HistoricalScheduleResult, IBKRError>`

Requests the historical trading schedule for a contract.
-   `contract`: The contract for which to request the schedule.
-   `start_date`: Optional start date/time.
-   `end_date`: Optional end date/time.
-   `use_rth`: Use regular trading hours.
-   `time_zone_id`: Desired time zone (note: TWS may return schedule in its own timezone).
-   **Returns**: `HistoricalScheduleResult` or `IBKRError`.

### `DataRefManager::cancel_historical_schedule(&self, req_id: i32) -> Result<(), IBKRError>`

Cancels a historical schedule request.
-   `req_id`: The request ID from `get_historical_schedule`.

### SecDefOptParamsResult Struct

**File:** `yatws/src/data_ref_manager.rs`

```rust
pub struct SecDefOptParamsResult {
  pub exchange: String,
  pub underlying_con_id: i32,
  pub trading_class: String,
  pub multiplier: String,
  pub expirations: Vec<String>, // YYYYMMDD format
  pub strikes: Vec<f64>,
}
```

---

## DataMarketManager

Manages requests for real-time and historical market data. Accessed via `IBKRClient::data_market()`.

**File:** `yatws/src/data_market_manager.rs`

### `DataMarketManager::request_market_data(&self, contract: &Contract, generic_tick_list: &[GenericTickType], snapshot: bool, regulatory_snapshot:
bool, mkt_data_options: &[(String, String)], market_data_type: Option<MarketDataType>) -> Result<i32, IBKRError>`

Requests streaming market data (ticks). Non-blocking.
-   `generic_tick_list`: Slice of `GenericTickType` enums. Empty for default.
-   `snapshot`: `true` for single snapshot, `false` for stream.
-   `regulatory_snapshot`: `true` for regulatory snapshot (TWS 963+).
-   `market_data_type`: Optional. Default `RealTime`.
-   **Returns**: Request ID (`i32`).
-   **Errors**: If market data type setting fails, encoding/send issues.

### `DataMarketManager::get_market_data<F>(&self, contract: &Contract, generic_tick_list: &[GenericTickType], snapshot: bool, regulatory_snapshot:
bool, mkt_data_options: &[(String, String)], market_data_type: Option<MarketDataType>, timeout: Duration, completion_check: F) ->
Result<MarketDataInfo, IBKRError>`
Where `F: Fn(&MarketDataInfo) -> bool`.

Requests streaming market data and blocks until `completion_check` is true, error, or timeout. Attempts to cancel stream afterwards.
-   `completion_check`: Closure taking `&MarketDataInfo`, returns `true` when condition met.
-   **Returns**: `MarketDataInfo` state.
-   **Errors**: If request fails, wait times out, etc.

### `DataMarketManager::cancel_market_data(&self, req_id: i32) -> Result<(), IBKRError>`

Cancels an active streaming market data request.
-   **Errors**: If cancellation message fails. Logs warning if `req_id` not found.

### `DataMarketManager::get_quote(&self, contract: &Contract, market_data_type: Option<MarketDataType>, timeout: Duration) ->
Result<Quote, IBKRError>`

Requests a single snapshot quote (Bid, Ask, Last). Blocking call.
-   `market_data_type`: Optional. Default `RealTime`.
-   **Returns**: `Quote` tuple `(Option<f64>, Option<f64>, Option<f64>)`.
-   **Errors**: `IBKRError::Timeout` or other request/send issues.

### `DataMarketManager::request_real_time_bars(&self, contract: &Contract, what_to_show: WhatToShow, use_rth: bool, real_time_bars_options:
&[(String, String)]) -> Result<i32, IBKRError>`

Requests streaming 5-second real-time bars. Non-blocking. (API only supports 5-sec bars).
-   `what_to_show`: `WhatToShow` enum ("TRADES", "MIDPOINT", "BID", "ASK").
-   `use_rth`: `true` for regular trading hours only.
-   **Returns**: Request ID (`i32`).
-   **Errors**: If request fails to encode/send.

### `DataMarketManager::get_realtime_bars(&self, contract: &Contract, what_to_show: WhatToShow, use_rth: bool, real_time_bars_options:
&[(String, String)], num_bars: usize, timeout: Duration) -> Result<Vec<Bar>, IBKRError>`

Requests a specific number of 5-second real-time bars. Blocking call.
-   `what_to_show`: `WhatToShow` enum.
-   `num_bars`: Number of bars to retrieve (>0).
-   **Returns**: `Vec<Bar>`.
-   **Errors**: `IBKRError::ConfigurationError` if `num_bars` is 0, `IBKRError::Timeout`, or other issues.

### `DataMarketManager::cancel_real_time_bars(&self, req_id: i32) -> Result<(), IBKRError>`

Cancels an active streaming real-time bars request.
-   **Errors**: If cancellation message fails.

### `DataMarketManager::request_tick_by_tick_data(&self, contract: &Contract, tick_type: TickByTickRequestType, number_of_ticks: i32, ignore_size:
bool) -> Result<i32, IBKRError>`

Requests streaming tick-by-tick data. Non-blocking.
-   `tick_type`: `TickByTickRequestType` enum ("Last", "AllLast", "BidAsk", "MidPoint").
-   `number_of_ticks`: `0` for streaming live data; `>0` for historical.
-   `ignore_size`: For "BidAsk", `true` to not report sizes.
-   **Returns**: Request ID (`i32`).
-   **Errors**: If request fails to encode/send.

### `DataMarketManager::get_tick_by_tick_data<F>(&self, contract: &Contract, tick_type: TickByTickRequestType, number_of_ticks: i32, ignore_size:
bool, timeout: Duration, completion_check: F) -> Result<TickByTickSubscription, IBKRError>`
Where `F: Fn(&TickByTickSubscription) -> bool`.

Requests streaming tick-by-tick data and blocks until `completion_check` is true, error, or timeout. Attempts to cancel stream
afterwards. `number_of_ticks` usually `0` for this.
-   **Returns**: `TickByTickSubscription` state.
-   **Errors**: If request fails, wait times out, etc.

### `DataMarketManager::cancel_tick_by_tick_data(&self, req_id: i32) -> Result<(), IBKRError>`

Cancels an active streaming tick-by-tick data request.
-   **Errors**: If cancellation message fails.

### `DataMarketManager::request_market_depth(&self, contract: &Contract, num_rows: i32, is_smart_depth: bool, mkt_depth_options:
&[(String, String)]) -> Result<i32, IBKRError>`

Requests streaming market depth data (Level II). Non-blocking.
-   `num_rows`: Number of rows on each side.
-   `is_smart_depth`: `true` for aggregated SMART depth (requires subscription).
-   **Returns**: Request ID (`i32`).
-   **Errors**: If request fails to encode/send.

### `DataMarketManager::get_market_depth<F>(&self, contract: &Contract, num_rows: i32, is_smart_depth: bool, mkt_depth_options:
&[(String, String)], timeout: Duration, completion_check: F) -> Result<MarketDepthSubscription, IBKRError>`
Where `F: Fn(&MarketDepthSubscription) -> bool`.

Requests streaming market depth and blocks until `completion_check` is true, error, or timeout. Attempts to cancel stream afterwards.
-   **Returns**: `MarketDepthSubscription` state.
-   **Errors**: If request fails, wait times out, etc.

### `DataMarketManager::cancel_market_depth(&self, req_id: i32) -> Result<(), IBKRError>`

Cancels an active streaming market depth request.
-   **Errors**: If cancellation message fails. Logs warning if `req_id` not found or `is_smart_depth` indeterminable.

### `DataMarketManager::get_historical_data(&self, contract: &Contract, end_date_time: Option<chrono::DateTime<chrono::Utc>>,
duration: DurationUnit, bar_size_setting: crate::contract::BarSize, what_to_show: WhatToShow, use_rth: bool, format_date: i32, keep_up_to_date: bool,
market_data_type: Option<MarketDataType>, chart_options: &[(String, String)]) -> Result<Vec<Bar>, IBKRError>`

Requests historical bar data. Blocking call.
-   `end_date_time`: Optional end point. `None` for present.
-   `duration`: `DurationUnit` enum (e.g., `DurationUnit::Day(3)`).
-   `bar_size_setting`: `crate::contract::BarSize` enum.
-   `what_to_show`: `WhatToShow` enum.
-   `use_rth`: `true` for regular trading hours only.
-   `format_date`: `1` for "yyyyMMdd HH:mm:ss", `2` for system time (seconds).
-   `keep_up_to_date`: `true` to subscribe to head bar updates.
-   `market_data_type`: Optional. Default `RealTime`.
-   **Returns**: `Vec<Bar>`.
-   **Errors**: `IBKRError::Timeout` or other request/send issues.

### `DataMarketManager::cancel_historical_data(&self, req_id: i32) -> Result<(), IBKRError>`

Cancels an ongoing historical data request.
-   **Errors**: If cancellation message fails. Logs warning if `req_id` not found.

### `DataMarketManager::get_scanner_parameters(&self, timeout: Duration) -> Result<ScanParameterResponse, IBKRError>`

Requests the XML document containing valid scanner parameters. Blocking call.
-   `timeout`: Maximum duration to wait for the XML response.
-   **Returns**: `ScanParameterResponse` (parsed from XML).
-   **Errors**: `IBKRError::Timeout` or other request/send/parse issues.

### `DataMarketManager::request_scanner_subscription(&self, subscription: &ScannerSubscription) -> Result<i32, IBKRError>`

Requests a market scanner subscription. Non-blocking. Results via `MarketDataHandler::scanner_data`.
-   `subscription`: `ScannerSubscription` struct defining scan parameters.
-   **Returns**: Request ID (`i32`).
-   **Errors**: If request fails to encode/send.

### `DataMarketManager::get_scanner_results(&self, subscription: &ScannerSubscription, timeout: Duration) -> Result<Vec<ScanData>, IBKRError>`

Requests market scanner results and blocks until received or timeout.
-   `subscription`: `ScannerSubscription` struct.
-   `timeout`: Maximum duration to wait.
-   **Returns**: `Vec<ScanData>`.
-   **Errors**: `IBKRError::Timeout` or other request/send issues.

### `DataMarketManager::cancel_scanner_subscription(&self, req_id: i32) -> Result<(), IBKRError>`

Cancels an active market scanner subscription.
-   **Errors**: If cancellation message fails.

### `DataMarketManager::calculate_implied_volatility(&self, contract: &Contract, option_price: f64, under_price: f64, timeout: Duration) -> Result<TickOptionComputationData, IBKRError>`

Calculates implied volatility for an option. Blocking call.
-   `contract`: The option `Contract`.
-   `option_price`: Market price of the option.
-   `under_price`: Current price of the underlying.
-   `timeout`: Maximum duration to wait.
-   **Returns**: `TickOptionComputationData` with implied volatility and greeks.
-   **Errors**: If calculation fails or times out.

### `DataMarketManager::cancel_calculate_implied_volatility(&self, req_id: i32) -> Result<(), IBKRError>`

Cancels an ongoing implied volatility calculation.
-   **Errors**: If cancellation message fails.

### `DataMarketManager::calculate_option_price(&self, contract: &Contract, volatility: f64, under_price: f64, timeout: Duration) -> Result<TickOptionComputationData, IBKRError>`

Calculates option price and greeks. Blocking call.
-   `contract`: The option `Contract`.
-   `volatility`: Volatility of the option (e.g., 0.20 for 20%).
-   `under_price`: Current price of the underlying.
-   `timeout`: Maximum duration to wait.
-   **Returns**: `TickOptionComputationData` with option price and greeks.
-   **Errors**: If calculation fails or times out.

### `DataMarketManager::cancel_calculate_option_price(&self, req_id: i32) -> Result<(), IBKRError>`

Cancels an ongoing option price calculation.
-   **Errors**: If cancellation message fails.

### `DataMarketManager::request_histogram_data(&self, contract: &Contract, use_rth: bool, time_period: TimePeriodUnit, _histogram_options: &[(String, String)]) -> Result<i32, IBKRError>`

Requests histogram data. Non-blocking. Data via `MarketDataHandler::histogram_data`.
-   `contract`: The `Contract`.
-   `use_rth`: `true` for regular trading hours only.
-   `time_period`: `TimePeriodUnit` enum (e.g., `TimePeriodUnit::Day(3)`).
-   `_histogram_options`: Reserved.
-   **Returns**: Request ID (`i32`).
-   **Errors**: If request fails or server version too low.

### `DataMarketManager::get_histogram_data(&self, contract: &Contract, use_rth: bool, time_period: TimePeriodUnit, _histogram_options: &[(String, String)], timeout: Duration) -> Result<Vec<HistogramEntry>, IBKRError>`

Requests histogram data and blocks until received or timeout.
-   (Same args as `request_histogram_data` plus `timeout`)
-   **Returns**: `Vec<HistogramEntry>`.
-   **Errors**: If request fails, times out, or server version too low.

### `DataMarketManager::cancel_histogram_data(&self, req_id: i32) -> Result<(), IBKRError>`

Cancels an ongoing histogram data request.
-   **Errors**: If cancellation message fails or server version too low.

### `DataMarketManager::request_historical_ticks(&self, contract: &Contract, start_date_time: Option<chrono::DateTime<chrono::Utc>>, end_date_time: Option<chrono::DateTime<chrono::Utc>>, number_of_ticks: i32, what_to_show: WhatToShow, use_rth: bool, ignore_size: bool, misc_options: &[(String, String)]) -> Result<i32, IBKRError>`

Requests historical tick data. Non-blocking. Data via `MarketDataHandler::historical_ticks_*`.
-   `start_date_time`, `end_date_time`: Optional date range.
-   `number_of_ticks`: Number of ticks (max 1000), or 0 for all in range.
-   `what_to_show`: `WhatToShow` enum ("TRADES", "MIDPOINT", "BID_ASK").
-   `use_rth`: `true` for regular trading hours only.
-   `ignore_size`: For "BID_ASK", `true` to omit sizes.
-   `misc_options`: Additional options.
-   **Returns**: Request ID (`i32`).
-   **Errors**: If request fails or server version too low.

### `DataMarketManager::get_historical_ticks(&self, contract: &Contract, start_date_time: Option<chrono::DateTime<chrono::Utc>>, end_date_time: Option<chrono::DateTime<chrono::Utc>>, number_of_ticks: i32, what_to_show: WhatToShow, use_rth: bool, ignore_size: bool, misc_options: &[(String, String)], timeout: Duration) -> Result<Vec<HistoricalTick>, IBKRError>`

Requests historical tick data and blocks until received or timeout.
-   (Same args as `request_historical_ticks` plus `timeout`)
-   **Returns**: `Vec<HistoricalTick>`.
-   **Errors**: If request fails, times out, or server version too low.

### `DataMarketManager::cancel_historical_ticks(&self, req_id: i32) -> Result<(), IBKRError>`

Cancels an ongoing historical ticks request (uses `cancelHistoricalData` message).
-   **Errors**: If cancellation message fails.

### Market Data Observer Traits

**File:** `yatws/src/data_observer.rs`

Various observer traits for different types of market data updates:

- `MarketDataObserver`: For tick price, size, string, etc. updates
- `RealTimeBarsObserver`: For real-time 5-second bar updates
- `TickByTickObserver`: For detailed tick-by-tick data
- `MarketDepthObserver`: For market depth (Level II book) updates
- `HistoricalDataObserver`: For historical bar data and updates
- `HistoricalTicksObserver`: For historical tick data

### Observer Registration Methods

- `observe_market_data<T: MarketDataObserver + Send + Sync + 'static>(&self, observer: T) -> ObserverId`
- `observe_realtime_bars<T: RealTimeBarsObserver + Send + Sync + 'static>(&self, observer: T) -> ObserverId`
- `observe_tick_by_tick<T: TickByTickObserver + Send + Sync + 'static>(&self, observer: T) -> ObserverId`
- `observe_market_depth<T: MarketDepthObserver + Send + Sync + 'static>(&self, observer: T) -> ObserverId`
- `observe_historical_data<T: HistoricalDataObserver + Send + Sync + 'static>(&self, observer: T) -> ObserverId`
- `observe_historical_ticks<T: HistoricalTicksObserver + Send + Sync + 'static>(&self, observer: T) -> ObserverId`

### Observer Removal Methods

- `remove_market_data_observer(&self, observer_id: ObserverId) -> bool`
- `remove_realtime_bars_observer(&self, observer_id: ObserverId) -> bool`
- `remove_tick_by_tick_observer(&self, observer_id: ObserverId) -> bool`
- `remove_market_depth_observer(&self, observer_id: ObserverId) -> bool`
- `remove_historical_data_observer(&self, observer_id: ObserverId) -> bool`
- `remove_historical_ticks_observer(&self, observer_id: ObserverId) -> bool`

### Market Data Subscription Types

**File:** `yatws/src/data_subscription.rs`

Various subscription builders for different types of market data:

- `subscribe_market_data(&self, contract: &Contract) -> TickDataSubscriptionBuilder`
- `subscribe_real_time_bars(&self, contract: &Contract, what_to_show: WhatToShow) -> RealTimeBarSubscriptionBuilder`
- `subscribe_tick_by_tick(&self, contract: &Contract, tick_type: TickByTickRequestType) -> TickByTickSubscriptionBuilder`
- `subscribe_market_depth(&self, contract: &Contract, num_rows: i32) -> MarketDepthSubscriptionBuilder`
- `subscribe_historical_data(&self, contract: &Contract, duration: DurationUnit, bar_size: BarSize, what_to_show: WhatToShow) -> HistoricalDataSubscriptionBuilder`
- `combine_subscriptions<T: Clone + Send + Sync + 'static>(&self) -> MultiSubscriptionBuilder<T, DataMarketManager>`

Each subscription builder provides a fluent interface for configuring the subscription and a `submit()` method to create the actual subscription. Subscription objects implement the `MarketDataSubscription` trait, providing methods like:
- `request_id()`
- `contract()`
- `is_completed()`
- `has_error()`
- `get_error()`
- `cancel()`
- `events()`

### Quote Type Alias

**File:** `yatws/src/data_market_manager.rs`

```rust
pub type Quote = (Option<f64>, Option<f64>, Option<f64>); // (Bid, Ask, Last)
```

---

## DataNewsManager

Manages requests for news providers, articles, and historical news. Accessed via `IBKRClient::data_news()`.

**File:** `yatws/src/data_news_manager.rs`

### `DataNewsManager::add_observer(&self, observer: Arc<dyn NewsObserver>) -> ObserverId`

Registers an observer to receive streaming news updates (bulletins and news ticks). Observer held by `Weak` pointer to allow for automatic cleanup when observer is dropped.
- **Returns**: Unique `ObserverId` for the registered observer.

### `DataNewsManager::remove_observer(&self, observer_id: ObserverId) -> bool`

Removes a previously registered news observer.
- **Returns**: `true` if an observer with the given ID was found and removed, `false` otherwise.

### `DataNewsManager::clear_observers(&self)`

Clears all registered news observers. After this call, no observers will receive further news updates from this manager.

### `DataNewsManager::get_news_providers(&self) -> Result<Vec<NewsProvider>, IBKRError>`

Requests and returns a list of available news providers. Blocking call.
-   **Errors**: `IBKRError::Timeout` or other request/send issues.

### `DataNewsManager::get_news_article(&self, provider_code: &str, article_id: &str, news_article_options: &[(String, String)]) ->
Result<NewsArticleData, IBKRError>`

Requests and returns content of a specific news article. Blocking call.
-   `provider_code`: Code of the news provider.
-   `article_id`: Unique ID of the article.
-   **Returns**: `NewsArticleData` (type and text).
-   **Errors**: `IBKRError::Timeout` or other request/send issues.

### `DataNewsManager::get_historical_news(&self, con_id: i32, provider_code: &str, start_date_time:
Option<chrono::DateTime<chrono::Utc>>, end_date_time: Option<chrono::DateTime<chrono::Utc>>, total_results: i32,
historical_news_options: &[(String, String)]) -> Result<Vec<HistoricalNews>, IBKRError>`

Requests and returns historical news headlines for a contract. Blocking call.
-   `con_id`: TWS contract ID.
-   `provider_code`: Provider code.
-   `start_date_time`, `end_date_time`: Optional date range.
-   `total_results`: Max number of headlines.
-   **Returns**: `Vec<HistoricalNews>`.
-   **Errors**: `IBKRError::Timeout`, `IBKRError::ApiError` (e.g., subscription required), or other issues.

### `DataNewsManager::request_news_bulletins(&self, all_msgs: bool) -> Result<(), IBKRError>`

Subscribes to live news bulletins. Non-blocking.
-   `all_msgs`: `true` for all bulletins, `false` for new only.
-   **Errors**: If request fails to encode/send.

### `DataNewsManager::cancel_news_bulletins(&self) -> Result<(), IBKRError>`

Cancels subscription to live news bulletins.
-   **Errors**: If cancellation message fails.

### `DataNewsManager::subscribe_news_bulletins_stream(&self, all_msgs: bool) -> NewsSubscriptionBuilder`

Creates a builder for a news bulletin subscription. News bulletins are general news items from TWS, not tied to a specific contract.
- `all_msgs`: If `true`, requests all available historical bulletins upon subscription followed by new ones. If `false`, requests only new bulletins.
- **Returns**: `NewsSubscriptionBuilder` for configuring and creating the subscription.

### `DataNewsManager::subscribe_historical_news_stream(&self, con_id: i32, provider_code: &str, total_results: i32) -> HistoricalNewsSubscriptionBuilder`

Creates a builder for a historical news subscription.
- `con_id`: The TWS contract ID of the instrument.
- `provider_code`: Provider code.
- `total_results`: The maximum number of headlines to return.
- **Returns**: `HistoricalNewsSubscriptionBuilder` for configuring and creating the subscription.

### NewsObserver Trait

**File:** `yatws/src/news.rs`

```rust
pub trait NewsObserver: Send + Sync {
    fn on_news_article(&self, article: &NewsArticle);
    fn on_error(&self, error_code: i32, error_message: &str);
}
```
- `on_news_article`: Called when a news bulletin or a news tick is received.
- `on_error`: Called when an error occurs related to news data.

### News Subscription Types

**File:** `yatws/src/news_subscription.rs`

#### NewsSubscription

Represents an active subscription to news bulletins. Provides a stream of `NewsEvent`s, which must be kept alive to receive updates.

- `request_id(&self) -> i32`: Returns a unique ID for this subscription.
- `events(&self) -> NewsIterator`: Returns an iterator over `NewsEvent`s from this subscription.
- `cancel(&self) -> Result<(), IBKRError>`: Cancels the subscription. Automatically called on drop if the subscription is still active.

#### NewsEvent Enum

```rust
pub enum NewsEvent {
    Bulletin { article: NewsArticle, timestamp: DateTime<Utc> },
    Error(IBKRError),
    Closed { timestamp: DateTime<Utc> },
}
```

#### HistoricalNewsSubscription

Represents an active subscription to historical news for a specific contract.

- `request_id(&self) -> i32`: Returns a unique ID for this subscription.
- `events(&self) -> HistoricalNewsIterator`: Returns an iterator over `HistoricalNewsEvent`s from this subscription.
- `cancel(&self) -> Result<(), IBKRError>`: Cancels the subscription. Automatically called on drop if the subscription is still active.

#### HistoricalNewsEvent Enum

```rust
pub enum HistoricalNewsEvent {
    Article(HistoricalNews),
    Complete,
    Error(IBKRError),
    Closed,
}
```

---

## FinancialAdvisorManager

Manages Financial Advisor (FA) configurations like groups, profiles, and aliases. Accessed via `IBKRClient::financial_advisor()`.

**File:** `yatws/src/financial_advisor_manager.rs`

### `FinancialAdvisorManager::request_fa_data(&self, fa_data_type: crate::financial_advisor::FADataType) -> Result<(), IBKRError>`

Requests Financial Advisor configuration data from TWS. This is a blocking call that waits for TWS to send the FA data. The internal FA configuration is updated upon successful retrieval.
-   `fa_data_type`: The type of FA data to request (`FADataType::Groups`, `FADataType::Profiles`, or `FADataType::Aliases`).
-   **Returns**: `Ok(())` if the data was successfully requested and parsed.
-   **Errors**: `IBKRError` if the request times out, parsing fails, or other communication issues occur.

### `FinancialAdvisorManager::replace_fa_data(&self, fa_data_type: crate::financial_advisor::FADataType, xml_data: &str) -> Result<(), IBKRError>`

Replaces Financial Advisor configuration data on TWS. This is a blocking call that waits for TWS to acknowledge the replacement.
-   `fa_data_type`: The type of FA data to replace.
-   `xml_data`: An XML string containing the new FA configuration.
-   **Returns**: `Ok(())` if the replacement was successfully acknowledged by TWS.
-   **Errors**: `IBKRError` if the request times out or other communication issues occur.

### `FinancialAdvisorManager::get_config(&self) -> FinancialAdvisorConfig`

Retrieves a clone of the current Financial Advisor configuration. This configuration includes hashmaps for `groups`, `profiles`, and `aliases`, along with their last update timestamps.
-   **Returns**: A `FinancialAdvisorConfig` struct.

Key related enums and structs:
-   `FADataType`: Enum for `Groups`, `Profiles`, `Aliases` (defined in `yatws::financial_advisor`).
-   `FinancialAdvisorConfig`, `FAGroup`, `FAProfile`, `FAAlias`: Structs representing the FA configuration data (defined in `yatws::financial_advisor`).

---

## DataFundamentalsManager

Manages requests for financial fundamental data and Wall Street Horizon (WSH) event data. Accessed via
`IBKRClient::data_financials()`.

**File:** `yatws/src/data_fin_manager.rs`

### `DataFundamentalsManager::get_fundamental_data(&self, contract: &Contract, report_type: FundamentalReportType, fundamental_data_options:
&[(String, String)]) -> Result<String, IBKRError>`

Requests and returns fundamental data for a contract. Blocking call. Data is XML.
-   `report_type`: `FundamentalReportType` enum.
-   **Returns**: XML `String`.
-   **Errors**: `IBKRError::Timeout` or other request/send issues.

### `DataFundamentalsManager::cancel_fundamental_data(&self, req_id: i32) -> Result<(), IBKRError>`

Cancels an ongoing fundamental data request.
-   **Errors**: If cancellation message fails. Logs warning if `req_id` not found.

### `DataFundamentalsManager::request_wsh_meta_data(&self) -> Result<i32, IBKRError>`

Requests Wall Street Horizon (WSH) metadata. Non-blocking. Metadata delivered via `FinancialDataHandler::wsh_meta_data`.
-   **Returns**: Request ID (`i32`).
-   **Errors**: If request fails to encode/send.

### `DataFundamentalsManager::cancel_wsh_meta_data(&self, req_id: i32) -> Result<(), IBKRError>`

Cancels an ongoing WSH metadata request.
-   **Errors**: If cancellation message fails.

### `DataFundamentalsManager::request_wsh_event_data(&self, wsh_event_data: &WshEventDataRequest) -> Result<i32, IBKRError>`

Requests a stream of WSH event data. Non-blocking. Events delivered via `FinancialDataHandler::wsh_event_data`.
-   `wsh_event_data`: `WshEventDataRequest` with filters.
-   **Returns**: Request ID (`i32`).
-   **Errors**: If request fails to encode/send.

### `DataFundamentalsManager::cancel_wsh_event_data(&self, req_id: i32) -> Result<(), IBKRError>`

Cancels an ongoing WSH event data streaming request.
-   **Errors**: If cancellation message fails.

### `DataFundamentalsManager::get_wsh_events(&self, request_details: &WshEventDataRequest, timeout: Duration) ->
Result<Vec<crate::data_wsh::WshEventData>, IBKRError>`

Requests and returns a specific set of WSH corporate event data. Blocking call. Fetches metadata if not cached. Waits for
`request_details.total_limit` events or timeout.
-   `request_details`: `WshEventDataRequest`. `total_limit` must be `Some(positive_number)`.
-   `timeout`: Overall timeout for the operation.
-   **Returns**: `Vec<crate::data_wsh::WshEventData>`.
-   **Errors**: If `total_limit` invalid, metadata/event fetch fails/times out, JSON parsing fails, or other issues.

---

## Financial Report Parser

**File:** `yatws/src/financial_report_parser.rs`

### `parse_fundamental_xml(xml_data: &str, report_type: FundamentalReportType) -> Result<ParsedFundamentalData, IBKRError>`

Parses fundamental data XML string into corresponding Rust structs.
-   `xml_data`: The XML string received from TWS API.
-   `report_type`: The type of report the `xml_data` represents (e.g., `FundamentalReportType::ReportsFinSummary`,
`FundamentalReportType::ReportSnapshot`).
-   **Returns**: A `Result` containing the parsed data in a `ParsedFundamentalData` enum variant, or an `IBKRError` if parsing fails.

---

## IBKRAlgo Enum

**File:** `yatws/src/order.rs`

Enum representing supported IBKR Algos and their parameters. Used with `OrderBuilder::with_ibkr_algo`.

```rust
pub enum IBKRAlgo {
  Adaptive { priority: AdaptivePriority },
  ArrivalPrice { max_pct_vol: f64, risk_aversion: RiskAversion, start_time: Option<DateTime<Utc>>, end_time: Option<DateTime<Utc>>, allow_past_end_time: bool, force_completion: bool },
  ClosePrice { max_pct_vol: f64, risk_aversion: RiskAversion, start_time: Option<DateTime<Utc>>, force_completion: bool },
  DarkIce { display_size: i32, start_time: Option<DateTime<Utc>>, end_time: Option<DateTime<Utc>>, allow_past_end_time: bool },
  AccumulateDistribute { component_size: i32, time_between_orders: i32, randomize_time_20pct: bool, randomize_size_55pct: bool, give_up: Option<i32>, catch_up_in_time: bool, wait_for_fill: bool, active_time_start: Option<DateTime<Utc>>, active_time_end: Option<DateTime<Utc>> },
  PercentageOfVolume { pct_vol: f64, start_time: Option<DateTime<Utc>>, end_time: Option<DateTime<Utc>>, no_take_liq: bool },
  TWAP { strategy_type: TwapStrategyType, start_time: Option<DateTime<Utc>>, end_time: Option<DateTime<Utc>>, allow_past_end_time: bool },
  PriceVariantPctVol { pct_vol: f64, delta_pct_vol: f64, min_pct_vol_for_price: f64, max_pct_vol_for_price: f64, start_time: Option<DateTime<Utc>>, end_time: Option<DateTime<Utc>>, no_take_liq: bool },
  SizeVariantPctVol { start_pct_vol: f64, end_pct_vol: f64, start_time: Option<DateTime<Utc>>, end_time: Option<DateTime<Utc>>, no_take_liq: bool },
  TimeVariantPctVol { start_pct_vol: f64, end_pct_vol: f64, start_time: Option<DateTime<Utc>>, end_time: Option<DateTime<Utc>>, no_take_liq: bool },
  VWAP { max_pct_vol: f64, start_time: Option<DateTime<Utc>>, end_time: Option<DateTime<Utc>>, allow_past_end_time: bool, no_take_liq: bool, speed_up: bool },
  BalanceImpactRisk { max_pct_vol: f64, risk_aversion: RiskAversion, force_completion: bool },
  MinimizeImpact { max_pct_vol: f64 },
  Custom { strategy: String, params: Vec<(String, String)> }, // Escape hatch
}

// Helper Enums:
pub enum AdaptivePriority { Urgent, Normal, Patient }
pub enum RiskAversion { GetDone, Aggressive, Neutral, Passive }
pub enum TwapStrategyType { Marketable, MatchingMidpoint, MatchingSameSide, MatchingLast }
pub enum ConditionConjunction { And, Or }
```
(See `order.rs` and `order_builder.rs` for full details and validation ranges).

---

## Rate Limiter

Provides rate limiting capabilities to respect Interactive Brokers' API limitations. Helps prevent your application from being disconnected due to excessive API usage. Accessed via methods on `IBKRClient`.

**File:** `yatws/src/rate_limiter.rs`

### RateLimiterConfig Struct

Configuration for rate limiting features. Controls limits for different types of API requests.

```rust
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
```

The rate limiter enforces three primary limits:
1. **Messages per Second**: Maximum 50 messages per second (token bucket algorithm)
2. **Historical Data**: Maximum 50 simultaneous open historical data requests (counter)
3. **Market Data Lines**: Default limit of 100 market data lines (counter)

### RateLimiterStatus Struct

Reports current usage statistics for the rate limiters.

```rust
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
```

### Example Usage

```rust
// Create a client
let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;

// Enable rate limiting with default settings
client.enable_rate_limiting()?;

// Or enable with custom settings
let mut config = RateLimiterConfig::default();
config.enabled = true;
config.max_messages_per_second = 40; // Be more conservative than default 50
config.max_historical_requests = 30; // Be more conservative than default 50
client.configure_rate_limiter(config)?;

// Check current status
if let Some(status) = client.get_rate_limiter_status() {
    println!("Current message rate: {:.2} msgs/sec", status.current_message_rate);
    println!("Active historical requests: {}/{}",
             status.active_historical_requests, 50);
    println!("Active market data lines: {}/{}",
             status.active_market_data_lines, 100);
}

// Clean up any stale request tracking (after 5 minutes)
let (hist_cleaned, mkt_cleaned) = client.cleanup_stale_rate_limiter_requests(
    Duration::from_secs(5 * 60)
)?;
println!("Cleaned up {} historical and {} market data requests",
         hist_cleaned, mkt_cleaned);

// Disable rate limiting if needed (use with caution)
client.disable_rate_limiting()?;
```
