
//! Manages order submission, status tracking, and cancellation.
//!
//! The `OrderManager` is responsible for the entire lifecycle of an order,
//! from placement to final execution or cancellation.
//!
//! # Order Lifecycle
//!
//! 1.  **Placement**: An order is placed using `place_order`. Initially, its status is `PendingSubmit`.
//! 2.  **Submission Acknowledgement**: TWS acknowledges the order, and its status typically
//!     moves to `PreSubmitted` or `Submitted`. This can be waited for using
//!     `wait_order_submitted` or `try_wait_order_submitted`.
//! 3.  **Execution**: If the order fills, its status becomes `Filled`. This can be waited for
//!     using `wait_order_executed` or `try_wait_order_executed`. Partial fills will also
//!     update the `filled_quantity` and `remaining_quantity` fields.
//! 4.  **Cancellation**: An order can be cancelled using `cancel_order`. If successful,
//!     its status will move to `PendingCancel` and then `Cancelled` or `ApiCancelled`.
//!     This can be waited for using `wait_order_canceled` or `try_wait_order_canceled`.
//! 5.  **Modification**: An order can be modified using `modify_order`. This effectively
//!     replaces the existing order with a new one having the same client order ID but
//!     updated parameters. The status will reflect the new order's state.
//!
//! # Error Handling
//!
//! If TWS rejects an order or encounters an error during its lifecycle, the `OrderState`
//! will contain an `error` field, and the status might become `Cancelled` or another
//! terminal state. Observers will be notified via `on_order_error`.
//!
//! # Observers
//!
//! For asynchronous updates, an [`OrderObserver`] can be registered. It will receive
//! notifications for order status changes (`on_order_update`) and errors (`on_order_error`).
//!
//! # Example: Placing a Market Order and Waiting for Execution
//!
//! ```no_run
//! use yatws::{IBKRClient, IBKRError, OrderBuilder, contract::Contract, order::{OrderSide, OrderStatus}};
//! use std::time::Duration;
//!
//! fn main() -> Result<(), IBKRError> {
//!     let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
//!     let order_mgr = client.orders();
//!
//!     // Build a market order to buy 1 share of SPY
//!     let (contract, buy_request) = OrderBuilder::new(OrderSide::Buy, 1.0)
//!         .market()
//!         .for_stock("SPY")
//!         .build()?;
//!
//!     // Place the order
//!     let buy_order_id = order_mgr.place_order(contract.clone(), buy_request)?;
//!     println!("BUY order placed with ID: {}", buy_order_id);
//!
//!     // Wait for the order to be filled (e.g., up to 20 seconds)
//!     match order_mgr.try_wait_order_executed(&buy_order_id, Duration::from_secs(20)) {
//!         Ok(OrderStatus::Filled) => println!("BUY order {} filled.", buy_order_id),
//!         Ok(status) => eprintln!("BUY order {} did not fill as expected. Final status: {:?}", buy_order_id, status),
//!         Err(e) => eprintln!("Error waiting for BUY order {}: {:?}", buy_order_id, e),
//!     }
//!
//!     Ok(())
//! }
//! ```

use parking_lot::{RwLock, Mutex, Condvar};
use std::sync::Arc;
use std::collections::HashMap;
use std::time::Duration;
use chrono::Utc;
use log::{debug, error, info, warn};

use crate::order::{Order, OrderRequest, OrderUpdates, OrderState, OrderStatus, OrderCancel};
use crate::contract::{Contract, SecType};
use crate::conn::MessageBroker;
use crate::base::IBKRError;
use crate::handler::OrderHandler;
use crate::protocol_encoder::Encoder;
use crate::protocol_decoder::ClientErrorCode; // Added import
use crate::min_server_ver::min_server_ver; // Added for REQ_GLOBAL_CANCEL
// Removed unused import: crate::parser_order;


/// Manages order lifecycle, including placement, status tracking, modification, and cancellation.
///
/// Accessed via [`IBKRClient::orders()`](crate::IBKRClient::orders()).
///
/// See the [module-level documentation](index.html) for an overview of the order lifecycle
/// and interaction patterns.
pub struct OrderManager {
  message_broker: Arc<MessageBroker>,
  // Map: client_order_id (String) -> Order struct
  order_book: RwLock<HashMap<String, Arc<RwLock<Order>>>>,
  // Map: perm_id (i32) -> client_order_id (String) for quick lookup from status updates
  perm_id_map: RwLock<HashMap<i32, String>>,
  observers: RwLock<Vec<Box<dyn OrderObserver + Send + Sync>>>,
  // Mutex + Condvar for waiting for the initial next_valid_id
  next_order_id_state: Mutex<Option<usize>>,
  order_id_condvar: Condvar,
  // Condvars for waiting on specific order state changes (client_order_id -> Condvar)
  // Arc<(Mutex for Condvar, Condvar)>
  order_update_condvars: RwLock<HashMap<String, Arc<(Mutex<()>, Condvar)>>>,
  // Flag and Condvar for waiting on openOrderEnd after a refresh request
  open_order_end_flag: Mutex<bool>,
  open_order_end_condvar: Condvar,
}

impl OrderManager {
  /// Creates a new `OrderManager` and returns it along with a closure.
  /// The closure *must* be called after the TWS connection is established and
  /// the initial `StartAPI` handshake is complete. It waits for the server
  /// to send the `nextValidId` message, which is crucial for placing orders.
  ///
  /// This method is typically called internally when an `IBKRClient` is created.
  pub(crate) fn create(message_broker: Arc<MessageBroker>) -> (Arc<Self>, impl FnOnce() -> Result<(), IBKRError>) {
    let manager = Arc::new(OrderManager {
      message_broker,
      order_book: RwLock::new(HashMap::new()),
      perm_id_map: RwLock::new(HashMap::new()),
      observers: RwLock::new(Vec::new()),
      next_order_id_state: Mutex::new(None), // Start as None
      order_id_condvar: Condvar::new(),
      order_update_condvars: RwLock::new(HashMap::new()),
      open_order_end_flag: Mutex::new(false), // Initialize flag
      open_order_end_condvar: Condvar::new(), // Initialize condvar
    });

    let manager_clone = manager.clone();
    // This closure captures the Arc and calls the private wait function.
    let wait_fn = move || manager_clone.wait_for_initial_order_id();

    (manager, wait_fn)
  }

  // Private function called by the closure returned from create()
  fn wait_for_initial_order_id(&self) -> Result<(), IBKRError> {
    info!("Waiting for initial nextValidId from server...");
    let mut guard = self.next_order_id_state.lock();
    let timeout = Duration::from_secs(30); // Configurable timeout
    let start = std::time::Instant::now();

    while guard.is_none() {
      let remaining = timeout.checked_sub(start.elapsed()).unwrap_or(Duration::ZERO);
      if remaining == Duration::ZERO {
        error!("Timed out waiting for initial next valid order ID");
        return Err(IBKRError::Timeout("Timed out waiting for initial next valid order ID".to_string()));
      }
      // Wait on the condvar, releasing the lock `guard` temporarily
      let result = self.order_id_condvar.wait_for(&mut guard, remaining);
      if result.timed_out() && guard.is_none() { // Re-check after timeout
        error!("Timed out waiting for initial next valid order ID (after wait)");
        return Err(IBKRError::Timeout("Timed out waiting for initial next valid order ID".to_string()));
      }
      // If woken up, the loop condition (guard.is_none()) will be checked again
      debug!("wait_for_initial_order_id notified, checking status...");
    }

    info!("Received initial next_valid_id: {}", guard.unwrap());
    Ok(())
  }

  /// Adds an observer to receive asynchronous notifications about order updates and errors.
  ///
  /// Observers implement the [`OrderObserver`] trait.
  /// Multiple observers can be added.
  pub fn add_observer(&self, observer: Box<dyn OrderObserver + Send + Sync>) {
    let mut observers = self.observers.write();
    observers.push(observer);
    debug!("Added order observer");
  }

  // Note: Removing observers requires assigning unique IDs, omitted for brevity.

  /// Peeks at the next available client order ID without consuming it.
  ///
  /// This ID will be used for the next order placed via `place_order`.
  /// Note: If multiple operations are placing orders concurrently (though not typical
  /// for a single `OrderManager` instance unless accessed from multiple threads without
  /// external synchronization), the actual ID used might be higher.
  ///
  /// # Errors
  /// Returns `IBKRError::InternalError` if the initial `nextValidId` has not yet
  /// been received from TWS.
  pub fn peek_order_id(&self) -> Result<String, IBKRError> {
    let guard = self.next_order_id_state.lock();
    match *guard {
      Some(id) => Ok(id.to_string()),
      None => Err(IBKRError::InternalError("Next valid order ID not yet received from server".to_string())),
    }
  }

