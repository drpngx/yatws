use crate::order_manager::OrderManager;
use crate::data_manager::DataManager;
use crate::account_manager::AccountManager;
use crate::conn::{Connection, SocketConnection, MessageBroker};
use crate::base::IBKRError;
use parking_lot::RwLock;
use std::sync::Arc;

pub struct IBKRClient {
  order_mgr: Arc<OrderManager>,
  account_mgr: Arc<AccountManager>,
  data_mgr: Arc<DataManager>,
}

impl IBKRClient {
  pub fn new(host: &str, port: u16, client_id: i32) -> Result<Self, IBKRError> {
    let conn = Box::new(SocketConnection::new(host, port, client_id)?);
    let message_broker = Arc::new(MessageBroker::new(conn));
    let order_mgr = Arc::new(OrderManager::new(message_broker.clone()));
    let account_mgr = AccountManager::new(message_broker.clone(), /* account */None);
    let data_mgr = Arc::new(DataManager::new(message_broker.clone()));

    Ok(IBKRClient {
      order_mgr,
      account_mgr,
      data_mgr,
    })
  }

  pub fn orders(&self) -> Arc<OrderManager> {
    self.order_mgr.clone()
  }

  pub fn account(&self) -> Arc<AccountManager> {
    self.account_mgr.clone()
  }

  pub fn data(&self) -> Arc<DataManager> {
    self.data_mgr.clone()
  }
}
