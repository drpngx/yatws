// gen_goldens.rs
// Use it like this:
// bazel-bin/yatws/gen_goldens live current-quote
// Look for "Test registration" below for available test cases.

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use inventory; // For test registration
use log::{debug, error, info, warn};
use once_cell::sync::Lazy; // For static registry initialization
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use yatws::{
  IBKRError,
  client::IBKRClient,
  order::{OrderRequest, OrderSide, OrderType, TimeInForce, OrderStatus},
  OrderBuilder,
  contract::Contract,
};

// --- Test Definition Infrastructure ---

// Define the signature for our test functions
type TestFn = fn(client: &IBKRClient, is_live: bool) -> Result<()>;

// Structure to hold test information collected by inventory
#[derive(Debug, Clone)]
pub struct TestDefinition {
  pub name: &'static str, // The canonical name used for sessions and CLI
  pub func: TestFn,
}

// Declare the static collection using inventory
// All 'inventory::submit!(TestDefinition { ... });' instances will populate this.
inventory::collect!(TestDefinition);

// Create a Lazy HashMap for quick name lookup (optional but convenient)
static TEST_REGISTRY: Lazy<HashMap<&'static str, &'static TestDefinition>> = Lazy::new(|| {
  inventory::iter::<TestDefinition>
    .into_iter()
    .map(|test_def| (test_def.name, test_def))
    .collect()
});

// --- CLI Argument Parsing ---

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
  #[clap(subcommand)]
  command: Command,
}

#[derive(Parser, Debug)]
enum Command {
  /// Run tests live against TWS/Gateway, logging results.
  Live(ModeArgs),
  /// Replay tests from a previously logged database session.
  Replay(ModeArgs),
}

#[derive(Parser, Debug)]
struct ModeArgs {
  /// Test name to run (e.g., time, account-details, order) or "all".
  /// This name will also be used as the session name in the database.
  #[arg()] // Positional argument
  test_name_or_all: String,

  /// TWS/Gateway host address.
  #[arg(long, default_value = "127.0.0.1")]
  host: String,

  /// TWS/Gateway port.
  #[arg(long, default_value_t = 4002)]
  port: u16,

  /// Client ID for the connection.
  #[arg(long, default_value_t = 101)]
  client_id: i32,

  /// Path to the SQLite database for logging/replaying interactions.
  #[arg(long, default_value = "yatws_golden.db")]
  db_path: PathBuf,
}

// --- Test Case Implementations ---

mod test_cases {
  use super::*; // Bring in necessary types and functions

  // Each function implements a specific test scenario.
  // The name used for registration outside this module determines the CLI name and session name.

