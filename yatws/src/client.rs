
//! The main entry point for interacting with the Interactive Brokers TWS API.
//!
//! The `IBKRClient` provides access to various "manager" modules, each responsible
//! for a specific area of the API (e.g., orders, account data, market data).
//!
//! # Interaction Patterns
//!
//! Most managers offer two primary ways to interact with the API:
//!
//! 1.  **Synchronous `get_*` methods:** These methods typically make a request to TWS
//!     and block until a response is received or a timeout occurs. They are convenient
//!     for one-off data retrieval.
//!     *Example:* `client.data_market().get_quote(...)`
//!
//! 2.  **Asynchronous `request_*` and `cancel_*` methods with Observers:** For streaming
//!     data or ongoing updates, you can use `request_*` methods. These methods usually
//!     return a request ID immediately. Data updates are then delivered asynchronously
//!     to "observers" that you register with the respective manager. `cancel_*` methods
//!     are used to stop these streams.
//!     *Example:* `client.data_market().request_market_data(...)` followed by registering
//!     an observer that implements `MarketDataObserver`.
//!
//! # Managers
//!
//! -   [`ClientManager`]: Handles general client-level operations like connection status and server time.
//! -   [`OrderManager`]: Manages order placement, cancellation, and status tracking.
//! -   [`AccountManager`]: Provides access to account summary, positions, and P&L.
//! -   [`DataRefManager`]: Fetches contract details and other reference data.
//! -   [`DataMarketManager`]: Handles market data requests (quotes, ticks, bars, depth).
//! -   [`DataNewsManager`]: Manages news provider information and article retrieval.
//! -   [`DataFundamentalsManager`]: Provides access to financial statement data and Wall Street Horizon (WSH) events.
//! -   [`FinancialAdvisorManager`]: Manages Financial Advisor configurations (groups, profiles, aliases).
//!
//! # Example: Creating a Client and Getting Server Time
//!
//! ```no_run
//! use yatws::{IBKRClient, IBKRError};
//! use std::time::Duration;
//!
//! fn main() -> Result<(), IBKRError> {
//!     // Create a live client (replace with your actual host, port, client_id)
//!     let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
//!
//!     // Access the ClientManager
//!     let client_operations = client.client();
//!
//!     // Request current server time (blocking call)
//!     match client_operations.request_current_time() {
//!         Ok(time) => println!("Server Time: {}", time),
//!         Err(e) => eprintln!("Error getting server time: {:?}", e),
//!     }
//!
//!     Ok(())
//! }
//! ```

use crate::order_manager::OrderManager;
use crate::account_manager::AccountManager;
use crate::data_ref_manager::DataRefManager;
use crate::data_market_manager::DataMarketManager;
use crate::data_news_manager::DataNewsManager;
use crate::data_fin_manager::DataFundamentalsManager;
use crate::financial_advisor_manager::FinancialAdvisorManager;
use crate::conn::{Connection, SocketConnection, MessageBroker};
use crate::conn_log::ConnectionLogger;
use crate::conn_mock::MockConnection;
use crate::base::IBKRError;
use log::{info, error, debug};
use std::sync::Arc;
use std::time::Duration;
use client_manager::ClientManager;
use crate::handler::MessageHandler;
use crate::rate_limiter::{RateLimiterConfig, RateLimiterStatus, RateLimiterManager};

/// The primary client for interacting with the Interactive Brokers TWS API.
///
/// It provides access to various specialized managers for different API functionalities.
/// See the [module-level documentation](index.html) for more details on interaction patterns.
pub struct IBKRClient {
  client_id: i32,
  rate_limiter_mgr: Arc<RateLimiterManager>,
  message_broker: Arc<MessageBroker>,
  client_mgr: Arc<ClientManager>,
  order_mgr: Arc<OrderManager>,
  account_mgr: Arc<AccountManager>,
  data_ref_mgr: Arc<DataRefManager>,
  data_market_mgr: Arc<DataMarketManager>,
  data_news_mgr: Arc<DataNewsManager>,
  data_fin_mgr: Arc<DataFundamentalsManager>,
  financial_advisor_mgr: Arc<FinancialAdvisorManager>,
}

