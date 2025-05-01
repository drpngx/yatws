use crate::order_manager::OrderManager;
use crate::account_manager::AccountManager;
use crate::data_ref_manager::DataRefManager;
use crate::data_market_manager::DataMarketManager;
use crate::data_news_manager::DataNewsManager;
use crate::data_fin_manager::DataFundamentalsManager;
use crate::conn::{Connection, SocketConnection, MessageBroker};
use crate::conn_log::ConnectionLogger;
use crate::conn_mock::MockConnection;
use crate::base::IBKRError;
use std::sync::Arc;
use mgr::ClientManager;
use crate::handler::MessageHandler;

pub struct IBKRClient {
  client_id: i32,
  client_mgr: Arc<ClientManager>,
  order_mgr: Arc<OrderManager>,
  account_mgr: Arc<AccountManager>,
  data_ref_mgr: Arc<DataRefManager>,
  data_market_mgr: Arc<DataMarketManager>,
  data_news_mgr: Arc<DataNewsManager>,
  data_fin_mgr: Arc<DataFundamentalsManager>,
}

impl IBKRClient {
  pub fn new(host: &str, port: u16, client_id: i32, log_config: Option<(String, String)>) -> Result<Self, IBKRError> {
    let logger = if let Some((dbpath, session_name)) = log_config {
      Some(ConnectionLogger::new(dbpath, &session_name, host, port, client_id)?)
    } else { None };
    let conn = Box::new(SocketConnection::new(host, port, client_id, logger.clone())?);
    let server_version = conn.get_server_version();
    let message_broker = Arc::new(MessageBroker::new(conn, logger));
    let client_mgr = ClientManager::new(message_broker.clone());
    let (order_mgr, order_init) = OrderManager::create(message_broker.clone());
    let account_mgr = AccountManager::new(message_broker.clone(), /* account */None);
    let data_ref_mgr = DataRefManager::new(message_broker.clone());
    let data_market_mgr = DataMarketManager::new(message_broker.clone());
    let data_news_mgr = DataNewsManager::new(message_broker.clone());
    let data_fin_mgr = DataFundamentalsManager::new(message_broker.clone());
    let msg_handler = MessageHandler::new(server_version, client_mgr.clone(),
                                          account_mgr.clone(), order_mgr.clone(),
                                          data_ref_mgr.clone(), data_market_mgr.clone(),
                                          data_news_mgr.clone(), data_fin_mgr.clone());
    message_broker.set_message_handler(msg_handler);
    order_init()?;
    Ok(IBKRClient {
      client_id,
      client_mgr,
      order_mgr,
      account_mgr,
      data_ref_mgr,
      data_market_mgr,
      data_news_mgr,
      data_fin_mgr,
    })
  }

  /// Create a new client using a stored interaction.
  pub fn from_db(db_path: &str, session_name: &str) -> Result<IBKRClient, IBKRError> {
    let conn = Box::new(MockConnection::new(db_path, session_name)?);
    let server_version = conn.get_server_version();
    let message_broker = Arc::new(MessageBroker::new(conn, None));
    let client_mgr = ClientManager::new(message_broker.clone());
    let (order_mgr, order_init) = OrderManager::create(message_broker.clone());
    let account_mgr = AccountManager::new(message_broker.clone(), /* account */None);
    let data_ref_mgr = DataRefManager::new(message_broker.clone());
    let data_market_mgr = DataMarketManager::new(message_broker.clone());
    let data_news_mgr = DataNewsManager::new(message_broker.clone());
    let data_fin_mgr = DataFundamentalsManager::new(message_broker.clone());
    let msg_handler = MessageHandler::new(server_version, client_mgr.clone(),
                                          account_mgr.clone(), order_mgr.clone(),
                                          data_ref_mgr.clone(), data_market_mgr.clone(),
                                          data_news_mgr.clone(), data_fin_mgr.clone());
    message_broker.set_message_handler(msg_handler);
    order_init()?;
    Ok(IBKRClient {
      client_id: 0,
      client_mgr,
      order_mgr,
      account_mgr,
      data_ref_mgr,
      data_market_mgr,
      data_news_mgr,
      data_fin_mgr,
    })
  }