  pub(super) fn time_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Get Server Time ---");
    let cl = client.client();
    match cl.request_current_time() {
      Ok(time) => {
        info!("Successfully received server time: {:?}", time);
        Ok(())
      }
      Err(e) => {
        error!("Failed to get server time: {:?}", e);
        Err(e.into())
      }
    }
  }

  pub(super) fn account_details_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Account Details (Summary & Positions) ---");
    let acct_mgr = client.account();

    info!("Requesting full account refresh...");
    match acct_mgr.refresh() {
      Ok(_) => info!("Account refresh request completed."),
      Err(IBKRError::Timeout(msg)) => {
        warn!("Account refresh timed out: {}. Proceeding with potentially stale data.", msg);
      }
      Err(e) => {
        error!("Account refresh failed critically: {:?}", e);
        return Err(e.into());
      }
    }
    // Allow time for potential background updates or processing after refresh signal
    std::thread::sleep(Duration::from_millis(500));

    info!("Fetching account info...");
    match acct_mgr.get_account_info() {
      Ok(info) => {
        info!("Account Info: {:#?}", info);
      }
      Err(e) => {
        // Don't fail test if info fails but positions might work (e.g., after timeout)
        error!("Failed to get account info (might be ok if refresh timed out): {:?}", e);
      }
    }

    info!("Fetching open positions...");
    match acct_mgr.list_open_positions() {
      Ok(positions) => {
        if positions.is_empty() {
          info!("No open positions found.");
        } else {
          info!("Open Positions:");
          for pos in positions {
            info!(
              "  Symbol: {}, Qty: {}, AvgCost: {}, MktPx: {}, MktVal: {}, UnPNL: {}",
              pos.symbol,
              pos.quantity,
              pos.average_cost,
              pos.market_price,
              pos.market_value,
              pos.unrealized_pnl
            );
          }
        }
        Ok(()) // Test passes if positions are retrieved, even if info had issues
      }
      Err(e) => {
        error!("Failed to list open positions: {:?}", e);
        Err(e.into()) // Fail if listing positions fails
      }
    }
  }

  pub(super) fn order_market_impl(client: &IBKRClient, is_live: bool) -> Result<()> {
    info!("--- Testing Market Order ---");
    if is_live {
      warn!("Ensure market is open and liquid for SPY stock for this test to fill MKT orders quickly.");
      warn!("This test will BUY 1 share of SPY and then SELL it.");
      std::thread::sleep(Duration::from_secs(3)); // Give user time to read warning
    }

    let order_mgr = client.orders();
    let acct_mgr = client.account();

    let (contract, buy_request) = OrderBuilder::new(OrderSide::Buy, 1.0).market().for_stock("SPY").build()?;
    debug!("Request: {:?}", buy_request);
    let buy_order_id = order_mgr.place_order(contract.clone(), buy_request).context("Failed to place BUY order")?;
    info!("BUY order placed with {}", buy_order_id);
    // Wait for Buy Execution
    let wait_timeout = Duration::from_secs(20);
    info!("Waiting up to {:?} for BUY order {} execution...", wait_timeout, buy_order_id);
    match order_mgr.try_wait_order_executed(&buy_order_id, wait_timeout) {
      Ok(OrderStatus::Filled) => info!("BUY order {} filled.", buy_order_id),
      Ok(status) => {
        error!("BUY order {} did not fill as expected. Final status: {:?}", buy_order_id, status);
        attempt_cleanup(client, &contract)?; // Try cleanup even if buy failed
        return Err(anyhow!("BUY order {} did not fill. Status: {:?}", buy_order_id, status));
      }
      Err(e) => {
        error!("Error or timeout waiting for BUY order {}: {:?}", buy_order_id, e);
        attempt_cleanup(client, &contract)?; // Try cleanup even if buy failed
        return Err(e).context(format!("Waiting for BUY order {} failed", buy_order_id));
      }
    }

    // Verify Position Exists
    info!("Verifying position exists after BUY...");
    std::thread::sleep(Duration::from_secs(3)); // Allow account data propagation
    acct_mgr.refresh_positions().context("Failed to refresh positions after BUY")?;
    match acct_mgr.list_open_positions()?.iter().find(|p| p.contract.symbol == "SPY") {
      Some(pos) if (pos.quantity - 1.0).abs() < f64::EPSILON => info!("Verified position exists: SPY Quantity = {}", pos.quantity),
      Some(pos) => {
        error!("Position exists but quantity is unexpected after BUY: {}", pos.quantity);
        attempt_cleanup(client, &contract)?;
        return Err(anyhow!("Unexpected position quantity after BUY: {}", pos.quantity));
      }
      None => {
        error!("Position SPY not found after supposed BUY execution!");
        attempt_cleanup(client, &contract)?; // Still try cleanup? Maybe partial state exists.
        return Err(anyhow!("Position SPY not found after BUY execution"));
      }
    }

    // --- SELL Order ---
    info!("Placing SELL MKT order for 1 share to close position...");
    let (_, sell_request) = OrderBuilder::new(OrderSide::Sell, 1.0).market().for_stock("SPY").build()?;
    let sell_order_id = order_mgr.place_order(contract.clone(), sell_request).context("Failed to place SELL order")?;
    info!("SELL order placed with ID: {}", sell_order_id);

    // Wait for Sell Execution
    info!("Waiting up to {:?} for SELL order {} execution...", wait_timeout, sell_order_id);
    match order_mgr.try_wait_order_executed(&sell_order_id, wait_timeout) {
      Ok(OrderStatus::Filled) => info!("SELL order {} filled.", sell_order_id),
      Ok(status) => {
        // Don't fail immediately, check final position state
        warn!("SELL order {} reached status {:?}, not Filled. Checking final position state.", sell_order_id, status);
      }
      Err(e) => {
        // Don't fail immediately, check final position state
        warn!("Error or timeout waiting for SELL order {}: {:?}. Checking final position state.", sell_order_id, e);
      }
    }

    // Verify Position Closed
    info!("Verifying position closed after SELL...");
    std::thread::sleep(Duration::from_secs(3)); // Allow account data propagation
    if let Err(e) = acct_mgr.refresh_positions() {
      warn!("Failed to refresh positions after SELL: {:?}. Manual verification may be needed.", e);
      // Continue to check last known state from memory
    }
    match acct_mgr.list_open_positions()?.iter().find(|p| p.contract.symbol == "SPY") {
      Some(pos) if pos.quantity.abs() < f64::EPSILON => {
        info!("Verified position closed (Zero Quantity). Test successful.");
        Ok(()) // Success!
      }
      Some(pos) => {
        error!("Position SPY still exists after SELL order! Final Quantity = {}", pos.quantity);
        error!("Manual intervention likely required to close the SPY position.");
        Err(anyhow!("Position SPY not closed. Final Quantity: {}", pos.quantity))
      }
      None => {
        info!("Verified position closed (Not found in list). Test successful.");
        Ok(()) // Success!
      }
    }?;
    // Now print out the executions for the day.
    info!("Retrieving all executed trades.");
    for e in acct_mgr.get_day_executions()? {
      info!("  oid: {}, {} {} x {:.0} @ {:.2}: comm: {:.2}",
            e.order_id, e.side, e.symbol, e.quantity, e.price,
            e.commission.unwrap_or(0.0));
    }
    Ok(())
  }

  pub(super) fn order_many_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Sending Orders In Rapid Successiuon ---");
    let order_mgr = client.orders();
    let limit_price = 546.0;  // Slightly lower than ask.
    let mut oid = vec![];
    for k in 0..10 {
      let limit_price = limit_price + (k as f64) * 0.01;
      let (contract, buy_request) = OrderBuilder::new(OrderSide::Buy, 1.0).limit(limit_price).for_stock("SPY").build()?;
      debug!("Request[{}]: {:?}", k, buy_request);
      let buy_order_id = order_mgr.place_order(contract.clone(), buy_request).context("Failed to place BUY order")?;
      info!("BUY order placed with {}", buy_order_id);
      oid.push(buy_order_id);
    }

    let oid = oid;  // not mut.
    log::info!("Wait for orders submitted.");
    for o in &oid {
      match order_mgr.try_wait_order_submitted(o, Duration::from_secs(1)) {
        Ok(status) => {
          info!("BUY order {} status: {:?}", o, status);
        }
        Err(e) => {
          error!("Error or timeout waiting for BUY order {}: {:?}", o, e);
        }
      }
    }
    log::info!("Cancel orders");
    for o in &oid {
      order_mgr.cancel_order(o).unwrap_or_else(|e| { log::warn!("Failed to cancel order: {:?}", e); false });
      match order_mgr.try_wait_order_canceled(o, Duration::from_secs(3)) {
        Ok(status) => {
          info!("BUY order {} status: {:?}", o, status);
        }
        Err(e) => {
          error!("Error or timeout waiting for cancel order {}: {:?}", o, e);
        }
      }
    }
    log::info!("All done");
    Ok(())
  }

  pub(super) fn order_limit_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Limit Order ---");
    let order_mgr = client.orders();
    let limit_price = 548.0;  // Slightly lower than ask.
    let (contract, buy_request) = OrderBuilder::new(OrderSide::Buy, 1.0).limit(limit_price).for_stock("SPY").build()?;
    debug!("Request: {:?}", buy_request);
    let buy_order_id = order_mgr.place_order(contract.clone(), buy_request).context("Failed to place BUY order")?;
    info!("BUY order placed with {}, waiting for submit ack", buy_order_id);

    match order_mgr.try_wait_order_submitted(&buy_order_id, Duration::from_secs(1)) {
      Ok(status) => {
        info!("BUY order {} status: {:?}", buy_order_id, status);
      }
      Err(e) => {
        error!("Error or timeout waiting for BUY order {}: {:?}", buy_order_id, e);
      }
    }
    log::info!("Cancel order");
    order_mgr.cancel_order(&buy_order_id)?;
    match order_mgr.try_wait_order_canceled(&buy_order_id, Duration::from_secs(3)) {
      Ok(status) => {
        info!("BUY order {} status: {:?}", buy_order_id, status);
      }
      Err(e) => {
        error!("Error or timeout waiting for cancel order {}: {:?}", buy_order_id, e);
      }
    }
    log::info!("All done");
    Ok(())
  }

  pub(super) fn current_quote_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Get Current Quote ---");
    let data_mgr = client.data_market();
    let contract = Contract::stock("AAPL"); // Test with SPY stock
    let timeout = Duration::from_secs(10); // Set a reasonable timeout

    info!("Requesting quote for {} with timeout {:?}", contract.symbol, timeout);

    match data_mgr.get_quote(&contract, timeout) {
      Ok((bid, ask, last)) => {
        info!("Successfully received quote for {}:", contract.symbol);
        info!("  Bid:  {:?}", bid);
        info!("  Ask:  {:?}", ask);
        info!("  Last: {:?}", last);
        // Basic validation: Check if at least one price is received.
        // In replay mode, we might get None if the log doesn't contain the ticks.
        // In live mode, we expect some prices unless the market is closed/illiquid.
        if bid.is_none() && ask.is_none() && last.is_none() {
          warn!("Received quote, but all prices were None.");
          // Decide if this should be an error or just a warning.
          // For now, let's treat it as success as long as the call didn't error out.
        }
        Ok(())
      }
      Err(e) => {
        error!("Failed to get quote for {}: {:?}", contract.symbol, e);
        Err(e.into())
      }
    }
  }

  // --- Helper for Order Test ---
  fn attempt_cleanup(client: &IBKRClient, contract: &Contract) -> Result<()> {
    warn!("Attempting emergency cleanup for SPY position...");
    let order_mgr = client.orders();
    let acct_mgr = client.account();
    std::thread::sleep(Duration::from_secs(1)); // Small delay before check

    if let Err(e) = acct_mgr.refresh_positions() {
      error!("Cleanup: Failed to refresh positions: {:?}", e);
      // Don't return error, cleanup is best-effort
    }
    // Use last known state even if refresh failed
    let positions = match acct_mgr.list_open_positions() {
      Ok(p) => p,
      Err(e) => {
        error!("Cleanup: Failed to list positions: {:?}", e);
        return Ok(()); // Cannot proceed with cleanup if list fails
      }
    };

    if let Some(pos) = positions.iter().find(|p| p.contract.symbol == "SPY") {
      if pos.quantity.abs() > f64::EPSILON {
        warn!("Cleanup: Found SPY position with quantity {}. Placing MKT order to close.", pos.quantity);
        let side = if pos.quantity > 0.0 { OrderSide::Sell } else { OrderSide::Buy };
        let qty_to_close = pos.quantity.abs();

        let cleanup_request = OrderRequest {
          side, order_type: OrderType::Market, quantity: qty_to_close,
          time_in_force: TimeInForce::Day, transmit: true, ..Default::default()
        };
        match order_mgr.place_order(contract.clone(), cleanup_request) {
          Ok(cleanup_id) => {
            info!("Cleanup: Placed closing order {}. Execution not guaranteed.", cleanup_id);
          }
          Err(e) => error!("Cleanup: Failed to place closing order: {:?}", e),
        }
      } else {
        info!("Cleanup: Found SPY position with zero quantity.");
      }
    } else {
      info!("Cleanup: No SPY position found to clean up.");
    }
    Ok(())
  }
}

