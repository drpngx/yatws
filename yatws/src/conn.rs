// yatws/src/conn.rs

use crate::base::IBKRError;
use crate::handler::MessageHandler;
pub use socket::SocketConnection;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use crate::rate_limiter::TokenBucketRateLimiter;

use crate::conn_log::{ConnectionLogger, LogDirection, bytes_to_center_dot_string};

#[derive(Clone)]
pub struct MessageBroker {
  // Connection is still Box<dyn Connection> behind an Arc<Mutex>
  connection: Arc<Mutex<Box<dyn Connection>>>,
  next_req_id: Arc<AtomicUsize>,
  // Add optional logger
  logger: Option<ConnectionLogger>, // Use the concrete type from conn_log
  rate_limiter: Option<Arc<TokenBucketRateLimiter>>,
  rate_limit_timeout: Duration,
}

const START_REQUEST_ID: usize = 1;

impl MessageBroker {
  // Modify new to accept optional logger
  pub fn new(connection: Box<dyn Connection>, logger: Option<ConnectionLogger>) -> Self {
    log::info!("Creating new MessageBroker.");
    Self {
      connection: Arc::new(Mutex::new(connection)),
      next_req_id: Arc::new(AtomicUsize::new(START_REQUEST_ID)),
      logger, // Store the logger
      rate_limiter: None, // No rate limiter by default
      rate_limit_timeout: Duration::from_secs(5), // Default timeout
    }
  }

  pub fn set_message_handler(&self, handler: MessageHandler) {
    let mut conn_guard = self.connection.lock();
    conn_guard.set_message_handler(handler);
  }

  // Instrument send_message
  pub fn send_message(&self, message_body: &[u8]) -> Result<(), IBKRError> {
    // Apply rate limiting if enabled
    if let Some(limiter) = &self.rate_limiter {
      if !limiter.acquire(self.rate_limit_timeout) {
        return Err(IBKRError::RateLimited("Message rate limit exceeded".to_string()));
      }
    }
    // Log before sending
    if let Some(logger) = &self.logger {
      // Note: Cloning the Arc is cheap
      logger.log_message(LogDirection::Send, message_body);
    }

    log::debug!("MessageBroker sending message ({} bytes)", message_body.len());
    log::trace!("  Message: {}", bytes_to_center_dot_string(message_body));
    let mut conn_guard = self.connection.lock();
    conn_guard.send_message_body(message_body) // Send after logging
  }

  pub fn next_request_id(&self) -> i32 {
    let id = self.next_req_id.fetch_add(1, Ordering::SeqCst);
    id as i32
  }

  pub fn get_server_version(&self) -> Result<i32, IBKRError> {
    let conn_guard = self.connection.lock();
    Ok(conn_guard.get_server_version())
  }

  #[allow(dead_code)]
  pub fn is_connected(&self) -> Result<bool, IBKRError> {
    let conn_guard = self.connection.lock();
    Ok(conn_guard.is_connected())
  }

  /// Set the rate limiter for this message broker.
  pub fn set_rate_limiter(&self, rate_limiter: Option<Arc<TokenBucketRateLimiter>>) {
    if let Some(limiter) = &rate_limiter {
      log::info!("Setting message rate limiter (enabled: {})", limiter.enabled());
    } else {
      log::info!("Removing message rate limiter");
    }

    // Use interior mutability to assign the new rate limiter
    unsafe {
      let me = self as *const Self as *mut Self;
      (*me).rate_limiter = rate_limiter;
    }
  }

  /// Set the timeout duration for rate limiting.
  pub fn set_rate_limit_timeout(&self, timeout: Duration) {
    log::debug!("Setting rate limit timeout to {:?}", timeout);

    // Use interior mutability to assign the new timeout
    unsafe {
      let me = self as *const Self as *mut Self;
      (*me).rate_limit_timeout = timeout;
    }
  }

  /// Disconnect the underlying connection.
  /// This method can be called even when there are multiple Arc references to the MessageBroker.
  pub fn disconnect(&self) -> Result<(), IBKRError> {
    log::info!("MessageBroker disconnect requested");
    let mut conn_guard = self.connection.lock();
    conn_guard.disconnect()
  }
}