impl IBKRClient {
  /// Creates a new `IBKRClient` for a live connection to TWS/Gateway.
  ///
  /// This method establishes a connection, performs initial handshakes, and
  /// waits for the `nextValidId` from the server before returning.
  ///
  /// # Arguments
  /// * `host` - The hostname or IP address of the TWS/Gateway.
  /// * `port` - The port number TWS/Gateway is listening on.
  /// * `client_id` - A unique client ID for this connection. Each client ID
  ///   represents a separate connection to TWS.
  /// * `log_config` - Optional configuration for logging interactions to a SQLite database.
  ///   If `Some((db_path, session_name))`, interactions will be logged.
  ///   `db_path` is the path to the SQLite file, and `session_name` is a label for this session.
  ///
  /// # Errors
  /// Returns `IBKRError` if the connection fails, handshake is unsuccessful,
  /// or if there's an issue initializing the logger.
  ///
  /// # Example
  /// ```no_run
  /// use yatws::IBKRClient;
  ///
  /// let client = IBKRClient::new("127.0.0.1", 4002, 101, None)
  ///     .expect("Failed to connect to TWS");
  /// println!("Connected with client ID: {}", client.client_id());
  /// ```
  pub fn new(host: &str, port: u16, client_id: i32, log_config: Option<(String, String)>) -> Result<Self, IBKRError> {
    let logger = if let Some((dbpath, session_name)) = log_config {
      Some(ConnectionLogger::new(dbpath, &session_name, host, port, client_id)?)
    } else { None };
    let conn = Box::new(SocketConnection::new(host, port, client_id, logger.clone())?);
    let server_version = conn.get_server_version();
    let message_broker = Arc::new(MessageBroker::new(conn, logger));
    let rate_config = RateLimiterConfig::default();
    let rate_limiter_mgr = Arc::new(RateLimiterManager::new(rate_config.clone()));
    message_broker.set_rate_limiter(Some(rate_limiter_mgr.get_message_limiter()));
    message_broker.set_rate_limit_timeout(rate_config.rate_limit_wait_timeout);
    let client_mgr = ClientManager::new(message_broker.clone());
    let (order_mgr, order_init) = OrderManager::create(message_broker.clone());
    let account_mgr = AccountManager::new(message_broker.clone(), /* account */None);
    let data_ref_mgr = DataRefManager::new(message_broker.clone());
    let data_market_mgr = DataMarketManager::new(
      message_broker.clone(),
      rate_limiter_mgr.get_market_data_limiter(),
      rate_limiter_mgr.get_historical_limiter()
    );
    let data_news_mgr = DataNewsManager::new(message_broker.clone(),
                                             rate_limiter_mgr.get_historical_limiter());
    let data_fin_mgr = DataFundamentalsManager::new(message_broker.clone());
    let financial_advisor_mgr = FinancialAdvisorManager::new(message_broker.clone());
    let msg_handler = MessageHandler::new(server_version, client_mgr.clone(),
                                          account_mgr.clone(), order_mgr.clone(),
                                          data_ref_mgr.clone(), data_market_mgr.clone(),
                                          data_news_mgr.clone(), data_fin_mgr.clone(),
                                          financial_advisor_mgr.clone());
    message_broker.set_message_handler(msg_handler);
    order_init()?;
    Ok(IBKRClient {
      client_id,
      rate_limiter_mgr,
      message_broker,
      client_mgr,
      order_mgr,
      account_mgr,
      data_ref_mgr,
      data_market_mgr,
      data_news_mgr,
      data_fin_mgr,
      financial_advisor_mgr,
    })
  }