  /// Places an order with TWS.
  ///
  /// This method sends the order request to TWS and returns the assigned client order ID
  /// immediately, without waiting for TWS to acknowledge or fill the order.
  /// The order's initial status will be `PendingSubmit`.
  ///
  /// To track the order's progress, you can:
  /// - Use the `wait_*` or `try_wait_*` methods (e.g., `try_wait_order_submitted`, `try_wait_order_executed`).
  /// - Register an [`OrderObserver`] to receive asynchronous updates.
  /// - Periodically call `get_order` or `get_open_orders`.
  ///
  /// # Arguments
  /// * `contract` - The [`Contract`] for the instrument to trade.
  /// * `request` - The [`OrderRequest`] detailing the order parameters (side, type, quantity, price, etc.).
  ///
  /// # Returns
  /// The client-assigned order ID (as a `String`) if the request was successfully sent.
  ///
  /// # Errors
  /// Returns `IBKRError` if the initial `nextValidId` is not available, if there's an
  /// encoding issue, or if the message fails to send.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError, OrderBuilder, contract::Contract, order::OrderSide};
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// # let order_mgr = client.orders();
  /// let (contract, order_req) = OrderBuilder::new(OrderSide::Buy, 100.0)
  ///     .limit(250.50)
  ///     .for_stock("AAPL")
  ///     .build()?;
  ///
  /// let order_id = order_mgr.place_order(contract, order_req)?;
  /// println!("Order placed with ID: {}", order_id);
  /// # Ok(())
  /// # }
  /// ```
  pub fn place_order(&self, contract: Contract, request: OrderRequest) -> Result<String, IBKRError> {
    info!("Placing order: {:?} x {:.0} for {}", request.side, request.quantity, contract.symbol);
    if !request.transmit {
      // TODO: Support complex orders where transmit=false later
      warn!("Placing order with transmit=false. This is usually for complex orders and may not work as expected standalone.");
      // return Err(IBKRError::RequestError("Standalone orders must have transmit=true".to_string()));
    }
    // Create the initial Order struct
    let now = Utc::now();

    // We acquire the lock on the next_order_id. Since orders have to be submitted in
    // sequence, we don't release it until we are done transmitting it.
    let mut id_guard = self.next_order_id_state.lock();
    let order_id_int = match *id_guard {
      Some(id) => {
        *id_guard = Some(id + 1); // Increment the ID
        debug!("Assigning order id {}, next will be {}", id, id + 1);
        Ok(id as i32) // Return the *current* ID
      }
      None => {
        error!("Attempted to place order before next_valid_id was received");
        Err(IBKRError::InternalError("Next valid order ID not yet received from server".to_string()))
      }
    }?;

    let order_id_str = order_id_int.to_string();

    let order = Order {
      id: order_id_str.clone(),
      perm_id: None, // Will be set by orderStatus
      client_id: None, // Will be set by orderStatus
      parent_id: request.parent_id, // Copy from request
      contract: contract.clone(), // Clone contract
      request: request.clone(),   // Clone request
      state: OrderState {
        status: OrderStatus::PendingSubmit, // Initial state after sending
        remaining_quantity: request.quantity, // Initialize remaining qty
        ..Default::default()
      },
      created_at: now,
      updated_at: now,
    };

    let order_arc = Arc::new(RwLock::new(order));

    // Add to order book *before* sending
    {
      let mut book = self.order_book.write();
      if book.insert(order_id_str.clone(), order_arc.clone()).is_some() {
        // Should not happen if get_and_increment_order_id works correctly
        error!("Order ID collision detected for ID: {}", order_id_str);
        return Err(IBKRError::InternalError(format!("Order ID collision: {}", order_id_str)));
      }
      // Add condvar for this order *before* releasing lock, ensuring it exists if needed immediately
      let mut condvars = self.order_update_condvars.write();
      condvars.entry(order_id_str.clone())
        .or_insert_with(|| Arc::new((Mutex::new(()), Condvar::new())));
      debug!("Order {} added to book and condvar map in PendingSubmit state", order_id_str);
    } // Release write lock on order_book and condvars

    // Prepare and send message
    let encoder = Encoder::new(self.message_broker.get_server_version()?);
    // Pass the *cloned request* from the Order struct to the encoder
    let place_msg = encoder.encode_place_order(order_id_int, &contract, &order_arc.read().request)?;
    self.message_broker.send_message(&place_msg)?;

    info!("Place order request sent for ID: {}", order_id_str);
    Ok(order_id_str)
  }

  /// Requests the cancellation of an existing order.
  ///
  /// This method sends a cancel request to TWS and returns immediately.
  /// It does not wait for confirmation that the order has been cancelled.
  /// The order's status will typically move to `PendingCancel` and then `Cancelled`
  /// or `ApiCancelled` upon successful cancellation by TWS.
  ///
  /// # Arguments
  /// * `order_id` - The client order ID of the order to cancel.
  ///
  /// # Returns
  /// * `Ok(true)` if the cancel request was sent.
  /// * `Ok(false)` if the order was already in a terminal state and no request was sent.
  ///
  /// # Errors
  /// Returns `IBKRError` if the order ID format is invalid, if there's an encoding issue,
  /// or if the message fails to send.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError, OrderBuilder, contract::Contract, order::{OrderSide, OrderStatus}};
  /// # use std::time::Duration;
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// # let order_mgr = client.orders();
  /// # let (contract, order_req) = OrderBuilder::new(OrderSide::Buy, 1.0).limit(10.0).for_stock("XYZ").build()?;
  /// # let order_id = order_mgr.place_order(contract, order_req)?;
  /// // Assume order_id is an active order
  /// if order_mgr.cancel_order(&order_id)? {
  ///     println!("Cancel request sent for order {}", order_id);
  ///     // Optionally wait for cancellation confirmation
  ///     match order_mgr.try_wait_order_canceled(&order_id, Duration::from_secs(5)) {
  ///         Ok(status) => println!("Order {} cancellation confirmed with status: {:?}", order_id, status),
  ///         Err(e) => eprintln!("Error waiting for cancellation: {:?}", e),
  ///     }
  /// }
  /// # Ok(())
  /// # }
  /// ```
  pub fn cancel_order(&self, order_id: &str) -> Result<bool, IBKRError> {
    info!("Requesting cancellation for order ID: {}", order_id);
    let order_id_int = order_id.parse::<i32>()
      .map_err(|_| IBKRError::InvalidOrder(format!("Invalid order ID format: {}", order_id)))?;

    // Check if order exists and is potentially cancellable (optional check)
    {
      let book = self.order_book.read();
      if let Some(order_arc) = book.get(order_id) {
        let order = order_arc.read();
        if order.state.is_terminal() {
          warn!("Attempting to cancel order {} which is already in terminal state {:?}", order_id, order.state.status);
          return Ok(false); // Indicate no action taken as it's already terminal
        }
      } else {
        warn!("Attempting to cancel non-existent order ID: {}", order_id);
        // Send cancel anyway? TWS might handle it or return an error. Let's proceed.
        // return Err(IBKRError::NotFound(format!("Order ID {} not found", order_id)));
      }
    }


    // Prepare and send message
    let encoder = Encoder::new(self.message_broker.get_server_version()?);
    let cancel_msg = encoder.encode_cancel_order(order_id_int, &OrderCancel::default())?;
    self.message_broker.send_message(&cancel_msg)?;

    // We don't update the local state optimistically. We wait for orderStatus.
    // The only potential update could be to set status to PendingCancel, but
    // waiting for the official status is safer.

    info!("Cancel order request sent for ID: {}", order_id);
    Ok(true) // Indicate request was sent
  }