// --- Connection Trait ---
pub trait Connection: Send + Sync + 'static {
  fn is_connected(&self) -> bool;
  fn disconnect(&mut self) -> Result<(), IBKRError>;
  fn send_message_body(&mut self, data: &[u8]) -> Result<(), IBKRError>;
  fn set_message_handler(&mut self, handler: MessageHandler);
  fn get_server_version(&self) -> i32;
}


// --- Socket Implementation Module ---
mod socket {
  use super::Connection;
  // Import logger items needed here
  use super::{ConnectionLogger, LogDirection};
  use crate::base::IBKRError;
  use crate::handler::MessageHandler;
  use crate::message_parser::{msg_to_string, process_message};
  use crate::min_server_ver::min_server_ver;

  use parking_lot::Mutex as PLMutex; // Use specific name if needed
  use std::sync::Arc;
  use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
  use log::{debug, error, info, warn};
  use std::io::{self, Cursor, ErrorKind, Read, Write};
  use std::net::{Shutdown, TcpStream, ToSocketAddrs};
  use socket2::{SockRef, TcpKeepalive};
  use std::thread;
  use std::time::Duration;
  use std::thread::sleep as thread_sleep;

  // --- SocketConnection Structs ---
  #[derive(Clone)]
  pub struct SocketConnection {
    #[allow(dead_code)]
    host: String,
    #[allow(dead_code)]
    port: u16,
    client_id: i32,
    inner_state: Arc<PLMutex<SocketConnectionInner>>,
    reader_thread: Arc<PLMutex<Option<thread::JoinHandle<()>>>>,
    stop_flag: Arc<PLMutex<bool>>,
    // Add optional logger to SocketConnection as well
    logger: Option<ConnectionLogger>,
  }

  // Inner state holds the potentially mutable parts
  struct SocketConnectionInner {
    server_version: i32,
    #[allow(dead_code)]
    connection_time: String,
    connected: bool,
    h3_sent: bool,
    stream: Option<TcpStream>,
    handler: Option<MessageHandler>,
    // Logger is not stored here, it's passed directly to the reader thread
  }

  // --- Helper Functions (read_exact_timeout, read_framed_message_body, write_framed_message) ---
  // (No changes needed in these helpers, they just handle bytes)
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
    stream.set_read_timeout(Some(timeout)).map_err(|e| IBKRError::SocketError(format!("Failed to set read timeout: {}", e)))?;