  /// Creates a new `IBKRClient` for replaying a previously logged session from a database.
  ///
  /// This is useful for testing and debugging by replaying recorded TWS interactions
  /// without needing a live connection.
  ///
  /// # Arguments
  /// * `db_path` - Path to the SQLite database file containing the logged session.
  /// * `session_name` - The name of the session to replay from the database.
  ///
  /// # Errors
  /// Returns `IBKRError` if the database cannot be opened, the session is not found,
  /// or there's an issue initializing the replay.
  ///
  /// # Example
  /// ```no_run
  /// use yatws::IBKRClient;
  ///
  /// // Assuming "yatws_golden.db" contains a session named "time_test"
  /// let replay_client = IBKRClient::from_db("yatws_golden.db", "time_test")
  ///     .expect("Failed to create replay client");
  /// println!("Replay client created for session 'time_test'");
  /// ```
  pub fn from_db(db_path: &str, session_name: &str) -> Result<IBKRClient, IBKRError> {
    let conn = Box::new(MockConnection::new(db_path, session_name)?);
    let server_version = conn.get_server_version();
    let message_broker = Arc::new(MessageBroker::new(conn, None));
    let client_mgr = ClientManager::new(message_broker.clone());
    let rate_config = RateLimiterConfig::default();
    let rate_limiter_mgr = Arc::new(RateLimiterManager::new(rate_config.clone()));
    message_broker.set_rate_limiter(Some(rate_limiter_mgr.get_message_limiter()));
    message_broker.set_rate_limit_timeout(rate_config.rate_limit_wait_timeout);
    let (order_mgr, order_init) = OrderManager::create(message_broker.clone());
    let account_mgr = AccountManager::new(message_broker.clone(), /* account */None);
    let data_ref_mgr = DataRefManager::new(message_broker.clone());
    let data_market_mgr = DataMarketManager::new(
      message_broker.clone(),
      rate_limiter_mgr.get_market_data_limiter(),
      rate_limiter_mgr.get_historical_limiter()
    );
    let data_news_mgr = DataNewsManager::new(message_broker.clone(),
                                             rate_limiter_mgr.get_historical_limiter());
    let data_fin_mgr = DataFundamentalsManager::new(message_broker.clone());
    let financial_advisor_mgr = FinancialAdvisorManager::new(message_broker.clone());
    let msg_handler = MessageHandler::new(server_version, client_mgr.clone(),
                                          account_mgr.clone(), order_mgr.clone(),
                                          data_ref_mgr.clone(), data_market_mgr.clone(),
                                          data_news_mgr.clone(), data_fin_mgr.clone(),
                                          financial_advisor_mgr.clone());
    message_broker.set_message_handler(msg_handler);
    order_init()?;
    Ok(IBKRClient {
      client_id: 0,
      rate_limiter_mgr,
      message_broker,
      client_mgr,
      order_mgr,
      account_mgr,
      data_ref_mgr,
      data_market_mgr,
      data_news_mgr,
      data_fin_mgr,
      financial_advisor_mgr,
    })
  }

  /// Returns the client ID used for this connection.
  pub fn client_id(&self) -> i32 { self.client_id }

  /// Provides access to the [`ClientManager`](crate::client_manager::ClientManager) for general client operations.
  ///
  /// The [`ClientManager`](crate::client_manager::ClientManager) handles functions like requesting server time,
  /// checking connection status, and managing API verification messages.
  pub fn client(&self) -> Arc<ClientManager> {
    self.client_mgr.clone()
  }

  /// Provides access to the [`OrderManager`] for order-related operations.
  ///
  /// The `OrderManager` is used to place, modify, and cancel orders,
  /// as well as to query order statuses and executions.
  pub fn orders(&self) -> Arc<OrderManager> {
    self.order_mgr.clone()
  }

  /// Provides access to the [`AccountManager`] for account and portfolio data.
  ///
  /// The `AccountManager` allows subscribing to account updates, fetching
  /// account summaries, positions, and Profit & Loss (P&L) information.
  pub fn account(&self) -> Arc<AccountManager> {
    self.account_mgr.clone()
  }

  /// Provides access to the [`DataRefManager`] for reference data.
  ///
  /// The `DataRefManager` is used to request contract details, exchange information,
  /// and other static reference data.
  pub fn data_ref(&self) -> Arc<DataRefManager> {
    self.data_ref_mgr.clone()
  }