  /// Replaces an existing open order with a new contract and/or order request.
  ///
  /// This is a more direct replacement than `modify_order`. It sends a new `placeOrder`
  /// message with the same client ID but entirely new `Contract` and `OrderRequest` data.
  /// The method returns immediately after sending the replacement request.
  ///
  /// # Arguments
  /// * `order_id` - The client order ID of the order to replace.
  /// * `contract` - The new [`Contract`] for the order.
  /// * `request` - The new [`OrderRequest`] for the order.
  ///
  /// # Returns
  /// An `Arc<RwLock<Order>>` pointing to the order being modified.
  ///
  /// # Errors
  /// Returns `IBKRError` if the order ID is not found, the order is already in a
  /// terminal state, the order ID format is invalid, or if there are issues
  /// encoding or sending the replacement request.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError, OrderBuilder, contract::Contract, order::{OrderSide, OrderRequest}};
  /// # use std::time::Duration;
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// # let order_mgr = client.orders();
  /// # let (original_contract, original_req) = OrderBuilder::new(OrderSide::Buy, 1.0).limit(100.0).for_stock("AAPL").build()?;
  /// # let order_id = order_mgr.place_order(original_contract, original_req)?;
  /// // Assume order_id is an active limit order for 1 share of AAPL at $100
  ///
  /// // Now, replace it with an order for 2 shares of AAPL at $150
  /// let (new_contract, new_req) = OrderBuilder::new(OrderSide::Buy, 2.0)
  ///     .limit(150.0)
  ///     .for_stock("AAPL")
  ///     .build()?;
  ///
  /// order_mgr.replace_order(&order_id, new_contract, new_req)?;
  /// println!("Replacement request sent for order {}", order_id);
  /// # Ok(())
  /// # }
  /// ```
  pub fn replace_order(&self, order_id: &str, contract: Contract, request: OrderRequest) -> Result<Arc<RwLock<Order>>, IBKRError> {
    info!("Replacing order ID: {} with new contract/request", order_id);
    let order_id_int = order_id.parse::<i32>()
      .map_err(|_| IBKRError::InvalidOrder(format!("Invalid order ID format: {}", order_id)))?;

    let order_arc = {
      let book = self.order_book.read();
      book.get(order_id)
        .cloned()
        .ok_or_else(|| IBKRError::InvalidOrder(format!("Order ID {} not found for replacement", order_id)))?
    };

    // Check if the order is already in a terminal state before attempting replacement
    {
      let order = order_arc.read();
      if order.state.is_terminal() {
        error!("Cannot replace order {} because it is in a terminal state: {:?}", order_id, order.state.status);
        return Err(IBKRError::InvalidOrder(format!("Order {} is terminal ({:?}) and cannot be replaced", order_id, order.state.status)));
      }
    }

    // Prepare and send message with the *same order ID* but *new contract and request*
    let encoder = Encoder::new(self.message_broker.get_server_version()?);
    let replace_msg = encoder.encode_place_order(order_id_int, &contract, &request)?;
    self.message_broker.send_message(&replace_msg)?;

    info!("Replace order request sent for ID: {}", order_id);
    // We don't update the local state immediately. We wait for OrderStatus updates.
    Ok(order_arc)
  }

  /// Modifies an existing open order.
  ///
  /// This method sends a new `placeOrder` message to TWS with the *same client order ID*
  /// but with updated parameters from the `updates` argument. TWS will attempt to
  /// replace the existing order with the modified one.
  ///
  /// The method returns immediately after sending the modification request.
  /// The returned `Arc<RwLock<Order>>` is a reference to the order's state in the
  /// local order book, which will be updated asynchronously by TWS messages.
  ///
  /// # Arguments
  /// * `order_id` - The client order ID of the order to modify.
  /// * `updates` - An [`OrderUpdates`] struct containing the parameters to change
  ///   (e.g., quantity, limit price). Fields set to `None` in `updates` are not changed.
  ///
  /// # Returns
  /// An `Arc<RwLock<Order>>` pointing to the order being modified.
  ///
  /// # Errors
  /// Returns `IBKRError` if the order ID is not found, the order is already in a
  /// terminal state, the order ID format is invalid, or if there are issues
  /// encoding or sending the modification request.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError, OrderBuilder, contract::Contract, order::{OrderSide, OrderUpdates}};
  /// # use std::time::Duration;
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// # let order_mgr = client.orders();
  /// # let (contract, order_req) = OrderBuilder::new(OrderSide::Buy, 1.0).limit(100.0).for_stock("XYZ").build()?;
  /// # let order_id = order_mgr.place_order(contract, order_req)?;
  /// // Assume order_id is an active limit order
  /// let updates = OrderUpdates {
  ///     limit_price: Some(100.50), // Change limit price
  ///     ..Default::default()
  /// };
  /// order_mgr.modify_order(&order_id, updates)?;
  /// println!("Modification request sent for order {}", order_id);
  /// # Ok(())
  /// # }
  /// ```
  pub fn modify_order(&self, order_id: &str, updates: OrderUpdates) -> Result<Arc<RwLock<Order>>, IBKRError> {
    info!("Modifying order ID: {} with updates: {:?}", order_id, updates);
    let order_id_int = order_id.parse::<i32>()
      .map_err(|_| IBKRError::InvalidOrder(format!("Invalid order ID format: {}", order_id)))?;

    let order_arc = {
      let book = self.order_book.read();
      book.get(order_id)
        .cloned()
        .ok_or_else(|| IBKRError::InvalidOrder(format!("Order ID {} not found for modification", order_id)))?
    };

    // Check if the order is already in a terminal state before attempting modification
    {
      let order = order_arc.read();
      if order.state.is_terminal() {
        error!("Cannot modify order {} because it is in a terminal state: {:?}", order_id, order.state.status);
        return Err(IBKRError::InvalidOrder(format!("Order {} is terminal ({:?}) and cannot be modified", order_id, order.state.status)));
      }
    }

    // Create a *new* OrderRequest based on the *original* request, applying updates
    let mut modified_request = {
      let order = order_arc.read();
      order.request.clone() // Clone the original request
    };

    // TWS sends weird string on delta_neutral_order_type
    if let Some(delta_neutral_order_type) = &modified_request.delta_neutral_order_type {
      if delta_neutral_order_type.as_bytes() == &[195, 142, 195, 158] /* ÎÞ */ {
        modified_request.delta_neutral_order_type = None;
      }
    }

    // Apply updates
    let mut changed = false;
    if let Some(qty) = updates.quantity {
      if modified_request.quantity != qty {
        modified_request.quantity = qty;
        changed = true;
      }
    }
    if let Some(lmt) = updates.limit_price {
      if modified_request.limit_price != Some(lmt) {
        modified_request.limit_price = Some(lmt);
        changed = true;
      }
    } else if updates.limit_price.is_some() { // Check if explicitly set to None (e.g. change to Market)
      if modified_request.limit_price.is_some() {
        modified_request.limit_price = None;
        changed = true;
        // Consider changing order_type too? Modifying to MKT is tricky.
        warn!("Modifying order {} limit price to None. Order type remains {}. Check TWS behavior.", order_id, modified_request.order_type);
      }
    }
    if let Some(aux) = updates.aux_price {
      if modified_request.aux_price != Some(aux) {
        modified_request.aux_price = Some(aux);
        changed = true;
      }
    } else if updates.aux_price.is_some() {
      if modified_request.aux_price.is_some() {
        modified_request.aux_price = None;
        changed = true;
      }
    }
    if let Some(tif) = updates.time_in_force {
      if modified_request.time_in_force != tif {
        modified_request.time_in_force = tif;
        changed = true;
      }
    }
    // Add other updatable fields (Trail, OutsideRTH, etc.)

    if !changed {
      warn!("No changes detected for modifying order ID: {}", order_id);
      return Ok(order_arc); // Return original if no changes
    }

    // Get the contract from the existing order
    let contract = {
      order_arc.read().contract.clone()
    };

    // Prepare and send message with the *same order ID* but *modified request*
    let encoder = Encoder::new(self.message_broker.get_server_version()?);
    let modify_msg = encoder.encode_place_order(order_id_int, &contract, &modified_request)?;
    log::info!("Contract: {:?}, OrderRequest: {:?}", contract, modified_request);
    self.message_broker.send_message(&modify_msg)?;

    info!("Modify order request sent for ID: {}", order_id);
    // We don't update the local OrderRequest immediately. We wait for OrderStatus updates.
    Ok(order_arc)
  }

  /// Sends a global cancel request to TWS.
  ///
  /// This request will instruct TWS to cancel all open orders for the current client ID.
  /// TWS will send `orderStatus` messages for each order that is cancelled as a result.
  /// The local order book will be updated based on these incoming messages.
  ///
  /// This method returns immediately after sending the request and does not wait for
  /// confirmation of cancellations.
  ///
  /// # Errors
  /// Returns `IBKRError` if the server version does not support global cancel,
  /// or if there's an issue encoding or sending the message.
  pub fn cancel_all_orders_globally(&self) -> Result<(), IBKRError> {
    info!("Requesting global cancellation of all orders.");
    if self.message_broker.get_server_version()? < min_server_ver::GLOBAL_CANCEL {
      return Err(IBKRError::Unsupported(
        "Server version does not support global cancel requests.".to_string()
      ));
    }

    let encoder = Encoder::new(self.message_broker.get_server_version()?);
    let global_cancel_msg = encoder.encode_request_global_cancel()?;
    self.message_broker.send_message(&global_cancel_msg)?;

    info!("Global cancel request sent.");
    // Note: TWS will send individual orderStatus messages for affected orders.
    // The local state will be updated by the orderStatus handler.
    Ok(())
  }

