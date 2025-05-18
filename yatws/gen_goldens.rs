// gen_goldens.rs
// Use it like this:
// bazel-bin/yatws/gen_goldens live current-quote
// Look for "Test registration" below for available test cases.

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use inventory; // For test registration
use log::{error, info, warn};
use once_cell::sync::Lazy; // For static registry initialization
use std::collections::HashMap;
use std::path::PathBuf;
use yatws::{
  IBKRClient,
  // data::{MarketDataType, TickType, FundamentalReportType, ParsedFundamentalData, TickOptionComputationData, GenericTickType, TickByTickRequestType, DurationUnit, TimePeriodUnit},
};

mod test_general;
mod test_account;
mod test_order;
mod test_fin;
mod test_market;
mod test_option;
mod test_news;
mod test_data_sub;
mod test_data_obs;

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
  pub(super) use crate::test_general::*;
  pub(super) use crate::test_account::*;
  pub(super) use crate::test_order::*;
  pub(super) use crate::test_fin::*;
  pub(super) use crate::test_market::*;
  pub(super) use crate::test_option::*;
  pub(super) use crate::test_news::*;
  pub(super) use crate::test_data_sub::*;
  pub(super) use crate::test_data_obs::*;
}

// --- Test Registration ---
// Associate canonical names with the implementation functions in the module.
inventory::submit! { TestDefinition { name: "time", func: test_cases::time_impl } }
inventory::submit! { TestDefinition { name: "account-details", func: test_cases::account_details_impl } }
inventory::submit! { TestDefinition { name: "order-market", func: test_cases::order_market_impl } }
inventory::submit! { TestDefinition { name: "order-limit", func: test_cases::order_limit_impl } }
inventory::submit! { TestDefinition { name: "order-many", func: test_cases::order_many_impl } }
inventory::submit! { TestDefinition { name: "current-quote", func: test_cases::current_quote_impl } }
inventory::submit! { TestDefinition { name: "realtime-data", func: test_cases::realtime_data_impl } }
inventory::submit! { TestDefinition { name: "realtime-bars-blocking", func: test_cases::realtime_bars_blocking_impl } }
inventory::submit! { TestDefinition { name: "market-data-blocking", func: test_cases::market_data_blocking_impl } }
inventory::submit! { TestDefinition { name: "tick-by-tick-blocking", func: test_cases::tick_by_tick_blocking_impl } }
inventory::submit! { TestDefinition { name: "market-depth-blocking", func: test_cases::market_depth_blocking_impl } }
inventory::submit! { TestDefinition { name: "historical-data", func: test_cases::historical_data_impl } }
inventory::submit! { TestDefinition { name: "cleanup-orders", func: test_cases::cleanup_orders_impl } }
inventory::submit! { TestDefinition { name: "order-global-cancel", func: test_cases::order_global_cancel_impl } }
inventory::submit! { TestDefinition { name: "order-exercise-option", func: test_cases::order_exercise_option_impl } }
inventory::submit! { TestDefinition { name: "order-what-if", func: test_cases::order_what_if_impl } }
inventory::submit! { TestDefinition { name: "box-spread-yield", func: test_cases::box_spread_yield_impl } }
inventory::submit! { TestDefinition { name: "financial-reports", func: test_cases::financial_reports_impl } }
inventory::submit! { TestDefinition { name: "historical-news", func: test_cases::historical_news_impl } }
inventory::submit! { TestDefinition { name: "scanner", func: test_cases::scanner_impl } }
inventory::submit! { TestDefinition { name: "option-calculations", func: test_cases::option_calculations_impl } }
inventory::submit! { TestDefinition { name: "histogram-data", func: test_cases::histogram_data_impl } }
inventory::submit! { TestDefinition { name: "historical-ticks", func: test_cases::historical_ticks_impl } }
inventory::submit! { TestDefinition { name: "historical-schedule", func: test_cases::historical_schedule_impl } }
inventory::submit! { TestDefinition { name: "account-subscription", func: test_cases::account_subscription_impl } }
// Add new test registrations here:
inventory::submit! { TestDefinition { name: "observe-market-data", func: test_cases::observe_market_data_impl } }
inventory::submit! { TestDefinition { name: "subscribe-market-data", func: test_cases::subscribe_market_data_impl } }
inventory::submit! { TestDefinition { name: "subscribe-historical-combined", func: test_cases::subscribe_historical_combined_impl } }
inventory::submit! { TestDefinition { name: "subscribe-news-bulletins", func: test_cases::subscribe_news_bulletins_impl } }
inventory::submit! { TestDefinition { name: "subscribe-historical-news", func: test_cases::subscribe_historical_news_impl } }
inventory::submit! { TestDefinition { name: "subscribe-real-time-bars", func: test_cases::subscribe_real_time_bars_impl } }
inventory::submit! { TestDefinition { name: "subscribe-tick-by-tick", func: test_cases::subscribe_tick_by_tick_impl } }
inventory::submit! { TestDefinition { name: "subscribe-market-depth", func: test_cases::subscribe_market_depth_impl } }
inventory::submit! { TestDefinition { name: "observe-realtime-bars", func: test_cases::observe_realtime_bars_impl } }
inventory::submit! { TestDefinition { name: "observe-tick-by-tick", func: test_cases::observe_tick_by_tick_impl } }
inventory::submit! { TestDefinition { name: "observe-market-depth", func: test_cases::observe_market_depth_impl } }
inventory::submit! { TestDefinition { name: "observe-historical-data", func: test_cases::observe_historical_data_impl } }
inventory::submit! { TestDefinition { name: "observe-historical-ticks", func: test_cases::observe_historical_ticks_impl } }
inventory::submit! { TestDefinition { name: "multi-subscription-mixed", func: test_cases::multi_subscription_mixed_impl } }
// Add more tests here: inventory::submit! { TestDefinition { name: "new-test-name", func: test_cases::new_test_impl } }
// inventory::submit! { TestDefinition { name: "wsh-events", func: test_cases::wsh_events_impl } }


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
