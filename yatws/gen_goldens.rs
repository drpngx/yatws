// gen_goldens.rs
// Use it like this:
// bazel-bin/yatws/gen_goldens live current-quote --generate-report
// bazel-bin/yatws/gen_goldens live all --generate-report
// Look for "Test registration" below for available test cases.

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use inventory; // For test registration
use log::{error, info, warn};
use once_cell::sync::Lazy; // For static registry initialization
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use chrono::{DateTime, Utc};
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
  start_time: DateTime<Utc>,
  end_time: DateTime<Utc>,
  total_duration: Duration,
  results: Vec<TestResult>,
  overall_success: bool,
  host: String,
  port: u16,
  client_id: i32,
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

  /// Generate a markdown test report and save to yatws/doc/test_results.md
  #[arg(long)]
  generate_report: bool,

  /// Custom path for the markdown report (overrides default yatws/doc/test_results.md)
  #[arg(long)]
  report_path: Option<PathBuf>,
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
inventory::submit! { TestDefinition { name: "options-strategy-builder", func: test_cases::options_strategy_builder_test_impl } }
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

// --- Main Execution Logic ---
fn main() -> Result<()> {
  env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
  // Initialize the registry by accessing it once (populates from inventory).
  Lazy::force(&TEST_REGISTRY);
  info!("Registered tests: {:?}", TEST_REGISTRY.keys().collect::<Vec<_>>());

  let args = Args::parse();
  let run_start_time = Utc::now();
  let run_start_instant = Instant::now();

  let (mode_args, is_live) = match args.command {
    Command::Live(m) => (m, true),
    Command::Replay(m) => (m, false),
  };
  let mode_str = if is_live { "Live" } else { "Replay" };

  let mut test_results = Vec::new();
  let mut overall_success = true;

  if mode_args.test_name_or_all.eq_ignore_ascii_case("all") {
    // --- Run ALL Tests Serially ---
    info!("Running ALL {} tests serially...", mode_str);
    let all_test_defs: Vec<&'static TestDefinition> = TEST_REGISTRY.values().cloned().collect(); // Get all registered tests

    if all_test_defs.is_empty() {
      warn!("No tests found in registry for 'all' run.");
    } else {
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
            let result = execute_test(test_def, &client, is_live);
            if !result.success {
              overall_success = false;
            }
            test_results.push(result);
          }
          Err(e) => {
            error!("Failed to create client for test '{}' ({}): {:#}", session_name, mode_str, e);
            overall_success = false;

            // Add a failed result for the client creation failure
            test_results.push(TestResult {
              name: session_name.to_string(),
              success: false,
              error_message: Some(format!("Client creation failed: {:#}", e)),
              duration: Duration::from_secs(0),
              timestamp: Utc::now(),
            });
          }
        }
        println!("----------------------------------------"); // Separator between tests
      }
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
          let result = execute_test(test_def, &client, is_live);
          if !result.success {
            overall_success = false;
          }
          test_results.push(result);
        }
        Err(e) => {
          error!("Failed to create client ({}): {:#}", mode_str, e);
          overall_success = false;

          // Add a failed result for the client creation failure
          test_results.push(TestResult {
            name: session_name.to_string(),
            success: false,
            error_message: Some(format!("Client creation failed: {:#}", e)),
            duration: Duration::from_secs(0),
            timestamp: Utc::now(),
          });
        }
      }
    } else {
      return Err(anyhow!(
        "Unknown test name specified: '{}'. Available tests: {:?}",
        requested_name, TEST_REGISTRY.keys().collect::<Vec<_>>()
      ));
    }
  }

  let run_end_time = Utc::now();
  let total_duration = run_start_instant.elapsed();

  info!("All specified operations finished.");

  // Generate markdown report if requested
  if mode_args.generate_report {
    let report = TestRunReport {
      mode: mode_str.to_string(),
      start_time: run_start_time,
      end_time: run_end_time,
      total_duration,
      results: test_results,
      overall_success,
      host: mode_args.host.clone(),
      port: mode_args.port,
      client_id: mode_args.client_id,
    };

    let report_path = mode_args.report_path.unwrap_or_else(|| {
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