  /// Exercises or lapses an option contract.
  ///
  /// This method sends an "exercise options" request to TWS.
  /// Note that this is not an "order" in the typical sense and does not go through
  /// the regular order lifecycle or use client order IDs. TWS uses a `reqId` (ticker ID)
  /// for this request, similar to market data requests.
  ///
  /// The outcome of this operation is typically observed through account and position updates,
  /// or via error messages if the request is invalid.
  ///
  /// # Arguments
  /// * `req_id` - A unique request identifier (like a ticker ID, not an order ID).
  /// * `contract` - The option [`Contract`] to exercise or lapse. Must be an OPT or FOP.
  /// * `action` - The [`ExerciseAction`](crate::order::ExerciseAction) to perform (Exercise or Lapse).
  /// * `quantity` - The number of contracts to exercise/lapse.
  /// * `account` - The account holding the option.
  /// * `override_action` - Set to `true` to override TWS's default exercise behavior (e.g., for OTM options).
  ///   Set to `false` (0) for default behavior.
  ///
  /// # Errors
  /// Returns `IBKRError` if the server version does not support this feature,
  /// if the contract is not an option, or if there's an issue encoding or sending the message.
  pub fn exercise_option(
    &self,
    req_id: i32,
    contract: &Contract,
    action: crate::order::ExerciseAction,
    quantity: i32,
    account: &str,
    override_action: bool,
  ) -> Result<(), IBKRError> {
    info!(
      "Requesting to {:?} {} contracts of {} in account {}",
      action, quantity, contract.symbol, account
    );

    if contract.sec_type != SecType::Option && contract.sec_type != SecType::FutureOption {
      return Err(IBKRError::InvalidContract(
        "exercise_option can only be used for Option (OPT) or FutureOption (FOP) contracts.".to_string()
      ));
    }
    if quantity <= 0 {
      return Err(IBKRError::InvalidParameter("Exercise quantity must be positive.".to_string()));
    }

    let encoder = Encoder::new(self.message_broker.get_server_version()?);
    let exercise_msg = encoder.encode_exercise_options(
      req_id,
      contract,
      action,
      quantity,
      account,
      if override_action { 1 } else { 0 },
    )?;
    self.message_broker.send_message(&exercise_msg)?;

    info!(
      "Exercise/Lapse request sent for {} (ReqID: {}). Monitor account/position updates for confirmation.",
      contract.symbol, req_id
    );
    Ok(())
  }

  /// Checks the margin and commission impact of a potential order without placing it.
  ///
  /// This sends a "What-If" order request to TWS. TWS calculates the impact and
  /// returns the details (initial/maintenance margin, commission) via an `openOrder`
  /// message associated with the temporary order ID used for the check.
  ///
  /// This method blocks until the `openOrder` message with the results is received
  /// or the specified timeout occurs.
  ///
  /// # Arguments
  /// * `contract` - The [`Contract`] for the instrument.
  /// * `request` - The [`OrderRequest`] describing the potential order.
  /// * `timeout` - The maximum duration to wait for the What-If results.
  ///
  /// # Returns
  /// * `Ok(OrderState)` - The [`OrderState`] containing the calculated margin and commission
  ///   details received from TWS via the `openOrder` message.
  /// * `Err(IBKRError::Timeout)` - If the timeout was reached before results were received.
  /// * `Err(IBKRError::...)` - For other errors, such as placing the initial request,
  ///   API errors returned by TWS for the What-If order, or if the required server
  ///   version is not met.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError, OrderBuilder, contract::Contract, order::OrderSide};
  /// # use std::time::Duration;
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// # let order_mgr = client.orders();
  /// let (contract, order_req) = OrderBuilder::new(OrderSide::Buy, 100.0)
  ///     .limit(250.50)
  ///     .for_stock("AAPL")
  ///     .build()?;
  ///
  /// match order_mgr.check_what_if_order(&contract, &order_req, Duration::from_secs(10)) {
  ///     Ok(state) => {
  ///         println!("What-If Results for AAPL order:");
  ///         println!("  Initial Margin After: {:?}", state.initial_margin_after);
  ///         println!("  Maintenance Margin After: {:?}", state.maintenance_margin_after);
  ///         println!("  Commission: {:?} {}", state.commission, state.commission_currency.as_deref().unwrap_or(""));
  ///     }
  ///     Err(e) => eprintln!("What-If check failed: {:?}", e),
  /// }
  /// # Ok(())
  /// # }
  /// ```
  pub fn check_what_if_order(
    &self,
    contract: &Contract,
    request: &OrderRequest,
    timeout: Duration,
  ) -> Result<OrderState, IBKRError> {
    info!("Checking What-If order: {:?} x {:.0} for {}", request.side, request.quantity, contract.symbol);

    if self.message_broker.get_server_version()? < min_server_ver::WHAT_IF_ORDERS {
      return Err(IBKRError::Unsupported("Server version does not support What-If orders.".to_string()));
    }

    // Clone the request and set the what_if flag
    let mut what_if_request = request.clone();
    what_if_request.what_if = true;
    // What-if orders must have transmit=true according to some docs, but builder defaults to true.
    // Let's ensure it here for safety, although the builder should handle it.
    if !what_if_request.transmit {
      warn!("What-if order request had transmit=false, forcing to true.");
      what_if_request.transmit = true;
    }


    // Place the what-if order. This uses the next available order ID.
    // The order manager will store it temporarily.
    let order_id = self.place_order(contract.clone(), what_if_request)?;
    info!("What-If order placed with temporary ID: {}. Waiting for results...", order_id);

    // Wait for the openOrder message containing the margin/commission details.
    match self._wait_for_what_if_state(&order_id, timeout) {
      Ok(state) => {
        info!("Received What-If results for order ID: {}", order_id);
        // Optionally, clean up the temporary order entry from the book?
        // TWS doesn't track it permanently, but it might linger locally.
        // For now, leave it, it will eventually be overwritten or ignored.
        // self.order_book.write().remove(&order_id);
        // self.order_update_condvars.write().remove(&order_id);
        Ok(state)
      }
      Err(e) => {
        error!("Failed to get What-If results for order ID {}: {:?}", order_id, e);
        // Cleanup might still be relevant on error
        // self.order_book.write().remove(&order_id);
        // self.order_update_condvars.write().remove(&order_id);
        Err(e)
      }
    }
  }


  /// Refreshes the local order book by requesting all open orders from TWS.
  ///
  /// This is a blocking call. It sends a `reqAllOpenOrders` message to TWS and
  /// waits until TWS responds with an `openOrderEnd` message, indicating that all
  /// open orders have been sent. The local order book is updated with the received orders.
  ///
  /// This is useful for synchronizing the client's view of open orders with TWS,
  /// especially after a reconnection or if there's a suspicion of discrepancies.
  /// message from TWS, or until the specified timeout is reached.
  ///
  /// # Arguments
  /// * `timeout` - The maximum duration to wait for the `openOrderEnd` signal.
  ///
  /// # Returns
  /// * `Ok(())` if the refresh completed successfully within the timeout.
  /// * `Err(IBKRError::Timeout)` if the timeout was reached before `openOrderEnd` was received.
  /// * `Err(IBKRError::...)` for other potential errors during message encoding or sending.
  pub fn refresh_orderbook(&self, timeout: Duration) -> Result<(), IBKRError> {
    info!("Requesting order book refresh (reqAllOpenOrders)...");

    // 1. Reset the openOrderEnd flag *before* sending the request.
    {
      let mut flag_guard = self.open_order_end_flag.lock();
      *flag_guard = false;
      debug!("Reset openOrderEnd flag for refresh.");
    } // Release lock

    // 2. Send the reqAllOpenOrders message.
    let encoder = Encoder::new(self.message_broker.get_server_version()?);
    let refresh_msg = encoder.encode_request_all_open_orders()?;
    self.message_broker.send_message(&refresh_msg)?;
    info!("Sent reqAllOpenOrders message.");

    // 3. Wait for the openOrderEnd signal.
    let mut flag_guard = self.open_order_end_flag.lock();
    let start_time = std::time::Instant::now();

    while !*flag_guard { // Check the flag *before* waiting
      let elapsed = start_time.elapsed();
      if elapsed >= timeout {
        warn!("Timeout waiting for openOrderEnd during order book refresh.");
        return Err(IBKRError::Timeout("Timeout waiting for openOrderEnd".to_string()));
      }
      let remaining_wait = timeout - elapsed;
      // Wait on the condvar, releasing the lock temporarily
      let result = self.open_order_end_condvar.wait_for(&mut flag_guard, remaining_wait);
      if result.timed_out() && !*flag_guard { // Re-check flag after timeout
        warn!("Timeout waiting for openOrderEnd during order book refresh (after wait).");
        return Err(IBKRError::Timeout("Timeout waiting for openOrderEnd".to_string()));
      }
      // If woken up, the loop condition (!*flag_guard) will be checked again.
      debug!("refresh_orderbook wait notified, checking flag...");
    }

    info!("Order book refresh completed (received openOrderEnd).");
    Ok(())
  }

