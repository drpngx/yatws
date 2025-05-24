// gen_goldens.rs
// Use it like this:
// bazel-bin/yatws/gen_goldens live current-quote --generate-report
// bazel-bin/yatws/gen_goldens live all --generate-report
// bazel-bin/yatws/gen_goldens live all --randomize-order --generate-report --run-id integration_tests
// bazel-bin/yatws/gen_goldens replay all --run-id integration_tests
// bazel-bin/yatws/gen_goldens replay current-quote --run-id integration_tests
// bazel-bin/yatws/gen_goldens replay current-quote  (auto-discover)

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use inventory; // For test registration
use log::{error, info, warn};
use once_cell::sync::Lazy; // For static registry initialization
use rand::seq::SliceRandom;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use chrono::{DateTime, Utc};
use yatws::IBKRClient;

mod test_general;
mod test_account;
mod test_order;
mod test_fin;
mod test_market;
mod test_option;
mod test_news;
mod test_data_sub;
mod test_data_obs;
mod test_session_manager;

use test_session_manager::{TestSessionManager};
use yatws::{SessionStatus};

// --- Test Definition Infrastructure ---

// Define the signature for our test functions
type TestFn = fn(client: &IBKRClient, is_live: bool) -> Result<()>;

// Structure to hold test information collected by inventory
#[derive(Debug, Clone)]
pub struct TestDefinition {
  pub name: &'static str, // The canonical name used for sessions and CLI
  pub func: TestFn,
  pub order: u32, // Submission order for deterministic execution
}

// Macro to help with ordered submission
macro_rules! submit_test {
  ($order:expr, $name:expr, $func:expr) => {
    inventory::submit! {
      TestDefinition {
        name: $name,
        func: $func,
        order: $order,
      }
    }
  };
}

// Structure to track individual test results for reporting
#[derive(Debug, Clone)]
struct TestResult {
  name: String,
  success: bool,
  error_message: Option<String>,
  duration: Duration,
  timestamp: DateTime<Utc>,
}

// Structure to hold the complete test run results
#[derive(Debug)]
struct TestRunReport {
  mode: String, // "Live" or "Replay"
  run_id: String,
  start_time: DateTime<Utc>,
  end_time: DateTime<Utc>,
  total_duration: Duration,
  results: Vec<TestResult>,
  overall_success: bool,
  host: String,
  port: u16,
  client_id: i32,
  randomized_order: bool,
  single_client_mode: bool,
}