  pub fn client_id(&self) -> i32 { self.client_id }

  pub fn client(&self) -> Arc<ClientManager> {
    self.client_mgr.clone()
  }

  pub fn orders(&self) -> Arc<OrderManager> {
    self.order_mgr.clone()
  }

  pub fn account(&self) -> Arc<AccountManager> {
    self.account_mgr.clone()
  }

  pub fn data_ref(&self) -> Arc<DataRefManager> {
    self.data_ref_mgr.clone()
  }

  pub fn data_market(&self) -> Arc<DataMarketManager> {
    self.data_market_mgr.clone()
  }

  pub fn data_news(&self) -> Arc<DataNewsManager> {
    self.data_news_mgr.clone()
  }

  pub fn data_financials(&self) -> Arc<DataFundamentalsManager> {
    self.data_fin_mgr.clone()
  }
}

mod mgr {
  use crate::base::IBKRError;
  use crate::handler::ClientHandler; // Import the trait we will implement
  use chrono::{DateTime, TimeZone, Utc};
  use log::{debug, error, info, warn};
  use parking_lot::{Mutex, Condvar};
  use crate::protocol_encoder::Encoder;
  use std::time::Duration;
  use std::sync::atomic::{AtomicBool, Ordering};
  use std::sync::Arc;
  use crate::conn::MessageBroker;

  #[derive(Debug, Default)]
  struct TimeRequestState {
    waiting_for_time: bool,
  }

  pub struct ClientManager {
    message_broker: Arc<MessageBroker>,
    // Use AtomicBool for simple connected status - cheaper than Mutex for reads
    connected: Arc<AtomicBool>,
    // Store last error details
    last_error: Arc<Mutex<Option<(i32, i32, String)>>>, // (id, code, msg)
    // Store last known server time (as Unix timestamp)
    last_server_time_unix: Arc<Mutex<Option<i64>>>,
    // Mutex and Condvar for blocking time request
    time_request_state: Mutex<TimeRequestState>,
    time_request_cond: Condvar,
  }

  impl ClientManager {
    /// Creates a new ClientManager instance wrapped in an Arc.
    pub fn new(message_broker: Arc<MessageBroker>) -> Arc<Self> {
      info!("Creating new ClientManager");
      Arc::new(ClientManager {
        message_broker, // Store the broker
        connected: Arc::new(AtomicBool::new(false)),
        last_error: Arc::new(Mutex::new(None)),
        last_server_time_unix: Arc::new(Mutex::new(None)),
        time_request_state: Mutex::new(TimeRequestState::default()),
        time_request_cond: Condvar::new(),
      })
    }

    // --- Public Accessor Methods ---

    /// Checks if the client believes it's connected.
    /// Note: This might lag slightly behind the actual socket state.
    pub fn is_connected(&self) -> bool {
      self.connected.load(Ordering::Relaxed)
    }

    /// Gets the last error received from TWS.
    /// Returns tuple (request_id, error_code, error_message).
    /// Request ID is -1 if not associated with a specific request.
    pub fn get_last_error(&self) -> Option<(i32, i32, String)> {
      self.last_error.lock().clone() // Clone the Option<(...)>
    }

    /// Gets the last known server time from TWS CURRENT_TIME message.
    pub fn get_server_time(&self) -> Option<DateTime<Utc>> {
      self.last_server_time_unix
        .lock()
        .and_then(|ts_unix| Utc.timestamp_opt(ts_unix, 0).single())
    }

