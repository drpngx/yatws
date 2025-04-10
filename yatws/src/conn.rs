// yatws/src/conn.rs

use std::fmt;
use crate::base::IBKRError;
use crate::handler::MessageHandler;
pub use socket::SocketConnection;

/// Trait defining the basic connection interface to TWS
pub trait Connection {
  /// Check if connected to TWS
  fn is_connected(&self) -> bool;

  /// Disconnect from TWS
  fn disconnect(&mut self) -> Result<(), IBKRError>;

  /// Send a raw message to TWS (assumes body only, framing added)
  fn send_message_body(&mut self, data: &[u8]) -> Result<(), IBKRError>;

  /// Set the message handler for processing incoming messages
  fn set_message_handler(&mut self, handler: MessageHandler);

  /// Show server version.
  fn get_server_version(&self) -> i32;
}

// --- Add necessary imports ---
mod socket {
  use std::io::ErrorKind;
  use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt}; // Added byteorder
  use log::{debug, error, info, warn};
  use std::io::{self, Cursor, Read, Write}; // Added io::Cursor
  use std::net::TcpStream;
  use std::sync::{Arc, Mutex};
  use std::thread;
  use std::time::Duration;

  use super::Connection;
  use crate::base::IBKRError;
  use crate::handler::MessageHandler;
  use crate::message_parser::process_message; // Assuming this exists
  use crate::min_server_ver::min_server_ver; // Assuming this exists
  // Placeholder for Decoder if it's used in the reader thread
  // use crate::protocol_decoder::Decoder;


  /// A basic socket connection to TWS that dispatches messages to a handler
  pub struct SocketConnection {
    host: String,
    port: u16,
    client_id: i32,
    server_version: i32,
    connection_time: String, // Store connection time from handshake
    connected: bool,
    stream: Option<TcpStream>,
    handler: Option<MessageHandler>,
    reader_thread: Option<thread::JoinHandle<()>>,
    stop_flag: Arc<Mutex<bool>>,
  }

  // Helper to read exactly `len` bytes with timeout
  // (Similar to the proxy's helper, crucial for reliable reads)
  fn read_exact_timeout(stream: &mut TcpStream, buf: &mut [u8], timeout: Duration) -> io::Result<()> {
    // Implementation adapted from the proxy - ensure stream has timeout set *before* calling this,
    // or modify this function to handle setting/restoring timeout.
    // For simplicity here, assume timeout is set on the stream beforehand.
    let start = std::time::Instant::now();
    let mut bytes_read = 0;
    while bytes_read < buf.len() {
      if start.elapsed() > timeout {
        return Err(io::Error::new(ErrorKind::TimedOut, "Read exact timed out"));
      }
      match stream.read(&mut buf[bytes_read..]) {
        Ok(0) => return Err(io::Error::new(ErrorKind::UnexpectedEof, "Connection closed while reading exact")),
        Ok(n) => bytes_read += n,
        Err(ref e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {
          // Check timeout explicitly after WouldBlock/TimedOut
          if start.elapsed() > timeout {
            return Err(io::Error::new(ErrorKind::TimedOut, "Read exact timed out"));
          }
          // Small sleep to prevent potential busy-wait on WouldBlock
          thread::sleep(Duration::from_millis(1));
          continue; // Retry read
        }
        Err(e) => return Err(e),
      }
    }
    Ok(())
  }

  // Helper to read a framed message (Length + Body), returns Body
  fn read_framed_message_body(stream: &mut TcpStream, timeout: Duration) -> Result<Vec<u8>, IBKRError> {
    let mut size_buf = [0u8; 4];
    // Ensure stream has read timeout set before calling this
    stream.set_read_timeout(Some(timeout)).map_err(|e| IBKRError::SocketError(format!("Failed to set read timeout: {}", e)))?;

    read_exact_timeout(stream, &mut size_buf, timeout).map_err(|e| {
      IBKRError::SocketError(format!("Reading message size: {}", e))
    })?;

    let size = Cursor::new(size_buf).read_u32::<BigEndian>().map_err(|e| IBKRError::ParseError(format!("Parsing message size: {}", e)))? as usize;

    if size == 0 { return Ok(Vec::new()); }

    const MAX_MSG_SIZE: usize = 10 * 1024 * 1024; // 10 MB limit
    if size > MAX_MSG_SIZE { return Err(IBKRError::InternalError(format!("Message size too large: {}", size))); }

    let mut msg_buf = vec![0u8; size];
    read_exact_timeout(stream, &mut msg_buf, timeout).map_err(|e| {
      IBKRError::SocketError(format!("Reading message body ({} bytes): {}", size, e))
    })?;

    Ok(msg_buf)
  }

  // Helper to write a framed message (prepends length to body)
  fn write_framed_message(stream: &mut TcpStream, msg_body: &[u8]) -> Result<(), IBKRError> {
    let size = msg_body.len() as u32; // Use u32 for length calculation
    let mut size_buf = [0u8; 4];
    // Use Cursor for safe writing
    Cursor::new(&mut size_buf[..]).write_u32::<BigEndian>(size)
      .map_err(|e| IBKRError::InternalError(format!("Failed to encode length: {}", e)))?; // Should not fail

    stream.write_all(&size_buf).map_err(|e| IBKRError::SocketError(e.to_string()))?;
    if size > 0 {
      stream.write_all(msg_body).map_err(|e| IBKRError::SocketError(e.to_string()))?;
    }
    stream.flush().map_err(|e| IBKRError::SocketError(e.to_string()))?;
    Ok(())
  }


  impl SocketConnection {
    /// Create a new TWS connection and connect immediately
    pub fn new(host: &str, port: u16, client_id: i32) -> Result<Self, IBKRError> {
      let mut connection = Self {
        host: host.to_string(),
        port,
        client_id,
        server_version: 0,
        connection_time: String::new(), // Initialize empty
        connected: false,
        stream: None,
        handler: None,
        reader_thread: None,
        stop_flag: Arc::new(Mutex::new(false)),
      };

      connection.connect()?; // Call connect internally
      Ok(connection)
    }

    /// Connect to TWS (called by new)
    fn connect(&mut self) -> Result<(), IBKRError> {
      if self.connected {
        return Err(IBKRError::AlreadyConnected);
      }

      info!("Connecting to TWS at {}:{}", self.host, self.port);
      let timeout = Duration::from_secs(10); // Use a reasonable timeout

      // Connect to the server with timeout
      let addr = format!("{}:{}", self.host, self.port);
      let socket_addr = addr.parse().map_err(|_| IBKRError::ConfigurationError(format!("Invalid address: {}", addr)))?;
      let mut stream = TcpStream::connect_timeout(&socket_addr, timeout)
        .map_err(|e| IBKRError::ConnectionFailed(format!("Connect failed: {}", e)))?;

      // Set write timeout (read timeout is handled per-read)
      stream.set_write_timeout(Some(timeout))
        .map_err(|e| IBKRError::ConnectionFailed(format!("Failed to set write timeout: {}",e)))?;


      // --- Step H1: Send Client Handshake ---
      info!("Sending Handshake H1...");
      let api_prefix = b"API\0";

      // Build version string payload
      const MIN_VERSION: i32 = 100;
      const MAX_VERSION: i32 = min_server_ver::MAX_SUPPORTED_VERSION; // Use constant from crate
      let version_payload = if MIN_VERSION < MAX_VERSION {
        format!("v{}..{}", MIN_VERSION, MAX_VERSION)
      } else {
        format!("v{}", MIN_VERSION)
      };
      // Add connection options if needed (usually empty)
      let connect_options = ""; // Optional connection parameters
      let version_payload_full = if !connect_options.is_empty() {
        format!("{} {}", version_payload, connect_options)
      } else {
        version_payload
      };
      let version_bytes = version_payload_full.as_bytes();
      let version_len = version_bytes.len() as u32; // Use u32 for length

      // Prepare the complete H1 message: API\0 + Length + Version String
      let mut h1_message = Vec::with_capacity(api_prefix.len() + 4 + version_bytes.len());
      h1_message.extend_from_slice(api_prefix); // Add "API\0"
      h1_message.write_u32::<BigEndian>(version_len).unwrap(); // Add 4-byte length
      h1_message.extend_from_slice(version_bytes); // Add version string bytes

      // Send the complete H1 message in one go
      stream.write_all(&h1_message)
        .map_err(|e| IBKRError::SocketError(format!("Sending H1: {}", e)))?;
      stream.flush()
        .map_err(|e| IBKRError::SocketError(format!("Flushing H1: {}", e)))?;
      info!("Sent Handshake H1 ({} bytes)", h1_message.len());


      // --- Step H2: Read Server Acknowledgement (Framed) ---
      info!("Waiting for Handshake H2 (Server Ack)...");
      let h2_body = read_framed_message_body(&mut stream, timeout)?;
      info!("Received Handshake H2 body ({} bytes)", h2_body.len());

      // Parse H2 body: ServerVersionString\0TimestampString\0
      let h2_parts: Vec<&[u8]> = h2_body.splitn(3, |&b| b == 0).collect();
      if h2_parts.len() < 2 || h2_parts[0].is_empty() {
        return Err(IBKRError::ParseError(format!("Invalid H2 body format: {:02X?}", h2_body)));
      }

      let server_version_str = std::str::from_utf8(h2_parts[0])
        .map_err(|e| IBKRError::ParseError(format!("Invalid UTF8 in server version: {}", e)))?;
      self.server_version = server_version_str.parse::<i32>()
        .map_err(|e| IBKRError::ParseError(format!("Parsing server version '{}': {}", server_version_str, e)))?;

      self.connection_time = std::str::from_utf8(h2_parts[1])
        .map_err(|e| IBKRError::ParseError(format!("Invalid UTF8 in connection time: {}", e)))?
        .to_string();
      info!("Parsed ServerVersion={}, ConnectionTime='{}'", self.server_version, self.connection_time);


      // --- Step H3: Send StartAPI (Framed) ---
      info!("Sending Handshake H3 (StartAPI)...");
      // Message Body: OutgoingMsgId(71)\0Version(2)\0ClientID\0OptionalCapabilities\0
      // Use version 2 for StartAPI
      let capabilities = ""; // Empty string for optional capabilities
      let h3_body_str = format!("71\02\0{}\0{}\0", self.client_id, capabilities);
      let h3_body_bytes = h3_body_str.as_bytes();

      // Send framed message
      write_framed_message(&mut stream, h3_body_bytes)?;
      info!("Sent Handshake H3 body ({} bytes)", h3_body_bytes.len());


      // --- Handshake Complete ---
      self.stream = Some(stream);
      self.connected = true;
      info!("Connected successfully to TWS (Server Version: {})", self.server_version);

      // Note: Reader thread is started only when handler is set

      Ok(())
    }

    /// Start the reader thread that processes incoming messages
    fn start_reader_thread(&mut self) -> Result<(), IBKRError> {
      if self.reader_thread.is_some() { return Ok(()); } // Already running
      if self.handler.is_none() { return Err(IBKRError::InternalError("No message handler set".to_string())); }
      if !self.connected || self.stream.is_none() { return Err(IBKRError::NotConnected); }

      let mut reader_stream = self.stream.as_ref().unwrap().try_clone()
        .map_err(|e| IBKRError::SocketError(format!("Cloning stream for reader: {}", e)))?;

      // Set read timeout for the reader thread's stream
      let read_timeout = Duration::from_secs(2); // Use a shorter timeout for the reader loop
      reader_stream.set_read_timeout(Some(read_timeout))
        .map_err(|e| IBKRError::SocketError(format!("Setting reader timeout: {}", e)))?;


      { *self.stop_flag.lock().unwrap() = false; } // Reset stop flag
      let stop_flag = self.stop_flag.clone();
      let server_version = self.server_version;
      let mut handler = self.handler.take().unwrap(); // Move handler

      let handle = thread::spawn(move || {
        debug!("Message reader thread started");
        let timeout = Duration::from_secs(2); // Timeout for read attempts inside loop

        // Main message processing loop
        loop {
          if *stop_flag.lock().unwrap() { debug!("Reader thread stopping"); break; }

          // Read next message (framed)
          match read_framed_message_body(&mut reader_stream, timeout) {
            Ok(msg_data) => {
              if msg_data.is_empty() { continue; } // Skip empty keep-alives?

              // We need to parse the message type ID from the start of msg_data
              let mut cursor = Cursor::new(&msg_data);
              match cursor.read_i32::<BigEndian>() { // Assuming message ID is i32 BE
                Ok(msg_type_id) => {
                  // Pass the *remaining* data (excluding the ID) to process_message?
                  // Or does process_message expect the full data including ID?
                  // Assuming process_message handles parsing from the full data:
                  if let Err(e) = process_message(&mut handler, msg_type_id, &msg_data) {
                    error!("Error processing message (type {}): {:?}", msg_type_id, e);
                  }
                }
                Err(e) => {
                  error!("Error reading message type ID: {:?} - Data: {:02X?}", e, msg_data);
                  // Avoid tight loop on parsing error
                  thread::sleep(Duration::from_millis(100));
                }
              }
            },
            Err(IBKRError::SocketError(ref msg)) => {
              if msg.contains("timed out") {
                // Normal read timeout, continue loop
                if *stop_flag.lock().unwrap() { debug!("Reader thread stopping after timeout check"); break; }
                continue;
              }
              // Check for connection closed errors
              if msg.contains("connection reset")
                || msg.contains("broken pipe")
                || msg.contains("connection refused")
                || msg.contains("closed")
                || msg.contains("EOF")
              {
                error!("Connection lost in reader thread: {}", msg);
                break; // Exit loop on fatal connection error
              } else {
                // Log other socket errors but maybe continue? Or break? Depends on severity.
                error!("Unhandled socket error in reader thread: {}", msg);
                thread::sleep(Duration::from_millis(100)); // Avoid tight loop
                // Consider breaking here too unless the error is recoverable
                // break;
              }
            },
            Err(e) => { // Other IBKRErrors (ParseError, InternalError)
              error!("Non-socket error in reader thread: {:?}", e);
              // Avoid tight loop on persistent errors
              thread::sleep(Duration::from_millis(100));
              // Decide if these errors are fatal for the loop
              // break;
            }
          }
        }
        // Ensure stop flag is set when thread exits
        *stop_flag.lock().unwrap() = true;
        debug!("Message reader thread ended");
        // Maybe notify main thread or handler that connection is lost?
        // handler.connection_closed(); // Example
      });

      self.reader_thread = Some(handle);
      Ok(())
    }
  }

  impl Connection for SocketConnection {
    fn is_connected(&self) -> bool {
      // Check both flag and if reader thread is still alive (if applicable)
      self.connected && !(*self.stop_flag.lock().unwrap())
    }

    fn disconnect(&mut self) -> Result<(), IBKRError> {
      if !self.connected && self.reader_thread.is_none() { return Ok(()); } // Already disconnected

      info!("Disconnecting from TWS");

      // 1. Signal reader thread to stop
      { *self.stop_flag.lock().unwrap() = true; }

      // 2. Shutdown the socket to interrupt blocking reads in the reader thread
      if let Some(stream) = &self.stream {
        // Shutdown both read and write - helps unblock the reader thread immediately
        if let Err(e) = stream.shutdown(std::net::Shutdown::Both) {
          // Ignore "NotConnected" errors, log others
          if e.kind() != std::io::ErrorKind::NotConnected {
            warn!("Error shutting down socket: {}", e);
          }
        }
      }

      // 3. Wait for reader thread to finish
      if let Some(handle) = self.reader_thread.take() {
        match handle.join() {
          Ok(_) => debug!("Reader thread joined successfully"),
          Err(e) => error!("Error joining reader thread: {:?}", e), // Should not panic often after shutdown
        }
      } else {
        debug!("No reader thread to join.");
      }

      // 4. Clear state
      self.stream = None; // Stream is likely closed now anyway
      self.connected = false;
      self.handler = None; // Remove handler on disconnect

      info!("Disconnected from TWS");
      Ok(())
    }

    // Renamed to send_message_body for clarity
    fn send_message_body(&mut self, data: &[u8]) -> Result<(), IBKRError> {
      if !self.is_connected() { return Err(IBKRError::NotConnected); } // Use is_connected check

      if let Some(stream) = &mut self.stream {
        // Use helper to write framed message
        write_framed_message(stream, data)
      } else {
        // This case should ideally not happen if is_connected is true
        Err(IBKRError::NotConnected)
      }
    }

    fn set_message_handler(&mut self, handler: MessageHandler) {
      // Check if already connected and reader is running; maybe stop/replace?
      // Simple version: just set handler and start reader if needed.
      if self.handler.is_some() {
        warn!("Replacing existing message handler.");
        // Potentially stop old reader thread if replacing? Depends on desired behavior.
      }

      self.handler = Some(handler);

      // Start the reader thread *only if* connected and thread isn't already running
      if self.connected && self.reader_thread.is_none() {
        if let Err(e) = self.start_reader_thread() {
          error!("Failed to start reader thread after setting handler: {:?}", e);
          // Consider disconnecting or returning error if reader fails to start?
          let _ = self.disconnect(); // Disconnect if reader can't start
          // return Err(e); // Propagate error?
        }
      } else if self.connected && self.reader_thread.is_some() {
        debug!("Reader thread already running, handler updated.");
        // Need to consider how the running thread gets the *new* handler.
        // This design requires handler to be set *before* connect or passed via channel.
        // The current `start_reader_thread` takes the handler via `self.handler.take()`,
        // so replacing it here won't affect the running thread.
        error!("Limitation: Cannot change handler for already running reader thread in this design.");
      }
    }

    fn get_server_version(&self) -> i32 { self.server_version }
  }

  impl Drop for SocketConnection {
    fn drop(&mut self) {
      // Ensure disconnect is called on drop
      if self.connected || self.reader_thread.is_some() {
        let _ = self.disconnect();
      }
    }
  }
}