impl TestRunReport {
  fn generate_markdown(&self) -> String {
    let mut md = String::new();

    // Header
    md.push_str("# YATWS Test Results\n\n");

    // Summary
    let status_emoji = if self.overall_success { "✅" } else { "❌" };
    let status_text = if self.overall_success { "PASSED" } else { "FAILED" };

    md.push_str(&format!("## Test Run Summary {}\n\n", status_emoji));
    md.push_str(&format!("- **Overall Status**: {}\n", status_text));
    md.push_str(&format!("- **Mode**: {}\n", self.mode));
    md.push_str(&format!("- **Run ID**: {}\n", self.run_id));
    md.push_str(&format!("- **Client Mode**: {}\n", if self.single_client_mode { "Single Client" } else { "Multi Client" }));
    md.push_str(&format!("- **Test Order**: {}\n", if self.randomized_order { "Randomized" } else { "Sequential" }));
    md.push_str(&format!("- **Start Time**: {}\n", self.start_time.format("%Y-%m-%d %H:%M:%S UTC")));
    md.push_str(&format!("- **End Time**: {}\n", self.end_time.format("%Y-%m-%d %H:%M:%S UTC")));
    md.push_str(&format!("- **Total Duration**: {:.2}s\n", self.total_duration.as_secs_f64()));
    md.push_str(&format!("- **Connection**: {}:{} (Client ID: {})\n", self.host, self.port, self.client_id));

    let passed = self.results.iter().filter(|r| r.success).count();
    let failed = self.results.len() - passed;
    md.push_str(&format!("- **Tests Passed**: {}\n", passed));
    md.push_str(&format!("- **Tests Failed**: {}\n", failed));
    md.push_str(&format!("- **Total Tests**: {}\n\n", self.results.len()));

    // Detailed Results
    md.push_str("## Detailed Test Results\n\n");

    if self.results.is_empty() {
      md.push_str("No tests were executed.\n\n");
    } else {
      // Create a table
      md.push_str("| Test Name | Status | Duration | Error |\n");
      md.push_str("|-----------|--------|----------|-------|\n");

      for result in &self.results {
        let status_emoji = if result.success { "✅" } else { "❌" };
        let status_text = if result.success { "PASS" } else { "FAIL" };
        let duration = format!("{:.3}s", result.duration.as_secs_f64());
        let error = result.error_message.as_deref().unwrap_or("-");

        md.push_str(&format!(
          "| {} | {} {} | {} | {} |\n",
          result.name,
          status_emoji,
          status_text,
          duration,
          // Escape markdown special characters in error messages
          error.replace('|', "\\|").replace('\n', " ").chars().take(100).collect::<String>()
        ));
      }
      md.push_str("\n");
    }

    // Failed Tests Details (if any)
    let failed_tests: Vec<_> = self.results.iter().filter(|r| !r.success).collect();
    if !failed_tests.is_empty() {
      md.push_str("## Failed Tests Details\n\n");
      for result in failed_tests {
        md.push_str(&format!("### {} ❌\n\n", result.name));
        md.push_str(&format!("- **Duration**: {:.3}s\n", result.duration.as_secs_f64()));
        md.push_str(&format!("- **Timestamp**: {}\n", result.timestamp.format("%Y-%m-%d %H:%M:%S UTC")));
        if let Some(error) = &result.error_message {
          md.push_str("- **Error**:\n```\n");
          md.push_str(error);
          md.push_str("\n```\n\n");
        }
      }
    }

    // Footer
    md.push_str("---\n");
    md.push_str(&format!("*Report generated on {}*\n", Utc::now().format("%Y-%m-%d %H:%M:%S UTC")));

    md
  }

  fn write_to_file(&self, path: &PathBuf) -> Result<()> {
    let markdown = self.generate_markdown();

    // Ensure the directory exists
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create directory: {:?}", parent))?;
    }

    std::fs::write(path, markdown)
      .with_context(|| format!("Failed to write markdown report to: {:?}", path))?;

    info!("Markdown report written to: {:?}", path);
    Ok(())
  }
}

// Declare the static collection using inventory
inventory::collect!(TestDefinition);

// Create a Lazy HashMap for quick name lookup (used only for single test lookups)
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
  /// Show database statistics and available sessions.
  Stats(StatsArgs),
}

#[derive(Parser, Debug)]
struct ModeArgs {
  /// Test name to run (e.g., time, account-details, order) or "all".
  #[arg()]
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

  /// Run ID for grouping related tests (defaults to timestamp or test name)
  #[arg(long)]
  run_id: Option<String>,

  /// Generate a markdown test report and save to yatws/doc/test_results.md
  #[arg(long)]
  generate_report: bool,

  /// Custom path for the markdown report (overrides default yatws/doc/test_results.md)
  #[arg(long)]
  report_path: Option<PathBuf>,

  /// Randomize the order of tests when running "all" (useful for detecting test dependencies)
  #[arg(long)]
  randomize_order: bool,

  /// Use single client for all tests (default: true for live, false for replay)
  #[arg(long)]
  single_client: Option<bool>,

  /// Minimum interval between client creations in seconds (only for multi-client mode)
  #[arg(long, default_value_t = 10)]
  client_throttle_seconds: u64,
}

#[derive(Parser, Debug)]
struct StatsArgs {
  /// Path to the SQLite database.
  #[arg(long, default_value = "yatws_golden.db")]
  db_path: PathBuf,

