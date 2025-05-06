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
- [DataNewsManager](#datanewsmanager)
- [DataFundamentalsManager](#datafundamentalsmanager)
- [Financial Report Parser](#financial-report-parser)

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

### `AccountManager::get_account_value(&self, key: &str) -> Result<Option<AccountValue>, IBKRError>`

Retrieves a specific account value by its TWS tag name (e.g., "NetLiquidation"). Requires active subscription.

### `AccountManager::get_account_info(&self) -> Result<AccountInfo, IBKRError>`

Retrieves a consolidated summary of account information (ID, type, equity, buying power, etc.). Requires active subscription.
-   **Errors**: If subscription fails, data not populated, or essential values missing/unparsable.

### `AccountManager::get_buying_power(&self) -> Result<f64, IBKRError>`

Retrieves current buying power. Requires active subscription.

### `AccountManager::get_cash_balance(&self) -> Result<f64, IBKRError>`

Retrieves total cash balance. Requires active subscription.

### `AccountManager::get_equity(&self) -> Result<f64, IBKRError>`

Retrieves net liquidation value (equity). Attempts "NetLiquidation", then "EquityWithLoanValue". Requires active subscription.

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
-   `for_option(symbol: &str, expiry: &str, strike: f64, right: OptionRight) -> Self`
-   `for_future(symbol: &str, expiry: &str) -> Self`
-   `for_forex(pair: &str) -> Self`
-   `for_continuous_future(symbol: &str) -> Self`
-   `for_bond(symbol: &str) -> Self`
-   `for_future_option(symbol: &str, expiry: &str, strike: f64, right: OptionRight) -> Self`
-   `for_warrant(symbol: &str) -> Self`
-   `for_index_option(symbol: &str, expiry: &str, strike: f64, right: OptionRight) -> Self`
-   `for_forward(symbol: &str, expiry: &str) -> Self`
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

-   `with_algo(strategy: &str, params: Vec<(&str, &str)>) -> Self`
-   `adaptive_algo(priority: &str) -> Self` (priority: "Urgent", "Normal", "Patient")
-   `vwap_algo(max_pct_vol: f64, start_time: Option<&str>, end_time: Option<&str>, allow_past_end: bool, no_take_liq: bool, speed_up:
bool) -> Self`

### Condition Methods

-   `with_next_condition_conjunction(conjunction: &str) -> Self` ("and" or "or")
-   `add_price_condition(con_id: i32, exchange: &str, price: f64, trigger_method: TriggerMethod, is_more: bool) -> Self`
-   `add_time_condition(time: &str, is_more: bool) -> Self` (time format: "YYYYMMDD HH:MM:SS (optional timezone)")
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

### `DataMarketManager::request_market_data(&self, contract: &Contract, generic_tick_list: &str, snapshot: bool, regulatory_snapshot:
bool, mkt_data_options: &[(String, String)], market_data_type: Option<MarketDataType>) -> Result<i32, IBKRError>`

Requests streaming market data (ticks). Non-blocking.
-   `generic_tick_list`: Comma-separated generic tick type IDs (e.g., "100,101,104"). Empty for default.
-   `snapshot`: `true` for single snapshot, `false` for stream.
-   `regulatory_snapshot`: `true` for regulatory snapshot (TWS 963+).
-   `market_data_type`: Optional. Default `RealTime`.
-   **Returns**: Request ID (`i32`).
-   **Errors**: If market data type setting fails, encoding/send issues.

### `DataMarketManager::get_market_data<F>(&self, contract: &Contract, generic_tick_list: &str, snapshot: bool, regulatory_snapshot:
bool, mkt_data_options: &[(String, String)], market_data_type: Option<MarketDataType>, timeout: Duration, completion_check: F) ->
Result<MarketDataSubscription, IBKRError>`
Where `F: FnMut(&MarketDataSubscription) -> bool`.

Requests streaming market data and blocks until `completion_check` is true, error, or timeout. Attempts to cancel stream afterwards.
-   `completion_check`: Closure taking `&MarketDataSubscription`, returns `true` when condition met.
-   **Returns**: `MarketDataSubscription` state.
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

### `DataMarketManager::request_real_time_bars(&self, contract: &Contract, what_to_show: &str, use_rth: bool, real_time_bars_options:
&[(String, String)]) -> Result<i32, IBKRError>`

Requests streaming 5-second real-time bars. Non-blocking. (API only supports 5-sec bars).
-   `what_to_show`: "TRADES", "MIDPOINT", "BID", "ASK".
-   `use_rth`: `true` for regular trading hours only.
-   **Returns**: Request ID (`i32`).
-   **Errors**: If request fails to encode/send.

### `DataMarketManager::get_realtime_bars(&self, contract: &Contract, what_to_show: &str, use_rth: bool, real_time_bars_options:
&[(String, String)], num_bars: usize, timeout: Duration) -> Result<Vec<Bar>, IBKRError>`

Requests a specific number of 5-second real-time bars. Blocking call.
-   `num_bars`: Number of bars to retrieve (>0).
-   **Returns**: `Vec<Bar>`.
-   **Errors**: `IBKRError::ConfigurationError` if `num_bars` is 0, `IBKRError::Timeout`, or other issues.

### `DataMarketManager::cancel_real_time_bars(&self, req_id: i32) -> Result<(), IBKRError>`

Cancels an active streaming real-time bars request.
-   **Errors**: If cancellation message fails.

### `DataMarketManager::request_tick_by_tick_data(&self, contract: &Contract, tick_type: &str, number_of_ticks: i32, ignore_size:
bool) -> Result<i32, IBKRError>`

Requests streaming tick-by-tick data. Non-blocking.
-   `tick_type`: "Last", "AllLast", "BidAsk", "MidPoint".
-   `number_of_ticks`: `0` for streaming live data; `>0` for historical.
-   `ignore_size`: For "BidAsk", `true` to not report sizes.
-   **Returns**: Request ID (`i32`).
-   **Errors**: If request fails to encode/send.

### `DataMarketManager::get_tick_by_tick_data<F>(&self, contract: &Contract, tick_type: &str, number_of_ticks: i32, ignore_size:
bool, timeout: Duration, completion_check: F) -> Result<TickByTickSubscription, IBKRError>`
Where `F: FnMut(&TickByTickSubscription) -> bool`.

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
Where `F: FnMut(&MarketDepthSubscription) -> bool`.

Requests streaming market depth and blocks until `completion_check` is true, error, or timeout. Attempts to cancel stream afterwards.
-   **Returns**: `MarketDepthSubscription` state.
-   **Errors**: If request fails, wait times out, etc.

### `DataMarketManager::cancel_market_depth(&self, req_id: i32) -> Result<(), IBKRError>`

Cancels an active streaming market depth request.
-   **Errors**: If cancellation message fails. Logs warning if `req_id` not found or `is_smart_depth` indeterminable.

### `DataMarketManager::get_historical_data(&self, contract: &Contract, end_date_time: Option<chrono::DateTime<chrono::Utc>>,
duration_str: &str, bar_size_setting: &str, what_to_show: &str, use_rth: bool, format_date: i32, keep_up_to_date: bool,
market_data_type: Option<MarketDataType>, chart_options: &[(String, String)]) -> Result<Vec<Bar>, IBKRError>`

Requests historical bar data. Blocking call.
-   `end_date_time`: Optional end point. `None` for present.
-   `duration_str`: Duration (e.g., "1 Y", "60 D").
-   `bar_size_setting`: Bar size (e.g., "1 day", "30 mins").
-   `what_to_show`: Data type (e.g., "TRADES", "MIDPOINT").
-   `use_rth`: `true` for regular trading hours only.
-   `format_date`: `1` for "yyyyMMdd HH:mm:ss", `2` for system time (seconds).
-   `keep_up_to_date`: `true` to subscribe to head bar updates.
-   `market_data_type`: Optional. Default `RealTime`.
-   **Returns**: `Vec<Bar>`.
-   **Errors**: `IBKRError::Timeout` or other request/send issues.

### `DataMarketManager::cancel_historical_data(&self, req_id: i32) -> Result<(), IBKRError>`

Cancels an ongoing historical data request.
-   **Errors**: If cancellation message fails. Logs warning if `req_id` not found.

### Quote Type Alias

**File:** `yatws/src/data_market_manager.rs`

```rust
pub type Quote = (Option<f64>, Option<f64>, Option<f64>); // (Bid, Ask, Last)
```

---

## DataNewsManager

Manages requests for news providers, articles, and historical news. Accessed via `IBKRClient::data_news()`.

**File:** `yatws/src/data_news_manager.rs`

### `DataNewsManager::add_observer(&self, observer: Arc<dyn NewsObserver>)`

Registers an observer to receive streaming news updates (bulletins, news ticks). Observer held by `Weak` pointer.

### `DataNewsManager::clear_observers(&self)`

Clears all registered news observers.

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

### `DataNewsManager::get_historical_news(&self, con_id: i32, provider_codes: &str, start_date_time:
Option<chrono::DateTime<chrono::Utc>>, end_date_time: Option<chrono::DateTime<chrono::Utc>>, total_results: i32,
historical_news_options: &[(String, String)]) -> Result<Vec<HistoricalNews>, IBKRError>`

Requests and returns historical news headlines for a contract. Blocking call.
-   `con_id`: TWS contract ID.
-   `provider_codes`: Comma-separated provider codes.
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

---

## DataFundamentalsManager

Manages requests for financial fundamental data and Wall Street Horizon (WSH) event data. Accessed via
`IBKRClient::data_financials()`.

**File:** `yatws/src/data_fin_manager.rs`

### `DataFundamentalsManager::get_fundamental_data(&self, contract: &Contract, report_type: &str, fundamental_data_options:
&[(String, String)]) -> Result<String, IBKRError>`

Requests and returns fundamental data for a contract. Blocking call. Data is XML.
-   `report_type`: E.g., "ReportsFinSummary", "ReportSnapshot", "ReportsFinStatements".
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