  /// Provides access to the [`DataMarketManager`] for market data.
  ///
  /// The `DataMarketManager` handles requests for real-time and historical
  /// market data, including quotes, ticks, bars, and market depth.
  pub fn data_market(&self) -> Arc<DataMarketManager> {
    self.data_market_mgr.clone()
  }

  /// Provides access to the [`DataNewsManager`] for news headlines and articles.
  ///
  /// The `DataNewsManager` allows fetching news providers, historical news,
  /// and specific news articles. It also supports streaming news bulletins.
  pub fn data_news(&self) -> Arc<DataNewsManager> {
    self.data_news_mgr.clone()
  }

  /// Provides access to the [`DataFundamentalsManager`] for financial data.
  ///
  /// The `DataFundamentalsManager` is used to request company fundamental data
  /// (e.g., financial statements, reports) and Wall Street Horizon (WSH) corporate event data.
  pub fn data_financials(&self) -> Arc<DataFundamentalsManager> {
    self.data_fin_mgr.clone()
  }

  /// Provides access to the [`FinancialAdvisorManager`] for FA configurations.
  ///
  /// The `FinancialAdvisorManager` allows requesting and replacing FA groups,
  /// profiles, and aliases.
  pub fn financial_advisor(&self) -> Arc<FinancialAdvisorManager> {
    self.financial_advisor_mgr.clone()
  }

  /// Configure rate limiting for API requests.
  ///
  /// This method configures the rate limiters to enforce Interactive Brokers' rate limits.
  /// By default, rate limiting is disabled. Use this method to enable it with
  /// custom settings, or `enable_rate_limiting()` to enable with default settings.
  ///
  /// # Arguments
  /// * `config` - The rate limiter configuration to apply.
  ///
  ///
  /// # Example
  /// ```no_run
  /// use yatws::{IBKRClient, RateLimiterConfig};
  /// use std::time::Duration;
  ///
  /// # fn main() -> Result<(), yatws::IBKRError> {
  /// let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  ///
  /// // Enable rate limiting with custom settings
  /// let mut config = RateLimiterConfig::default();
  /// config.enabled = true;
  /// config.max_messages_per_second = 40; // Be more conservative than default 50
  /// config.max_historical_requests = 30; // Be more conservative than default 50
  /// client.configure_rate_limiter(config)?;
  /// # Ok(())
  /// # }
  /// ```
  pub fn configure_rate_limiter(&self, config: RateLimiterConfig) -> Result<(), IBKRError> {
    let mgr = &self.rate_limiter_mgr;
    mgr.configure(config.clone());

    // Update message broker's rate limiter settings
    if config.enabled {
      self.message_broker.set_rate_limiter(Some(mgr.get_message_limiter()));
    } else {
      self.message_broker.set_rate_limiter(None);
    }

    self.message_broker.set_rate_limit_timeout(config.rate_limit_wait_timeout);

    Ok(())
  }

  /// Get the current rate limiter status.
  ///
  /// Returns a struct with information about the current usage of each rate limiter.
  ///
  /// # Returns
  /// * `Option<RateLimiterStatus>` - The current status, or `None` if rate limiting
  ///   is not configured.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::IBKRClient;
  /// # fn main() -> Result<(), yatws::IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// if let Some(status) = client.get_rate_limiter_status() {
  ///     println!("Rate limiting enabled: {}", status.enabled);
  ///     println!("Current message rate: {:.2} msgs/sec", status.current_message_rate);
  ///     println!("Active historical requests: {}/{}",
  ///             status.active_historical_requests, 50);
  /// } else {
  ///     println!("Rate limiting not configured");
  /// }
  /// # Ok(())
  /// # }
  /// ```
  pub fn get_rate_limiter_status(&self) -> Option<RateLimiterStatus> {
    Some(self.rate_limiter_mgr.get_status())
  }