  /// Show detailed information about a specific run
  #[arg(long)]
  run_id: Option<String>,
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
submit_test!(1, "time", test_cases::time_impl);
submit_test!(2, "account-details", test_cases::account_details_impl);
submit_test!(3, "order-market", test_cases::order_market_impl);
submit_test!(4, "order-limit", test_cases::order_limit_impl);
submit_test!(5, "order-many", test_cases::order_many_impl);
submit_test!(6, "current-quote", test_cases::current_quote_impl);
submit_test!(7, "realtime-data", test_cases::realtime_data_impl);
submit_test!(8, "realtime-bars-blocking", test_cases::realtime_bars_blocking_impl);
submit_test!(9, "market-data-blocking", test_cases::market_data_blocking_impl);
submit_test!(10, "tick-by-tick-blocking", test_cases::tick_by_tick_blocking_impl);
submit_test!(11, "market-depth-blocking", test_cases::market_depth_blocking_impl);
submit_test!(12, "historical-data", test_cases::historical_data_impl);
submit_test!(13, "cleanup-orders", test_cases::cleanup_orders_impl);
submit_test!(14, "order-global-cancel", test_cases::order_global_cancel_impl);
submit_test!(15, "order-exercise-option", test_cases::order_exercise_option_impl);
submit_test!(16, "order-what-if", test_cases::order_what_if_impl);
submit_test!(17, "box-spread-yield", test_cases::box_spread_yield_impl);
submit_test!(18, "financial-reports", test_cases::financial_reports_impl);
submit_test!(19, "historical-news", test_cases::historical_news_impl);
submit_test!(20, "scanner", test_cases::scanner_impl);
submit_test!(21, "option-calculations", test_cases::option_calculations_impl);
submit_test!(22, "histogram-data", test_cases::histogram_data_impl);
submit_test!(23, "historical-ticks", test_cases::historical_ticks_impl);
submit_test!(24, "historical-schedule", test_cases::historical_schedule_impl);
submit_test!(25, "account-subscription", test_cases::account_subscription_impl);
submit_test!(26, "observe-market-data", test_cases::observe_market_data_impl);
submit_test!(27, "subscribe-market-data", test_cases::subscribe_market_data_impl);
submit_test!(28, "subscribe-historical-combined", test_cases::subscribe_historical_combined_impl);
submit_test!(29, "subscribe-news-bulletins", test_cases::subscribe_news_bulletins_impl);
submit_test!(30, "subscribe-historical-news", test_cases::subscribe_historical_news_impl);
submit_test!(31, "subscribe-real-time-bars", test_cases::subscribe_real_time_bars_impl);
submit_test!(32, "subscribe-tick-by-tick", test_cases::subscribe_tick_by_tick_impl);
submit_test!(33, "subscribe-market-depth", test_cases::subscribe_market_depth_impl);
submit_test!(34, "observe-realtime-bars", test_cases::observe_realtime_bars_impl);
submit_test!(35, "observe-tick-by-tick", test_cases::observe_tick_by_tick_impl);
submit_test!(36, "observe-market-depth", test_cases::observe_market_depth_impl);
submit_test!(37, "observe-historical-data", test_cases::observe_historical_data_impl);
submit_test!(38, "observe-historical-ticks", test_cases::observe_historical_ticks_impl);
submit_test!(39, "multi-subscription-mixed", test_cases::multi_subscription_mixed_impl);
submit_test!(40, "options-strategy-builder", test_cases::options_strategy_builder_test_impl);

// --- Helper Functions ---

/// Executes a single test and returns the result
fn execute_test(test_def: &TestDefinition, client: &IBKRClient, is_live: bool) -> TestResult {
  let start_time = Instant::now();
  let timestamp = Utc::now();

  info!("Running test: {}", test_def.name);

  let result = (test_def.func)(client, is_live);
  let duration = start_time.elapsed();

  match result {
    Ok(()) => {
      info!("Test PASSED: {} (Duration: {:.3}s)", test_def.name, duration.as_secs_f64());
      TestResult {
        name: test_def.name.to_string(),
        success: true,
        error_message: None,
        duration,
        timestamp,
      }
    }
    Err(e) => {
      let error_msg = format!("{:#}", e);
      error!("Test FAILED: {} (Duration: {:.3}s): {}", test_def.name, duration.as_secs_f64(), error_msg);
      TestResult {
        name: test_def.name.to_string(),
        success: false,
        error_message: Some(error_msg),
        duration,
        timestamp,
      }
    }
  }
}

/// Handles client creation throttling for live connections
fn throttle_client_creation(last_client_creation: &mut Option<Instant>, throttle_duration: Duration, is_live: bool) {
  if !is_live {
    return;
  }

  if let Some(last_time) = *last_client_creation {
    let elapsed = last_time.elapsed();
    if elapsed < throttle_duration {
      let sleep_duration = throttle_duration - elapsed;
      info!(
        "Client throttling: waiting {:.1}s before creating next client (elapsed: {:.1}s, minimum interval: {:.0}s)",
        sleep_duration.as_secs_f64(),
        elapsed.as_secs_f64(),
        throttle_duration.as_secs_f64()
      );
      std::thread::sleep(sleep_duration);
    } else {
      info!(
        "Client throttling: sufficient time elapsed ({:.1}s >= {:.0}s), proceeding with client creation",
        elapsed.as_secs_f64(),
        throttle_duration.as_secs_f64()
      );
    }
  } else {
    info!("Client throttling: first client creation, no throttling needed");
  }
}

/// Generate a default run ID from timestamp
fn generate_default_run_id() -> String {
  Utc::now().format("%Y%m%d_%H%M%S").to_string()
}

/// Determine if single client mode should be used
fn should_use_single_client(is_live: bool, explicit_choice: Option<bool>, test_count: usize) -> bool {
  match explicit_choice {
    Some(choice) => choice,
    None => {
      // Default logic: single client for live runs with multiple tests, multi-client for replay
      if is_live {
        test_count > 1 // Use single client for multi-test live runs
      } else {
        false // Use multi-client for replay (each test gets its own session)
      }
    }
  }
}

// --- Run All Tests Implementation ---

fn run_all_tests_single_client(
  mode_args: &ModeArgs,
  is_live: bool,
  all_test_defs: &[&'static TestDefinition],
  run_id: &str,
) -> Result<Vec<TestResult>> {
  let mut test_results = Vec::new();

  if is_live {
    info!("=== SINGLE CLIENT MODE (Live) ===");
    info!("Creating single client for {} tests in run '{}'", all_test_defs.len(), run_id);

    // Create test session manager for the run
    let mut session_mgr = TestSessionManager::new(&mode_args.db_path.to_string_lossy(), run_id);

    // Create single client for entire run (handshake/preamble logged to parent session)
    let client = session_mgr.create_client(&mode_args.host, mode_args.port, mode_args.client_id)
      .context("Failed to create client for run")?;

    info!("Client created successfully, running {} tests sequentially", all_test_defs.len());

    // Run each test with the same client
    for (i, test_def) in all_test_defs.iter().enumerate() {
      info!("===== Test {}/{}: {} =====", i + 1, all_test_defs.len(), test_def.name);

      // Start test session (child session)
      session_mgr.start_test(test_def.name)
        .with_context(|| format!("Failed to start test session for '{}'", test_def.name))?;

      // Execute test with shared client
      let result = execute_test(test_def, &client, true);

      // End test session with status
      let status = if result.success { SessionStatus::Completed } else { SessionStatus::Failed };
      session_mgr.end_test(status)
        .with_context(|| format!("Failed to end test session for '{}'", test_def.name))?;

      test_results.push(result);

      // Reset client state for next test (if not the last test)
      if i < all_test_defs.len() - 1 {
        info!("Resetting client state for next test...");
        client.reset()
          .with_context(|| format!("Failed to reset client state after test '{}'", test_def.name))?;

        // Brief pause between tests
        std::thread::sleep(Duration::from_secs(1));
      }

      info!("----------------------------------------");
    }

    // Complete the run
    let run_status = if test_results.iter().all(|r| r.success) {
      SessionStatus::Completed
    } else {
      SessionStatus::Failed
    };
    session_mgr.complete_run(run_status)
      .context("Failed to complete run")?;

    info!("Single client run completed successfully");

  } else {
    info!("=== SINGLE CLIENT MODE (Replay) ===");
    info!("Replaying run '{}' with all completed tests", run_id);

    // Get available tests from the run
    let available_tests = TestSessionManager::get_available_tests(&mode_args.db_path, run_id)
      .with_context(|| format!("Failed to get available tests for run '{}'", run_id))?;

    if available_tests.is_empty() {
      warn!("No completed tests found for run '{}'", run_id);
      return Ok(test_results);
    }

    info!("Found {} completed tests in run '{}': {:?}", available_tests.len(), run_id, available_tests);

    // Create replay client for the entire run
    let replay_client = TestSessionManager::replay_latest_run(&mode_args.db_path, run_id)
      .with_context(|| format!("Failed to create replay client for run '{}'", run_id))?;

    // Run only the tests that are available in the recorded session
    for test_def in all_test_defs {
      if available_tests.contains(&test_def.name.to_string()) {
        info!("Replaying test: {}", test_def.name);
        let result = execute_test(test_def, &replay_client, false);
        test_results.push(result);
      } else {
        info!("Skipping test '{}' - not recorded in run '{}'", test_def.name, run_id);
      }
    }
  }

  Ok(test_results)
}

fn run_all_tests_multi_client(
  mode_args: &ModeArgs,
  is_live: bool,
  all_test_defs: &[&'static TestDefinition],
  run_id: &str,
) -> Result<Vec<TestResult>> {
  let mut test_results = Vec::new();

  // Client creation throttling state (only for live)
  let mut last_client_creation: Option<Instant> = None;
  let throttle_duration = Duration::from_secs(mode_args.client_throttle_seconds);

  if is_live && mode_args.client_throttle_seconds > 0 {
    info!("=== MULTI CLIENT MODE (Live) ===");
    info!("Client throttling enabled: minimum {}s interval between client creations", mode_args.client_throttle_seconds);
  } else {
    info!("=== MULTI CLIENT MODE ({}) ===", if is_live { "Live" } else { "Replay" });
  }

  for (i, test_def) in all_test_defs.iter().enumerate() {
    let session_name = if is_live {
      format!("{}_{}", run_id, test_def.name) // Unique session name for each test
    } else {
      test_def.name.to_string()
    };

    info!("===== Test {}/{}: {} =====", i + 1, all_test_defs.len(), test_def.name);

    // Apply client creation throttling for live connections
    throttle_client_creation(&mut last_client_creation, throttle_duration, is_live);

    let client_result = if is_live {
      // Create individual client for each test
      IBKRClient::new(&mode_args.host, mode_args.port, mode_args.client_id, Some((
        mode_args.db_path.to_string_lossy().to_string(),
        session_name
      )))
    } else {
      // Try to replay individual test session
      TestSessionManager::auto_replay_test(&mode_args.db_path, test_def.name)
    };

    match client_result {
      Ok(client) => {
        // Update throttling timestamp after successful client creation
        if is_live {
          last_client_creation = Some(Instant::now());
        }

        let result = execute_test(test_def, &client, is_live);
        test_results.push(result);
      }
      Err(e) => {
        error!("Failed to create client for test '{}': {:#}", test_def.name, e);

        // Add a failed result for the client creation failure
        test_results.push(TestResult {
          name: test_def.name.to_string(),
          success: false,
          error_message: Some(format!("Client creation failed: {:#}", e)),
          duration: Duration::from_secs(0),
          timestamp: Utc::now(),
        });
      }
    }

    info!("----------------------------------------");
  }

  Ok(test_results)
}

fn run_all_tests(mode_args: &ModeArgs, is_live: bool, run_id: &str) -> Result<Vec<TestResult>> {
  // Collect all tests and sort by order field to ensure deterministic execution
  let mut all_test_defs: Vec<&'static TestDefinition> = inventory::iter::<TestDefinition>().into_iter().collect();

  if mode_args.randomize_order {
    // Randomize the order
    let mut rng = rand::rng();
    all_test_defs.shuffle(&mut rng);
    info!("Test order randomized. Execution order:");
    for (i, test_def) in all_test_defs.iter().enumerate() {
      info!("  {}: {} (original order: {})", i + 1, test_def.name, test_def.order);
    }
  } else {
    // Sort by order field for deterministic execution
    all_test_defs.sort_by_key(|test_def| test_def.order);
    info!("Running tests in defined order (1-{}).", all_test_defs.len());
  }

  if all_test_defs.is_empty() {
    warn!("No tests found in registry for 'all' run.");
    return Ok(Vec::new());
  }

  // Determine client mode
  let single_client = should_use_single_client(is_live, mode_args.single_client, all_test_defs.len());

  info!("Run configuration:");
  info!("  Mode: {}", if is_live { "Live" } else { "Replay" });
  info!("  Run ID: {}", run_id);
  info!("  Client Mode: {}", if single_client { "Single Client" } else { "Multi Client" });
  info!("  Test Count: {}", all_test_defs.len());
  info!("  Test Order: {}", if mode_args.randomize_order { "Randomized" } else { "Sequential" });

  if single_client {
    run_all_tests_single_client(mode_args, is_live, &all_test_defs, run_id)
  } else {
    run_all_tests_multi_client(mode_args, is_live, &all_test_defs, run_id)
  }
}

fn run_single_test(mode_args: &ModeArgs, is_live: bool, test_name: &str) -> Result<TestResult> {
  // Look up the test definition by name
  let test_def = TEST_REGISTRY.get(test_name)
    .ok_or_else(|| anyhow!("Unknown test name: '{}'. Available tests: {:?}", test_name, TEST_REGISTRY.keys().collect::<Vec<_>>()))?;

  info!("===== Single Test: {} ({}) =====", test_name, if is_live { "Live" } else { "Replay" });

  let client_result = if is_live {
    // Use test name as session name for single test
    IBKRClient::new(&mode_args.host, mode_args.port, mode_args.client_id, Some((
      mode_args.db_path.to_string_lossy().to_string(),
      test_name.to_string()
    )))
  } else {
    // Try to replay the test
    TestSessionManager::auto_replay_test(&mode_args.db_path, test_name)
  };

  match client_result {
    Ok(client) => {
      let result = execute_test(test_def, &client, is_live);
      Ok(result)
    }
    Err(e) => {
      error!("Failed to create client for test '{}': {:#}", test_name, e);
      Ok(TestResult {
        name: test_name.to_string(),
        success: false,
        error_message: Some(format!("Client creation failed: {:#}", e)),
        duration: Duration::from_secs(0),
        timestamp: Utc::now(),
      })
    }
  }
}

// --- Stats Command Implementation ---

fn show_stats(args: &StatsArgs) -> Result<()> {
  info!("Database Statistics for: {:?}", args.db_path);

  if !args.db_path.exists() {
    return Err(anyhow!("Database file does not exist: {:?}", args.db_path));
  }

  // Get overall statistics
  let stats = TestSessionManager::get_session_stats(&args.db_path)
    .context("Failed to get session statistics")?;

  println!("\n=== YATWS Session Database Statistics ===");
  println!("Database: {:?}", args.db_path);
  println!("Total Runs: {}", stats.total_runs);
  println!("Total Completed Tests: {}", stats.total_completed_tests);
  println!("Total Failed Tests: {}", stats.total_failed_tests);
  println!("Unique Test Names: {}", stats.unique_test_names.len());

  if !stats.unique_test_names.is_empty() {
    println!("\nAvailable Test Types:");
    for (i, test_name) in stats.unique_test_names.iter().enumerate() {
      println!("  {}: {}", i + 1, test_name);
    }
  }

  // Show specific run details if requested
  if let Some(run_id) = &args.run_id {
    println!("\n=== Run Details: '{}' ===", run_id);

    match TestSessionManager::find_latest_run(&args.db_path, run_id) {
      Ok(Some(session_info)) => {
        println!("Run found:");
        println!("  ID: {}", session_info.id);
        println!("  Host: {}:{}", session_info.host, session_info.port);
        println!("  Client ID: {}", session_info.client_id);
        println!("  Created: {}", session_info.created_at);
        if let Some(completed) = session_info.completed_at {
          println!("  Completed: {}", completed);
        }
        println!("  Status: {}", session_info.status);
        if let Some(server_version) = session_info.server_version {
          println!("  Server Version: {}", server_version);
        }

        // Get completed tests for this run
        match TestSessionManager::get_available_tests(&args.db_path, run_id) {
          Ok(tests) => {
            if tests.is_empty() {
              println!("  Completed Tests: None");
            } else {
              println!("  Completed Tests ({}):", tests.len());
              for (i, test) in tests.iter().enumerate() {
                println!("    {}: {}", i + 1, test);
              }
            }
          }
          Err(e) => {
            warn!("Failed to get completed tests for run '{}': {}", run_id, e);
          }
        }
      }
      Ok(None) => {
        println!("Run '{}' not found.", run_id);
      }
      Err(e) => {
        return Err(anyhow!("Failed to query run '{}': {}", run_id, e));
      }
    }
  }

  println!();
  Ok(())
}

// --- Main Execution Logic ---
fn main() -> Result<()> {
  env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

  // Initialize the registry by accessing it once (populates from inventory).
  Lazy::force(&TEST_REGISTRY);
  info!("Registered tests: {:?}", TEST_REGISTRY.keys().collect::<Vec<_>>());

  let args = Args::parse();

  match args.command {
    Command::Stats(stats_args) => {
      show_stats(&stats_args)
    }
    Command::Live(ref mode_args) | Command::Replay(ref mode_args) => {
      let is_live = matches!(args.command, Command::Live(_));
      let mode_str = if is_live { "Live" } else { "Replay" };

      let run_start_time = Utc::now();
      let run_start_instant = Instant::now();

      let mut test_results = Vec::new();
      let mut overall_success = true;

      // Determine run ID
      let run_id = mode_args.run_id.clone().unwrap_or_else(|| {
        if mode_args.test_name_or_all.eq_ignore_ascii_case("all") {
          generate_default_run_id()
        } else {
          mode_args.test_name_or_all.clone()
        }
      });

      info!("Starting {} execution with run ID: '{}'", mode_str, run_id);

      if mode_args.test_name_or_all.eq_ignore_ascii_case("all") {
        // Run all tests
        match run_all_tests(&mode_args, is_live, &run_id) {
          Ok(results) => {
            overall_success = results.iter().all(|r| r.success);
            test_results = results;
          }
          Err(e) => {
            error!("Failed to run all tests: {:#}", e);
            overall_success = false;
          }
        }
      } else {
        // Run single test
        match run_single_test(&mode_args, is_live, &mode_args.test_name_or_all) {
          Ok(result) => {
            overall_success = result.success;
            test_results.push(result);
          }
          Err(e) => {
            error!("Failed to run single test: {:#}", e);
            overall_success = false;
          }
        }
      }

      let run_end_time = Utc::now();
      let total_duration = run_start_instant.elapsed();

      info!("All specified operations finished.");

      // Generate markdown report if requested
      if mode_args.generate_report {
        let single_client_mode = should_use_single_client(
          is_live,
          mode_args.single_client,
          if mode_args.test_name_or_all.eq_ignore_ascii_case("all") {
            inventory::iter::<TestDefinition>().count()
          } else { 1 }
        );

        let report = TestRunReport {
          mode: mode_str.to_string(),
          run_id: run_id.clone(),
          start_time: run_start_time,
          end_time: run_end_time,
          total_duration,
          results: test_results,
          overall_success,
          host: mode_args.host.clone(),
          port: mode_args.port,
          client_id: mode_args.client_id,
          randomized_order: mode_args.randomize_order && mode_args.test_name_or_all.eq_ignore_ascii_case("all"),
          single_client_mode,
        };

        let report_path = mode_args.report_path.clone().unwrap_or_else(|| {
          PathBuf::from("yatws/doc/test_results.md")
        });

        if let Err(e) = report.write_to_file(&report_path) {
          error!("Failed to write markdown report: {:#}", e);
          // Don't fail the entire run just because report writing failed
        }
      }

      if overall_success {
        info!("Overall Result: PASSED");
        Ok(())
      } else {
        Err(anyhow!("One or more tests FAILED."))
      }
    }
  }
}
