// yatws/tws_snoop.rs
use clap::Parser;
use log::{debug, error, info, warn};
use std::{
  io::{self, Read, Write, ErrorKind, Cursor},
  net::{TcpListener, TcpStream, Shutdown},
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
  thread,
  time::Duration,
};
use env_logger::Env;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

// --- Structs, Enums, main(), helpers (read_exact_timeout, read_framed_message_body, write_framed_message, log_message_body) remain the same ---
/// TWS API Minimal Proxy (Handshake Aware, Message Delimiting)
#[derive(Parser, Debug)]
#[clap(author, version, about = "Minimal TWS API Proxy")]
struct CliArgs {
  /// Address to listen on
  #[clap(long, default_value = "127.0.0.1:3002")]
  listen: String,

  /// Address of the TWS server
  #[clap(long, default_value = "127.0.0.1:4002")]
  server: String,

  /// Show raw message body data in hex
  #[clap(long)]
  hex: bool,

  /// Timeout for read operations in seconds
  #[clap(long, default_value = "10")]
  read_timeout: u64,
}

#[derive(Debug)]
enum ProxyError {
  Io(io::Error),
  Other(String),
}

impl From<io::Error> for ProxyError {
  fn from(e: io::Error) -> Self {
    ProxyError::Io(e)
  }
}

impl std::fmt::Display for ProxyError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ProxyError::Io(e) => write!(f, "IO Error: {}", e),
      ProxyError::Other(s) => write!(f, "Proxy Error: {}", s),
    }
  }
}

impl std::error::Error for ProxyError {}


fn main() -> Result<(), Box<dyn std::error::Error>> {
  env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
  let args = CliArgs::parse();

  info!("Starting Minimal TWS Proxy");
  info!("Listening on: {}", args.listen);
  info!("Forwarding to: {}", args.server);
  if args.hex { info!("Hex output enabled for message bodies"); }

  let listener = TcpListener::bind(&args.listen)?;

  for stream in listener.incoming() {
    match stream {
      Ok(client_stream) => {
        let client_addr = client_stream.peer_addr().map_or_else(
          |_| "unknown".to_string(), |a| a.to_string()
        );
        info!("[{}] Client connected", client_addr);

        let server_addr = args.server.clone();
        let show_hex = args.hex;
        let read_timeout = Duration::from_secs(args.read_timeout);

        thread::spawn(move || {
          info!("[{}] Handling connection", client_addr);
          if let Err(e) = handle_connection(
            client_stream,
            &server_addr,
            show_hex,
            read_timeout,
          ) {
            error!("[{}] Connection error: {}", client_addr, e);
          }
          info!("[{}] Connection handler finished.", client_addr);
        });
      }
      Err(e) => {
        error!("Error accepting client connection: {}", e);
      }
    }
  }

  Ok(())
}


fn read_exact_timeout(stream: &mut TcpStream, buf: &mut [u8], timeout: Duration) -> io::Result<()> {
  let deadline = std::time::Instant::now() + timeout;
  let mut bytes_read = 0;
  while bytes_read < buf.len() {
    let now = std::time::Instant::now();
    let remaining_timeout = match deadline.checked_duration_since(now) {
      Some(dur) if dur.is_zero() => return Err(io::Error::new(ErrorKind::TimedOut, "Read exact timed out (zero duration)")),
      Some(dur) => dur,
      None => return Err(io::Error::new(ErrorKind::TimedOut, "Read exact timed out (deadline passed)")),
    };

    stream.set_read_timeout(Some(remaining_timeout))?;

    match stream.read(&mut buf[bytes_read..]) {
      Ok(0) => return Err(io::Error::new(ErrorKind::UnexpectedEof, "Connection closed while reading exact")),
      Ok(n) => {
        bytes_read += n;
      }
      Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
        // Check deadline *after* WouldBlock before potentially retrying
        if std::time::Instant::now() >= deadline {
          return Err(io::Error::new(ErrorKind::TimedOut, "Read exact timed out (after WouldBlock)"));
        }
        // Consider a tiny sleep? Or just rely on the next loop iteration's timeout setting.
        // thread::sleep(Duration::from_millis(1));
      }
      Err(ref e) if e.kind() == ErrorKind::TimedOut => {
        // This might happen if the timeout set by set_read_timeout expires during the read() call
        return Err(io::Error::new(ErrorKind::TimedOut, "Read exact timed out (explicit TimedOut error)"));
      }
      Err(e) => return Err(e),
    }
  }
  // Set timeout back to None or original value if necessary
  // stream.set_read_timeout(None)?;
  Ok(())
}

