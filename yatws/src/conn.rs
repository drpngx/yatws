// yatws/src/conn.rs

use crate::base::IBKRError;
use crate::handler::MessageHandler;

/// Trait defining the basic connection interface to TWS
pub trait Connection {
  /// Check if connected to TWS
  fn is_connected(&self) -> bool;

  /// Disconnect from TWS
  fn disconnect(&mut self) -> Result<(), IBKRError>;

  /// Send a raw message to TWS
  fn send_message(&mut self, data: &[u8]) -> Result<(), IBKRError>;

  /// Set the message handler for processing incoming messages
  fn set_message_handler(&mut self, handler: MessageHandler);
}

mod socket {
  use std::io::{Read, Write};
  use std::net::TcpStream;
  use std::time::Duration;
  use std::sync::{Arc, Mutex};
  use std::thread;
  use log::{info, error, debug};

  use crate::base::IBKRError;
  use crate::handler::MessageHandler;
  use crate::message_parser::process_message;
  use crate::min_server_ver::min_server_ver;
  use super::Connection;

  /// A basic socket connection to TWS that dispatches messages to a handler
  pub struct SocketConnection {
    host: String,
    port: u16,
    client_id: i32,
    server_version: i32,
    connected: bool,
    stream: Option<TcpStream>,
    handler: Option<MessageHandler>,
    reader_thread: Option<thread::JoinHandle<()>>,
    stop_flag: Arc<Mutex<bool>>,
  }

  impl SocketConnection {
    /// Create a new TWS connection and connect immediately
    pub fn new(host: &str, port: u16, client_id: i32) -> Result<Self, IBKRError> {
      let mut connection = Self {
        host: host.to_string(),
        port,
        client_id,
        server_version: 0,
        connected: false,
        stream: None,
        handler: None,
        reader_thread: None,
        stop_flag: Arc::new(Mutex::new(false)),
      };

      connection.connect()?;
      Ok(connection)
    }

    /// Connect to TWS (called by new)
    fn connect(&mut self) -> Result<(), IBKRError> {
      if self.connected {
        return Err(IBKRError::AlreadyConnected);
      }

      info!("Connecting to TWS at {}:{}", self.host, self.port);

      // Connect to the server
      let addr = format!("{}:{}", self.host, self.port);
      let mut stream = TcpStream::connect(addr)
        .map_err(|e| IBKRError::ConnectionFailed(e.to_string()))?;

      // Set timeouts
      stream.set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| IBKRError::ConnectionFailed(e.to_string()))?;

      stream.set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| IBKRError::ConnectionFailed(e.to_string()))?;

      // Send initial "API" header
      stream.write_all(b"API\0")
        .map_err(|e| IBKRError::SocketError(e.to_string()))?;
      stream.flush()
        .map_err(|e| IBKRError::SocketError(e.to_string()))?;

      // Build version string - using min and max version
      const MIN_VERSION: i32 = 100; // Minimum supported API version
      const MAX_VERSION: i32 = min_server_ver::MAX_SUPPORTED_VERSION;
      let version_string = if MIN_VERSION < MAX_VERSION {
        format!("v{}..{}", MIN_VERSION, MAX_VERSION)
      } else {
        format!("v{}", MIN_VERSION)
      };

      // Add connection options if needed
      let connect_options = ""; // Optional connection parameters
      let out = if !connect_options.is_empty() {
        format!("{} {}", version_string, connect_options)
      } else {
        version_string
      };

      // Send length-prefixed version string
      let out_bytes = out.as_bytes();
      let length = out_bytes.len() as i32;
      let length_bytes = length.to_be_bytes();

      stream.write_all(&length_bytes)
        .map_err(|e| IBKRError::SocketError(e.to_string()))?;
      stream.write_all(out_bytes)
        .map_err(|e| IBKRError::SocketError(e.to_string()))?;
      stream.flush()
        .map_err(|e| IBKRError::SocketError(e.to_string()))?;

      // Read server version
      let mut buffer = [0u8; 8192];
      let n = stream.read(&mut buffer)
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

      // Send client ID - simple implementation
      let client_id_msg = format!("71\0{}\0", self.client_id); // 71 is start_api message
      stream.write_all(client_id_msg.as_bytes())
        .map_err(|e| IBKRError::SocketError(e.to_string()))?;
      stream.flush()
        .map_err(|e| IBKRError::SocketError(e.to_string()))?;

      self.stream = Some(stream);
      self.connected = true;
      info!("Connected to TWS (server version: {})", self.server_version);