    match read_exact_timeout(stream, &mut size_buf, timeout) {
      Ok(_) => (),
      Err(e) => {
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

    let _ = stream.set_read_timeout(None);
    Ok(msg_buf)
  }

  fn write_framed_message(stream: &mut TcpStream, msg_body: &[u8]) -> Result<(), IBKRError> {
    let size = msg_body.len();
    if size > (u32::MAX as usize) {
      return Err(IBKRError::InvalidParameter("Message body too large".to_string()));
    }
    let size_u32 = size as u32;

    stream.write_u32::<BigEndian>(size_u32)
      .map_err(|e| IBKRError::SocketError(format!("Writing message size: {}", e)))?;

    if size > 0 {
      stream.write_all(msg_body)
        .map_err(|e| IBKRError::SocketError(format!("Writing message body: {}", e)))?;
    }

    stream.flush().map_err(|e| IBKRError::SocketError(format!("Flushing message: {}", e)))?;
    Ok(())
  }


  // --- SocketConnection Implementation ---
  impl SocketConnection {
    // Modify new to accept logger (as before)
    pub fn new(host: &str, port: u16, client_id: i32, logger: Option<ConnectionLogger>) -> Result<Self, IBKRError> {
      info!("Initiating connection to TWS at {}:{}", host, port);

      let (stream, server_version, connection_time) = Self::connect_h1_h2(host, port)?;

      if let Some(ref logr) = logger {
        logr.set_session_server_version(server_version);
      }

      let inner = SocketConnectionInner {
        server_version,
        connection_time,
        connected: false,
        h3_sent: false,
        stream: Some(stream),
        handler: None,
      };

      let connection = Self {
        host: host.to_string(),
        port,
        client_id,
        inner_state: Arc::new(PLMutex::new(inner)),
        reader_thread: Arc::new(PLMutex::new(None)),
        stop_flag: Arc::new(PLMutex::new(false)),
        logger, // Store the passed logger
      };

      info!("TCP connection established, server version received. Waiting for message handler to send StartAPI.");
      Ok(connection)
    }

    // connect_h1_h2 remains the same
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

      // Flush small packets.
      stream.set_nodelay(true).unwrap_or_else(|e| log::warn!("Failed to set nodelay: {:?}", e));
      // Only tokio::TcpStream has keepalive.
      {
        let sock_ref = SockRef::from(&stream);
        let keepalive = TcpKeepalive::new()
          .with_time(Duration::from_secs(60))       // Time before sending first keepalive probe
          .with_interval(Duration::from_secs(20))   // Interval between keepalive probes
          .with_retries(32);                        // Number of failed probes before dropping the connection
        sock_ref.set_tcp_keepalive(&keepalive).unwrap_or_else(|e| log::warn!("Failed to set keepalive: {:?}", e));
      }

      // --- Handshake H1 ---
      info!("Sending Handshake H1...");
      let api_prefix = b"API\0";
      const MIN_VERSION: i32 = 100;
      const MAX_VERSION: i32 = min_server_ver::MAX_SUPPORTED_VERSION;
      let version_payload = if MIN_VERSION < MAX_VERSION { format!("v{}..{}", MIN_VERSION, MAX_VERSION) } else { format!("v{}", MIN_VERSION) };
      let connect_options = ""; // Optional connect options like "+PACEAPI"
      let version_payload_full = if !connect_options.is_empty() { format!("{} {}", version_payload, connect_options) } else { version_payload };
      let version_bytes = version_payload_full.as_bytes();
      let version_len_u32 = version_bytes.len() as u32;

      let mut h1_message = Vec::with_capacity(api_prefix.len() + 4 + version_bytes.len());
      h1_message.extend_from_slice(api_prefix);
      h1_message.write_u32::<BigEndian>(version_len_u32).unwrap(); // Use byteorder
      h1_message.extend_from_slice(version_bytes);

      // Send H1
      stream.write_all(&h1_message).map_err(|e| IBKRError::SocketError(format!("Sending H1: {}", e)))?;
      stream.flush().map_err(|e| IBKRError::SocketError(format!("Flushing H1: {}", e)))?;
      info!("Sent Handshake H1 ({} bytes)", h1_message.len());

      // --- H2: Read Server Ack ---
      info!("Waiting for Handshake H2 (Server Ack)...");
      let h2_body = read_framed_message_body(&mut stream, timeout)?;
      info!("Received Handshake H2 body ({} bytes)", h2_body.len());

      let h2_parts: Vec<&[u8]> = h2_body.splitn(3, |&b| b == 0).collect(); // Split by null bytes
      if h2_parts.len() < 2 || h2_parts[0].is_empty() || h2_parts[1].is_empty() {
        return Err(IBKRError::ParseError(format!("Invalid H2 body format: {:02X?}", h2_body)));
      }
      let server_version_str = std::str::from_utf8(h2_parts[0]).map_err(|e| IBKRError::ParseError(format!("Invalid UTF8 server version: {}", e)))?;
      let server_version = server_version_str.parse::<i32>().map_err(|e| IBKRError::ParseError(format!("Parsing server version '{}': {}", server_version_str, e)))?;
      let connection_time = std::str::from_utf8(h2_parts[1]).map_err(|e| IBKRError::ParseError(format!("Invalid UTF8 connection time: {}", e)))?.to_string();
      info!("Parsed ServerVersion={}, ConnectionTime='{}'", server_version, connection_time);

      Ok((stream, server_version, connection_time))
    }


    // start_reader_thread remains the same
    fn start_reader_thread(
      &self,
      mut reader_stream: TcpStream,
      mut handler: MessageHandler,
      server_version: i32,
      logger_clone: Option<ConnectionLogger>,
    ) -> Result<(), IBKRError> {
      log::info!("Starting the reader thread.");

      let stop_flag_clone = self.stop_flag.clone();
      let reader_thread_handle_clone = self.reader_thread.clone();
      let thread_logger = logger_clone.clone();

      *self.stop_flag.lock() = false;

      let handle = thread::spawn(move || {
        debug!("Message reader thread started (Server Version: {})", server_version);
        let read_loop_timeout = Duration::from_secs(2);

        loop {
          if *stop_flag_clone.lock() {
            debug!("Reader thread received stop signal.");
            break;
          }

          match read_framed_message_body(&mut reader_stream, read_loop_timeout) {
            Ok(msg_data) => {
              if msg_data.is_empty() { continue; }

              // --- Log Received Message ---
              if let Some(logger) = &thread_logger {
                logger.log_message(LogDirection::Recv, &msg_data);
              }
              // --- End Logging ---

              if let Err(e) = process_message(&mut handler, &msg_data) {
                error!("Error processing message [{}]: {:?}", msg_to_string(&msg_data), e);
              }
            },
            Err(IBKRError::Timeout(_)) => {
              continue;
            }
            Err(IBKRError::ConnectionFailed(msg)) | Err(IBKRError::SocketError(msg)) => {
              if msg.contains("timed out") || msg.contains("reset") || msg.contains("broken pipe") || msg.contains("refused") || msg.contains("closed") || msg.contains("EOF") || msg.contains("network") || msg.contains("host is down") {
                error!("Connection lost/error in reader thread: {}", msg);
                handler.client.connection_closed(); // Notify handler
                break; // Exit loop
              } else {
                warn!("Unhandled socket/connection error in reader: {}", msg);
                thread_sleep(Duration::from_millis(100));
              }
            },
            Err(e) => {
              error!("Non-IO error in reader thread: {:?}", e);
              thread_sleep(Duration::from_millis(100));
            }
          }
        } // End loop

        // --- Cleanup ---
        info!("Message reader thread stopping cleanup...");
        *stop_flag_clone.lock() = true;

        *reader_thread_handle_clone.lock() = None;
        debug!("Message reader thread finished.");
      }); // End thread::spawn

      *self.reader_thread.lock() = Some(handle);
      Ok(())
    }
  } // end impl SocketConnection