// --- Test Registration ---
// Associate canonical names with the implementation functions in the module.
inventory::submit! { TestDefinition { name: "time", func: test_cases::time_impl } }
inventory::submit! { TestDefinition { name: "account-details", func: test_cases::account_details_impl } }
inventory::submit! { TestDefinition { name: "order-market", func: test_cases::order_market_impl } }
inventory::submit! { TestDefinition { name: "order-limit", func: test_cases::order_limit_impl } }
inventory::submit! { TestDefinition { name: "order-many", func: test_cases::order_many_impl } }
inventory::submit! { TestDefinition { name: "current-quote", func: test_cases::current_quote_impl } }
// Add more tests here: inventory::submit! { TestDefinition { name: "new-test-name", func: test_cases::new_test_impl } }


// --- Helper Functions ---

/// Creates a client for live connection, ensuring DB directory exists.
fn create_live_client(args: &ModeArgs, session: &str) -> Result<IBKRClient> {
  info!(
    "Creating LIVE client for session '{}' (Host: {}:{}, ClientID: {})",
    session, args.host, args.port, args.client_id
  );
  info!("Logging to DB: {:?}", args.db_path);

  if let Some(parent) = args.db_path.parent() {
    std::fs::create_dir_all(parent)
      .with_context(|| format!("Failed to create directory for DB: {:?}", parent))?;
  }

  let log_config = Some((args.db_path.to_string_lossy().to_string(), session.to_string()));
  IBKRClient::new(&args.host, args.port, args.client_id, log_config)
    .context("Failed to create IBKRClient for live connection")
}