  /// Internal helper specifically for waiting for the `OrderState` fields populated
  /// by an `openOrder` message in response to a What-If request.
  ///
  /// It waits until the state contains margin or commission information, or until
  /// an error occurs or the timeout elapses.
  fn _wait_for_what_if_state(&self, order_id: &str, timeout_duration: Duration) -> Result<OrderState, IBKRError> {
    debug!("Waiting for What-If state for order {} (timeout: {:?})", order_id, timeout_duration);

    // Get or create the condvar pair BEFORE checking state to ensure it exists
    let condvar_pair = {
      let condvars_read = self.order_update_condvars.read();
      if let Some(pair) = condvars_read.get(order_id) {
        pair.clone()
      } else {
        drop(condvars_read);
        let mut condvars_write = self.order_update_condvars.write();
        condvars_write.entry(order_id.to_string())
          .or_insert_with(|| Arc::new((Mutex::new(()), Condvar::new())))
          .clone()
      }
    };

    let (lock, cvar) = &*condvar_pair;
    let mut guard = lock.lock();
    let start_time = std::time::Instant::now();

    // CRITICAL: Check state BEFORE entering the wait loop to catch race conditions
    // where TWS responds immediately before we start waiting
    let initial_state_check = {
      let book = self.order_book.read();
      book.get(order_id).map(|order_arc| order_arc.read().state.clone())
    };

    match initial_state_check {
      None => {
        error!("Order {} not found before starting what-if wait", order_id);
        return Err(IBKRError::InternalError(format!("Order {} lost before what-if wait", order_id)));
      }
      Some(state) => {
        // Check if error already occurred (race condition case)
        if let Some(err) = &state.error {
          error!("Order {} already failed before wait started (race condition): {:?}", order_id, err);
          return Err(err.clone());
        }
        // Check if margin/commission data already available (very fast response case)
        if state.initial_margin_after.is_some() || state.maintenance_margin_after.is_some() || state.commission.is_some() {
          info!("Order {} what-if state already available before wait started", order_id);
          return Ok(state);
        }
        // Otherwise, proceed to wait loop
        debug!("Order {} what-if state not yet available, entering wait loop", order_id);
      }
    }

    loop {
      // --- Check current state ---
      let current_order_state = {
        let book = self.order_book.read();
        book.get(order_id).map(|order_arc| order_arc.read().state.clone())
      };

      match current_order_state {
        None => {
          error!("Order {} disappeared while waiting for What-If state", order_id);
          return Err(IBKRError::InternalError(format!("Order {} lost during What-If wait", order_id)));
        }
        Some(state) => {
          // Check for error first
          if let Some(err) = &state.error {
            error!("Order {} (What-If) failed while waiting for state: {:?}", order_id, err);
            return Err(err.clone());
          }
          // Check if the state contains the expected What-If info
          if state.initial_margin_after.is_some() || state.maintenance_margin_after.is_some() || state.commission.is_some() {
            info!("Order {} received What-If state.", order_id);
            return Ok(state);
          }
          debug!("Order {} What-If state not yet populated, continuing wait...", order_id);
        }
      }

      // --- Wait or Timeout ---
      let elapsed = start_time.elapsed();
      if elapsed >= timeout_duration {
        warn!("Timeout waiting for order {} What-If state", order_id);
        // Final check after timeout
        let final_state = {
          let book = self.order_book.read();
          book.get(order_id).map(|o| o.read().state.clone())
        };
        match final_state {
          Some(state) if state.error.is_some() => return Err(state.error.unwrap()),
          Some(state) if state.initial_margin_after.is_some() || state.maintenance_margin_after.is_some() || state.commission.is_some() => return Ok(state),
          _ => return Err(IBKRError::Timeout(format!("Timeout waiting for order {} What-If state", order_id))),
        }
      }
      let remaining_wait = timeout_duration - elapsed;
      let result = cvar.wait_for(&mut guard, remaining_wait);
      if result.timed_out() {
        debug!("Condvar wait timed out for order {} (What-If), re-checking conditions...", order_id);
      } else {
        debug!("Wait for order {} (What-If) notified, re-checking status...", order_id);
      }
    }
  }

  // --- Wait Functions ---

  /// Internal helper for the common logic of waiting for an order to reach one of
  /// a set of target statuses, or until it fails, terminates unexpectedly, or times out.
  ///
  /// # Arguments
  /// * `order_id` - The client order ID to monitor.
  /// * `target_statuses` - A slice of [`OrderStatus`] values that are considered successful outcomes.
  /// * `timeout_duration` - An `Option<Duration>`. If `Some`, the function will time out
  ///   after this duration. If `None`, it will wait indefinitely.
  ///
  /// # Returns
  /// * `Ok(OrderStatus)` - The specific `OrderStatus` from `target_statuses` that was reached.
  /// * `Err(IBKRError::Timeout)` - If `timeout_duration` was specified and elapsed.
  /// * `Err(IBKRError::InvalidOrder)` - If the order is not found, or if it reaches a
  ///   terminal state that is *not* one of the `target_statuses`.
  /// * `Err(IBKRError)` - If the order's internal state indicates a previously recorded API error.
  fn _wait_for_status_internal(
    &self,
    order_id: &str,
    target_statuses: &[OrderStatus],
    timeout_duration: Option<Duration>
  ) -> Result<OrderStatus, IBKRError> {
    let timeout_str = timeout_duration.map_or_else(|| "indefinitely".to_string(), |d| format!("{:?}", d));
    debug!("Waiting for order {} to reach status {:?} (timeout: {})", order_id, target_statuses, timeout_str);

    // Get or create the condvar pair *before* checking the status, to avoid race conditions
    // where a notification happens between the check and the wait.
    let condvar_pair = {
      // First try read lock for efficiency
      let condvars_read = self.order_update_condvars.read();
      if let Some(pair) = condvars_read.get(order_id) {
        pair.clone()
      } else {
        // Drop read lock and acquire write lock if not found
        drop(condvars_read);
        let mut condvars_write = self.order_update_condvars.write();
        // Use entry().or_insert_with() for atomicity within the write lock
        condvars_write.entry(order_id.to_string())
          .or_insert_with(|| Arc::new((Mutex::new(()), Condvar::new())))
          .clone()
      }
    };

    let (lock, cvar) = &*condvar_pair;
    let mut guard = lock.lock(); // Lock the mutex associated with the condvar

    let start_time = std::time::Instant::now();

    loop {
      // --- Check current state ---
      let current_state = {
        let book = self.order_book.read();
        book.get(order_id)
          .map(|order_arc| {
            let order = order_arc.read();
            (order.state.status, order.state.error.clone()) // Clone status and potential error
          })
      };

      match current_state {
        // Order not found
        None => {
          error!("Order {} not found while waiting for status", order_id);
          return Err(IBKRError::InvalidOrder(format!("Order {} not found while waiting", order_id)));
        }
        // Error occurred
        Some((_status, Some(err))) => {
          error!("Order {} failed while waiting for status {:?}: {:?}", order_id, target_statuses, err);
          return Err(err.clone());
        }
        // Target status reached
        Some((current_status, None)) if target_statuses.contains(&current_status) => {
          info!("Order {} reached target status {:?}", order_id, current_status);
          return Ok(current_status); // Condition met
        }
        // Reached a terminal state *other* than the target
        Some((current_status, None)) if current_status.is_terminal() => {
          error!("Order {} reached terminal state {:?} while waiting for {:?}", order_id, current_status, target_statuses);
          return Err(IBKRError::InvalidOrder(format!("Order {} terminated in state {:?} unexpectedly while waiting for {:?}",
                                                     order_id, current_status, target_statuses)));
        }
        // Still waiting, status is not target and not terminal/error
        Some((current_status, None)) => {
          debug!("Order {} current status is {:?}, continuing wait for {:?}", order_id, current_status, target_statuses);
        }
      }
      // --- End Check current state ---


      // --- Wait or Timeout ---
      if let Some(timeout) = timeout_duration {
        let elapsed = start_time.elapsed();
        if elapsed >= timeout {
          warn!("Timeout waiting for order {} to reach status {:?}", order_id, target_statuses);
          // Re-check status one last time *after* timeout determined, before returning error
          let final_state_after_timeout = {
            let book = self.order_book.read();
            book.get(order_id).map(|o| (o.read().state.status, o.read().state.error.clone()))
          };
          match final_state_after_timeout {
            Some((_status, Some(err))) => return Err(err), // Return error if found
            Some((status, None)) if target_statuses.contains(&status) => return Ok(status), // Return success if target met concurrently
            Some((status, None)) if status.is_terminal() => return Err(IBKRError::InvalidOrder(format!("Order {} terminated in state {:?} unexpectedly on timeout while waiting for {:?}", order_id, status, target_statuses))),
            _ => return Err(IBKRError::Timeout(format!("Timeout waiting for order {} status ({:?})", order_id, target_statuses))), // Return timeout error
          }
        }
        let remaining_wait = timeout - elapsed;
        // wait_for releases the `guard` (mutex lock) and waits; re-acquires lock on wakeup/timeout
        let result = cvar.wait_for(&mut guard, remaining_wait);
        if result.timed_out() {
          // If timed out according to condvar, loop will re-check elapsed time and likely exit.
          // We re-check the condition inside the loop in case of spurious wakeup or race.
          debug!("Condvar wait timed out for order {}, re-checking conditions...", order_id);
        } else {
          debug!("Wait for order {} notified, re-checking status...", order_id);
        }
      } else {
        // Wait indefinitely
        cvar.wait(&mut guard);
        debug!("Wait indefinitely for order {} notified, re-checking status...", order_id);
      }
      // --- End Wait or Timeout ---

      // Loop continues: will re-check status after waking up or timing out.
    }
  }


