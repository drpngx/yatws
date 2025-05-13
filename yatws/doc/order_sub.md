Okay, here's a summary of the public interface changes and new public structs/traits proposed so far to support the
`OrderSubscription` model:

**I. New Public Structs & Enums**

1.  **`crate::order::CommissionReportData`**
    *   **Purpose:** To hold structured data from a TWS commission report.
    *   **Fields:**
        *   `execution_id: String`
        *   `commission: f64`
        *   `currency: String`
        *   `realized_pnl: Option<f64>`
        *   `yield_amount: Option<f64>`
        *   `yield_redemption_date: Option<i32>` (Format YYYYMMDD)

2.  **`crate::order::OrderEvent`**
    *   **Purpose:** Represents different types of updates or events related to a single order's lifecycle, intended to be emitted by
`OrderSubscription`.
    *   **Variants:**
        *   `StatusUpdate { order_id, status, filled_quantity, ..., timestamp }`
        *   `DetailsUpdate { order, timestamp }`
        *   `Execution { order_id, execution, timestamp }`
        *   `CommissionReport { order_id, report, timestamp }`
        *   `Error { order_id, error, timestamp }`
        *   `Closed { order_id, timestamp }`

**II. New Public Traits**

1.  **`crate::handler::OrderObserver`**
    *   **Purpose:** Defines callbacks for an observer wishing to receive granular, asynchronous order updates directly from
`OrderManager`. This will be used by the internal observer of `OrderSubscription`.
    *   **Methods:**
        *   `on_status_update(&self, order_id: &str, status: OrderStatus, filled: f64, ...)`
        *   `on_details_update(&self, order: &Order)`
        *   `on_order_error(&self, order_id: &str, error: &IBKRError)`

**III. Modifications to Existing Public Traits**

1.  **`crate::account::AccountObserver`**
    *   **Modified Method:**
        *   No methods removed or signatures changed for existing methods.
    *   **New Method Added:**
        *   `on_commission_report_for_order(&self, client_order_id: &str, report: &CommissionReportData)`: Called by `AccountManager`
when a commission report is received and successfully correlated to a client order ID.

**IV. Anticipated New Public Struct (Core of the Feature)**

*   **`OrderSubscription`** (To be defined, likely in a new file e.g., `yatws/src/order_subscription.rs`)
    *   **Purpose:** Provides a focused, resource-managed way to handle the lifecycle and events of a single order.
    *   **Anticipated Public Methods (Conceptual):**
        *   `OrderSubscription::new(order_manager: Arc<OrderManager>, account_manager: Arc<AccountManager>, contract: Contract,
request: OrderRequest) -> Result<Self, IBKRError>`
        *   `order_id(&self) -> &str`
        *   `last_known_order_state(&self) -> Option<Order>`
        *   `next_event(&self) -> Result<OrderEvent, crossbeam_channel::RecvError>` (blocking)
        *   `try_next_event(&self) -> Result<OrderEvent, crossbeam_channel::TryRecvError>` (non-blocking)
        *   `events(&self) -> crossbeam_channel::Iter<'_, OrderEvent>` (iterator)
        *   `request_cancellation(&self) -> Result<bool, IBKRError>`
        *   `close(&self) -> Result<(), IBKRError>` (explicit cleanup)
        *   (Implicit `Drop` implementation for automatic cleanup)

**V. Modifications to Existing Public Methods in Managers**

1.  **`OrderManager` (in `yatws/src/order_manager.rs`)**
    *   `add_observer<T: OrderObserver + Send + Sync + 'static>(&self, observer: T) -> usize`: Signature will be updated to accept a
generic `OrderObserver` and return a `usize` ID for removal.
    *   `remove_observer(&self, observer_id: usize) -> bool`: A new public method will be added to allow deregistration of observers
by ID.

    *(The internal logic of `OrderManager`'s `OrderHandler` implementation will change to call the new `OrderObserver` methods, but
this doesn't alter its public `OrderHandler` trait conformance.)*

2.  **`AccountManager` (in `yatws/src/account_manager.rs`)**
    *   No direct changes to its public method signatures are anticipated for *this specific feature*, but its internal
`AccountHandler` implementation (specifically `commission_report`) will be modified to correlate commissions to client order IDs and
then call the new `AccountObserver::on_commission_report_for_order` method.

This summary outlines the primary public-facing changes and additions required to implement the `OrderSubscription` model as
discussed. The bulk of the work will be in the implementation details of these components and the internal logic of the managers.
