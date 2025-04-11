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
  use super::Connection;
  use crate::base::IBKRError;
  use crate::handler::MessageHandler;
  use crate::message_parser::process_message;
  use crate::min_server_ver::min_server_ver;

  use parking_lot::Mutex; // Only Mutex needed here now
  use std::sync::Arc;
  use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
  use log::{debug, error, info, warn};
  use std::io::{self, Cursor, ErrorKind, Read, Write};
  use std::net::{Shutdown, TcpStream, ToSocketAddrs};
  use std::thread;
  use std::time::Duration;
  use std::thread::sleep as thread_sleep;

  // --- SocketConnection Structs ---
  #[derive(Clone)]
  pub struct SocketConnection {
    host: String,
    port: u16,
    client_id: i32,
    // Core state protected by Mutex
    inner_state: Arc<Mutex<SocketConnectionInner>>,
    // Reader thread handle and stop flag, managed separately
    reader_thread: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
    stop_flag: Arc<Mutex<bool>>,
  }

  // Holds the actual connection state
  struct SocketConnectionInner {
    server_version: i32,
    connection_time: String,
    connected: bool, // API fully active (H3 sent, reader started)?
    h3_sent: bool,   // Has StartAPI (H3) been sent?
    stream: Option<TcpStream>, // Holds stream after H1/H2 until disconnect
    handler: Option<MessageHandler>, // Handler set by user
  }

  // --- Helper Functions (unchanged) ---
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
    /// Create a new TWS connection structure, connect TCP, and perform
    /// handshake steps H1 (client version) and H2 (server ack).
    /// The connection is not fully active (`is_connected` returns false) until
    /// `set_message_handler` is called successfully for the first time.
    pub fn new(host: &str, port: u16, client_id: i32) -> Result<Self, IBKRError> {
      info!("Initiating connection to TWS at {}:{}", host, port);

      // Perform initial connection and H1/H2
      let (stream, server_version, connection_time) = Self::connect_h1_h2(host, port)?;

      // Create the shared state
      let inner = SocketConnectionInner {
        server_version,
        connection_time,
        connected: false, // Not fully active yet
        h3_sent: false,   // StartAPI not sent yet
        stream: Some(stream), // Store the stream
        handler: None,    // Set later
      };

      let connection = Self {
        host: host.to_string(),
        port,
        client_id,
        inner_state: Arc::new(Mutex::new(inner)),
        reader_thread: Arc::new(Mutex::new(None)),
        stop_flag: Arc::new(Mutex::new(false)),
      };

      info!("TCP connection established, server version received. Waiting for message handler to send StartAPI.");
      Ok(connection)
    }

    /// Performs TCP connection and Handshake steps H1 and H2. (Internal Helper)
    fn connect_h1_h2(host: &str, port: u16) -> Result<(TcpStream, i32, String), IBKRError> {
      let timeout = Duration::from_secs(10);
      let addr = format!("{}:{}", host, port);

      let socket_addrs: Vec<_> = match addr.as_str().to_socket_addrs() {
        Ok(iter) => iter.collect(),
        Err(e) => return Err(IBKRError::ConfigurationError(format!("Invalid address '{}': {}", addr, e))),
      };
      if socket_addrs.is_empty() {
        return Err(IBKRError::ConfigurationError(format!("No valid IP addresses found for {}", addr)));
      }

      let mut stream = TcpStream::connect_timeout(&socket_addrs[0], timeout)
        .map_err(|e| IBKRError::ConnectionFailed(format!("Connect failed to {}: {}", socket_addrs[0], e)))?;

      stream.set_write_timeout(Some(timeout))
        .map_err(|e| IBKRError::ConnectionFailed(format!("Failed to set write timeout: {}", e)))?;

      // --- Handshake H1 ---
      info!("Sending Handshake H1...");
      let api_prefix = b"API\0";
      const MIN_VERSION: i32 = 100;
      const MAX_VERSION: i32 = min_server_ver::MAX_SUPPORTED_VERSION;
      let version_payload = if MIN_VERSION < MAX_VERSION { format!("v{}..{}", MIN_VERSION, MAX_VERSION) } else { format!("v{}", MIN_VERSION) };
      let connect_options = "";
      let version_payload_full = if !connect_options.is_empty() { format!("{} {}", version_payload, connect_options) } else { version_payload };
      let version_bytes = version_payload_full.as_bytes();
      let version_len_u32 = version_bytes.len() as u32;

      let mut h1_message = Vec::with_capacity(api_prefix.len() + 4 + version_bytes.len());
      h1_message.extend_from_slice(api_prefix);
      h1_message.write_u32::<BigEndian>(version_len_u32).unwrap();
      h1_message.extend_from_slice(version_bytes);

      stream.write_all(&h1_message).map_err(|e| IBKRError::SocketError(format!("Sending H1: {}", e)))?;
      stream.flush().map_err(|e| IBKRError::SocketError(format!("Flushing H1: {}", e)))?;
      info!("Sent Handshake H1 ({} bytes)", h1_message.len());

      // --- H2: Read Server Ack ---
      info!("Waiting for Handshake H2 (Server Ack)...");
      let h2_body = read_framed_message_body(&mut stream, timeout)?;
      info!("Received Handshake H2 body ({} bytes)", h2_body.len());

      let h2_parts: Vec<&[u8]> = h2_body.splitn(3, |&b| b == 0).collect();
      if h2_parts.len() < 2 || h2_parts[0].is_empty() || h2_parts[1].is_empty() {
        return Err(IBKRError::ParseError(format!("Invalid H2 body format: {:02X?}", h2_body)));
      }
      let server_version_str = std::str::from_utf8(h2_parts[0]).map_err(|e| IBKRError::ParseError(format!("Invalid UTF8 server version: {}", e)))?;
      let server_version = server_version_str.parse::<i32>().map_err(|e| IBKRError::ParseError(format!("Parsing server version '{}': {}", server_version_str, e)))?;
      let connection_time = std::str::from_utf8(h2_parts[1]).map_err(|e| IBKRError::ParseError(format!("Invalid UTF8 connection time: {}", e)))?.to_string();
      info!("Parsed ServerVersion={}, ConnectionTime='{}'", server_version, connection_time);

      Ok((stream, server_version, connection_time))
    }

    /// Starts the dedicated reader thread. (Internal Helper)
    /// Assumes the caller holds the lock to inner_state and has validated conditions.
    /// Takes ownership of the handler and a cloned stream.
    fn start_reader_thread(
      &self, // Needed to access Arcs
      mut reader_stream: TcpStream,
      mut handler: MessageHandler,
      server_version: i32,
    ) -> Result<(), IBKRError> {
      log::info!("Starting the reader thread.");

      // Clone Arcs needed for the thread
      let stop_flag_clone = self.stop_flag.clone();
      let state_clone = self.inner_state.clone();
      let reader_thread_handle_clone = self.reader_thread.clone();

      // Reset stop flag *before* spawning
      *self.stop_flag.lock() = false;

      let handle = thread::spawn(move || {
        debug!("Message reader thread started (Server Version: {})", server_version);
        let read_loop_timeout = Duration::from_secs(2);

        loop {
          // Check stop flag first
          if *stop_flag_clone.lock() {
            debug!("Reader thread received stop signal.");
            break;
          }

          match read_framed_message_body(&mut reader_stream, read_loop_timeout) {
            Ok(msg_data) => {
              if msg_data.is_empty() { continue; } // Skip empty keep-alives
              if let Err(e) = process_message(&mut handler, &msg_data) {
                error!("Error processing message: {:?}", e);
                // Decide if error is fatal? Maybe break?
              }
            },
            Err(IBKRError::Timeout(_)) => {
              // Normal read timeout, loop again
              continue;
            }
            Err(IBKRError::ConnectionFailed(msg)) | Err(IBKRError::SocketError(msg)) => {
              // Check for common connection loss errors before logging potentially noisy ones
              if msg.contains("timed out") || msg.contains("reset") || msg.contains("broken pipe") || msg.contains("refused") || msg.contains("closed") || msg.contains("EOF") || msg.contains("network") || msg.contains("host is down") {
                error!("Connection lost/error in reader thread: {}", msg);
                handler.connection_closed(); // Notify handler
                break; // Exit loop
              } else {
                warn!("Unhandled socket/connection error in reader: {}", msg); // Use warn for less critical errors
                // Consider breaking or sleeping
                thread_sleep(Duration::from_millis(100));
              }
            },
            Err(e) => { // Other IBKRErrors (ParseError, InternalError)
              error!("Non-IO error in reader thread: {:?}", e);
              // Consider if these should break the loop
              thread_sleep(Duration::from_millis(100));
            }
          }
        } // End loop

        // --- Cleanup after loop ends ---
        info!("Message reader thread stopping cleanup...");
        // Ensure stop flag is set upon exit, regardless of reason
        *stop_flag_clone.lock() = true;

        // Update shared state (mark as disconnected, remove stream ref potentially)
        {
          let mut state = state_clone.lock();
          state.connected = false;
          // state.stream should ideally be cleared by disconnect, but reader dying
          // implicitly means the main write stream might also be dead.
          // Disconnect should handle clearing state.stream properly.
        }

        // Clear the JoinHandle in the main struct
        *reader_thread_handle_clone.lock() = None;
        debug!("Message reader thread finished.");
      }); // End thread::spawn

      // Store the handle
      *self.reader_thread.lock() = Some(handle);
      Ok(())
    }

  } // end impl SocketConnection


  // --- Implement the Connection Trait ---
  impl Connection for SocketConnection {
    /// Checks if the API connection is fully active (StartAPI sent and reader running).
    fn is_connected(&self) -> bool {
      let state = self.inner_state.lock();
      // Considered connected only if H3 was sent AND the stop flag isn't set.
      // We rely on disconnect/reader thread exit to set connected=false if issues arise after H3.
      state.connected && !(*self.stop_flag.lock())
    }

    fn disconnect(&mut self) -> Result<(), IBKRError> {
      info!("Disconnecting from TWS...");
      let mut reader_handle_guard = self.reader_thread.lock();
      let mut state = self.inner_state.lock();

      let was_ever_partially_connected = state.stream.is_some(); // Did H1/H2 succeed?
      let is_reader_running = reader_handle_guard.is_some();

      if !was_ever_partially_connected && !is_reader_running && !*self.stop_flag.lock() {
        info!("Already disconnected (or never connected).");
        *self.stop_flag.lock() = true; // Ensure stop flag is set anyway
        return Ok(());
      }

      // 1. Signal reader thread (if it might exist)
      *self.stop_flag.lock() = true;

      // 2. Shutdown socket (if stream exists)
      if let Some(stream) = &mut state.stream { // Get mut ref
        if let Err(e) = stream.shutdown(Shutdown::Both) {
          if e.kind() != ErrorKind::NotConnected && e.kind() != ErrorKind::BrokenPipe {
            warn!("Error shutting down socket during disconnect: {}", e);
          }
        }
      }

      // 3. Reset state *before* dropping lock/joining thread
      state.connected = false;
      state.h3_sent = false; // Reset H3 flag too
      state.stream = None;   // Drop the stream
      state.handler = None;  // Drop the handler

      // 4. Take handle and drop locks before join
      let reader_handle = reader_handle_guard.take();
      drop(state);
      drop(reader_handle_guard);

      // 5. Wait for reader thread
      if let Some(handle) = reader_handle {
        debug!("Waiting for reader thread to join...");
        if let Err(e) = handle.join() {
          error!("Error joining reader thread: {:?}", e);
          // Reader thread cleanup should still attempt to clear its handle ref
        } else {
          debug!("Reader thread joined successfully.");
        }
        // Ensure handle is cleared even if join failed/panicked
        *self.reader_thread.lock() = None;
      } else {
        debug!("No active reader thread to join.");
      }

      info!("Disconnected from TWS.");
      Ok(())
    }

    /// Sends a message body. Requires the connection to be fully active (`is_connected` true).
    fn send_message_body(&mut self, data: &[u8]) -> Result<(), IBKRError> {
      let mut state = self.inner_state.lock();

      // Check if API is fully active (H3 sent, reader started, not stopped)
      if !state.connected {
        return Err(IBKRError::NotConnected);
      }

      // Stream must be Some if connected is true
      if let Some(stream) = &mut state.stream {
        write_framed_message(stream, data)
      } else {
        // This indicates an inconsistent state. Should not happen if logic is correct.
        error!("Inconsistent state: connected=true but stream is None during send.");
        state.connected = false; // Correct the state
        state.h3_sent = false;
        *self.stop_flag.lock() = true; // Signal stop
        Err(IBKRError::InternalError("Connection state inconsistent".to_string()))
      }
    }

    /// Sets the message handler. The first call triggers StartAPI (H3) and starts the reader thread.
    fn set_message_handler(&mut self, handler: MessageHandler) {
      let mut state = self.inner_state.lock();

      if state.handler.is_some() {
        warn!("Replacing existing message handler.");
        if state.connected && self.reader_thread.lock().is_some() {
          error!("Reader thread is active; replacing handler may lead to unexpected behavior. Consider disconnecting first.");
          // We are just replacing the handler field here; the running thread keeps the old one.
        }
      }
      state.handler = Some(handler); // Store the new handler regardless

      // Check conditions to send H3 and start reader:
      // 1. H1/H2 must have completed (stream exists).
      // 2. H3 shouldn't have been sent already.
      // 3. Reader thread should not be running already (safety check).
      let should_send_h3 = state.stream.is_some() && !state.h3_sent && self.reader_thread.lock().is_none();

      if should_send_h3 {
        info!("Message handler set. Sending StartAPI (H3) and starting reader thread...");

        // --- Send H3 (StartAPI) ---
        let capabilities = "";
        let h3_body_str = format!("71\02\0{}\0{}\0", self.client_id, capabilities);
        let h3_body_bytes = h3_body_str.as_bytes();

        // Get mutable access to the stream (unwrap safe due to check)
        let stream = state.stream.as_mut().unwrap();
        if let Err(e) = write_framed_message(stream, h3_body_bytes) {
          error!("Failed to send StartAPI (H3): {:?}. Disconnecting.", e);
          // Don't keep lock while calling disconnect
          drop(state);
          let _ = self.disconnect(); // Attempt cleanup
          // We could return the error, but disconnect is likely better.
          // Or, we could set state to disconnected here. Let disconnect handle it.
          return; // Exit after trying disconnect
        }
        info!("Sent Handshake H3 body ({} bytes)", h3_body_bytes.len());
        state.h3_sent = true; // Mark H3 as sent *before* starting reader

        // --- Start Reader Thread ---
        // Clone the stream for the reader (unwrap safe)
        let reader_stream = match state.stream.as_ref().unwrap().try_clone() {
          Ok(s) => s,
          Err(e) => {
            error!("Failed to clone stream for reader thread: {:?}. Disconnecting.", e);
            state.h3_sent = false; // Revert H3 sent flag?
            drop(state);
            let _ = self.disconnect(); // Attempt cleanup
            return;
          }
        };
        // Take the handler we just stored (unwrap safe)
        let handler_for_thread = state.handler.take().unwrap();
        let server_version = state.server_version;

        // Mark as connected *just before* starting reader
        state.connected = true;

        // Drop the lock *before* spawning the thread
        drop(state);

        // Call the helper to spawn the thread
        if let Err(e) = self.start_reader_thread(reader_stream, handler_for_thread, server_version) {
          error!("Failed to start reader thread: {:?}. Disconnecting.", e);
          // Need to re-acquire lock to reset state? Or just disconnect?
          // Let disconnect handle state reset.
          let _ = self.disconnect();
        } else {
          info!("Reader thread started successfully. Connection fully active.");
          // Need to put the handler back if start_reader_thread failed?
          // No, start_reader_thread takes ownership. If it fails, the handler is dropped.
          // If disconnect runs, it clears the handler anyway.
        }
      } else if state.h3_sent {
        debug!("Handler set/replaced, but StartAPI already sent. Reader thread state unchanged.");
      } else {
        debug!("Handler set, but prerequisites for sending StartAPI not met (stream missing?).");
      }
    }


    fn get_server_version(&self) -> i32 {
      // Server version is available after `new` succeeds.
      self.inner_state.lock().server_version
    }
  } // end impl Connection


  // --- Implement Drop (unchanged) ---
  impl Drop for SocketConnection {
    fn drop(&mut self) {
      let needs_disconnect = {
        if *self.stop_flag.lock() {
          false
        } else {
          let state = self.inner_state.lock();
          state.stream.is_some() || self.reader_thread.lock().is_some()
        }
      };

      if needs_disconnect {
        info!("Dropping SocketConnection, ensuring disconnect...");
        if let Err(e) = self.disconnect() {
          error!("Error during disconnect on drop: {:?}", e);
        }
      } else {
        info!("Dropping SocketConnection, disconnect not required.");
      }
    }
  }

} // end mod socket