fn read_framed_message_body(stream: &mut TcpStream, read_timeout: Duration) -> Result<Vec<u8>, ProxyError> {
  let mut size_buf = [0u8; 4];
  read_exact_timeout(stream, &mut size_buf, read_timeout).map_err(|e| {
    if e.kind() == ErrorKind::TimedOut {
      ProxyError::Io(io::Error::new(e.kind(), "Timed out reading message size header"))
    } else {
      ProxyError::Io(e)
    }
  })?;

  let size = Cursor::new(size_buf).read_u32::<BigEndian>()? as usize;
  if size == 0 { return Ok(Vec::new()); }

  const MAX_MSG_SIZE: usize = 10 * 1024 * 1024;
  if size > MAX_MSG_SIZE { return Err(ProxyError::Other(format!("Declared message size too large: {} bytes", size))); }

  let mut msg_buf = vec![0u8; size];
  read_exact_timeout(stream, &mut msg_buf, read_timeout).map_err(|e| {
    if e.kind() == ErrorKind::TimedOut {
      ProxyError::Io(io::Error::new(e.kind(), format!("Timed out reading message body ({} bytes)", size)))
    } else {
      ProxyError::Io(e)
    }
  })?;
  Ok(msg_buf)
}

fn write_framed_message(stream: &mut TcpStream, msg_body: &[u8]) -> Result<(), ProxyError> {
  let size = msg_body.len() as u32;
  let mut size_buf = [0u8; 4];
  Cursor::new(&mut size_buf[..]).write_u32::<BigEndian>(size)?;
  stream.write_all(&size_buf)?;
  if size > 0 { stream.write_all(msg_body)?; }
  stream.flush()?;
  Ok(())
}

fn log_message_body(direction: &str, body: &[u8], show_hex: bool) {
  if body.is_empty() { debug!("{} Sent/Received Empty Message", direction); return; }
  if show_hex {
    info!("{} Body ({} bytes): {:02X?}", direction, body.len(), body);
  } else {
    match std::str::from_utf8(body) {
      Ok(s) => info!("{} Body ({} bytes): {}", direction, body.len(), s.replace('\0', "|")),
      Err(_) => info!("{} Body ({} bytes): Non-UTF8/Binary Data", direction, body.len()),
    }
  }
}