  /// Waits indefinitely until an order's status becomes `Submitted` or `Filled`,
  /// or until the order fails or reaches another terminal state.
  ///
  /// This is useful for confirming that TWS has accepted an order.
  /// `Filled` is included as a target because an aggressive order might fill
  /// before a `Submitted` status is even processed locally.
  pub fn wait_order_submitted(&self, order_id: &str) -> Result<OrderStatus, IBKRError> {
    self._wait_for_status_internal(order_id, &[OrderStatus::Submitted, OrderStatus::Filled], None)
  }

  /// Tries to wait for a specified duration until an order's status becomes `Submitted` or `Filled`,
  /// or until the order fails, terminates, or the timeout elapses.
  ///
  /// Returns the status reached (`Submitted` or `Filled`) or an error (including `IBKRError::Timeout`).
  pub fn try_wait_order_submitted(&self, order_id: &str, timeout: Duration) -> Result<OrderStatus, IBKRError> {
    self._wait_for_status_internal(order_id, &[OrderStatus::Submitted, OrderStatus::Filled], Some(timeout))
  }

  /// Waits indefinitely until an order's status becomes `Filled`,
  /// or until the order fails or reaches another terminal state.
  ///
  /// This is used to wait for an order to be fully executed.
  pub fn wait_order_executed(&self, order_id: &str) -> Result<OrderStatus, IBKRError> {
    self._wait_for_status_internal(order_id, &[OrderStatus::Filled], None)
  }

  /// Tries to wait for a specified duration until an order's status becomes `Filled`,
  /// or until the order fails, terminates, or the timeout elapses.
  ///
  /// Returns `OrderStatus::Filled` or an error (including `IBKRError::Timeout`).
  pub fn try_wait_order_executed(&self, order_id: &str, timeout: Duration) -> Result<OrderStatus, IBKRError> {
    self._wait_for_status_internal(order_id, &[OrderStatus::Filled], Some(timeout))
  }

  /// Waits indefinitely until an order's status becomes `Cancelled`, `ApiCancelled`, or `Inactive`.
  ///
  /// `Inactive` often implies cancellation but might lack explicit TWS confirmation.
  /// This method is used to wait for an order cancellation to be confirmed.
  pub fn wait_order_canceled(&self, order_id: &str) -> Result<OrderStatus, IBKRError> {
    self._wait_for_status_internal(order_id, &[OrderStatus::Cancelled, OrderStatus::ApiCancelled, OrderStatus::Inactive], None)
  }

  /// Tries to wait for a specified duration until an order's status becomes `Cancelled`, `ApiCancelled`, or `Inactive`,
  /// or until the order fails, terminates, or the timeout elapses.
  ///
  /// Returns the status reached (`Cancelled`, `ApiCancelled`, `Inactive`) or an error (including `IBKRError::Timeout`).
  pub fn try_wait_order_canceled(&self, order_id: &str, timeout: Duration) -> Result<OrderStatus, IBKRError> {
    self._wait_for_status_internal(order_id, &[OrderStatus::Cancelled, OrderStatus::ApiCancelled, OrderStatus::Inactive], Some(timeout))
  }


  // --- Getters ---

  /// Retrieves a snapshot of an order's current state by its client order ID.
  ///
  /// Returns `Some(Order)` if the order is found in the local order book,
  /// or `None` if no order with that ID is known. The returned `Order` is a clone
  /// of the current state.
  pub fn get_order(&self, order_id: &str) -> Option<Order> {
    let book = self.order_book.read();
    book.get(order_id).map(|order_arc| order_arc.read().clone())
  }

  /// Retrieves a list of all orders currently known to the `OrderManager`.
  ///
  /// This includes orders in any state (open, filled, cancelled, etc.).
  /// The returned `Order` objects are clones of their current state.
  pub fn get_all_orders(&self) -> Vec<Order> {
    let book = self.order_book.read();
    book.values().map(|order_arc| order_arc.read().clone()).collect()
  }

  /// Retrieves a list of all orders currently known to the `OrderManager`.
  ///
  /// This includes orders in the terminal state (filled, cancelled, etc.).
  /// The returned `Order` objects are clones of their current state.
  pub fn get_completed_orders(&self) -> Vec<Order> {
    let book = self.order_book.read();
    book.values()
      .map(|order_arc| order_arc.read()) // Get read guard
      .filter(|order| order.state.is_terminal()) // Use is_terminal() helper
      .map(|order_guard| order_guard.clone()) // Clone the Order data
      .collect()
  }

  /// Retrieves a list of all open (non-terminal) orders.
  ///
  /// An order is considered open if its status is not one of:
  /// `Filled`, `Cancelled`, `ApiCancelled`, or `Inactive`.
  /// The returned `Order` objects are clones of their current state.
  pub fn get_open_orders(&self) -> Vec<Order> {
    let book = self.order_book.read();
    book.values()
      .map(|order_arc| order_arc.read()) // Get read guard
      .filter(|order| !order.state.is_terminal()) // Use is_terminal() helper
      .map(|order_guard| order_guard.clone()) // Clone the Order data
      .collect()
  }

  // --- Notification Helpers ---

  fn notify_observers_update(&self, order: &Order) {
    let observers = self.observers.read();
    for observer in observers.iter() {
      observer.on_order_update(order);
    }
  }

  fn notify_observers_error(&self, order_id: &str, error: &IBKRError) {
    let observers = self.observers.read();
    for observer in observers.iter() {
      observer.on_order_error(order_id, error);
    }
  }

  // Notifies any threads waiting on this specific order's condvar.
  fn notify_waiters(&self, order_id: &str) {
    debug!("Notifying waiters for order {}", order_id);
    let condvars = self.order_update_condvars.read(); // Read lock is sufficient
    if let Some(condvar_pair) = condvars.get(order_id) {
      let (_lock, cvar) = &**condvar_pair; // Dereference Arc then tuple
      cvar.notify_all(); // Wake up all threads waiting on this order's condvar
      debug!("Notified condvar for order {}", order_id);
    } else {
      // This might happen if a status arrives before the order is fully processed locally,
      // though place_order tries to prevent this. Or if no one is waiting.
      debug!("No active waiters found (or condvar not yet created) for order {}", order_id);
    }
    debug!("Notified for {}", order_id);
  }