    /// Requests the current server time from TWS and blocks until received or timeout.
    /// Updates the internal server time on success.
    pub fn request_current_time(&self) -> Result<DateTime<Utc>, IBKRError> {
      info!("Requesting current server time...");

      let server_version = self.message_broker.get_server_version()?;
      let encoder = Encoder::new(server_version);
      let request_msg = encoder.encode_request_current_time()?;

      let wait_timeout = Duration::from_secs(5); // Shorter timeout for simple time request
      let start_time = std::time::Instant::now();

      { // Lock state to set waiting flag
        let mut req_state = self.time_request_state.lock();
        if req_state.waiting_for_time {
          return Err(IBKRError::AlreadyRunning("RequestCurrentTime already in progress".to_string()));
        }
        req_state.waiting_for_time = true;
        // Clear previous time before request? Optional.
        // *self.last_server_time_unix.lock() = None;
      } // Release lock before send

      // Send the request
      self.message_broker.send_message(&request_msg)?;
      info!("RequestCurrentTime message sent.");

      // Wait for response
      { // Re-lock state for waiting
        let mut req_state = self.time_request_state.lock();

        while req_state.waiting_for_time && start_time.elapsed() < wait_timeout {
          let remaining = wait_timeout.checked_sub(start_time.elapsed()).unwrap_or(Duration::from_millis(1));
          let wait_result = self.time_request_cond.wait_for(&mut req_state, remaining);

          if wait_result.timed_out() {
            // Check condition again after timeout
            if req_state.waiting_for_time {
              warn!("RequestCurrentTime timed out waiting for response.");
              req_state.waiting_for_time = false; // Reset flag on timeout
              return Err(IBKRError::Timeout("Request for current time timed out".to_string()));
            } else {
              // Condition became false just before timeout, break and proceed
              debug!("RequestCurrentTime timed out but response received concurrently.");
              break;
            }
          }
          // If woken, the loop condition (req_state.waiting_for_time) will be checked
          debug!("RequestCurrentTime wait notified. Still waiting? {}", req_state.waiting_for_time);
        } // End while loop

        // Check final state after loop
        if req_state.waiting_for_time {
          // Should only happen if loop exited due to timeout check consistency issue
          warn!("RequestCurrentTime loop finished but flag not cleared.");
          req_state.waiting_for_time = false; // Reset flag
          return Err(IBKRError::Timeout("Request for current time timed out (consistency)".to_string()));
        }
        // If loop exited because waiting_for_time became false, we have the result
      } // Lock released

      info!("RequestCurrentTime response received.");
      // Fetch the stored time
      self.get_server_time().ok_or_else(|| {
        IBKRError::InternalError("Server time response received but value not found in state".to_string())
      })
    }

    // --- Internal Methods (used by Handler implementation) ---

    /// Sets the connected status. Called internally during connect/disconnect phases.
    pub(crate) fn set_connected_status(&self, status: bool) {
      self.connected.store(status, Ordering::Relaxed);
      info!("ClientManager connection status set to: {}", status);
    }
  }

use crate::protocol_decoder::ClientErrorCode; // Added import

  impl ClientHandler for ClientManager {
    /// Handles error messages from TWS or the client library.
    /// `id` can be a request ID, order ID, or -1 for general errors.
    fn handle_error(&self, id: i32, code: ClientErrorCode, msg: &str) {
      let error_code_int = code as i32; // Get integer code for logging/matching

      // Log differently based on severity (codes >= 2000 are usually info/warnings)
      if error_code_int >= 2000 && error_code_int < 3000 {
        info!("TWS Info/Warning (ID: {}, Code: {:?}={}): {}", id, code, error_code_int, msg);
      } else {
        error!("TWS Error (ID: {}, Code: {:?}={}): {}", id, code, error_code_int, msg);
      }

      // Store the last error
      let mut last_error_guard = self.last_error.lock();
      *last_error_guard = Some((id, error_code_int, msg.to_string()));

      // TODO: Add observer notification if observers are implemented
      // self.notify_observers_error(id, code, msg);

      // Specific error codes might indicate disconnection
      match code { // Match on the enum now
        // See https://interactivebrokers.github.io/tws-api/message_codes.html
        // Codes indicating connectivity issues or fatal errors.
        ClientErrorCode::AlreadyConnected |
        ClientErrorCode::ConnectFail |
        ClientErrorCode::UpdateTws |
        ClientErrorCode::NotConnected |
        ClientErrorCode::BadLength |
        ClientErrorCode::FailSend |
        // Add TWS specific codes if needed, e.g., 1100, 1101, 1102, 1300, 2109, 2110
        // ClientErrorCode::ConnectivityBetweenTwsAndIb | ...
         _ => {
            // Check integer code for TWS specific disconnects not in enum yet
            match error_code_int {
                1100 | 1101 | 1102 | 1300 | 2109 | 2110 => {
                    warn!("Error code {} ({:?}) indicates potential disconnection.", error_code_int, code);
                    self.set_connected_status(false);
                }
                _ => {} // Other errors don't automatically set disconnected status
            }
         }
      }
    }