  // --- Implement the Connection Trait ---
  impl Connection for SocketConnection {
    fn is_connected(&self) -> bool {
      let state = self.inner_state.lock();
      state.connected && !(*self.stop_flag.lock())
    }

    fn disconnect(&mut self) -> Result<(), IBKRError> {
      info!("Disconnecting from TWS...");
      // Log disconnect attempt? Maybe less useful than logging messages.
      let mut reader_handle_guard = self.reader_thread.lock();
      let mut state = self.inner_state.lock();

      let was_ever_partially_connected = state.stream.is_some();
      let is_reader_running = reader_handle_guard.is_some();

      if !was_ever_partially_connected && !is_reader_running && !*self.stop_flag.lock() {
        info!("Already disconnected (or never connected).");
        *self.stop_flag.lock() = true;
        return Ok(());
      }

      // 1. Signal reader
      *self.stop_flag.lock() = true;

      // 2. Shutdown socket
      if let Some(stream) = &mut state.stream {
        if let Err(e) = stream.shutdown(Shutdown::Both) {
          if e.kind() != ErrorKind::NotConnected && e.kind() != ErrorKind::BrokenPipe {
            warn!("Error shutting down socket during disconnect: {}", e);
          }
        }
      }

      // 3. Reset state
      state.connected = false;
      state.h3_sent = false;
      state.stream = None;
      state.handler = None;

      // 4. Take handle, drop locks
      let reader_handle = reader_handle_guard.take();
      drop(state);
      drop(reader_handle_guard);

      // 5. Join reader
      if let Some(handle) = reader_handle {
        debug!("Waiting for reader thread to join...");
        if let Err(e) = handle.join() {
          error!("Error joining reader thread: {:?}", e);
        } else {
          debug!("Reader thread joined successfully.");
        }
        *self.reader_thread.lock() = None;
      } else {
        debug!("No active reader thread to join.");
      }

      info!("Disconnected from TWS.");
      Ok(())
    }

