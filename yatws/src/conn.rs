// yatws/src/conn.rs

use std::fmt;
use crate::base::IBKRError;
use crate::handler::MessageHandler;
pub use socket::SocketConnection;
use parking_lot::{RwLock, Mutex};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone)]
pub struct MessageBroker {
  // *** FIX: Store Arc<Mutex<Box<dyn Connection>>> ***
  connection: Arc<Mutex<Box<dyn Connection>>>, // Store the Box inside the Mutex
  next_req_id: Arc<AtomicUsize>,
}

impl MessageBroker {
  pub fn new(connection: Box<dyn Connection>) -> Self {
    log::info!("Creating new MessageBroker.");
    Self {
      // Now creating Arc<Mutex<Box<dyn T>>> which is valid
      connection: Arc::new(Mutex::new(connection)),
      next_req_id: Arc::new(AtomicUsize::new(9000)),
    }
  }

  pub fn set_message_handler(&self, handler: MessageHandler) {
    let mut conn_guard = self.connection.lock();
    conn_guard.set_message_handler(handler);
  }

  // Methods now operate on the Box inside the guard
  pub fn send_message(&self, message_body: &[u8]) -> Result<(), IBKRError> {
    log::debug!("MessageBroker sending message ({} bytes)", message_body.len());
    let mut conn_guard = self.connection.lock(); // Get guard for Mutex<Box<...>>
    // Access the method on the value *inside* the Box using deref coercion
    conn_guard.send_message_body(message_body)
  }

  pub fn next_request_id(&self) -> i32 {
    // Ordering::SeqCst is correct, use statement added above
    let id = self.next_req_id.fetch_add(1, Ordering::SeqCst);
    id as i32
  }

  pub fn get_server_version(&self) -> Result<i32, IBKRError> {
    let conn_guard = self.connection.lock();
    // Access method on value inside Box
    Ok(conn_guard.get_server_version())
  }

  pub fn is_connected(&self) -> Result<bool, IBKRError> {
    let conn_guard = self.connection.lock();
    // Access method on value inside Box
    Ok(conn_guard.is_connected())
  }
}


// --- Connection Trait ---
/// Trait defining the basic connection interface to TWS
/// Requires Send + Sync + 'static for safe sharing across threads (e.g., in Arc<Mutex<>>)
pub trait Connection: Send + Sync + 'static {
  /// Check if connected to TWS
  fn is_connected(&self) -> bool;

  /// Disconnect from TWS
  fn disconnect(&mut self) -> Result<(), IBKRError>; // Takes &mut self

  /// Send a raw message to TWS (assumes body only, framing added)
  fn send_message_body(&mut self, data: &[u8]) -> Result<(), IBKRError>; // Takes &mut self

  /// Set the message handler for processing incoming messages
  fn set_message_handler(&mut self, handler: MessageHandler); // Takes &mut self

  /// Show server version.
  fn get_server_version(&self) -> i32; // Takes &self
}


// --- Socket Implementation Module ---
mod socket {
  use super::Connection; // Import trait from parent module
  use crate::base::IBKRError;
  use crate::handler::MessageHandler; // Ensure this is imported
  use crate::message_parser::process_message;
  use crate::min_server_ver::min_server_ver;

  // --- Use parking_lot primitives ---
  use parking_lot::{Mutex, RwLock}; // Keep RwLock if used elsewhere, Mutex definitely needed
  use std::sync::Arc; // Arc is from std

  use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
  use log::{debug, error, info, warn};
  use std::io::{self, Cursor, ErrorKind, Read, Write};
  use std::net::{Shutdown, TcpStream, ToSocketAddrs}; // Import necessary net items
  use std::thread;
  use std::time::Duration;
  use std::thread::sleep as thread_sleep; // Alias sleep


  // --- SocketConnection Structs (using parking_lot::Mutex) ---
  /// A basic socket connection to TWS that dispatches messages to a handler.
  /// This outer struct is Send + Sync + Clone because its state is in Arc<Mutex>.
  #[derive(Clone)] // Add Clone derive
  pub struct SocketConnection {
    host: String, // Keep these for reference/reconnect?
    port: u16,
    client_id: i32,
    // Core state protected by Mutex for interior mutability
    inner_state: Arc<Mutex<SocketConnectionInner>>,
    // Reader thread handle and stop flag, managed separately but using Arc/Mutex
    reader_thread: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
    stop_flag: Arc<Mutex<bool>>,
  }