fn handle_connection(
  mut client_stream: TcpStream,
  server_addr: &str,
  show_hex: bool,
  read_timeout: Duration,
) -> Result<(), ProxyError> {

  let client_addr = client_stream.peer_addr().map_or_else(|_| "?".to_string(), |a| a.to_string());
  let log_prefix = format!("[{}]", client_addr);

  // --- 1. Connect to TWS Server ---
  info!("{} Connecting to TWS server at {}", log_prefix, server_addr);
  let mut server_stream = match TcpStream::connect_timeout(&server_addr.parse().map_err(|e| ProxyError::Other(format!("Invalid server address: {}",e)))?, read_timeout) {
    Ok(stream) => { info!("{} Connected to TWS server", log_prefix); stream }
    Err(e) => { error!("{} Failed to connect to TWS server {}: {}", log_prefix, server_addr, e); let _ = client_stream.shutdown(Shutdown::Both); return Err(ProxyError::Io(e)); }
  };

  // --- 2. Set Timeouts ---
  // Timeouts are set within read_exact_timeout now. Keep overall stream timeout if needed for writes?
  // client_stream.set_read_timeout(Some(read_timeout))?;
  // server_stream.set_read_timeout(Some(read_timeout))?;
  client_stream.set_write_timeout(Some(read_timeout))?; // Add write timeout for safety
  server_stream.set_write_timeout(Some(read_timeout))?;

  // --- 3. Clone Streams ---
  let mut client_read = client_stream.try_clone()?;
  let mut client_write = client_stream;
  let mut server_read = server_stream.try_clone()?;
  let mut server_write = server_stream;

  // --- 4. Perform Handshake ---
  info!("{} Starting Handshake...", log_prefix);

  // Step H1: Client -> Server (API Version String - Explicit Read & Forward)
  let mut api_prefix_buf = [0u8; 4]; // To read "API\0"
  let mut size_buf = [0u8; 4];       // To read size header

  info!("{} Waiting for HANDSHAKE Init prefix (API\\0) from client...", log_prefix);
  read_exact_timeout(&mut client_read, &mut api_prefix_buf, read_timeout)
    .map_err(|e| ProxyError::Io(io::Error::new(e.kind(), format!("Reading API prefix from client: {}", e))))?;

  if &api_prefix_buf != b"API\0" {
    return Err(ProxyError::Other(format!("Invalid handshake prefix from client: {:02X?}", api_prefix_buf)));
  }
  info!("{} [C->S] Received HANDSHAKE Init prefix: API\\0", log_prefix);

  info!("{} Waiting for HANDSHAKE Init size from client...", log_prefix);
  read_exact_timeout(&mut client_read, &mut size_buf, read_timeout)
    .map_err(|e| ProxyError::Io(io::Error::new(e.kind(), format!("Reading API size from client: {}", e))))?;

  let version_size = Cursor::new(size_buf).read_u32::<BigEndian>()? as usize;
  info!("{} [C->S] Received HANDSHAKE Init size: {}", log_prefix, version_size);

  const MAX_VERSION_SIZE: usize = 128; // Sanity check
  if version_size > MAX_VERSION_SIZE {
    return Err(ProxyError::Other(format!("Invalid handshake version size from client: {}", version_size)));
  }
  if version_size == 0 {
    return Err(ProxyError::Other("Received zero size for version string from client".to_string()));
  }

  let mut version_buf = vec![0u8; version_size];
  info!("{} Waiting for HANDSHAKE Init version string ({} bytes) from client...", log_prefix, version_size);
  read_exact_timeout(&mut client_read, &mut version_buf, read_timeout)
    .map_err(|e| ProxyError::Io(io::Error::new(e.kind(), format!("Reading API version string from client: {}", e))))?;

  info!("{} [C->S] Received HANDSHAKE Init version string.", log_prefix);
  log_message_body(&format!("{} [C->S] HANDSHAKE Version String", log_prefix), &version_buf, show_hex);

  // Now forward the parts sequentially to the server
  server_write.write_all(&api_prefix_buf)?;
  server_write.write_all(&size_buf)?;
  server_write.write_all(&version_buf)?;
  server_write.flush()?; // Crucial: Flush after sending the complete H1 message
  info!("{} [C->S] Forwarded HANDSHAKE Init (Prefix + Size + Version)", log_prefix);

  // Step H2: Server -> Client (Server Version + Time - Framed)
  info!("{} Waiting for HANDSHAKE Ack from server...", log_prefix);
  let server_ack_body = read_framed_message_body(&mut server_read, read_timeout)?;
  info!("{} [S->C] Received HANDSHAKE Ack", log_prefix);
  log_message_body(&format!("{} [S->C] HANDSHAKE Ack Body", log_prefix), &server_ack_body, show_hex);
  write_framed_message(&mut client_write, &server_ack_body)?;
  info!("{} [S->C] Forwarded HANDSHAKE Ack", log_prefix);

  // Step H3: Client -> Server (StartAPI - Framed)
  info!("{} Waiting for HANDSHAKE StartAPI from client...", log_prefix);
  let start_api_body = read_framed_message_body(&mut client_read, read_timeout)?;
  info!("{} [C->S] Received HANDSHAKE StartAPI", log_prefix);
  log_message_body(&format!("{} [C->S] HANDSHAKE StartAPI Body", log_prefix), &start_api_body, show_hex);
  write_framed_message(&mut server_write, &start_api_body)?;
  info!("{} [C->S] Forwarded HANDSHAKE StartAPI", log_prefix);

  info!("{} Handshake Complete. Starting forwarding...", log_prefix);

  // --- 5. Start Forwarding Threads ---
  let stop_signal = Arc::new(AtomicBool::new(false));
  let c2s_handle = { /* ... same as before ... */
                            let stop = stop_signal.clone();
                            let prefix = format!("{} [C->S]", log_prefix);
                            thread::spawn(move || { run_forwarding_loop(&prefix, &mut client_read, &mut server_write, stop, show_hex, read_timeout) })
  };
  let s2c_handle = { /* ... same as before ... */
                            let stop = stop_signal.clone();
                            let prefix = format!("{} [S->C]", log_prefix);
                            thread::spawn(move || { run_forwarding_loop(&prefix, &mut server_read, &mut client_write, stop, show_hex, read_timeout) })
  };

  // --- 6. Wait for Forwarding to Finish ---
  let _ = c2s_handle.join().map_err(|e| ProxyError::Other(format!("C->S thread panic: {:?}", e)));
  info!("{} Client->Server forwarding loop finished.", log_prefix);
  let _ = s2c_handle.join().map_err(|e| ProxyError::Other(format!("S->C thread panic: {:?}", e)));
  info!("{} Server->Client forwarding loop finished.", log_prefix);

  Ok(())
}