      Ok(())
    }

    /// Start the reader thread that processes incoming messages
    fn start_reader_thread(&mut self) -> Result<(), IBKRError> {
      if self.reader_thread.is_some() {
        return Ok(());  // Thread already running
      }

      if self.handler.is_none() {
        return Err(IBKRError::InternalError("No message handler set".to_string()));
      }

      if !self.connected || self.stream.is_none() {
        return Err(IBKRError::NotConnected);
      }

      // Clone the stream for the reader thread
      let reader_stream = self.stream.as_ref().unwrap().try_clone()
        .map_err(|e| IBKRError::SocketError(e.to_string()))?;

      // Reset stop flag
      {
        let mut stop = self.stop_flag.lock().unwrap();
        *stop = false;
      }

      // Clone the stop flag for the thread
      let stop_flag = self.stop_flag.clone();
      let server_version = self.server_version;

      // Move the handler into the thread
      let mut handler = self.handler.take().unwrap();

      // Create the reader thread
      let handle = thread::spawn(move || {
        debug!("Message reader thread started");

        // Create a decoder for reading messages
        let mut buffer = [0u8; 4]; // For message length

        // Create decoder for incoming messages
        let mut decoder = crate::protocol_decoder::Decoder::new(reader_stream, server_version);

        // Main message processing loop
        loop {
          // Check stop flag
          if *stop_flag.lock().unwrap() {
            debug!("Reader thread stopping");
            break;
          }

          // Use the protocol decoder to read and decode messages
          match decoder.decode_message() {
            Ok((msg_type, msg_data)) => {
              // Process the message using the provided handler
              if let Err(e) = process_message(&mut handler, msg_type, &msg_data) {
                error!("Error processing message: {:?}", e);
              }
            },
            Err(e) => {
              match e {
                IBKRError::SocketError(ref msg) if msg.contains("timed out") => {
                  // Read timeout, just continue
                  thread::sleep(Duration::from_millis(10));
                  continue;
                },
                IBKRError::SocketError(ref msg) if msg.contains("connection reset")
                  || msg.contains("broken pipe")
                  || msg.contains("connection refused") => {
                    error!("Connection lost: {}", msg);
                    break;
                  },
                _ => {
                  // Log other errors but continue processing
                  error!("Error decoding message: {:?}", e);
                  // Small delay to avoid tight loop on persistent errors
                  thread::sleep(Duration::from_millis(100));
                }
              }
            }
          }
        }

        debug!("Message reader thread ended");
      });

      self.reader_thread = Some(handle);
      Ok(())
    }
  }

  impl Connection for SocketConnection {
    fn is_connected(&self) -> bool {
      self.connected
    }

    fn disconnect(&mut self) -> Result<(), IBKRError> {
      if !self.connected {
        return Ok(());
      }

      info!("Disconnecting from TWS");

      // Signal reader thread to stop
      {
        let mut stop = self.stop_flag.lock().unwrap();
        *stop = true;
      }

      // Wait for reader thread to finish
      if let Some(handle) = self.reader_thread.take() {
        match handle.join() {
          Ok(_) => debug!("Reader thread joined successfully"),
          Err(e) => error!("Error joining reader thread: {:?}", e),
        }
      }

      self.stream = None;
      self.connected = false;
      self.handler = None;

      info!("Disconnected from TWS");
      Ok(())
    }

    fn send_message(&mut self, data: &[u8]) -> Result<(), IBKRError> {
      if !self.connected {
        return Err(IBKRError::NotConnected);
      }

      if let Some(stream) = &mut self.stream {
        // Write message length as big-endian i32
        let len = data.len() as i32;
        let len_bytes = len.to_be_bytes();
        stream.write_all(&len_bytes)
          .map_err(|e| IBKRError::SocketError(e.to_string()))?;

        // Write message data
        stream.write_all(data)
          .map_err(|e| IBKRError::SocketError(e.to_string()))?;

        stream.flush()
          .map_err(|e| IBKRError::SocketError(e.to_string()))?;

        Ok(())
      } else {
        Err(IBKRError::NotConnected)
      }
    }

    fn set_message_handler(&mut self, handler: MessageHandler) {
      self.handler = Some(handler);

      // Start the reader thread if we're connected
      if self.connected {
        if let Err(e) = self.start_reader_thread() {
          error!("Failed to start reader thread: {:?}", e);
        }
      }
    }
  }

  impl Drop for SocketConnection {
    fn drop(&mut self) {
      if self.connected {
        let _ = self.disconnect();
      }
    }
  }
}
