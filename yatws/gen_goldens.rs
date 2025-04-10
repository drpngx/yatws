// connect.rs

use yatws::conn::SocketConnection;
use yatws::conn::Connection;
use std::time::Duration;


fn main() {
  // Initialize logging
  env_logger::init();

  // Connection parameters
  let host = "127.0.0.1";
  let port = 4002;
  let client_id = 123;

  println!("Connecting to TWS at {}:{} with client ID {}", host, port, client_id);

  // Create a new connection
  let conn = SocketConnection::new(host, port, client_id).unwrap();

  println!("Server: {}", conn.get_server_version());
}
