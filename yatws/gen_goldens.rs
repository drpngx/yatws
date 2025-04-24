// connect.rs

use yatws::conn::SocketConnection;
use yatws::conn::Connection;
use std::time::Duration;
use yatws::client::IBKRClient;


fn _main() {
  // Initialize logging
  env_logger::init();

  // Connection parameters
  let host = "127.0.0.1";
  let port = 4002;
  let client_id = 123;

  println!("Connecting to TWS at {}:{} with client ID {}", host, port, client_id);

  // Create a new connection
  let conn = SocketConnection::new(host, port, client_id, None).unwrap();

  println!("Server: {}", conn.get_server_version());
}

fn main() {
  // Initialize logging
  env_logger::init();

  // Connection parameters
  let host = "127.0.0.1";
  let port = 4002;
  // let port = 3002;  // the snooper
  let client_id = 123;

  println!("Connecting to TWS at {}:{} with client ID {}", host, port, client_id);
  let log_config = ("yatws/testdata/conn_log.db".to_string(), "connect".to_string());

  // Create a new connection
  //let client = IBKRClient::new(host, port, client_id, Some(log_config)).unwrap();
  let client = IBKRClient::from_db(&log_config.0, &log_config.1).unwrap();

  // let acct = client.account();
  // acct.refresh().unwrap();
  // log::info!("Refresh requested");
  // std::thread::sleep(Duration::from_millis(3000));
  // println!("Value: {:?}", acct.get_account_info());
  let cl = client.client();
  println!("Time: {:?}", cl.request_current_time());
}
