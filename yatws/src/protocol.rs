// yatws/src/protocol.rs
// Protocol module for the TWS API

use crate::protocol_encoder::Encoder;
use crate::protocol_decoder::Decoder;

use crate::base::IBKRError;
use crate::contract::Contract;
use crate::order::Order;
use chrono::{DateTime, Utc};
use log::info;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// TWS connection manager
pub struct Connection {
  host: String,
  port: u16,
  client_id: i32,
  server_version: i32,
  connected: bool,
  stream: Option<TcpStream>,
  encoder: Option<Encoder>,
  next_request_id: i32,
}

impl Connection {
  /// Create a new TWS connection
  pub fn new(host: &str, port: u16, client_id: i32) -> Self {
    Self {
      host: host.to_string(),
      port,
      client_id,
      server_version: 0,
      connected: false,
      stream: None,
      encoder: None,
      next_request_id: 0,
    }
  }

  /// Connect to TWS
  pub fn connect(&mut self) -> Result<(), IBKRError> {
    if self.connected {
      return Err(IBKRError::AlreadyConnected);
    }

    info!("Connecting to TWS at {}:{}", self.host, self.port);

    // Connect to the server
    let addr = format!("{}:{}", self.host, self.port);
    let stream = TcpStream::connect(addr)
      .map_err(|e| IBKRError::ConnectionFailed(e.to_string()))?;

    // Set timeouts
    stream.set_read_timeout(Some(Duration::from_secs(5)))
      .map_err(|e| IBKRError::ConnectionFailed(e.to_string()))?;

    stream.set_write_timeout(Some(Duration::from_secs(5)))
      .map_err(|e| IBKRError::ConnectionFailed(e.to_string()))?;

    // Make a copy for encoder
    let encoder_stream = stream.try_clone()
      .map_err(|e| IBKRError::ConnectionFailed(e.to_string()))?;

    self.stream = Some(stream);

    // Create encoder with default server version (will be updated after handshake)
    let mut encoder = Encoder::new(encoder_stream, 0);

    // Send initial handshake
    encoder.encode_client_version()?;
    encoder.flush()?;

    // Now read the server version
    let mut buffer = [0u8; 8192];
    let n = self.stream.as_mut().unwrap().read(&mut buffer)
      .map_err(|e| IBKRError::SocketError(e.to_string()))?;

    if n == 0 {
      return Err(IBKRError::ConnectionFailed("Server closed connection".to_string()));
    }

    // Parse server version from the response
    let mut version_str = String::new();
    for i in 0..n {
      if buffer[i] == 0 {
        break;
      }
      version_str.push(buffer[i] as char);
    }

    self.server_version = version_str.parse::<i32>()
      .map_err(|_| IBKRError::ParseError("Invalid server version".to_string()))?;

    // Create encoder with actual server version
    self.encoder = Some(Encoder::new(
      self.stream.as_mut().unwrap().try_clone().unwrap(),
      self.server_version,
    ));

    // Send client ID
    self.send_client_id()?;

    // Request next valid ID
    self.encoder.as_mut().unwrap().encode_request_ids()?;
    self.encoder.as_mut().unwrap().flush()?;

    self.connected = true;
    info!("Connected to TWS (server version: {})", self.server_version);

    Ok(())
  }

  /// Disconnect from TWS
  pub fn disconnect(&mut self) -> Result<(), IBKRError> {
    if !self.connected {
      return Ok(());
    }

    info!("Disconnecting from TWS");

    self.encoder = None;
    self.stream = None;
    self.connected = false;

    info!("Disconnected from TWS");

    Ok(())
  }

  /// Send client ID to TWS
  fn send_client_id(&mut self) -> Result<(), IBKRError> {
    if let Some(encoder) = &mut self.encoder {
      // In a real implementation, this would follow the protocol for sending client ID
      // For now, we'll just log it
      info!("Sending client ID: {}", self.client_id);
    }

    Ok(())
  }

  /// Generate a new request ID
  pub fn get_request_id(&mut self) -> i32 {
    let id = self.next_request_id;
    self.next_request_id += 1;
    id
  }

  /// Check if connected to TWS
  pub fn is_connected(&self) -> bool {
    self.connected
  }

  /// Get server version
  pub fn get_server_version(&self) -> i32 {
    self.server_version
  }

  /// Request market data for a contract
  pub fn request_market_data(&mut self, contract: &Contract, snapshot: bool) -> Result<i32, IBKRError> {
    if !self.connected {
      return Err(IBKRError::NotConnected);
    }

    let req_id = self.get_request_id();

    if let Some(encoder) = &mut self.encoder {
      encoder.encode_request_market_data(req_id, contract, snapshot)?;
      encoder.flush()?;
    } else {
      return Err(IBKRError::NotConnected);
    }

    info!("Requested market data for {} (req_id: {})", contract.symbol, req_id);

    Ok(req_id)
  }

  /// Cancel market data request
  pub fn cancel_market_data(&mut self, req_id: i32) -> Result<(), IBKRError> {
    if !self.connected {
      return Err(IBKRError::NotConnected);
    }

    if let Some(encoder) = &mut self.encoder {
      encoder.encode_cancel_market_data(req_id)?;
      encoder.flush()?;
    } else {
      return Err(IBKRError::NotConnected);
    }

    info!("Cancelled market data request (req_id: {})", req_id);

    Ok(())
  }