fn run_forwarding_loop(
  log_prefix: &str,
  source: &mut TcpStream,
  dest: &mut TcpStream,
  stop_signal: Arc<AtomicBool>,
  show_hex: bool,
  read_timeout: Duration,
) {
  info!("{} Forwarding loop started.", log_prefix);
  loop {
    if stop_signal.load(Ordering::Relaxed) { info!("{} Stop signal received, exiting loop.", log_prefix); break; }

    match read_framed_message_body(source, read_timeout) {
      Ok(body) => {
        log_message_body(log_prefix, &body, show_hex);
        if let Err(e) = write_framed_message(dest, &body) {
          error!("{} Error writing to destination: {}. Exiting loop.", log_prefix, e);
          stop_signal.store(true, Ordering::Relaxed); break;
        }
      }
      Err(ProxyError::Io(e)) => {
        match e.kind() {
          ErrorKind::TimedOut | ErrorKind::WouldBlock => {
            if stop_signal.load(Ordering::Relaxed) { info!("{} Stop signal received after timeout, exiting loop.", log_prefix); break; }
            continue; // Normal timeout
          }
          ErrorKind::UnexpectedEof | ErrorKind::ConnectionReset | ErrorKind::BrokenPipe => {
            info!("{} Source connection closed/reset ({:?}). Exiting loop.", log_prefix, e.kind());
            stop_signal.store(true, Ordering::Relaxed); break;
          }
          _ => { error!("{} IO Error reading from source: {}. Exiting loop.", log_prefix, e); stop_signal.store(true, Ordering::Relaxed); break; }
        }
      }
      Err(e) => { error!("{} Error reading framed message: {}. Exiting loop.", log_prefix, e); stop_signal.store(true, Ordering::Relaxed); break; }
    }
  }
  stop_signal.store(true, Ordering::Relaxed);
  info!("{} Forwarding loop terminated.", log_prefix);
  if let Err(e) = dest.shutdown(Shutdown::Write) { if e.kind() != ErrorKind::NotConnected && e.kind() != ErrorKind::BrokenPipe { warn!("{} Error shutting down destination write stream: {}", log_prefix, e); } }
}