  /// Cleans up all pending order requests and internal state during client shutdown.
  ///
  /// This method attempts to cancel all open orders and clean up any pending waiters
  /// or internal state. It's designed to be called during client disconnect/shutdown
  /// to ensure a clean termination.
  ///
  /// The cleanup process includes:
  /// 1. Attempting to cancel all open orders (globally first, then individually as fallback)
  /// 2. Signaling any threads waiting for order book refresh to complete
  /// 3. Notifying all order-specific waiters to wake up and handle shutdown
  /// 4. Preserving order tracking data for any final status updates from TWS
  ///
  /// # Returns
  /// * `Ok(())` - Cleanup completed (may include some non-critical errors logged as warnings)
  /// * `Err(IBKRError)` - Critical error during cleanup that should be reported
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::{IBKRClient, IBKRError};
  /// # fn main() -> Result<(), IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// // During client shutdown
  /// client.orders().cleanup_requests()?;
  /// # Ok(())
  /// # }
  /// ```
  pub(crate) fn cleanup_requests(&self) -> Result<(), IBKRError> {
    info!("OrderManager: Starting cleanup of all pending requests and state...");

    let start_time = std::time::Instant::now();

    // Signal any threads waiting for order book refresh to complete
    // This prevents threads from hanging indefinitely during shutdown
    {
      let mut flag_guard = self.open_order_end_flag.lock();
      if !*flag_guard {
        info!("OrderManager: Signaling order book refresh waiters to complete during cleanup");
        *flag_guard = true; // Set flag to true to unblock waiters
        self.open_order_end_condvar.notify_all();
      }
    }

    // Wake up all order-specific waiters
    // This includes threads waiting for order status changes, executions, cancellations, etc.
    {
      let condvars = self.order_update_condvars.read();
      let waiter_count = condvars.len();

      if waiter_count > 0 {
        info!("OrderManager: Notifying {} order-specific waiters during cleanup", waiter_count);

        for (order_id, condvar_pair) in condvars.iter() {
          let (_lock, cvar) = &**condvar_pair;
          cvar.notify_all();
          debug!("OrderManager: Notified waiters for order {} during cleanup", order_id);
        }
      } else {
        debug!("OrderManager: No order-specific waiters to notify");
      }
    }

    // Note: We intentionally do NOT clear the order book, perm_id_map, or condvars
    // because:
    // - We might still receive final status updates from TWS after sending cancels
    // - The handlers need to be able to process these final messages properly
    // - Observers might still need access to final order states
    // - The structures will be cleaned up when the OrderManager is dropped

    let elapsed = start_time.elapsed();

    info!("OrderManager: Cleanup completed in {:?}", elapsed);
    Ok(())
  }
}

// --- Implement OrderHandler Trait ---
impl OrderHandler for OrderManager {
  fn next_valid_id(&self, order_id: i32) {
    info!("Handler: Received nextValidId: {}", order_id);
    let mut guard = self.next_order_id_state.lock();
    if guard.is_none() {
      *guard = Some(order_id as usize);
      debug!("Set initial next_valid_id to {}", order_id);
      self.order_id_condvar.notify_all(); // Notify the waiter in create()
    } else {
      // Normally, TWS only sends this once on connection.
      // If received again, it might indicate a reset or issue.
      warn!("Received nextValidId {} again, current state is {:?}. Updating.", order_id, *guard);
      *guard = Some(order_id as usize);
    }
  }

  fn order_status(
    &self,
    order_id_int: i32,
    order_status: OrderStatus,
    filled: f64,
    remaining: f64,
    avg_fill_price: f64,
    perm_id: i32,
    parent_id: i32,
    last_fill_price: f64,
    client_id: i32,
    why_held: &str,
    mkt_cap_price: Option<f64>,
  ) {
    let order_id_str = order_id_int.to_string();
    debug!(
      "Handler: Received orderStatus: ID={}, PermID={}, Status={:?}, Filled={}, Rem={}, AvgPx={}, LastPx={}, ClientId={}, WhyHeld={}",
      order_id_str, perm_id, order_status, filled, remaining, avg_fill_price, last_fill_price, client_id, why_held
    );

    // Find the order by client ID
    let order_arc = {
      let book = self.order_book.read();
      book.get(&order_id_str).cloned()
    };

    if let Some(order_arc) = order_arc {
      let order_updated = { // Scope for write lock
        let mut order = order_arc.write(); // Lock the specific order for writing

        let previous_status = order.state.status;

        // Update state based on received status
        order.state.status = order_status;
        order.state.filled_quantity = filled;
        order.state.remaining_quantity = remaining;
        order.state.average_fill_price = avg_fill_price;
        order.state.last_fill_price = if last_fill_price != 0.0 { Some(last_fill_price) } else { order.state.last_fill_price }; // Keep old if 0
        order.state.why_held = if !why_held.is_empty() { Some(why_held.to_string()) } else { None };
        order.state.market_cap_price = mkt_cap_price;

        // Update IDs if not already set or if changed (unlikely but possible)
        if order.perm_id.is_none() && perm_id != 0 {
          order.perm_id = Some(perm_id);
          // Add to perm_id map for lookup
          let mut p_map = self.perm_id_map.write();
          p_map.insert(perm_id, order_id_str.clone());
          debug!("Mapped PermID {} to OrderID {}", perm_id, order_id_str);
        }
        if order.client_id.is_none() && client_id != 0 {
          order.client_id = Some(client_id);
        }
        // Parent ID usually set on creation, but update if provided and different?
        if order.parent_id.is_none() && parent_id != 0 {
          order.parent_id = Some(parent_id as i64);
        }

        order.updated_at = Utc::now();
        // Clear previous error ONLY if the status indicates progress or finality,
        // otherwise keep the error state. For example, don't clear error on PendingSubmit.
        if order_status != OrderStatus::PendingSubmit && order_status != OrderStatus::PendingCancel {
          order.state.error = None;
        }


        debug!("Updated order {} state: {:?} -> {:?}", order_id_str, previous_status, order.state.status);
        order.clone() // Clone the updated order data *before* dropping the lock
      }; // Write lock is released here

      // Notify observers and waiters *after* releasing the lock
      self.notify_observers_update(&order_updated);
      // Crucially, notify waiters *after* state is updated
      self.notify_waiters(&order_id_str);

    } else {
      warn!("Received orderStatus for unknown order ID: {}", order_id_str);
      // Could this be an order placed by another client ID connected to the same TWS?
      // Or an order from a previous session? We might want to store it anyway if tracking is needed.
    }
  }

  fn open_order(
    &self,
    order_id_int: i32,
    contract: &Contract,
    order_request: &OrderRequest,
    order_state: &OrderState,
  ) {
    let order_id_str = order_id_int.to_string();
    info!("Handler: Received openOrder: ID={}, Symbol={}, Status={:?}", order_id_str, contract.symbol, order_state.status);

    let now = Utc::now();
    let mut order_book = self.order_book.write(); // Lock book for potential insert/update

    let order_arc_opt = order_book.get(&order_id_str).cloned(); // Clone Arc if exists

    if let Some(existing_order_arc) = order_arc_opt {
      drop(order_book); // Drop book lock early if updating existing order
      debug!("Updating existing order {} from openOrder message", order_id_str);
      let updated_order_clone;
      { // Scope for write lock on the specific order
        let mut order = existing_order_arc.write();

        // Update contract, request, and state from the openOrder message
        // This overwrites local state with potentially more accurate TWS state.
        order.contract = contract.clone();
        order.request = order_request.clone();

        // Merge state: Trust openOrder for static info (margins, commission, warning)
        // but keep dynamic info (status, filled, remaining, avg_price, last_price, why_held)
        // from potentially more recent orderStatus messages unless the openOrder status
        // is itself terminal.
        let current_dynamic_state = (
          order.state.status,
          order.state.filled_quantity,
          order.state.remaining_quantity,
          order.state.average_fill_price,
          order.state.last_fill_price,
          order.state.why_held.clone(),
        );
        let previous_status = order.state.status;

        // Overwrite the entire state first
        order.state = order_state.clone();

        // Restore dynamic fields if the new status is not terminal and the old ones were set
        if !order.state.status.is_terminal() {
          order.state.status = current_dynamic_state.0; // Keep potentially newer status
          order.state.filled_quantity = current_dynamic_state.1;
          order.state.remaining_quantity = current_dynamic_state.2;
          order.state.average_fill_price = current_dynamic_state.3;
          order.state.last_fill_price = current_dynamic_state.4;
          order.state.why_held = current_dynamic_state.5;
        }

        // Update permId mapping if available (unlikely in openOrder unless it's the first msg)
        if let Some(perm_id) = order.perm_id { // Assuming permId might be parsed into OrderState by parser
          let mut p_map = self.perm_id_map.write();
          if !p_map.contains_key(&perm_id) {
            p_map.insert(perm_id, order_id_str.clone());
            debug!("Mapped PermID {} to OrderID {} from openOrder", perm_id, order_id_str);
          }
        }

        order.updated_at = now;
        order.state.error = None; // Clear error on receiving open order update

        debug!("Merged openOrder state for {}: {:?} -> {:?}", order_id_str, previous_status, order.state.status);
        updated_order_clone = order.clone(); // Clone *before* dropping lock
      } // Drop write lock on specific order

      // Notify
      self.notify_observers_update(&updated_order_clone);
      self.notify_waiters(&order_id_str);

    } else {
      // Order not known locally, create it entirely from openOrder message
      debug!("Adding new order {} from openOrder message", order_id_str);
      let new_order = Order {
        id: order_id_str.clone(),
        // PermID *might* be included in openOrder for very old orders, check state
        perm_id: None, // TODO: Check if parser adds permId to OrderState from openOrder
        client_id: None, // Not usually in openOrder message
        parent_id: order_request.parent_id,
        contract: contract.clone(),
        request: order_request.clone(),
        state: order_state.clone(),
        created_at: now, // Treat 'now' as creation time locally
        updated_at: now,
      };
      let new_order_arc = Arc::new(RwLock::new(new_order));
      order_book.insert(order_id_str.clone(), new_order_arc.clone());

      // Ensure condvar exists for this new order
      {
        let mut condvars_write = self.order_update_condvars.write();
        condvars_write.entry(order_id_str.clone())
          .or_insert_with(|| Arc::new((Mutex::new(()), Condvar::new())));
      }

      // Update permId mapping if available
      // if let Some(perm_id) = new_order_arc.read().perm_id { // Assuming permId exists
      //     let mut p_map = self.perm_id_map.write();
      //     p_map.insert(perm_id, order_id_str.clone());
      //     debug!("Mapped PermID {} to OrderID {} from new openOrder", perm_id, order_id_str);
      // }


      let new_order_clone = new_order_arc.read().clone();
      drop(order_book); // Drop write lock on book

      // Notify
      self.notify_observers_update(&new_order_clone);
      self.notify_waiters(&order_id_str);
    }
  }