/// Creates a client for replay connection.
fn create_replay_client(args: &ModeArgs, session: &str) -> Result<IBKRClient> {
  info!(
    "Creating REPLAY client for session '{}' from DB: {:?}",
    session, args.db_path
  );
  if !args.db_path.exists() {
    return Err(anyhow!("Database path does not exist: {:?}", args.db_path));
  }
  IBKRClient::from_db(&args.db_path.to_string_lossy(), session)
    .context("Failed to create IBKRClient for replay")
}

// --- Main Execution Logic ---
fn main() -> Result<()> {
  env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
  // Initialize the registry by accessing it once (populates from inventory).
  Lazy::force(&TEST_REGISTRY);
  info!("Registered tests: {:?}", TEST_REGISTRY.keys().collect::<Vec<_>>());

  let args = Args::parse();
  let mut overall_success = true;

  let (mode_args, is_live) = match args.command {
    Command::Live(m) => (m, true),
    Command::Replay(m) => (m, false),
  };
  let mode_str = if is_live { "Live" } else { "Replay" };

  if mode_args.test_name_or_all.eq_ignore_ascii_case("all") {
    // --- Run ALL Tests Serially ---
    info!("Running ALL {} tests serially...", mode_str);
    let all_test_defs: Vec<&'static TestDefinition> = TEST_REGISTRY.values().cloned().collect(); // Get all registered tests

    if all_test_defs.is_empty() {
      warn!("No tests found in registry for 'all' run.");
      return Ok(());
    }

    for test_def in all_test_defs {
      let session_name = test_def.name; // Use the name from the definition
      info!("===== Preparing Test: {} ({}) =====", session_name, mode_str);

      let client_result = if is_live {
        create_live_client(&mode_args, session_name)
      } else {
        create_replay_client(&mode_args, session_name)
      };

      match client_result {
        Ok(client) => {
          // Call the function pointer from the definition
          if let Err(e) = (test_def.func)(&client, is_live) {
            error!("Test FAILED: {} ({}): {:#}", session_name, mode_str, e); // Use {:#} for detailed error
            overall_success = false;
          } else {
            info!("Test PASSED: {} ({}).", session_name, mode_str);
          }
        }
        Err(e) => {
          error!("Failed to create client for test '{}' ({}): {:#}", session_name, mode_str, e);
          overall_success = false; // Cannot run test if client fails
        }
      }
      println!("----------------------------------------"); // Separator between tests
    }
  } else {
    // --- Run Single Test ---
    let requested_name = &mode_args.test_name_or_all;
    // Look up the test definition by name using the map
    if let Some(test_def) = TEST_REGISTRY.get(requested_name.as_str()) {
      let session_name = test_def.name; // Use the canonical name from definition
      info!("===== Preparing Test: {} ({}) =====", session_name, mode_str);

      let client_result = if is_live {
        create_live_client(&mode_args, session_name)
      } else {
        create_replay_client(&mode_args, session_name)
      };

      match client_result {
        Ok(client) => {
          if let Err(e) = (test_def.func)(&client, is_live) {
            error!("Test FAILED ({}): {:#}", mode_str, e);
            overall_success = false;
          } else {
            info!("Test PASSED ({}).", mode_str);
          }
        }
        Err(e) => {
          error!("Failed to create client ({}): {:#}", mode_str, e);
          overall_success = false;
        }
      }
    } else {
      return Err(anyhow!(
        "Unknown test name specified: '{}'. Available tests: {:?}",
        requested_name, TEST_REGISTRY.keys().collect::<Vec<_>>()
      ));
    }
  }

  info!("All specified operations finished.");

  if overall_success {
    info!("Overall Result: PASSED");
    Ok(())
  } else {
    Err(anyhow!("One or more tests FAILED."))
  }
}