  /// Enable rate limiting with default configuration.
  ///
  /// This is a convenience method that enables rate limiting with the
  /// following default settings:
  /// * 50 messages per second
  /// * 50 simultaneous historical data requests
  /// * 100 simultaneous market data lines
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::IBKRClient;
  /// # fn main() -> Result<(), yatws::IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// // Enable rate limiting with default settings
  /// client.enable_rate_limiting()?;
  /// # Ok(())
  /// # }
  /// ```
  pub fn enable_rate_limiting(&self) -> Result<(), IBKRError> {
    let mgr = &self.rate_limiter_mgr;
    let mut config = RateLimiterConfig::default();
    config.enabled = true;
    mgr.configure(config.clone());

    // Update message broker settings
    self.message_broker.set_rate_limiter(Some(mgr.get_message_limiter()));
    self.message_broker.set_rate_limit_timeout(config.rate_limit_wait_timeout);

    Ok(())
  }

  /// Disable rate limiting.
  ///
  /// This method disables all rate limiting, allowing messages and requests
  /// to be sent without limitation. Use with caution, as this may cause IBKR
  /// to throttle or disconnect your application if API usage is excessive.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::IBKRClient;
  /// # fn main() -> Result<(), yatws::IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// // Disable rate limiting
  /// client.disable_rate_limiting()?;
  /// # Ok(())
  /// # }
  /// ```
  pub fn disable_rate_limiting(&self) -> Result<(), IBKRError> {
    let mgr = &self.rate_limiter_mgr;
    let mut config = RateLimiterConfig::default();
    config.enabled = false;
    mgr.configure(config);

    // Remove rate limiter from message broker
    self.message_broker.set_rate_limiter(None);

    Ok(())
  }

  /// Clean up stale rate limiter requests.
  ///
  /// This method is used to clean up any stale requests that never received
  /// a completion message. This can happen if a request fails unexpectedly
  /// or if there's a bug in the handler implementation.
  ///
  /// # Arguments
  /// * `older_than` - The minimum age for a request to be considered stale.
  ///
  /// # Returns
  /// * `Result<(u32, u32), IBKRError>` - The number of historical and market data
  ///   requests that were cleaned up, or an error if the rate limiter manager
  ///   is not initialized.
  ///
  /// # Example
  /// ```no_run
  /// # use yatws::IBKRClient;
  /// # use std::time::Duration;
  /// # fn main() -> Result<(), yatws::IBKRError> {
  /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
  /// // Clean up requests older than 5 minutes
  /// let (hist_cleaned, mkt_cleaned) = client.cleanup_stale_rate_limiter_requests(
  ///     Duration::from_secs(5 * 60)
  /// )?;
  /// println!("Cleaned up {} historical and {} market data requests", hist_cleaned, mkt_cleaned);
  /// # Ok(())
  /// # }
  /// ```
  pub fn cleanup_stale_rate_limiter_requests(&self, older_than: Duration) -> Result<(u32, u32), IBKRError> {
    let mgr = &self.rate_limiter_mgr;
    Ok(mgr.cleanup_stale_requests(older_than))
  }

  /// Explicitly disconnect from TWS
  ///
  /// This is called automatically when the client is dropped, but you can
  /// call it manually for cleaner shutdown.
  pub fn disconnect(&self) -> Result<(), IBKRError> {
    info!("IBKRClient disconnect requested (client_id: {})", self.client_id);

    self.account().cleanup_requests()
      .unwrap_or_else(|e| error!("Failed to clean up account manager: {}.", e));

    self.data_financials().cleanup_requests()
      .unwrap_or_else(|e| error!("Failed to clean up data financials manager: {}.", e));

    self.data_market().cleanup_requests()
      .unwrap_or_else(|e| error!("Failed to clean up data market manager: {}.", e));

    self.data_news().cleanup_requests()
      .unwrap_or_else(|e| error!("Failed to clean up data news manager: {}.", e));

    self.data_ref().cleanup_requests()
      .unwrap_or_else(|e| error!("Failed to clean up data reference manager: {}.", e));

    self.financial_advisor().cleanup_requests()
      .unwrap_or_else(|e| error!("Failed to clean up financial advisor manager: {}.", e));

    self.orders().cleanup_requests()
      .unwrap_or_else(|e| error!("Failed to clean up order manager: {}.", e));

    // Then disconnect the underlying connection
    self.message_broker.disconnect()
  }
}