  /// Request contract details
  pub fn request_contract_details(&mut self, contract: &Contract) -> Result<i32, IBKRError> {
    if !self.connected {
      return Err(IBKRError::NotConnected);
    }

    let req_id = self.get_request_id();

    if let Some(encoder) = &mut self.encoder {
      encoder.encode_request_contract_data(req_id, contract)?;
      encoder.flush()?;
    } else {
      return Err(IBKRError::NotConnected);
    }

    info!("Requested contract details for {} (req_id: {})", contract.symbol, req_id);

    Ok(req_id)
  }

  /// Place an order
  pub fn place_order(&mut self, order: &Order) -> Result<String, IBKRError> {
    if !self.connected {
      return Err(IBKRError::NotConnected);
    }

    let order_id = order.id.clone();
    let order_id_int = order_id.parse::<i32>().unwrap_or_else(|_| self.get_request_id());

    if let Some(encoder) = &mut self.encoder {
      encoder.encode_place_order(order_id_int, &order.contract, order)?;
      encoder.flush()?;
    } else {
      return Err(IBKRError::NotConnected);
    }

    info!("Placed order {} for {}: {} {}",
          order_id, order.contract.symbol, order.request.side, order.request.quantity);

    Ok(order_id)
  }

  /// Cancel an order
  pub fn cancel_order(&mut self, order_id: &str) -> Result<(), IBKRError> {
    if !self.connected {
      return Err(IBKRError::NotConnected);
    }

    let order_id_int = order_id.parse::<i32>()
      .map_err(|_| IBKRError::InvalidParameter(format!("Invalid order ID: {}", order_id)))?;

    if let Some(encoder) = &mut self.encoder {
      encoder.encode_cancel_order(order_id_int)?;
      encoder.flush()?;
    } else {
      return Err(IBKRError::NotConnected);
    }

    info!("Cancelled order {}", order_id);

    Ok(())
  }

  /// Request all open orders
  pub fn request_all_open_orders(&mut self) -> Result<(), IBKRError> {
    if !self.connected {
      return Err(IBKRError::NotConnected);
    }

    if let Some(encoder) = &mut self.encoder {
      encoder.encode_request_all_open_orders()?;
      encoder.flush()?;
    } else {
      return Err(IBKRError::NotConnected);
    }

    info!("Requested all open orders");

    Ok(())
  }

  /// Request positions
  pub fn request_positions(&mut self) -> Result<(), IBKRError> {
    if !self.connected {
      return Err(IBKRError::NotConnected);
    }

    if let Some(encoder) = &mut self.encoder {
      encoder.encode_request_positions()?;
      encoder.flush()?;
    } else {
      return Err(IBKRError::NotConnected);
    }

    info!("Requested positions");

    Ok(())
  }

  /// Request account data
  pub fn request_account_data(&mut self, subscribe: bool, account_code: &str) -> Result<(), IBKRError> {
    if !self.connected {
      return Err(IBKRError::NotConnected);
    }

    if let Some(encoder) = &mut self.encoder {
      encoder.encode_request_account_data(subscribe, account_code)?;
      encoder.flush()?;
    } else {
      return Err(IBKRError::NotConnected);
    }

    if subscribe {
      info!("Subscribed to account data for account {}", account_code);
    } else {
      info!("Unsubscribed from account data for account {}", account_code);
    }

    Ok(())
  }

  /// Request historical data
  pub fn request_historical_data(
    &mut self,
    contract: &Contract,
    end_date_time: DateTime<Utc>,
    duration_str: &str,
    bar_size_setting: &str,
    what_to_show: &str,
    use_rth: bool,
  ) -> Result<i32, IBKRError> {
    if !self.connected {
      return Err(IBKRError::NotConnected);
    }

    let req_id = self.get_request_id();

    if let Some(encoder) = &mut self.encoder {
      encoder.encode_request_historical_data(
        req_id,
        contract,
        end_date_time,
        duration_str,
        bar_size_setting,
        what_to_show,
        use_rth,
        true,
      )?;
      encoder.flush()?;
    } else {
      return Err(IBKRError::NotConnected);
    }

    info!("Requested historical data for {} (req_id: {})", contract.symbol, req_id);

    Ok(req_id)
  }

  /// Request managed accounts
  pub fn request_managed_accounts(&mut self) -> Result<(), IBKRError> {
    if !self.connected {
      return Err(IBKRError::NotConnected);
    }

    if let Some(encoder) = &mut self.encoder {
      encoder.encode_request_managed_accounts()?;
      encoder.flush()?;
    } else {
      return Err(IBKRError::NotConnected);
    }

    info!("Requested managed accounts");

    Ok(())
  }

  /// Process incoming messages
  pub fn process_messages<F>(&mut self, callback: F) -> Result<(), IBKRError>
  where
    F: FnMut(i32, &[u8]) -> Result<(), IBKRError>
  {
    if !self.connected || self.stream.is_none() {
      return Err(IBKRError::NotConnected);
    }

    let stream_clone = self.stream.as_ref().unwrap().try_clone()
      .map_err(|e| IBKRError::SocketError(e.to_string()))?;

    let mut decoder = Decoder::new(stream_clone, self.server_version);

    // In a real implementation, this would include a loop to read and dispatch messages
    // We'll just provide the structure here

    Ok(())
  }
}
