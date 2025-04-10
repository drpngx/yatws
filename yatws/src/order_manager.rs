use parking_lot::RwLock;
use crate::order::OrderObserver;
use std::sync::Arc;
use std::collections::HashMap;
use crate::order::Order;
use crate::conn::MessageBroker;

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
  // pub fn place_market_order(&self, symbol: &str, quantity: f64, side: OrderSide) -> Result<String, IBKRError>;
  // pub fn place_limit_order(&self, symbol: &str, quantity: f64, price: f64, side: OrderSide) -> Result<String, IBKRError>;
  // pub fn place_stop_order(&self, symbol: &str, quantity: f64, stop_price: f64, side: OrderSide) -> Result<String, IBKRError>;
  // pub fn place_custom_order(&self, contract: Contract, order: OrderRequest) -> Result<String, IBKRError>;

  // Order management methods
  // pub fn cancel_order(&self, order_id: &str) -> Result<bool, IBKRError>;
  // pub fn modify_order(&self, order_id: &str, updates: OrderUpdates) -> Result<Arc<RwLock<Order>>, IBKRError>;
  // pub fn get_order(&self, order_id: &str) -> Option<Order>;
  // pub fn get_all_orders(&self) -> Vec<Order>;
  // pub fn get_open_orders(&self) -> Vec<Order>;
}
