use parking_lot::RwLock;
use std::sync::Arc;
use std::collections::HashMap;
use crate::conn::MessageBroker;

pub struct DataManager {
  message_broker: Arc<MessageBroker>,
}

impl DataManager {
  pub fn new(message_broker: Arc<MessageBroker>) -> Self {
    DataManager {
      message_broker,
    }
  }
}