    // Send is logged by MessageBroker wrapper now, no change here
    fn send_message_body(&mut self, data: &[u8]) -> Result<(), IBKRError> {
      let mut state = self.inner_state.lock();

      if !state.connected {
        return Err(IBKRError::NotConnected);
      }

      if let Some(stream) = &mut state.stream {
        write_framed_message(stream, data)
      } else {
        error!("Inconsistent state: connected=true but stream is None during send.");
        state.connected = false;
        state.h3_sent = false;
        *self.stop_flag.lock() = true;
        Err(IBKRError::InternalError("Connection state inconsistent".to_string()))
      }
    }


    // Instrument set_message_handler to send H3 and start reader
    fn set_message_handler(&mut self, handler: MessageHandler) {
      let mut state = self.inner_state.lock();

      if state.handler.is_some() {
        warn!("Replacing existing message handler.");
        // (... warning about active reader ...)
      }
      state.handler = Some(handler);

      let should_send_h3 = state.stream.is_some() && !state.h3_sent && self.reader_thread.lock().is_none();

      if should_send_h3 {
        info!("Message handler set. Sending StartAPI (H3) and starting reader thread...");

        // --- Send H3 (StartAPI) ---
        let capabilities = ""; // Optional capabilities string
        let h3_body_str = format!("71\02\0{}\0{}\0", self.client_id, capabilities);
        let h3_body_bytes = h3_body_str.as_bytes();

        // --- Log H3 Send ---
        // Clone logger before potentially moving it
        let logger_clone_for_h3 = self.logger.clone();
        if let Some(logger) = &logger_clone_for_h3 {
          logger.log_message(LogDirection::Send, h3_body_bytes);
        }
        // --- End Logging ---

        let stream = state.stream.as_mut().unwrap();
        if let Err(e) = write_framed_message(stream, h3_body_bytes) {
          error!("Failed to send StartAPI (H3): {:?}. Disconnecting.", e);
          drop(state);
          let _ = self.disconnect();
          return;
        }
        info!("Sent Handshake H3 body ({} bytes)", h3_body_bytes.len());
        state.h3_sent = true;

        // --- Start Reader Thread ---
        let reader_stream = match state.stream.as_ref().unwrap().try_clone() {
          Ok(s) => s,
          Err(e) => {
            error!("Failed to clone stream for reader thread: {:?}. Disconnecting.", e);
            state.h3_sent = false;
            drop(state);
            let _ = self.disconnect();
            return;
          }
        };
        let handler_for_thread = state.handler.take().unwrap();
        let server_version = state.server_version;

        state.connected = true;

        // Pass the logger clone to start_reader_thread
        // Clone self.logger here before dropping state
        let logger_clone_for_reader = self.logger.clone();

        drop(state); // Drop lock before spawning/calling disconnect

        // Call the helper, passing the logger
        if let Err(e) = self.start_reader_thread(reader_stream, handler_for_thread, server_version, logger_clone_for_reader) {
          error!("Failed to start reader thread: {:?}. Disconnecting.", e);
          let _ = self.disconnect(); // Disconnect handles state reset
        } else {
          info!("Reader thread started successfully. Connection fully active.");
        }
      } else if state.h3_sent {
        debug!("Handler set/replaced, but StartAPI already sent. Reader thread state unchanged.");
      } else {
        debug!("Handler set, but prerequisites for sending StartAPI not met (stream missing?).");
      }
    }


    fn get_server_version(&self) -> i32 {
      self.inner_state.lock().server_version
    }
  } // end impl Connection


  // --- Implement Drop ---
  impl Drop for SocketConnection {
    fn drop(&mut self) {
      // Use PLMutex guard here if stop_flag is PLMutex
      let needs_disconnect = {
        if *self.stop_flag.lock() {
          false
        } else {
          let state = self.inner_state.lock();
          // Check reader thread existence using PLMutex guard
          state.stream.is_some() || self.reader_thread.lock().is_some()
        }
      };

      if needs_disconnect {
        info!("Dropping SocketConnection, ensuring disconnect...");
        if let Err(e) = self.disconnect() {
          error!("Error during disconnect on drop: {:?}", e);
        }
      } else {
        // Only log if not already disconnected to avoid noise
        if self.inner_state.lock().stream.is_some() || self.reader_thread.lock().is_some() {
          info!("Dropping SocketConnection, already disconnected or stopping.");
        } else {
          info!("Dropping SocketConnection, already fully disconnected/never connected.");
        }
      }
    }
  }
} // end mod socket