    /// Handles notification that the connection was closed (e.g., detected by reader thread).
    fn connection_closed(&self) {
      error!("ClientManager notified: Connection Closed.");
      self.set_connected_status(false);
      // Clear last error maybe? Or keep it for diagnostics? Keep for now.
      // TODO: Notify observers
      // self.notify_observers_connection_closed();
    }

    /// Handles the CURRENT_TIME message.
    fn current_time(&self, time_unix: i64) {
      // Store the received time
      *self.last_server_time_unix.lock() = Some(time_unix);

      let dt = Utc.timestamp_opt(time_unix, 0).single();
      debug!("Handler: Received Current Time: {} ({:?})", time_unix, dt);

      // --- Signal the waiting thread ---
      { // Lock state briefly to update flag and notify
        let mut req_state = self.time_request_state.lock();
        if req_state.waiting_for_time {
          req_state.waiting_for_time = false;
          debug!("Signaling RequestCurrentTime waiter.");
          self.time_request_cond.notify_all(); // Notify potentially waiting thread
        }
      }
    }

    // --- Implement other ClientHandler methods (can start as stubs) ---

    fn verify_message_api(&self, _api_data: &str) {
      info!("ClientManager: Received VerifyMessageAPI (processing not implemented)");
      // TODO: Parse api_data if needed for verification logic
    }

    fn verify_completed(&self, is_successful: bool, error_text: &str) {
      info!("ClientManager: Received VerifyCompleted: Success={}, Error='{}'", is_successful, error_text);
      // let mut status = self.verify_status.lock();
      // *status = Some((is_successful, error_text.to_string()));
      // TODO: Notify observers
    }

    fn verify_and_auth_message_api(&self, _api_data: &str, _xyz_challenge: &str) {
      info!("ClientManager: Received VerifyAndAuthMessageAPI (processing not implemented)");
      // TODO: Implement challenge response logic if needed
    }

    fn verify_and_auth_completed(&self, is_successful: bool, error_text: &str) {
      info!("ClientManager: Received VerifyAndAuthCompleted: Success={}, Error='{}'", is_successful, error_text);
      // let mut status = self.auth_status.lock();i-
      // *status = Some((is_successful, error_text.to_string()));
      // TODO: Notify observers
    }

    fn display_group_list(&self, req_id: i32, groups: &str) {
      debug!("ClientManager: Received DisplayGroupList: ReqID={}, Groups='{}'", req_id, groups);
      // TODO: Store or process group list if needed
    }

    fn display_group_updated(&self, req_id: i32, contract_info: &str) {
      debug!("ClientManager: Received DisplayGroupUpdated: ReqID={}, ContractInfo='{}'", req_id, contract_info);
      // TODO: Store or process group update if needed
    }

    fn head_timestamp(&self, req_id: i32, timestamp_str: &str) {
      debug!("ClientManager: Received HeadTimestamp: ReqID={}, Timestamp='{}'", req_id, timestamp_str);
      // TODO: Parse and potentially store this timestamp associated with req_id
    }

    fn user_info(&self, _req_id: i32, white_branding_id: &str) {
      // Note: req_id is always 0 for this message from TWS
      info!("ClientManager: Received UserInfo: WhiteBrandingID='{}'", white_branding_id);
      // TODO: Store white branding ID if needed
    }
  }
}