impl Drop for IBKRClient {
  fn drop(&mut self) {
    info!("Dropping IBKRClient (client_id: {}), ensuring connection is closed...", self.client_id);

    // Call our explicit disconnect method
    match self.disconnect() {
      Ok(()) => {
        info!("IBKRClient connection closed successfully during drop");
      },
      Err(e) => {
        // Log the error but don't panic during drop
        error!("Error closing connection during IBKRClient drop: {:?}", e);
      }
    }

    debug!("IBKRClient drop completed for client_id: {}", self.client_id);
  }
}

/// Manages client-level interactions and state with TWS.
///
/// This includes handling connection status, server time requests, API verification messages,
/// and general error reporting from TWS.
///
/// Accessed via [`IBKRClient::client()`](crate::IBKRClient::client()).
pub mod client_manager {
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
    /// Creates a new `ClientManager` instance wrapped in an `Arc`.
    ///
    /// This is typically called internally when an `IBKRClient` is created.
    pub(crate) fn new(message_broker: Arc<MessageBroker>) -> Arc<Self> {
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

    /// Checks if the client believes it is currently connected to TWS/Gateway.
    ///
    /// Note: This status is based on messages received from TWS and internal tracking.
    /// It might lag slightly behind the actual socket state in some edge cases.
    pub fn is_connected(&self) -> bool {
      self.connected.load(Ordering::Relaxed)
    }

    /// Retrieves the last error message received from TWS or the client library.
    ///
    /// Returns an `Option` containing a tuple: `(request_id, error_code, error_message)`.
    /// - `request_id`: The ID of the request associated with the error, or -1 if it's a general error.
    /// - `error_code`: The TWS error code.
    /// - `error_message`: A descriptive message for the error.
    ///
    /// Returns `None` if no error has been recorded since the last check or connection.
    pub fn get_last_error(&self) -> Option<(i32, i32, String)> {
      self.last_error.lock().clone() // Clone the Option<(...)>
    }

    /// Gets the last known server time, as received from a `CURRENT_TIME` message from TWS.
    ///
    /// This value is updated when `request_current_time` is called or if TWS sends
    /// time updates spontaneously (though the latter is not typical).
    /// Returns `None` if server time has not yet been received.
    pub fn get_server_time(&self) -> Option<DateTime<Utc>> {
      self.last_server_time_unix
        .lock()
        .and_then(|ts_unix| Utc.timestamp_opt(ts_unix, 0).single())
    }

    /// Requests the current server time from TWS and blocks until the response is received or a timeout occurs.
    ///
    /// On success, updates the internal server time (accessible via `get_server_time`) and returns the `DateTime<Utc>`.
    ///
    /// # Errors
    /// Returns `IBKRError::Timeout` if the server does not respond within the predefined timeout.
    /// Returns other `IBKRError` variants for connection or encoding issues.
    ///
    /// # Example
    /// ```no_run
    /// # use yatws::{IBKRClient, IBKRError};
    /// # fn main() -> Result<(), IBKRError> {
    /// # let client = IBKRClient::new("127.0.0.1", 4002, 101, None)?;
    /// let server_time = client.client().request_current_time()?;
    /// println!("Current TWS Server Time: {}", server_time);
    /// # Ok(())
    /// # }
    /// ```
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

    /// Sets the connected status.
    /// This is called internally by the connection logic or when specific error messages
    /// indicate a disconnection.
    pub(crate) fn set_connected_status(&self, status: bool) {
      let old_status = self.connected.swap(status, Ordering::Relaxed);
      if old_status != status {
        info!("ClientManager connection status changed: {} -> {}", old_status, status);
      }
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
      let was_connected = self.connected.swap(false, Ordering::Relaxed);
      if was_connected {
        error!("ClientManager notified: Connection Closed.");
        // TODO: Notify observers
        // self.notify_observers_connection_closed();
      } else {
        debug!("ClientManager: connection_closed() called but already disconnected");
      }
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