  // Holds the actual connection state and mutable data
  struct SocketConnectionInner {
    server_version: i32,
    connection_time: String,
    connected: bool,
    stream: Option<TcpStream>, // TcpStream is not Clone, must be Option and moved/taken
    handler: Option<MessageHandler>, // Handler moves to the reader thread
  }

  // --- Helper Functions (read_exact_timeout, read_framed_message_body, write_framed_message) ---
  // These remain largely the same, operating on the raw TcpStream.

  fn read_exact_timeout(stream: &mut TcpStream, buf: &mut [u8], timeout: Duration) -> io::Result<()> {
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
          if start.elapsed() > timeout {
            return Err(io::Error::new(ErrorKind::TimedOut, "Read exact timed out after WouldBlock"));
          }
          thread_sleep(Duration::from_millis(1)); // Prevent busy-wait
          continue;
        }
        Err(e) => return Err(e),
      }
    }
    Ok(())
  }

  fn read_framed_message_body(stream: &mut TcpStream, timeout: Duration) -> Result<Vec<u8>, IBKRError> {
    let mut size_buf = [0u8; 4];
    // Set read timeout for this specific operation
    stream.set_read_timeout(Some(timeout)).map_err(|e| IBKRError::SocketError(format!("Failed to set read timeout: {}", e)))?;

    match read_exact_timeout(stream, &mut size_buf, timeout) {
      Ok(_) => (),
      Err(e) => {
        // Reset timeout maybe? Best effort.
        let _ = stream.set_read_timeout(None);
        return Err(match e.kind() {
          ErrorKind::TimedOut => IBKRError::Timeout(format!("Reading message size: {}", e)),
          ErrorKind::UnexpectedEof => IBKRError::ConnectionFailed(format!("Reading message size (EOF): {}", e)),
          _ => IBKRError::SocketError(format!("Reading message size: {}", e)),
        });
      }
    };

    let size = Cursor::new(size_buf).read_u32::<BigEndian>().map_err(|e| IBKRError::ParseError(format!("Parsing message size: {}", e)))? as usize;

    if size == 0 {
      // Reset timeout after successful (empty) read
      let _ = stream.set_read_timeout(None);
      return Ok(Vec::new());
    }

    const MAX_MSG_SIZE: usize = 10 * 1024 * 1024;
    if size > MAX_MSG_SIZE {
      let _ = stream.set_read_timeout(None);
      return Err(IBKRError::InternalError(format!("Message size too large: {}", size)));
    }

    let mut msg_buf = vec![0u8; size];
    match read_exact_timeout(stream, &mut msg_buf, timeout) {
      Ok(_) => (),
      Err(e) => {
        let _ = stream.set_read_timeout(None); // Reset timeout
        return Err(match e.kind() {
          ErrorKind::TimedOut => IBKRError::Timeout(format!("Reading message body ({} bytes): {}", size, e)),
          ErrorKind::UnexpectedEof => IBKRError::ConnectionFailed(format!("Reading message body (EOF, {} bytes): {}", size, e)),
          _ => IBKRError::SocketError(format!("Reading message body ({} bytes): {}", size, e)),
        });
      }
    };

    // Reset timeout after successful read
    let _ = stream.set_read_timeout(None);
    Ok(msg_buf)
  }

  fn write_framed_message(stream: &mut TcpStream, msg_body: &[u8]) -> Result<(), IBKRError> {
    let size = msg_body.len();
    if size > (u32::MAX as usize) {
      return Err(IBKRError::InvalidParameter("Message body too large".to_string()));
    }
    let size_u32 = size as u32;

    // Write size (u32 BE)
    stream.write_u32::<BigEndian>(size_u32)
      .map_err(|e| IBKRError::SocketError(format!("Writing message size: {}", e)))?;

    // Write body if not empty
    if size > 0 {
      stream.write_all(msg_body)
        .map_err(|e| IBKRError::SocketError(format!("Writing message body: {}", e)))?;
    }

    // Flush the stream
    stream.flush().map_err(|e| IBKRError::SocketError(format!("Flushing message: {}", e)))?;
    Ok(())
  }


  // --- SocketConnection Implementation ---
  impl SocketConnection {
    /// Create a new TWS connection and connect immediately
    pub fn new(host: &str, port: u16, client_id: i32) -> Result<Self, IBKRError> {
      let inner = SocketConnectionInner {
        server_version: 0,
        connection_time: String::new(),
        connected: false,
        stream: None,
        handler: None,
      };

      let mut connection = Self {
        host: host.to_string(),
        port,
        client_id,
        inner_state: Arc::new(Mutex::new(inner)), // parking_lot::Mutex
        reader_thread: Arc::new(Mutex::new(None)), // parking_lot::Mutex
        stop_flag: Arc::new(Mutex::new(false)),   // parking_lot::Mutex
      };

      connection.connect()?; // Call connect internally
      Ok(connection)
    }

    /// Connect to TWS (called by new)
    fn connect(&mut self) -> Result<(), IBKRError> {
      // lock() returns guard directly
      let mut state = self.inner_state.lock();
      // Optional: Check state.is_poisoned() if strict handling needed

      if state.connected {
        return Err(IBKRError::AlreadyConnected);
      }

      info!("Connecting to TWS at {}:{}", self.host, self.port);
      let timeout = Duration::from_secs(10);
      let addr = format!("{}:{}", self.host, self.port);

      // Use ToSocketAddrs on &str
      let socket_addrs: Vec<_> = match addr.as_str().to_socket_addrs() {
        Ok(iter) => iter.collect(),
        Err(e) => return Err(IBKRError::ConfigurationError(format!("Invalid address '{}': {}", addr, e))),
      };
      if socket_addrs.is_empty() {
        return Err(IBKRError::ConfigurationError(format!("No valid IP addresses found for {}", addr)));
      }

      // Try connecting
      let mut stream = TcpStream::connect_timeout(&socket_addrs[0], timeout)
        .map_err(|e| IBKRError::ConnectionFailed(format!("Connect failed to {}: {}", socket_addrs[0], e)))?;

      stream.set_write_timeout(Some(timeout))
        .map_err(|e| IBKRError::ConnectionFailed(format!("Failed to set write timeout: {}", e)))?;
      // Read timeout is set per-read

      // --- Handshake H1, H2, H3 ---
      info!("Sending Handshake H1...");
      let api_prefix = b"API\0";
      const MIN_VERSION: i32 = 100; // Example min TWS API version supported by client handshake
      const MAX_VERSION: i32 = min_server_ver::MAX_SUPPORTED_VERSION; // Max TWS API version from constants
      let version_payload = if MIN_VERSION < MAX_VERSION { format!("v{}..{}", MIN_VERSION, MAX_VERSION) } else { format!("v{}", MIN_VERSION) };
      let connect_options = ""; // Optional connection parameters (e.g., " clientId=...")
      let version_payload_full = if !connect_options.is_empty() { format!("{} {}", version_payload, connect_options) } else { version_payload };
      let version_bytes = version_payload_full.as_bytes();
      let version_len_u32 = version_bytes.len() as u32; // Assuming length fits in u32

      // Prepare H1: API\0 + Length (u32 BE) + Version String
      let mut h1_message = Vec::with_capacity(api_prefix.len() + 4 + version_bytes.len());
      h1_message.extend_from_slice(api_prefix);
      h1_message.write_u32::<BigEndian>(version_len_u32).unwrap(); // Use unwrap for Vec write
      h1_message.extend_from_slice(version_bytes);

      stream.write_all(&h1_message).map_err(|e| IBKRError::SocketError(format!("Sending H1: {}", e)))?;
      stream.flush().map_err(|e| IBKRError::SocketError(format!("Flushing H1: {}", e)))?;
      info!("Sent Handshake H1 ({} bytes)", h1_message.len());

      // --- H2: Read Server Ack ---
      info!("Waiting for Handshake H2 (Server Ack)...");
      // Note: read_framed_message_body handles setting/unsetting read timeout
      let h2_body = read_framed_message_body(&mut stream, timeout)?;
      info!("Received Handshake H2 body ({} bytes)", h2_body.len());

      let h2_parts: Vec<&[u8]> = h2_body.splitn(3, |&b| b == 0).collect();
      if h2_parts.len() < 2 || h2_parts[0].is_empty() || h2_parts[1].is_empty() {
        return Err(IBKRError::ParseError(format!("Invalid H2 body format: {:02X?}", h2_body)));
      }
      let server_version_str = std::str::from_utf8(h2_parts[0]).map_err(|e| IBKRError::ParseError(format!("Invalid UTF8 server version: {}", e)))?;
      state.server_version = server_version_str.parse::<i32>().map_err(|e| IBKRError::ParseError(format!("Parsing server version '{}': {}", server_version_str, e)))?;
      state.connection_time = std::str::from_utf8(h2_parts[1]).map_err(|e| IBKRError::ParseError(format!("Invalid UTF8 connection time: {}", e)))?.to_string();
      info!("Parsed ServerVersion={}, ConnectionTime='{}'", state.server_version, state.connection_time);

      // --- H3: Send StartAPI ---
      info!("Sending Handshake H3 (StartAPI)...");
      let capabilities = ""; // Optional capabilities
      let h3_body_str = format!("71\02\0{}\0{}\0", self.client_id, capabilities); // MsgId=71, Version=2
      let h3_body_bytes = h3_body_str.as_bytes();
      write_framed_message(&mut stream, h3_body_bytes)?; // write_framed_message handles framing
      info!("Sent Handshake H3 body ({} bytes)", h3_body_bytes.len());
      // --- End Handshake ---

      state.stream = Some(stream); // Store the stream in the locked state
      state.connected = true;
      info!("Connected successfully to TWS (Server Version: {})", state.server_version);

      // Keep lock until potential reader start check
      let handler_is_set = state.handler.is_some();
      drop(state); // Release lock

      // Start reader thread ONLY if handler was already set during connect() call
      if handler_is_set {
        if let Err(e) = self.start_reader_thread() {
          error!("Failed to start reader thread during connect: {:?}", e);
          let _ = self.disconnect(); // Attempt disconnect
          return Err(e);
        }
      }

      Ok(())
    }

    /// Start the reader thread. Assumes lock on inner_state is NOT held when called.
    fn start_reader_thread(&self) -> Result<(), IBKRError> {
      log::info!("Starting the reader thread.");
      // Check if already running
      if self.reader_thread.lock().is_some() { // lock() returns guard
        return Ok(());
      }
      log::info!("Locking the state.");

      // Lock state to check conditions and move handler/stream
      let mut state = self.inner_state.lock(); // lock() returns guard

      if state.handler.is_none() {
        return Err(IBKRError::InternalError("No message handler set to start reader".to_string()));
      }
      if !state.connected || state.stream.is_none() {
        return Err(IBKRError::NotConnected);
      }

      // Clone the stream for the reader thread
      let mut reader_stream = match state.stream.as_ref().unwrap().try_clone() {
        Ok(s) => s,
        Err(e) => return Err(IBKRError::SocketError(format!("Cloning stream for reader: {}", e))),
      };
      // Note: No need to set read timeout here, read_framed_message_body does it per call

      // Reset stop flag before starting
      *self.stop_flag.lock() = false; // lock() returns guard
      let stop_flag = self.stop_flag.clone(); // Clone Arc for thread
      let server_version = state.server_version;
      // Move handler out of state into thread
      let mut handler = state.handler.take().unwrap();
      // Clone Arc for state access in thread's cleanup
      let state_clone = self.inner_state.clone();
      // Clone Arc for thread handle access in thread's cleanup
      let reader_thread_handle_clone = self.reader_thread.clone();

      drop(state); // Release lock on inner_state before spawning thread

      let handle = thread::spawn(move || {
        debug!("Message reader thread started");
        let read_loop_timeout = Duration::from_secs(2); // Timeout for each read attempt

        loop {
          // Check stop flag first
          if *stop_flag.lock() { // lock() returns guard
            debug!("Reader thread received stop signal.");
            break;
          }

          match read_framed_message_body(&mut reader_stream, read_loop_timeout) {
            Ok(msg_data) => {
              if msg_data.is_empty() { continue; } // Skip empty keep-alives?

              if let Err(e) = process_message(&mut handler, &msg_data) {
                error!("Error processing message type: {:?}", e);
                // Decide if error is fatal? break; ?
              }
            },
            Err(IBKRError::Timeout(_)) => {
              // Normal read timeout, just loop again
              continue;
            }
            Err(IBKRError::ConnectionFailed(msg)) | Err(IBKRError::SocketError(msg)) => {
              // Check for specific connection loss errors
              if msg.contains("timed out") || msg.contains("reset") || msg.contains("broken pipe") || msg.contains("refused") || msg.contains("closed") || msg.contains("EOF") || msg.contains("network") || msg.contains("host is down") {
                error!("Connection lost in reader thread: {}", msg);
                handler.connection_closed(); // Notify handler *before* exit
                break; // Exit loop
              } else {
                error!("Unhandled socket/connection error in reader: {}", msg);
                thread_sleep(Duration::from_millis(100));
                // Consider if this should also break?
              }
            },
            Err(e) => { // Other IBKRErrors (ParseError, InternalError)
              error!("Non-IO error in reader thread: {:?}", e);
              thread_sleep(Duration::from_millis(100));
              // Decide if these are fatal? break; ?
            }
          }
        } // End loop

        // --- Cleanup after loop ends ---
        info!("Message reader thread stopping cleanup...");
        *stop_flag.lock() = true; // Ensure stop flag is set

        // Update shared state
        {
          let mut state = state_clone.lock(); // lock() returns guard
          state.connected = false;
          state.stream = None; // Stream is dead
          // Handler goes out of scope here, effectively dropped
        }

        // Clear the JoinHandle in the main struct
        {
          *reader_thread_handle_clone.lock() = None; // lock() returns guard
        }
        debug!("Message reader thread finished.");
      }); // End thread::spawn

      // Store the handle
      *self.reader_thread.lock() = Some(handle); // lock() returns guard
      Ok(())
    }
  } // end impl SocketConnection


  // --- Implement the Connection Trait ---
  impl Connection for SocketConnection {
    fn is_connected(&self) -> bool {
      // lock() returns guard directly
      self.inner_state.lock().connected && !(*self.stop_flag.lock())
    }

    // disconnect requires &mut self because it modifies the SocketConnection fields (reader_thread)
    fn disconnect(&mut self) -> Result<(), IBKRError> {
      info!("Disconnecting from TWS...");
      // lock() returns guards directly
      let mut reader_handle_guard = self.reader_thread.lock();
      let mut state = self.inner_state.lock();

      if !state.connected && reader_handle_guard.is_none() {
        info!("Already disconnected.");
        return Ok(());
      }

      // 1. Signal reader thread
      *self.stop_flag.lock() = true;

      // 2. Shutdown socket
      if let Some(stream) = &state.stream {
        if let Err(e) = stream.shutdown(Shutdown::Both) {
          if e.kind() != ErrorKind::NotConnected {
            warn!("Error shutting down socket during disconnect: {}", e);
          }
        }
      }

      // 3. Take handle and drop locks before join
      let reader_handle = reader_handle_guard.take();
      drop(state);
      drop(reader_handle_guard);

      // 4. Wait for reader thread
      if let Some(handle) = reader_handle {
        debug!("Waiting for reader thread to join...");
        if let Err(e) = handle.join() {
          error!("Error joining reader thread: {:?}", e);
        } else {
          debug!("Reader thread joined successfully.");
        }
      } else {
        debug!("No active reader thread to join.");
        // Manually clean up state if reader wasn't running
        let mut final_state = self.inner_state.lock();
        if final_state.connected {
          final_state.connected = false;
          final_state.stream = None;
          final_state.handler = None;
        }
      }

      // State *should* be updated by reader thread or manual cleanup above
      info!("Disconnected from TWS.");
      Ok(())
    }

    // send_message_body requires &mut self because write_framed_message needs &mut TcpStream
    fn send_message_body(&mut self, data: &[u8]) -> Result<(), IBKRError> {
      // lock() returns guard directly
      let mut state = self.inner_state.lock();

      if !state.connected { return Err(IBKRError::NotConnected); }

      if let Some(stream) = &mut state.stream {
        // Pass mutable ref to stream
        write_framed_message(stream, data)
      } else {
        Err(IBKRError::NotConnected)
      }
    }

    // set_message_handler requires &mut self because it might trigger start_reader_thread
    fn set_message_handler(&mut self, handler: MessageHandler) {
      // lock() returns guard directly
      let mut state = self.inner_state.lock();

      if state.handler.is_some() {
        warn!("Replacing existing message handler.");
        error!("Running reader thread may still use the old handler!");
      }
      state.handler = Some(handler);
      let is_connected = state.connected;
      drop(state); // Drop lock before potentially starting reader

      let is_reader_running = self.reader_thread.lock().is_some();
      if is_connected && !is_reader_running {
        info!("Setting handler and starting reader thread...");
        if let Err(e) = self.start_reader_thread() {
          error!("Failed to start reader thread after setting handler: {:?}", e);
          let _ = self.disconnect(); // Attempt disconnect
        }
      } else if is_connected {
        debug!("Handler set, but reader thread already running.");
      } else {
        debug!("Handler set, but not connected.");
      }
    }

    fn get_server_version(&self) -> i32 {
      // lock() returns guard directly
      self.inner_state.lock().server_version
    }
  } // end impl Connection


  // --- Implement Drop ---
  impl Drop for SocketConnection {
    fn drop(&mut self) {
      // Check connection status without relying on is_connected self borrow
      let needs_disconnect = {
        let state = self.inner_state.lock();
        state.connected || self.reader_thread.lock().is_some()
      };
      if needs_disconnect {
        info!("Dropping SocketConnection, ensuring disconnect...");
        if let Err(e) = self.disconnect() {
          error!("Error during disconnect on drop: {:?}", e);
        }
      }
    }
  }

} // end mod socket