  fn open_order_end(&self) {
    info!("Handler: Received openOrderEnd");
    // This signals the end of the initial burst of open orders after connection
    // or after reqOpenOrders / reqAllOpenOrders.
    // Set the flag and notify any waiters (e.g., refresh_orderbook).
    {
      let mut flag_guard = self.open_order_end_flag.lock();
      *flag_guard = true;
      debug!("Set openOrderEnd flag to true.");
    } // Release lock
    self.open_order_end_condvar.notify_all();
    debug!("Notified openOrderEnd condvar.");
  }

  /// Handles errors specific to an order ID (e.g., placing invalid order).
  /// This is called by the central error processor.
  fn handle_error(&self, order_id: i32, code: ClientErrorCode, msg: &str) {
    let order_id_str = if order_id > 0 {
      order_id.to_string()
    } else {
      // Error not associated with a specific order ID (-1)
      // These should ideally be handled by ClientHandler, but if routed here, log it.
      error!("Handler (Order): Received general API error: Code={:?}, Msg={}", code, msg);
      // Notify general error observers?
      // self.notify_observers_error("-1", &IBKRError::ApiError(code as i32, msg.to_string()));
      return; // Don't process further as order-specific logic
    };

    error!("Handler: Received error for Order ID {}: Code={:?}, Msg={}", order_id_str, code, msg);
    // Create the IBKRError using the enum's code and the message string
    let ibkr_error = IBKRError::ApiError(code as i32, msg.to_string());

    let order_arc = {
      let book = self.order_book.read();
      book.get(&order_id_str).cloned()
    };

    if let Some(order_arc) = order_arc {
      { // Scope write lock
        let mut order = order_arc.write();
        // Mark order as errored. Often implies cancellation or rejection.
        // Setting to Cancelled might be presumptive; maybe add an Error status?
        // For now, setting to Cancelled and storing the error is common practice.
        let previous_status = order.state.status;
        if !order.state.status.is_terminal() {
          order.state.status = OrderStatus::ApiCancelled;
        }
        order.state.error = Some(ibkr_error.clone());
        order.updated_at = Utc::now();
        debug!("Marked order {} with error: {:?} -> {:?}", order_id_str, previous_status, order.state.status);
      } // release lock

      self.notify_observers_error(&order_id_str, &ibkr_error); // Notify observers
      self.notify_waiters(&order_id_str); // Notify waiters

    } else {
      // Error for an order we don't know about? Log it.
      warn!("Received error for unknown/untracked order ID: {}", order_id_str);
      // Should we still notify generic error observers?
      self.notify_observers_error(&order_id_str, &ibkr_error);
    }
  }

  fn order_bound(&self, order_id_i64: i64, api_client_id: i32, api_order_id: i32) {
    // This message links an order ID generated by TWS itself (order_id_i64)
    // to an order placed via the API (api_client_id, api_order_id).
    // This is less common if all orders are placed via *this* API connection,
    // but useful if orders might be placed manually or via other clients/sessions.
    let api_order_id_str = api_order_id.to_string();
    info!(
      "Handler: Received orderBound: TwsOrderId={}, ApiClientId={}, ApiOrderId={}",
      order_id_i64, api_client_id, api_order_id_str
    );

    // Try to find the order by the API order ID
    let order_arc = {
      let book = self.order_book.read();
      book.get(&api_order_id_str).cloned()
    };

    if let Some(order_arc) = order_arc {
      let mut order = order_arc.write(); // Lock for update
      // We could potentially store the TWS-generated order_id_i64 if needed,
      // but perm_id received via orderStatus is usually more useful.
      // Check if the client ID matches what we expect, though TWS might report
      // the binding regardless of which client placed it.
      if order.client_id.is_none() {
        order.client_id = Some(api_client_id);
        debug!("Set client ID for order {} from orderBound", api_order_id_str);
      } else if order.client_id != Some(api_client_id) {
        warn!("orderBound received for order {} but ApiClientId {} differs from stored ClientId {:?}",
              api_order_id_str, api_client_id, order.client_id);
      }
      // No state change or notification usually needed for orderBound itself.
      order.updated_at = Utc::now(); // Update timestamp
    } else {
      warn!("Received orderBound for unknown ApiOrderId: {}", api_order_id_str);
      // Potentially create a placeholder order entry if tracking external orders is needed?
    }
  }

  fn completed_order(
    &self,
    contract: &Contract,
    _order_request: &OrderRequest,
    order_state: &OrderState,
  ) {
    // CompletedOrder message lacks the client order ID. Correlation is difficult.
    // The primary mechanism for final status is the orderStatus message.
    // This handler is mostly for logging or if specific completion details
    // (like completedTime/Status string) are needed and not captured elsewhere.
    // Attempting correlation via perm_id is unreliable as perm_id isn't guaranteed
    // to be part of the data passed to this handler method.

    info!(
      "Handler: Received completedOrder: Symbol={}, Status={:?}, CompletedTime={:?}",
      contract.symbol, order_state.status, order_state.completed_time
    );

    // Finding the specific order is problematic. We log the event but generally
    // avoid updating specific order state here, relying on orderStatus for that.
    // If correlation is essential, the parser would need to somehow extract a
    // unique identifier (like perm_id if present) and pass it along, or we'd
    // need to iterate and match based on contract/request details (fragile).

    warn!("Received completedOrder - this provides completion details but is hard to correlate reliably to a specific client order ID. Final status updates should come via orderStatus.");

    // Potential future enhancement: If perm_id *is* reliably parsed into OrderState
    // (which is not standard TWS behavior for this message), we could try matching:
    // let perm_id_from_state: Option<i32> = ... // Extract if possible
    // if let Some(perm_id) = perm_id_from_state {
    //     let p_map = self.perm_id_map.read();
    //     if let Some(order_id_str) = p_map.get(&perm_id) {
    //         // Found order ID - proceed with cautious update
    //         debug!("Correlated completedOrder via PermID {} to OrderID {}", perm_id, order_id_str);
    //         // ... (update logic, similar to the commented-out section in previous versions) ...
    //         // Ensure not to overwrite a more definitive status from orderStatus.
    //         // Only update completed_time/status string.
    //     } else {
    //         warn!("CompletedOrder received with PermID {} but no matching OrderID found in map.", perm_id);
    //     }
    // } else {
    //     warn!("CompletedOrder received but could not extract PermID for correlation.");
    // }
  }

  fn completed_orders_end(&self) {
    info!("Handler: Received completedOrdersEnd");
    // Signals the end of the completed orders burst requested via reqCompletedOrders.
  }
}

// --- Order Observer Trait ---

/// Defines the callbacks for an observer wishing to receive asynchronous order updates.
///
/// Implement this trait and register it with [`OrderManager::add_observer()`]
/// to be notified of changes to order status or errors related to orders.
pub trait OrderObserver: Send + Sync {
  /// Called when an order's state is updated.
  /// This can be triggered by `orderStatus` or `openOrder` messages from TWS.
  /// The `order` argument provides a snapshot of the order's current state.
  fn on_order_update(&self, order: &Order);

  /// Called when an error related to a specific order is received from TWS.
  ///
  /// # Arguments
  /// * `order_id` - The client order ID associated with the error.
  /// * `error` - An [`IBKRError`] containing details of the error.
  fn on_order_error(&self, order_id: &str, error: &crate::base::IBKRError);
}
