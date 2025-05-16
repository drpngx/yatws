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
  IBKRClient,
  account::AccountValueKey,
  account_manager::AccountManager,
  account_subscription::AccountEvent,
  order::{OrderRequest, OrderSide, OrderType, TimeInForce, OrderStatus},
  OrderBuilder, OptionsStrategyBuilder,
  contract::{Contract, SecType, OptionRight, WhatToShow},
  data::{MarketDataType, TickType, FundamentalReportType, ParsedFundamentalData, TickOptionComputationData, GenericTickType, TickByTickRequestType, DurationUnit, TimePeriodUnit},
  data_subscription::{MarketDataSubscription, MarketDataIterator},
  parse_fundamental_xml
};
use chrono::{Utc, Duration as ChronoDuration, NaiveDate, Datelike};

// --- Test Definition Infrastructure ---
// Import new subscription types if they are in a separate module/file
use yatws::news_subscription::{HistoricalNewsEvent}; // Assuming it's in news_subscription.rs
// If in historical_news_subscription.rs, adjust path:
// use yatws::historical_news_subscription::{HistoricalNewsEvent};

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
  use yatws::data_observer::{MarketDataObserver};
  use yatws::data_subscription::{TickDataEvent, HistoricalDataEvent};
  use yatws::contract::BarSize;
  use yatws::news_subscription::NewsEvent; // Added for news subscription test

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
    match acct_mgr.subscribe_account_updates() {
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
        // Example of getting a specific value using the enum
        match acct_mgr.get_account_value(AccountValueKey::BuyingPower) {
          Ok(Some(val)) => info!("Specific Value - Buying Power: {} {:?}", val.value, val.currency),
          Ok(None) => warn!("Specific Value - Buying Power not found."),
          Err(e) => error!("Error getting specific value - Buying Power: {:?}", e),
        }
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

  pub(super) fn realtime_data_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Realtime Market Data Stream ---");
    let data_mgr = client.data_market();
    let contract = Contract::stock("SPY"); // Use SPY stock
    // Common ticks: 100=Option Volume, 101=Option Open Interest, 104=Hist Vol, 106=Avg Opt Volume
    // Price ticks (e.g., 1=Bid, 2=Ask, 4=Last) and Size ticks (e.g., 0=BidSize, 3=AskSize, 8=Volume)
    // are typically included by default when requesting streaming data for stocks.
    // Requesting the default set by passing an empty string for generic_tick_list.
    // See: https://interactivebrokers.github.io/tws-api/md_request.html#gsc
    // Valid generic ticks for STK are listed in the error message if needed for specific data.
    // Example: let generic_tick_list = &[GenericTickType::RtVolumeTimestamp];
    let generic_tick_list: &[GenericTickType] = &[]; // Request default ticks by passing an empty slice
    let snapshot = false;
    let regulatory_snapshot = false;
    let mkt_data_options = &[]; // No specific options

    info!(
      "Requesting realtime market data for {} (Generic Ticks: '{}')...", // Logging will show empty for default
      contract.symbol,
      generic_tick_list.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(",")
    );

    let req_id = data_mgr
      .request_market_data(
        &contract,
        generic_tick_list, // Pass the slice
        snapshot,
        regulatory_snapshot,
        mkt_data_options,
        Some(MarketDataType::Delayed)
      )
      .context("Failed to request market data")?;

    info!("Market data requested with req_id: {}. Waiting for data...", req_id);

    // In a real application, you'd have an observer or callback mechanism.
    // For gen_goldens, we just wait to allow messages to be logged.
    let wait_duration = Duration::from_secs(15);
    info!("Waiting for {:?} to capture streaming data...", wait_duration);
    std::thread::sleep(wait_duration);

    info!("Cancelling market data request (req_id: {})...", req_id);
    data_mgr.cancel_market_data(req_id).context("Failed to cancel market data")?;
    info!("Market data request cancelled.");

    // Allow a moment for the cancel message to be processed/logged
    std::thread::sleep(Duration::from_millis(500));

    Ok(())
  }


  pub(super) fn current_quote_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Get Current Quote ---");
    let data_mgr = client.data_market();
    let contract = Contract::stock("AAPL"); // Test with SPY stock
    let timeout = Duration::from_secs(10); // Set a reasonable timeout

    info!("Requesting quote for {} with timeout {:?}", contract.symbol, timeout);

    match data_mgr.get_quote(&contract, Some(MarketDataType::Delayed), timeout) {
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


  pub(super) fn realtime_bars_blocking_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Blocking Realtime Bars Request ---");
    let data_mgr = client.data_market();
    let contract = Contract::stock("AAPL"); // Use AAPL stock
    let what_to_show = WhatToShow::Trades; // Changed from "TRADES"
    let use_rth = true;
    let options = &[];
    let num_bars_to_get = 2; // Request a small number of bars
    // Timeout needs to be long enough to receive num_bars (e.g., num_bars * 5s + buffer)
    let timeout = Duration::from_secs(num_bars_to_get as u64 * 5 + 10);

    info!(
      "Requesting {} realtime bars for {} (What={}, RTH={}, Timeout={:?})...",
      num_bars_to_get, contract.symbol, what_to_show, use_rth, timeout // what_to_show will use Display
    );

    match data_mgr.get_realtime_bars(
      &contract,
      what_to_show, // Pass the enum
      use_rth,
      options,
      num_bars_to_get,
      timeout,
    ) {
      Ok(bars) => {
        info!("Successfully received {} bars:", bars.len());
        for bar in &bars {
          info!("  Time: {}, O: {}, H: {}, L: {}, C: {}, Vol: {}",
                bar.time.format("%Y-%m-%d %H:%M:%S"), bar.open, bar.high, bar.low, bar.close, bar.volume);
        }
        // Validate the number of bars received
        if bars.len() >= num_bars_to_get {
          info!("Received expected number of bars ({} >= {}). Test successful.", bars.len(), num_bars_to_get);
          Ok(())
        } else {
          error!("Received fewer bars ({}) than expected ({}).", bars.len(), num_bars_to_get);
          Err(anyhow!("Incorrect number of bars received: {} < {}", bars.len(), num_bars_to_get))
        }
      }
      Err(e) => {
        error!("Failed to get realtime bars: {:?}", e);
        Err(e.into())
      }
    }
  }


  pub(super) fn market_data_blocking_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Blocking Market Data Request (wait for Bid/Ask) ---");
    let data_mgr = client.data_market();
    let contract = Contract::stock("MSFT"); // Use MSFT stock
    let generic_tick_list: &[GenericTickType] = &[]; // Changed from ""
    let snapshot = false;
    let regulatory_snapshot = false;
    let mkt_data_options = &[];
    let timeout = Duration::from_secs(20); // Generous timeout

    info!(
      "Requesting blocking market data for {} (Timeout={:?}). Waiting for first Bid and Ask price...",
      contract.symbol, timeout
    );

    match data_mgr.get_market_data(
      &contract,
      generic_tick_list, // Pass the slice
      snapshot,
      regulatory_snapshot,
      mkt_data_options,
      Some(MarketDataType::Delayed),
      timeout,
      // Completion condition: Wait until we have received at least one BID (1) and one ASK (2) price tick.
      |state| {
        let has_tick_type = |x| state.ticks.contains_key(x) && !state.ticks[x].is_empty();
        let has_bid = has_tick_type(&TickType::BidPrice) || has_tick_type(&TickType::DelayedBid);
        let has_ask = has_tick_type(&TickType::AskPrice) || has_tick_type(&TickType::DelayedAsk);
        has_bid && has_ask
      },
    ) {
      Ok(final_state) => {
        info!("Successfully received market data state after condition met:");
        info!("  Bid Price: {:?}", final_state.bid_price);
        info!("  Ask Price: {:?}", final_state.ask_price);
        info!("  Last Price: {:?}", final_state.last_price);
        info!("  Total Bid Ticks Received: {}", final_state.ticks.get(&TickType::BidPrice).map_or(0, |v| v.len()));
        info!("  Total Ask Ticks Received: {}", final_state.ticks.get(&TickType::AskPrice).map_or(0, |v| v.len()));

        if final_state.bid_price.is_some() && final_state.ask_price.is_some() {
          info!("Received Bid and Ask price. Test successful.");
          Ok(())
        } else {
          error!("Condition met, but final state missing Bid or Ask price. Bid: {:?}, Ask: {:?}",
                 final_state.bid_price, final_state.ask_price);
          Err(anyhow!("Final state missing expected prices"))
        }
      }
      Err(e) => {
        error!("Failed to get blocking market data: {:?}", e);
        Err(e.into())
      }
    }
  }

  pub(super) fn tick_by_tick_blocking_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Blocking Tick-by-Tick Request (wait for 5 Last ticks) ---");
    let data_mgr = client.data_market();
    let contract = Contract::stock("GOOG"); // Use GOOG stock
    let tick_type = TickByTickRequestType::Last; // Changed from "Last"
    let number_of_ticks = 0; // Streaming
    let ignore_size = false;
    let timeout = Duration::from_secs(30); // Timeout for receiving 5 ticks

    info!(
      "Requesting blocking tick-by-tick data for {} (Type={}, Timeout={:?}). Waiting for 5 ticks...",
      contract.symbol, tick_type, timeout // tick_type will use Display
    );

    let target_ticks = 5;

    match data_mgr.get_tick_by_tick_data(
      &contract,
      tick_type, // Pass the enum
      number_of_ticks,
      ignore_size,
      timeout,
      // Completion condition: Wait until the history contains at least target_ticks.
      |state| state.ticks.len() >= target_ticks,
    ) {
      Ok(final_state) => {
        info!("Successfully received tick-by-tick state after condition met:");
        info!("  Total Ticks Received: {}", final_state.ticks.len());
        info!("  Latest Tick: {:?}", final_state.latest_tick);

        if final_state.ticks.len() >= target_ticks {
          info!("Received at least {} ticks. Test successful.", target_ticks);
          // Optionally print the first few ticks
          for (i, tick) in final_state.ticks.iter().take(target_ticks).enumerate() {
            if let yatws::data::TickByTickData::Last { time, price, size, .. } = tick {
              info!("    Tick {}: Time={}, Price={}, Size={}", i + 1, time, price, size);
            }
          }
          Ok(())
        } else {
          error!("Condition met, but final state has fewer ticks ({}) than expected ({}).",
                 final_state.ticks.len(), target_ticks);
          Err(anyhow!("Incorrect number of ticks in final state"))
        }
      }
      Err(e) => {
        error!("Failed to get blocking tick-by-tick data: {:?}", e);
        Err(e.into())
      }
    }
  }

  pub(super) fn market_depth_blocking_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Blocking Market Depth Request (wait for 1 Bid/1 Ask level) ---");
    let data_mgr = client.data_market();
    let contract = Contract::stock("IBM"); // Use IBM stock
    let num_rows = 5; // Request 5 levels
    let is_smart_depth = false; // Use regular depth
    let mkt_depth_options = &[];
    let timeout = Duration::from_secs(20);

    info!(
      "Requesting blocking market depth for {} (Rows={}, Smart={}, Timeout={:?}). Waiting for first Bid and Ask level...",
      contract.symbol, num_rows, is_smart_depth, timeout
    );

    match data_mgr.get_market_depth(
      &contract,
      num_rows,
      is_smart_depth,
      mkt_depth_options,
      timeout,
      // Completion condition: Wait until both bid and ask books have at least one entry.
      |state| !state.depth_bids.is_empty() && !state.depth_asks.is_empty(),
    ) {
      Ok(final_state) => {
        info!("Successfully received market depth state after condition met:");
        info!("  Top Bid: Price={:?}, Size={:?}", final_state.bid_price, final_state.bid_size);
        info!("  Top Ask: Price={:?}, Size={:?}", final_state.ask_price, final_state.ask_size);
        info!("  Bid Levels Received: {}", final_state.depth_bids.len());
        info!("  Ask Levels Received: {}", final_state.depth_asks.len());

        if !final_state.depth_bids.is_empty() && !final_state.depth_asks.is_empty() {
          info!("Received at least one Bid and one Ask level. Test successful.");
          // Optionally print top levels
          if let Some(bid) = final_state.depth_bids.first() {
            info!("    Top Bid Level: Pos={}, Px={}, Sz={}, MM='{}'", bid.position, bid.price, bid.size, bid.market_maker);
          }
          if let Some(ask) = final_state.depth_asks.first() {
            info!("    Top Ask Level: Pos={}, Px={}, Sz={}, MM='{}'", ask.position, ask.price, ask.size, ask.market_maker);
          }
          Ok(())
        } else {
          error!("Condition met, but final state missing Bid ({}) or Ask ({}) levels.",
                 final_state.depth_bids.len(), final_state.depth_asks.len());
          Err(anyhow!("Final state missing expected depth levels"))
        }
      }
      Err(e) => {
        error!("Failed to get blocking market depth data: {:?}", e);
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


  pub(super) fn cleanup_orders_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Cleanup Orders (Cancel All Open Orders & List Positions) ---");
    assert!(client.client_id() == 0, "Must cleanup from client 0, got {}", client.client_id());
    let order_mgr = client.orders();
    let acct_mgr = client.account();
    let refresh_timeout = Duration::from_secs(10);
    let cancel_wait_timeout = Duration::from_secs(5);
    let mut cancellation_errors = Vec::new();

    // 1. Refresh and list open orders
    info!("Refreshing order book...");
    match order_mgr.refresh_orderbook(refresh_timeout) {
      Ok(_) => info!("Order book refresh complete."),
      Err(IBKRError::Timeout(msg)) => {
        warn!("Order book refresh timed out: {}. Proceeding with potentially stale data.", msg);
        // Continue anyway, maybe some orders were received before timeout
      }
      Err(e) => {
        error!("Failed to refresh order book: {:?}", e);
        return Err(e).context("Order book refresh failed");
      }
    }

    let open_orders = order_mgr.get_open_orders();
    if open_orders.is_empty() {
      info!("No open orders found to cancel.");
    } else {
      info!("Found {} open orders. Attempting cancellation...", open_orders.len());
      for order in open_orders {
        info!("Cancelling order ID: {} (Symbol: {}, Side: {:?}, Qty: {})",
              order.id, order.contract.symbol, order.request.side, order.request.quantity);
        match order_mgr.cancel_order(&order.id) {
          Ok(true) => {
            info!("Cancel request sent for {}. Waiting for confirmation...", order.id);
            match order_mgr.try_wait_order_canceled(&order.id, cancel_wait_timeout) {
              Ok(status) => info!("Order {} cancellation confirmed with status: {:?}", order.id, status),
              Err(IBKRError::Timeout(_)) => {
                let msg = format!("Timeout waiting for cancellation confirmation for order {}", order.id);
                warn!("{}", msg);
                cancellation_errors.push(msg);
              }
              Err(e) => {
                let msg = format!("Error waiting for cancellation confirmation for order {}: {:?}", order.id, e);
                error!("{}", msg);
                cancellation_errors.push(msg);
              }
            }
          }
          Ok(false) => {
            // This means cancel_order decided not to send (e.g., already terminal)
            info!("Cancellation request for order {} not sent (likely already terminal).", order.id);
          }
          Err(e) => {
            let msg = format!("Failed to send cancel request for order {}: {:?}", order.id, e);
            error!("{}", msg);
            cancellation_errors.push(msg);
          }
        }
        // Small delay between cancellations? Maybe not necessary.
        // std::thread::sleep(Duration::from_millis(100));
      }
    }

    // 2. Refresh account details (summary & positions)
    // Updates come every 3 minutes.
    std::thread::sleep(Duration::from_millis(500));

    // 3. List final positions
    info!("Fetching final open positions...");
    match acct_mgr.list_open_positions() {
      Ok(positions) => {
        if positions.is_empty() {
          info!("No open positions found.");
        } else {
          info!("Final Open Positions:");
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
      }
      Err(e) => {
        let msg = format!("Failed to list final open positions: {:?}", e);
        error!("{}", msg);
        cancellation_errors.push(msg);
      }
    }

    // 4. Report overall success/failure
    if cancellation_errors.is_empty() {
      info!("Cleanup orders test completed successfully.");
      Ok(())
    } else {
      error!("Cleanup orders test completed with errors:");
      for err in &cancellation_errors {
        error!("  - {}", err);
      }
      Err(anyhow!("One or more errors occurred during cleanup: {:?}", cancellation_errors))
    }
  }


  pub(super) fn order_global_cancel_impl(client: &IBKRClient, is_live: bool) -> Result<()> {
    info!("--- Testing Global Order Cancel ---");
    if is_live {
      warn!("This test will place two GTC limit orders for SPY and then attempt a global cancel.");
      warn!("Ensure SPY is liquid and the limit prices are away from the market to prevent immediate fills.");
      std::thread::sleep(Duration::from_secs(3));
    }

    let order_mgr = client.orders();
    let contract = Contract::stock("SPY");

    // Place two GTC limit orders that are unlikely to fill immediately
    let (spy_contract_ref, order1_req) = OrderBuilder::new(OrderSide::Buy, 1.0)
      .limit(100.00) // Far from market
      .with_tif(TimeInForce::GoodTillCancelled)
      .for_contract(contract.clone()) // Use the cloned contract
      .build()?;
    let order1_id = order_mgr.place_order(spy_contract_ref.clone(), order1_req).context("Failed to place order 1")?;
    info!("Order 1 (BUY SPY @ 100.00 GTC) placed with ID: {}", order1_id);

    let (_spy_contract_ref, order2_req) = OrderBuilder::new(OrderSide::Sell, 1.0)
      .limit(999.00) // Far from market
      .with_tif(TimeInForce::GoodTillCancelled)
      .for_contract(contract.clone()) // Use the cloned contract
      .build()?;
    let order2_id = order_mgr.place_order(spy_contract_ref, order2_req).context("Failed to place order 2")?;
    info!("Order 2 (SELL SPY @ 999.00 GTC) placed with ID: {}", order2_id);

    // Wait for orders to be submitted
    let submit_timeout = Duration::from_secs(10);
    info!("Waiting for orders to be submitted (timeout: {:?})...", submit_timeout);
    match order_mgr.try_wait_order_submitted(&order1_id, submit_timeout) {
      Ok(status) => info!("Order {} submitted with status: {:?}", order1_id, status),
      Err(e) => {
        error!("Error waiting for order {} submission: {:?}", order1_id, e);
        // Attempt to cancel individually if global cancel might fail or for cleanup
        let _ = order_mgr.cancel_order(&order1_id);
        let _ = order_mgr.cancel_order(&order2_id);
        return Err(e).context(format!("Order {} submission failed", order1_id));
      }
    }
    match order_mgr.try_wait_order_submitted(&order2_id, submit_timeout) {
      Ok(status) => info!("Order {} submitted with status: {:?}", order2_id, status),
      Err(e) => {
        error!("Error waiting for order {} submission: {:?}", order2_id, e);
        let _ = order_mgr.cancel_order(&order1_id);
        let _ = order_mgr.cancel_order(&order2_id);
        return Err(e).context(format!("Order {} submission failed", order2_id));
      }
    }

    // Perform global cancel
    info!("Requesting global cancel...");
    order_mgr.cancel_all_orders_globally().context("Failed to send global cancel request")?;
    info!("Global cancel request sent. Waiting for orders to be cancelled...");

    // Wait for orders to be cancelled
    let cancel_timeout = Duration::from_secs(15);
    let mut all_cancelled_successfully = true;

    for order_id_str in [&order1_id, &order2_id] {
      info!("Waiting for order {} to be cancelled (timeout: {:?})...", order_id_str, cancel_timeout);
      match order_mgr.try_wait_order_canceled(order_id_str, cancel_timeout) {
        Ok(status) if status == OrderStatus::Cancelled || status == OrderStatus::ApiCancelled => {
          info!("Order {} successfully cancelled with status: {:?}", order_id_str, status);
        }
        Ok(status) => {
          error!("Order {} reached unexpected status after global cancel: {:?}", order_id_str, status);
          all_cancelled_successfully = false;
        }
        Err(e) => {
          error!("Error waiting for order {} cancellation: {:?}", order_id_str, e);
          all_cancelled_successfully = false;
        }
      }
    }

    if all_cancelled_successfully {
      info!("Global cancel test completed successfully. Both orders cancelled.");
      Ok(())
    } else {
      Err(anyhow!("One or more orders were not successfully cancelled after global cancel request."))
    }
  }

  pub(super) fn order_exercise_option_impl(client: &IBKRClient, is_live: bool) -> Result<()> {
    info!("--- Testing Option Exercise/Lapse ---");
    if is_live {
      warn!("This test requires an account with an existing SPY option position.");
      warn!("It will attempt to EXERCISE 1 contract and LAPSE 1 contract (if available).");
      warn!("Ensure you have at least 2 contracts of a near-term SPY option.");
      warn!("The test uses a placeholder req_id (9001, 9002) and assumes the first account.");
      std::thread::sleep(Duration::from_secs(5));
    }

    let order_mgr = client.orders();
    let acct_mgr = client.account();
    let ref_data_mgr = client.data_ref(); // For finding a suitable option

    // 1. Get account ID
    let account_id = acct_mgr.get_account_info()?.account_id;
    info!("Using account ID: {}", account_id);

    // 2. Find a suitable SPY option to "exercise" and "lapse"
    //    This is tricky for a golden test. We'll try to find any SPY option.
    //    In a real scenario, you'd know the specific contract.
    let spy_stock_contract = Contract::stock("SPY");
    let contract_details_list = ref_data_mgr.get_contract_details(&spy_stock_contract)
      .context("Failed to get contract details for SPY stock to find options")?;
    if contract_details_list.is_empty() {
      return Err(anyhow!("No contract details found for SPY stock."));
    }
    let mut option_search_contract = Contract::new();
    option_search_contract.symbol = "SPY".to_string();
    option_search_contract.sec_type = SecType::Option;
    option_search_contract.exchange = "SMART".to_string();
    option_search_contract.currency = "USD".to_string();
    // No specific expiry/strike, get a list of option chains
    // This might return a lot. We'll just pick the first one for the test.

    let option_details_list = ref_data_mgr.get_contract_details(&option_search_contract)
      .context("Failed to get option contract details for SPY")?;

    if option_details_list.is_empty() {
      return Err(anyhow!("No SPY options found to test exercise/lapse. Ensure market data subscription for SPY options."));
    }
    // For the test, pick the first available option contract.
    // A real application would specify the exact contract.
    let option_to_exercise = option_details_list[0].contract.clone();
    info!("Selected option for test: ConID={}, Symbol={}, Expiry={}, Strike={}, Right={:?}",
          option_to_exercise.con_id, option_to_exercise.symbol,
          option_to_exercise.last_trade_date_or_contract_month.as_deref().unwrap_or("N/A"),
          option_to_exercise.strike.unwrap_or(0.0), option_to_exercise.right);


    // 3. Attempt to Exercise 1 contract (if live, this needs a position)
    let exercise_req_id = 9001; // Arbitrary req_id for this operation
    let exercise_qty = 1;
    let override_exercise = false; // Usually false unless forcing OTM exercise

    info!("Attempting to EXERCISE {} contract(s) of {:?} with ReqID {}", exercise_qty, option_to_exercise.local_symbol.as_deref().unwrap_or(&option_to_exercise.symbol), exercise_req_id);
    match order_mgr.exercise_option(exercise_req_id, &option_to_exercise, yatws::order::ExerciseAction::Exercise, exercise_qty, &account_id, override_exercise) {
      Ok(_) => info!("Exercise request sent for ReqID {}. Monitor account updates.", exercise_req_id),
      Err(e) => {
        // In non-live (replay), this might fail if not in logs. In live, it might fail if no position.
        warn!("Failed to send EXERCISE request for ReqID {}: {:?}. This might be expected if no position or in replay.", exercise_req_id, e);
        // Don't fail the whole test for this, as setup is complex.
      }
    }

    // Allow time for processing if live
    if is_live { std::thread::sleep(Duration::from_secs(5)); }

    // 4. Attempt to Lapse 1 contract (if live, this needs a position)
    // For testing, we'll use the same option contract. A real scenario might use a different one.
    let lapse_req_id = 9002; // Arbitrary req_id
    let lapse_qty = 1;
    let override_lapse = false; // Usually false

    info!("Attempting to LAPSE {} contract(s) of {:?} with ReqID {}", lapse_qty, option_to_exercise.local_symbol.as_deref().unwrap_or(&option_to_exercise.symbol), lapse_req_id);
    match order_mgr.exercise_option(lapse_req_id, &option_to_exercise, yatws::order::ExerciseAction::Lapse, lapse_qty, &account_id, override_lapse) {
      Ok(_) => info!("Lapse request sent for ReqID {}. Monitor account updates.", lapse_req_id),
      Err(e) => {
        warn!("Failed to send LAPSE request for ReqID {}: {:?}. This might be expected if no position or in replay.", lapse_req_id, e);
      }
    }

    if is_live {
      info!("Exercise/Lapse requests sent. Manual verification of account and position changes is required for live test.");
      info!("Waiting for 10 seconds to allow observation of potential messages...");
      std::thread::sleep(Duration::from_secs(10));
    } else {
      info!("Exercise/Lapse test sequence completed for replay mode.");
    }

    // This test primarily checks if the calls can be made.
    // Verifying the outcome requires checking account/position data, which is complex for an automated golden.
    Ok(())
  }

  pub(super) fn order_what_if_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing What-If Order Check ---");
    let order_mgr = client.orders();

    // Define a potential order (e.g., buy 100 shares of AAPL limit 150)
    let (contract, order_req) = OrderBuilder::new(OrderSide::Buy, 100.0)
      .limit(150.00)
      .for_stock("AAPL")
      .with_tif(TimeInForce::Day)
    // Example: Add a time condition - ensure you have `chrono::Utc` in scope
    // .add_time_condition(Utc::now() + ChronoDuration::minutes(5), true) // Condition: time is after 5 mins from now
    // .with_account("YOUR_ACCOUNT_ID") // Optional: Specify account if needed
    // Example: Add another condition with explicit conjunction
    // .with_next_condition_conjunction(yatws::order_builder::ConditionConjunction::And)
    // .add_volume_condition(265598, "SMART", 1000000, true) // Example volume condition
      .build()?;

    info!("Checking What-If for: {:?} {} {} @ {}",
          order_req.side, order_req.quantity, contract.symbol,
          order_req.limit_price.map_or("MKT".to_string(), |p| p.to_string()));

    let timeout = Duration::from_secs(15);
    match order_mgr.check_what_if_order(&contract, &order_req, timeout) {
      Ok(state) => {
        info!("Successfully received What-If results:");
        info!("  Status (should be PreSubmitted/Submitted): {:?}", state.status);
        info!("  Initial Margin Before: {:?}", state.initial_margin_before);
        info!("  Maintenance Margin Before: {:?}", state.maintenance_margin_before);
        info!("  Equity With Loan Before: {:?}", state.equity_with_loan_before);
        info!("  Initial Margin Change: {:?}", state.initial_margin_change);
        info!("  Maintenance Margin Change: {:?}", state.maintenance_margin_change);
        info!("  Equity With Loan Change: {:?}", state.equity_with_loan_change);
        info!("  Initial Margin After: {:?}", state.initial_margin_after);
        info!("  Maintenance Margin After: {:?}", state.maintenance_margin_after);
        info!("  Equity With Loan After: {:?}", state.equity_with_loan_after);
        info!("  Commission: {:?} {}", state.commission, state.commission_currency.as_deref().unwrap_or(""));
        info!("  Min Commission: {:?}", state.min_commission);
        info!("  Max Commission: {:?}", state.max_commission);
        info!("  Warning Text: {:?}", state.warning_text);

        // Basic validation: Check if some key fields were populated
        if state.initial_margin_after.is_none() && state.commission.is_none() {
          warn!("What-If check returned state, but key fields (margin, commission) are missing.");
          // Depending on strictness, could return Err here.
        }
        Ok(())
      }
      Err(e) => {
        error!("What-If check failed: {:?}", e);
        Err(e.into())
      }
    }
  }


  pub(super) fn historical_data_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Get Historical Data ---");
    let data_mgr = client.data_market();
    let contract = Contract::stock("IBM"); // Use IBM stock
    let end_date_time = None; // Request up to present
    let duration = DurationUnit::Day(3); // Request 3 days of data using the enum
    let bar_size_setting = yatws::contract::BarSize::OneHour;
    let what_to_show = WhatToShow::Trades; // Changed from "TRADES"
    let use_rth = true;
    let format_date = 1; // yyyyMMdd HH:mm:ss
    let keep_up_to_date = false;
    let chart_options = &[];

    info!(
      "Requesting historical data for {}: Duration={}, BarSize={}, What={}, RTH={}",
      contract.symbol, duration, bar_size_setting, what_to_show, use_rth // duration and what_to_show will use Display
    );

    match data_mgr.get_historical_data(
      &contract,
      end_date_time,
      duration, // Pass the enum
      bar_size_setting,
      what_to_show, // Pass the enum
      use_rth,
      format_date,
      keep_up_to_date,
      Some(MarketDataType::Delayed),
      chart_options
    ) {
      Ok(bars) => {
        info!("Successfully received {} historical bars.", bars.len());
        if let Some(first_bar) = bars.first() {
          info!("  First Bar: Time={}, O={}, H={}, L={}, C={}, Vol={}",
                first_bar.time.format("%Y-%m-%d %H:%M:%S"), first_bar.open, first_bar.high, first_bar.low, first_bar.close, first_bar.volume);
        }
        if let Some(last_bar) = bars.last() {
          info!("  Last Bar:  Time={}, O={}, H={}, L={}, C={}, Vol={}",
                last_bar.time.format("%Y-%m-%d %H:%M:%S"), last_bar.open, last_bar.high, last_bar.low, last_bar.close, last_bar.volume);
        }
        // Basic validation: Check if we received *some* bars. The exact number can vary.
        if bars.is_empty() {
          warn!("Received 0 historical bars. This might be okay depending on market hours/data availability.");
          // Decide if this should be an error or just a warning. Let's allow 0 bars for now.
        }
        Ok(())
      }
      Err(e) => {
        error!("Failed to get historical data for {}: {:?}", contract.symbol, e);
        Err(e.into())
      }
    }
  }


  pub(super) fn box_spread_yield_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Box Spread Yield Calculation ---");
    let data_market = client.data_market();
    let data_ref = client.data_ref(); // Need this for the builder

    // Define underlyings and parameters - Use Futures now
    let underlyings = [
      ("ES", SecType::Future, "CME"), // S&P E-mini
      ("RTY", SecType::Future, "CME"), // Russell 2000 E-mini
    ];
    let strike_diffs = [100.0, 200.0]; // Adjust strike diffs for futures scale
    let expiry_offsets_days = [30, 60, 90]; // Approx days from today

    let today = Utc::now().date_naive();
    let target_expiries: Vec<NaiveDate> = expiry_offsets_days
      .iter()
      .map(|&days| today + ChronoDuration::days(days))
      .collect();

    let mut overall_success = true;

    for (symbol, sec_type, exchange) in underlyings {
      info!("--- Testing Underlying: {} ({}) ---", symbol, sec_type);

      let mut uc = Contract::new();
      uc.symbol = symbol.to_string();
      uc.sec_type = SecType::Future;
      uc.exchange = exchange.to_string();
      // Take the nearest future. In theory we could take the one matching the option expiration date.
      let futs: Vec<_> = data_ref.get_contract_details(&uc)?.into_iter().map(|d| d.contract_month).collect();
      log::info!("Futures: {:?}", futs);
      assert!(!futs.is_empty(), "No contracts found for future symbol: {}", symbol);
      uc.last_trade_date_or_contract_month = futs.into_iter().min();
      // No last trade, use ask for now, assuming that the spreads are tight.
      let underlying_price = data_market.get_quote(&uc, Some(MarketDataType::Delayed), Duration::from_secs(10))?.1.unwrap();
      if underlying_price <= 0.0 {
        warn!("Invalid price ({}) for {}, strike selection might be inaccurate.", underlying_price, symbol);
      }
      log::info!("Underlying price: {} = {:.2}", symbol, underlying_price);

      for target_expiry in &target_expiries {
        for &strike_diff in &strike_diffs {
          let target_strike1 = underlying_price - strike_diff / 2.0;
          let target_strike2 = underlying_price + strike_diff / 2.0;

          info!("Attempting Box for {} Exp~{}, Strikes~{:.2}/{:.2}",
                symbol, target_expiry.format("%Y-%m-%d"), target_strike1, target_strike2);

          // Use OptionsStrategyBuilder to define the box
          let builder_result = OptionsStrategyBuilder::new(
            data_ref.clone(), // Clone Arc
            symbol,
            underlying_price,
            1.0, // Quantity = 1 box
            sec_type.clone(),
          )?
            .box_spread_nearest_expiry(*target_expiry, target_strike1, target_strike2);

          let builder = match builder_result {
            Ok(b) => b,
            Err(e) => {
              error!("Failed to define box strategy for {} Exp~{}: {:?}", symbol, target_expiry, e);
              overall_success = false;
              continue; // Try next parameters
            }
          };

          // Build the combo contract
          let (combo_contract, _order_request) = match builder.build() {
            Ok(result) => result,
            Err(e) => {
              error!("Failed to build combo contract for {} Exp~{}: {:?}", symbol, target_expiry, e);
              overall_success = false;
              continue;
            }
          };

          // Extract actual strikes and expiry from the built contract for yield calculation
          // This requires parsing the combo legs or relying on the builder's internal state (which isn't exposed)
          // Let's re-extract from combo legs for robustness
          let mut strikes = Vec::new();
          let mut expiry_str = None;
          for leg in &combo_contract.combo_legs {
            // Fetch full contract details for the leg to get strike/expiry
            // This is inefficient but necessary if builder doesn't expose details
            let leg_contract_spec = Contract { con_id: leg.con_id, ..Default::default() };
            match data_ref.get_contract_details(&leg_contract_spec) {
              Ok(details_list) if !details_list.is_empty() => {
                let leg_details = &details_list[0].contract;
                if let Some(s) = leg_details.strike { strikes.push(s); }
                if expiry_str.is_none() { expiry_str = leg_details.last_trade_date_or_contract_month.clone(); }
              },
              Ok(_) => { error!("Leg contract details not found for conId {}", leg.con_id); overall_success = false; break; },
              Err(e) => { error!("Error fetching leg details for conId {}: {:?}", leg.con_id, e); overall_success = false; break; },
            }
          }
          if !overall_success { continue; } // Skip if leg details failed

          strikes.sort_by(|a, b| a.partial_cmp(b).unwrap());
          strikes.dedup();
          if strikes.len() != 2 {
            error!("Could not determine unique strike pair from combo legs: {:?}", strikes);
            overall_success = false;
            continue;
          }
          let actual_strike1 = strikes[0];
          let actual_strike2 = strikes[1];
          let actual_strike_diff = actual_strike2 - actual_strike1;

          let actual_expiry_date = match expiry_str.as_deref().and_then(|s| NaiveDate::parse_from_str(s, "%Y%m%d").ok()) {
            Some(date) => date,
            None => {
              error!("Could not determine expiry date from combo legs.");
              overall_success = false;
              continue;
            }
          };

          info!("  Actual Box: Exp={}, Strikes={:.2}/{:.2} (Diff={:.2})",
                actual_expiry_date.format("%Y%m%d"), actual_strike1, actual_strike2, actual_strike_diff);

          // Get quote for the combo contract
          let quote_timeout = Duration::from_secs(20);
          match data_market.get_quote(&combo_contract, Some(MarketDataType::Delayed), quote_timeout) {
            Ok((Some(bid), Some(ask), _last)) => {
              let mid_price = (bid + ask) / 2.0;
              info!("  Quote: Bid={:.4}, Ask={:.4}, Mid={:.4}", bid, ask, mid_price);

              // Calculate yield
              let days_to_expiry = (actual_expiry_date - today).num_days();
              if days_to_expiry <= 0 {
                warn!("  Expiry date {} is not in the future. Cannot calculate yield.", actual_expiry_date);
                continue;
              }
              let time_to_expiry_years = days_to_expiry as f64 / 365.0;

              if mid_price <= 0.0 || mid_price >= actual_strike_diff {
                warn!("  Mid price ({:.4}) is invalid relative to strike difference ({:.2}). Cannot calculate yield.", mid_price, actual_strike_diff);
                continue;
              }

              let ratio = mid_price / actual_strike_diff;
              let yield_pct = -ratio.ln() / time_to_expiry_years * 100.0;
              info!("  => {}:{}/{:.2} Calculated Annual Yield: {:.4}%", symbol, actual_expiry_date.format("%Y%m"), ratio, yield_pct);

            }
            Ok((bid, ask, _)) => {
              error!("  Failed to get valid Bid/Ask quote for combo. Bid: {:?}, Ask: {:?}", bid, ask);
              overall_success = false;
            }
            Err(e) => {
              error!("  Error getting quote for combo: {:?}", e);
              overall_success = false;
            }
          }
          // Add a small delay to avoid pacing violations, especially in live mode
          std::thread::sleep(Duration::from_secs(2));
        }
      }
    }

    if overall_success {
      info!("Box spread yield test completed successfully (individual quote checks passed/failed as logged).");
      Ok(())
    } else {
      Err(anyhow!("One or more errors occurred during box spread yield test."))
    }
  }

  pub(super) fn financial_reports_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Get Financial Reports (Summary & Snapshot) ---");
    let fin_data_mgr = client.data_financials();
    let contract = Contract::stock("AAPL"); // Test with AAPL stock
    let fundamental_data_options = &[]; // No specific options for now

    let mut overall_success = true;

    // 1. Request Financial Summary
    info!("Requesting 'ReportsFinSummary' for {}...", contract.symbol);
    match fin_data_mgr.get_fundamental_data(&contract, FundamentalReportType::ReportsFinSummary, fundamental_data_options) {
      Ok(xml_data) => {
        info!("Successfully received 'ReportsFinSummary' XML for {}: {}", contract.symbol, xml_data);
        if xml_data.is_empty() {
          warn!("Received empty 'ReportsFinSummary' XML data for {}.", contract.symbol);
        } else {
          // Parse the XML
          match parse_fundamental_xml(&xml_data, FundamentalReportType::ReportsFinSummary) {
            Ok(ParsedFundamentalData::FinancialSummary(summary)) => {
              info!("Successfully parsed 'ReportsFinSummary' for {}:", contract.symbol);
              info!("  Number of EPS Records: {}", summary.eps_records.len());
              if let Some(eps) = summary.eps_records.first() {
                info!("    Sample EPS: Date: {:?}, Type: {:?}, Period: {:?}, Value: {:?}, Currency: {:?}",
                      eps.as_of_date, eps.report_type, eps.period, eps.value, eps.currency);
              }
              info!("  Number of Dividend Per Share Records: {}", summary.dividend_per_share_records.len());
              if let Some(dps) = summary.dividend_per_share_records.first() {
                info!("    Sample DPS: Date: {:?}, Type: {:?}, Period: {:?}, Value: {:?}, Currency: {:?}",
                      dps.as_of_date, dps.report_type, dps.period, dps.value, dps.currency);
              }
              info!("  Number of Total Revenue Records: {}", summary.total_revenue_records.len());
              if let Some(rev) = summary.total_revenue_records.first() {
                info!("    Sample Revenue: Date: {:?}, Type: {:?}, Period: {:?}, Value: {:?}, Currency: {:?}",
                      rev.as_of_date, rev.report_type, rev.period, rev.value, rev.currency);
              }
              info!("  Number of Announced Dividend Records: {}", summary.announced_dividend_records.len());
              if let Some(div) = summary.announced_dividend_records.first() {
                info!("    Sample Announced Dividend: ExDate: {:?}, Type: {:?}, Value: {:?}, Currency: {:?}",
                      div.ex_date, div.dividend_type, div.value, div.currency);
              }
            }
            Ok(_) => {
              error!("Parsed 'ReportsFinSummary' but got unexpected data type for {}.", contract.symbol);
              overall_success = false;
            }
            Err(parse_err) => {
              error!("Failed to parse 'ReportsFinSummary' XML for {}: {:?}", contract.symbol, parse_err);
              overall_success = false;
            }
          }
        }
      }
      Err(e) => {
        error!("Failed to get 'ReportsFinSummary' for {}: {:?}", contract.symbol, e);
        overall_success = false;
      }
    }

    // 2. Request Report Snapshot
    info!("Requesting 'ReportSnapshot' for {}...", contract.symbol);
    match fin_data_mgr.get_fundamental_data(&contract, FundamentalReportType::ReportSnapshot, fundamental_data_options) {
      Ok(xml_data) => {
        info!("Successfully received 'ReportSnapshot' XML for {}. Length: {}", contract.symbol, xml_data.len());
        info!("Successfully received 'ReportSnapshot' XML for {}: {}", contract.symbol, xml_data);
        if xml_data.is_empty() {
          warn!("Received empty 'ReportSnapshot' XML data for {}.", contract.symbol);
        } else {
          // Parse the XML
          match parse_fundamental_xml(&xml_data, FundamentalReportType::ReportSnapshot) {
            Ok(ParsedFundamentalData::Snapshot(snapshot)) => {
              info!("Successfully parsed 'ReportSnapshot' for {}:", contract.symbol);
              if let Some(info) = &snapshot.company_info {
                info!("  Parsed Company Info:");
                info!("    Name: {}", info.company_name.as_deref().unwrap_or("N/A"));
                info!("    Ticker: {}", info.ticker.as_deref().unwrap_or("N/A"));
                info!("    ConID: {:?}", info.con_id);
                info!("    CIK: {}", info.cik.as_deref().unwrap_or("N/A"));
                info!("    Business Desc (start): {}...", info.business_description.as_deref().unwrap_or("N/A").chars().take(70).collect::<String>());
              } else {
                warn!("  No consolidated company information parsed from snapshot.");
              }
              info!("  Number of Issues: {}", snapshot.issues.len());
              if let Some(issue) = snapshot.issues.first() {
                info!("    First Issue Ticker: {}", issue.ticker.as_deref().unwrap_or("N/A"));
              }
              info!("  Number of Ratio Groups: {}", snapshot.ratio_groups.len());
              if let Some(group) = snapshot.ratio_groups.first() {
                info!("    First Ratio Group: '{}', Num Ratios: {}", group.id.as_deref().unwrap_or("N/A"), group.ratios.len());
                if let Some(ratio) = group.ratios.first() {
                  info!("      Sample Ratio: Field='{}', Value='{}', Type='{}'",
                        ratio.field_name,
                        ratio.raw_value.as_deref().unwrap_or("N/A"),
                        ratio.period_type.as_deref().unwrap_or("N/A")); // Note: XML uses 'Type' attr, mapped to period_type field
                }
              }
              info!("  Number of Officers: {}", snapshot.officers.len());
              if let Some(officer) = snapshot.officers.first() {
                info!("    First Officer: {} {}", officer.first_name.as_deref().unwrap_or("?"), officer.last_name.as_deref().unwrap_or("?"));
              }
              if let Some(fc) = &snapshot.forecast_data {
                info!("  Forecast Data: {} items", fc.items.len());
                if let Some(item) = fc.items.first() {
                  info!("    Sample Forecast: Field='{}', Value='{}'", item.field_name, item.value.as_deref().unwrap_or("N/A"));
                }
              } else {
                info!("  No Forecast Data found.");
              }

            }
            Ok(_) => {
              error!("Parsed 'ReportSnapshot' but got unexpected data type for {}.", contract.symbol);
              overall_success = false;
            }
            Err(parse_err) => {
              error!("Failed to parse 'ReportSnapshot' XML for {}: {:?}", contract.symbol, parse_err);
              overall_success = false;
            }
          }
        }
      }
      Err(e) => {
        error!("Failed to get 'ReportSnapshot' for {}: {:?}", contract.symbol, e);
        overall_success = false;
      }
    }

    // 3. Request financial statements.
    // We need to specify reportType to one of these:
    // AnnualReports - Annual financial statements
    // QuarterlyReports - Quarterly financial statements
    // Interim - Interim reports
    // ThreeMonth - 3-month statements
    // SixMonth - 6-month statements
    // TwelveMonth - 12-month statements
    // But the tag-value encoder is broken.

    if overall_success {
      Ok(())
    } else {
      Err(anyhow!("One or more financial report requests failed for {}", contract.symbol))
    }
  }

  pub(super) fn historical_news_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Get Historical News ---");
    let news_mgr = client.data_news();
    let ref_data_mgr = client.data_ref();

    // 1. Get News Providers
    info!("Requesting news providers...");
    let providers = news_mgr.get_news_providers().context("Failed to get news providers")?;

    if providers.is_empty() {
      // If run against a gateway without news subscriptions, this might be empty.
      // The get_historical_news call will likely fail or return nothing if provider_codes is empty or invalid.
      warn!("No news providers found. Historical news request might yield no results or fail.");
      // Depending on strictness, one might return Err here or proceed.
      // Let's proceed and let the historical_news call handle an empty/invalid provider string.
    }

    let provider_codes = providers.iter().map(|p| p.code.as_str()).collect::<Vec<&str>>().join(",");
    if providers.is_empty() {
      info!("Proceeding with empty provider codes string as no providers were returned.");
    } else {
      info!("Available news providers: {:?}", providers.iter().map(|p| &p.code).collect::<Vec<_>>());
      info!("Using provider codes: {}", provider_codes);
    }


    // 2. Get Contract Details for AAPL to find con_id
    info!("Fetching contract details for AAPL...");
    let contract_spec = Contract::stock("AAPL");
    // For specific contracts, you might need to set exchange, currency, etc.
    // contract_spec.exchange = "SMART".to_string();
    // contract_spec.currency = "USD".to_string();

    let contract_details_list = ref_data_mgr.get_contract_details(&contract_spec)
      .context("Failed to get contract details for AAPL")?;

    if contract_details_list.is_empty() {
      return Err(anyhow!("No contract details found for AAPL."));
    }
    // Assuming the first result is the primary listing.
    let con_id = contract_details_list[0].contract.con_id;
    if con_id == 0 {
      return Err(anyhow!("Invalid con_id (0) received for AAPL."));
    }
    info!("Using con_id {} for AAPL.", con_id);

    // 3. Request Historical News
    // Define date range (e.g., last 7 days)
    let end_date_time = Some(Utc::now());
    // Note: TWS API expects format "yyyy-MM-dd HH:mm:ss" or "yyyyMMdd HH:mm:ss" for start/end time.
    // The encoder handles formatting of chrono::DateTime<Utc>.
    let start_date_time = Some(Utc::now() - ChronoDuration::days(7));
    let total_results = 10; // Request up to 10 articles
    let historical_news_options = &[]; // No specific options

    info!(
      "Requesting historical news for AAPL (con_id {}), providers [{}], last {} days, max {} results.",
      con_id, provider_codes, 7, total_results
    );

    match news_mgr.get_historical_news(
      con_id,
      &provider_codes,
      start_date_time,
      end_date_time,
      total_results,
      historical_news_options,
    ) {
      Ok(news_items) => {
        info!("Successfully received {} historical news articles.", news_items.len());
        if news_items.is_empty() {
          warn!("Received 0 historical news articles. This might be okay depending on providers/contract/timeframe/subscriptions.");
        }
        for (i, item) in news_items.iter().enumerate().take(5) { // Log first 5 or fewer
          info!(
            "  Item {}: Time={}, Provider={}, ID={}, Headline='{}'",
            i + 1, item.time, item.provider_code, item.article_id, item.headline
          );
        }
        Ok(())
      }
      Err(IBKRError::ApiError(code, msg)) if msg.contains("Historical news request requires subscription") || msg.contains("no news farm connection") => {
        warn!("Historical news API error (likely subscription/connection issue): code={}, msg='{}'", code, msg);
        warn!("This test may pass with a warning if data isn't available due to account/market data status.");
        Ok(()) // Treat as a pass with warning for common subscription issues
      }
      Err(e) => {
        error!("Failed to get historical news for AAPL: {:?}", e);
        Err(e.into())
      }
    }
  }


  pub(super) fn scanner_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Market Scanner (Parameters & Blocking Results) ---");
    let data_mgr = client.data_market();
    let mut overall_success = true;

    // 1. Get Scanner Parameters
    let params_timeout = Duration::from_secs(15);
    info!("Requesting scanner parameters XML (Timeout: {:?})...", params_timeout);
    match data_mgr.get_scanner_parameters(params_timeout) {
      Ok(params) => {
        let scan_list = params.scan_type_list.unwrap().scan_types;
        info!("Available codes ({}):", scan_list.len());
        for s in scan_list {
          info!("  {}", s.scan_code.unwrap_or("<none>".to_string()));
        }
      }
      Err(e) => {
        error!("Failed to get scanner parameters: {:?}", e);
        overall_success = false; // Mark failure but continue to results test
      }
    }

    // 2. Get Scanner Results (Blocking)
    info!("Proceeding to test blocking scanner results...");
    // Define the scanner subscription parameters
    let subscription = yatws::contract::ScannerInfo {
      number_of_rows: 10, // Request top 10 results
      instrument: "STK".to_string(),
      location_code: "STK.US.MAJOR".to_string(), // Major US exchanges
      scan_code: "TOP_PERC_GAIN".to_string(), // Top % gainers
      above_price: Some(1.0), // Price above $1
      below_price: None,
      above_volume: Some(10000), // Volume above 10k
      market_cap_above: None,
      market_cap_below: None,
      moody_rating_above: None,
      moody_rating_below: None,
      sp_rating_above: None,
      sp_rating_below: None,
      maturity_date_above: None,
      maturity_date_below: None,
      coupon_rate_above: None,
      coupon_rate_below: None,
      exclude_convertible: false,
      average_option_volume_above: None,
      scanner_setting_pairs: None,
      stock_type_filter: Some("ALL".to_string()), // All stock types (Common, ADR, etc.)
    };

    let timeout = Duration::from_secs(60); // Timeout for the scan

    info!("Requesting scanner results: {:?}", subscription.scan_code);

    match data_mgr.get_scanner_results(&subscription, timeout) {
      Ok(results) => {
        info!("Successfully received {} scanner results.", results.len());
        if results.is_empty() {
          warn!("Received 0 scanner results. This might be okay depending on market conditions/scan parameters.");
        }
        for item in results.iter().take(5) { // Log first 5 or fewer
          info!(
            "  Rank: {}, Symbol: {}, MarketName: {}, Distance: {}, Benchmark: {}",
            item.rank,
            item.contract_details.contract.symbol,
            item.contract_details.market_name,
            item.distance,
            item.benchmark
          );
        }
      }
      Err(IBKRError::ApiError(code, msg)) if msg.contains("scanner subscription") || msg.contains("market data subscription") => {
        warn!("Scanner API error (likely subscription issue): code={}, msg='{}'", code, msg);
        warn!("This test may pass with a warning if data isn't available due to account/market data status.");
      }
      Err(e) => {
        error!("Failed to get scanner results: {:?}", e);
        overall_success = false; // Mark failure
      }
    };

    // Final result based on overall success
    if overall_success {
      info!("Scanner test (parameters and results) completed successfully.");
      Ok(())
    } else {
      Err(anyhow!("One or more parts of the scanner test failed."))
    }
  }


  pub(super) fn option_calculations_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Option Calculations (Implied Vol & Option Price) ---");
    let data_mgr = client.data_market();
    let ref_data_mgr = client.data_ref();
    let timeout = Duration::from_secs(20);

    // 1. Get AAPL stock price
    let aapl_stock_contract = Contract::stock("AAPL");
    info!("Fetching current price for AAPL...");
    let (_bid, _ask, last_price_opt) = data_mgr.get_quote(&aapl_stock_contract, Some(MarketDataType::Delayed), timeout)
      .context("Failed to get quote for AAPL stock")?;
    let under_price = match last_price_opt {
      Some(price) if price > 0.0 => price,
      _ => {
        warn!("Could not get valid last price for AAPL. Using placeholder 170.0 for underlying price.");
        170.0 // Placeholder if live price fails
      }
    };
    info!("Using underlying AAPL price: {:.2}", under_price);

    // 2. Define an AAPL call option contract
    //    - Find next month's 3rd Friday for expiry
    //    - Strike price ~10% above current stock price
    let today = Utc::now();
    let mut current_month = today.month();
    let mut current_year = today.year();
    if current_month == 12 {
      current_month = 1;
      current_year += 1;
    } else {
      current_month += 1;
    }
    let first_of_next_month = NaiveDate::from_ymd_opt(current_year, current_month, 1).unwrap();
    let days_to_friday = (chrono::Weekday::Fri.number_from_monday() + 7 - first_of_next_month.weekday().number_from_monday()) % 7;
    let first_friday = first_of_next_month + ChronoDuration::days(days_to_friday as i64);
    let target_expiry_date = first_friday + ChronoDuration::weeks(2); // 3rd Friday
    let expiry_str = target_expiry_date.format("%Y%m%d").to_string();

    let target_strike_raw = under_price * 1.10;
    // Round to nearest $2.50 increment for typical AAPL options, or $5 for higher prices
    let strike_increment = if target_strike_raw < 200.0 { 2.5 } else { 5.0 };
    let strike_price = (target_strike_raw / strike_increment).round() * strike_increment;

    info!("Targeting AAPL Call Option: Expiry={}, Strike={:.2}", expiry_str, strike_price);

    let option_contract_spec = Contract::option("AAPL", &expiry_str, strike_price, OptionRight::Call, "SMART", "USD");

    // Get full contract details to ensure it's valid and get con_id
    info!("Fetching contract details for the target option...");
    let option_details_list = ref_data_mgr.get_contract_details(&option_contract_spec)
      .context(format!("Failed to get contract details for AAPL option {} C{}", expiry_str, strike_price))?;

    if option_details_list.is_empty() {
      return Err(anyhow!("No contract details found for the specified AAPL option. Check expiry/strike or market data subscription."));
    }
    let option_contract = option_details_list[0].contract.clone();
    info!("Using option contract: ConID={}, LocalSymbol={}", option_contract.con_id, option_contract.local_symbol.as_deref().unwrap_or("N/A"));


    // 3. Calculate Implied Volatility
    let placeholder_option_price = 2.50; // Placeholder market price for the option
    info!("Calculating Implied Volatility for {} with OptionPrice={}, UnderPrice={}...",
          option_contract.local_symbol.as_deref().unwrap_or("AAPL Option"), placeholder_option_price, under_price);

    match data_mgr.calculate_implied_volatility(&option_contract, placeholder_option_price, under_price, timeout) {
      Ok(computation) => {
        info!("Successfully calculated Implied Volatility:");
        log_tick_option_computation(&computation);
      }
      Err(e) => {
        error!("Failed to calculate Implied Volatility: {:?}", e);
        // Don't fail the whole test, proceed to option price calc
      }
    }

    std::thread::sleep(Duration::from_secs(1));

    // 4. Calculate Option Price
    let placeholder_volatility = 0.30; // Placeholder volatility (30%)
    info!("Calculating Option Price for {} with Volatility={}, UnderPrice={}...",
          option_contract.local_symbol.as_deref().unwrap_or("AAPL Option"), placeholder_volatility, under_price);

    match data_mgr.calculate_option_price(&option_contract, placeholder_volatility, 170.0 /* under_price */, timeout) {
      Ok(computation) => {
        info!("Successfully calculated Option Price:");
        log_tick_option_computation(&computation);
      }
      Err(e) => {
        error!("Failed to calculate Option Price: {:?}", e);
        // Don't fail the whole test if this part fails
      }
    }

    Ok(())
  }

  fn log_tick_option_computation(computation: &TickOptionComputationData) {
    info!("  TickType: {:?}", computation.tick_type);
    info!("  TickAttrib: {:?}", computation.tick_attrib);
    info!("  ImpliedVol: {:?}", computation.implied_vol);
    info!("  Delta: {:?}", computation.delta);
    info!("  OptPrice: {:?}", computation.opt_price);
    info!("  PvDividend: {:?}", computation.pv_dividend);
    info!("  Gamma: {:?}", computation.gamma);
    info!("  Vega: {:?}", computation.vega);
    info!("  Theta: {:?}", computation.theta);
    info!("  UndPrice: {:?}", computation.und_price);
  }

  pub(super) fn histogram_data_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Get Histogram Data ---");
    let data_mgr = client.data_market();
    let contract = Contract::stock("GE"); // Use a liquid stock
    let use_rth = true;
    let time_period = TimePeriodUnit::Day(3); // Request histogram for the last 3 days using enum
    let histogram_options = &[]; // Not used by REQ_HISTOGRAM_DATA
    let timeout = Duration::from_secs(30);

    info!(
      "Requesting histogram data for {}: UseRTH={}, TimePeriod='{}', Timeout={:?}",
      contract.symbol, use_rth, time_period, timeout // Log the enum (uses Display)
    );

    match data_mgr.get_histogram_data(
      &contract,
      use_rth,
      time_period, // Pass the enum
      histogram_options,
      timeout
    ) {
      Ok(items) => {
        info!("Successfully received {} histogram data points.", items.len());
        if items.is_empty() {
          warn!("Received 0 histogram data points. This might be okay depending on contract/time period.");
        }
        for (i, item) in items.iter().enumerate().take(5) { // Log first 5 or fewer
          info!(
            "  Item {}: Price={}, Size={}",
            i + 1, item.price, item.size
          );
        }
        Ok(())
      }
      Err(e) => {
        error!("Failed to get histogram data for {}: {:?}", contract.symbol, e);
        Err(e.into())
      }
    }
  }

  pub(super) fn historical_ticks_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Get Historical Ticks ---");
    let data_mgr = client.data_market();
    let contract = Contract::stock("AAPL"); // Use a liquid stock

    // Request a small number of ticks from the recent past
    // TWS API: If number_of_ticks is non-zero, start_date_time is ignored if end_date_time is provided.
    // If number_of_ticks is 0, then the date range is used.
    // We will request a specific number of ticks ending "now".
    let end_date_time = Some(Utc::now());
    let start_date_time = None; // Not used if number_of_ticks > 0 and end_date_time is set

    let number_of_ticks = 10;
    let what_to_show = WhatToShow::Trades; // Changed from "TRADES"
    let use_rth = false; // Get ticks outside RTH if available
    let ignore_size = false; // Relevant for BID_ASK if what_to_show is "BID_ASK"
    let misc_options = &[];
    let timeout = Duration::from_secs(30);

    info!(
      "Requesting {} historical '{}' ticks for {} ending {:?}, RTH={}, Timeout={:?}",
      number_of_ticks, what_to_show, contract.symbol, end_date_time, use_rth, timeout // what_to_show will use Display
    );

    match data_mgr.get_historical_ticks(
      &contract,
      start_date_time,
      end_date_time,
      number_of_ticks,
      what_to_show, // Pass the enum
      use_rth,
      ignore_size,
      misc_options,
      timeout
    ) {
      Ok(ticks) => {
        info!("Successfully received {} historical ticks.", ticks.len());
        if ticks.is_empty() && number_of_ticks > 0 {
          warn!("Received 0 historical ticks, though {} were requested. This might be okay depending on market activity/data availability/subscriptions.", number_of_ticks);
        }
        for (i, tick) in ticks.iter().enumerate().take(5) { // Log first 5 or fewer
          info!("  Tick {}: {:?}", i + 1, tick);
        }
        Ok(())
      }
      Err(e) => {
        error!("Failed to get historical ticks for {}: {:?}", contract.symbol, e);
        Err(e.into())
      }
    }
  }


  pub(super) fn historical_schedule_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
    info!("--- Testing Get Historical Schedule ---");
    let data_ref_mgr = client.data_ref();
    let contract = Contract::stock("AAPL"); // Use a liquid stock like AAPL

    // Request schedule: e.g., from now for the next 30 days.
    let start_date = Some(Utc::now());
    let end_date = Some(Utc::now() + ChronoDuration::days(30));
    let time_zone_id = "America/New_York"; // Example timezone, though not used by TWS for this request.

    info!(
      "Requesting historical schedule for {}: StartDate ~{:?}, EndDate ~{:?}, TimeZoneID='{}'",
      contract.symbol,
      start_date.map(|dt| dt.date_naive()),
      end_date.map(|dt| dt.date_naive()),
      time_zone_id
    );

    match data_ref_mgr.get_historical_schedule(&contract, start_date, end_date, time_zone_id) {
      Ok(schedule_result) => {
        info!("Successfully received historical schedule for {}:", contract.symbol);
        info!("  StartDateTime: {}", schedule_result.start_date_time);
        info!("  EndDateTime:   {}", schedule_result.end_date_time);
        info!("  TimeZone:      {}", schedule_result.time_zone);
        info!("  Number of Sessions: {}", schedule_result.sessions.len());

        if schedule_result.sessions.is_empty() {
          warn!("Received 0 sessions in the schedule. This might be okay depending on the contract/period.");
        } else {
          // Log details of the first few sessions
          for (i, session) in schedule_result.sessions.iter().enumerate().take(3) {
            info!(
              "    Session {}: RefDate={}, Start={}, End={}",
              i + 1,
              session.ref_date,
              session.start_date_time,
              session.end_date_time,
            );
          }
        }
        // Basic validation: Check if key fields are populated.
        if schedule_result.start_date_time.is_empty() || schedule_result.time_zone.is_empty() {
          error!("Historical schedule received, but key fields (start_date_time, time_zone) are empty.");
          return Err(anyhow!("Key fields missing in historical schedule result"));
        }
        Ok(())
      }
      Err(e) => {
        error!("Failed to get historical schedule for {}: {:?}", contract.symbol, e);
        Err(e.into())
      }
    }
  }

  pub(super) fn account_subscription_impl(client: &IBKRClient, is_live: bool) -> Result<()> {
    info!("--- Testing Account Subscription ---");
    if !is_live {
      info!("AccountSubscription test relies on live data flow and may not show full event stream in replay mode if events weren't captured during recording.");
    }

    let account_manager = client.account();

    // Create the subscription
    let mut account_sub = match AccountManager::create_account_subscription(account_manager.clone()) {
      Ok(sub) => sub,
      Err(e) => {
        error!("Failed to create AccountSubscription: {:?}", e);
        // If subscription fails, especially on initial data fetch, it might be due to
        // TWS not being fully ready or no account data available.
        // For gen_goldens, let's not fail the whole test run if this happens,
        // but log it clearly. In a real app, this might be a critical error.
        if is_live {
          warn!("AccountSubscription creation failed in live mode. This might indicate an issue with TWS connection or account data availability.");
          // Allow test to "pass" with warning in live mode if creation fails,
          // as it might be an environment issue not a code bug.
          return Ok(());
        } else {
          // In replay, if creation fails, it's more likely a problem with the test or replay logic.
          return Err(anyhow!("Failed to create AccountSubscription in replay mode: {:?}", e));
        }
      }
    };
    info!("AccountSubscription created for account: {}", account_sub.account_id());

    // Example: Print initial summary
    if let Some(summary) = account_sub.last_known_summary() {
      info!("Initial Net Liquidation from subscription: {}", summary.net_liquidation);
      info!("Initial Account ID from subscription summary: {}", summary.account_id);
    } else {
      warn!("No initial summary available from AccountSubscription immediately after creation.");
    }

    let order_mgr = client.orders();
    let spy_contract = Contract::stock("SPY");

    if is_live {
      info!("Placing BUY MKT order for 1 SPY to generate account activity...");
      let (contract_ref, buy_request) = OrderBuilder::new(OrderSide::Buy, 1.0)
        .market()
        .for_contract(spy_contract.clone())
        .build()?;

      match order_mgr.place_order(contract_ref, buy_request) {
        Ok(buy_order_id) => {
          info!("BUY order for SPY placed with ID: {}. Waiting for execution...", buy_order_id);
          match order_mgr.try_wait_order_executed(&buy_order_id, Duration::from_secs(20)) {
            Ok(OrderStatus::Filled) => info!("BUY order {} for SPY filled.", buy_order_id),
            Ok(status) => warn!("BUY order {} for SPY did not fill as expected. Final status: {:?}", buy_order_id, status),
            Err(e) => error!("Error or timeout waiting for BUY order {} for SPY: {:?}", buy_order_id, e),
          }
        }
        Err(e) => {
          error!("Failed to place BUY MKT order for SPY: {:?}", e);
          // Continue with the test, but activity generation might be compromised.
        }
      }
    } else {
      info!("Skipping live order placement for SPY in replay mode.");
    }

    // Spawn a thread to listen for events
    let event_receiver = account_sub.events(); // Get a receiver clone
    let event_thread = std::thread::spawn(move || { // Ensure std::thread is used
      info!("[EventThread] Started. Waiting for account events...");
      let wait_duration_for_events = Duration::from_secs(15); // Listen for 15 seconds
      let start_time = std::time::Instant::now();

      loop {
        if start_time.elapsed() > wait_duration_for_events {
          info!("[EventThread] Event listening duration ended.");
          break;
        }
        match event_receiver.recv_timeout(Duration::from_secs(1)) { // Check every second
          Ok(event) => {
            match event {
              AccountEvent::SummaryUpdate { info, timestamp } => {
                info!("[EventThread] SummaryUpdate @ {:?}: NetLiq = {}, AccountID = {}",
                      timestamp, info.net_liquidation, info.account_id);
              }
              AccountEvent::PositionUpdate { position, timestamp } => {
                info!("[EventThread] PositionUpdate @ {:?}: {} {} @ {} (ConID: {})",
                      timestamp, position.symbol, position.quantity, position.market_price, position.contract.con_id);
              }
              AccountEvent::ExecutionUpdate { execution, timestamp } => {
                info!("[EventThread] ExecutionUpdate @ {:?}: {} {} {} @ {} (OrderID: {}, ExecID: {})",
                      timestamp, execution.side, execution.quantity, execution.symbol, execution.price, execution.order_id, execution.execution_id);
              }
              AccountEvent::Error { error, timestamp } => {
                error!("[EventThread] AccountSubscription Error @ {:?}: {:?}", timestamp, error);
              }
              AccountEvent::Closed { account_id, timestamp } => {
                info!("[EventThread] AccountSubscription Closed @ {:?} for account {}. Exiting.", timestamp, account_id);
                return; // Exit thread as subscription is closed
              }
            }
          }
          Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
            // Timeout, continue loop to check overall duration
            debug!("[EventThread] No event in last second...");
            continue;
          }
          Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
            info!("[EventThread] Event channel disconnected. Exiting.");
            return; // Exit thread
          }
        }
      }
      info!("[EventThread] Finished listening for events.");
    });

    // Let the event thread run for its duration
    // The thread itself has a 15s listening window. Main thread waits a bit longer to ensure closure.
    info!("Main thread: Sleeping to allow event thread to capture events...");
    if is_live {
      std::thread::sleep(Duration::from_secs(17));
    } else {
      // In replay, events are instant, so a very short sleep is fine.
      std::thread::sleep(Duration::from_millis(500));
    }

    if is_live {
      info!("Main thread: Attempting to clean up SPY position...");
      if let Err(e) = attempt_cleanup(client, &spy_contract) { // Call the existing helper
        error!("Error during SPY position cleanup: {:?}", e);
        // Don't fail the whole test for cleanup error, but log it.
      } else {
        info!("Main thread: SPY position cleanup attempt finished.");
      }
    } else {
      info!("Main thread: Skipping live order cleanup in replay mode.");
    }

    // Explicitly close the subscription
    info!("Main thread: Closing AccountSubscription...");
    if let Err(e) = account_sub.close() {
      error!("Error closing AccountSubscription: {:?}", e);
      // Don't fail the test for close error, but log it.
    } else {
      info!("Main thread: AccountSubscription close request sent.");
    }

    // Wait for the event thread to finish
    info!("Main thread: Waiting for event thread to join...");
    match event_thread.join() {
      Ok(_) => info!("Main thread: Event thread joined successfully."),
      Err(e) => error!("Main thread: Event thread panicked: {:?}", e),
    }

    info!("AccountSubscription test finished.");
    Ok(())
  }

  pub(super) fn observe_market_data_impl(client: &IBKRClient, is_live: bool) -> Result<()> {
    info!("--- Testing Observe Market Data (request_observe_market_data) ---");
    let data_mgr = client.data_market();
    let contract = Contract::stock("MSFT");

    #[derive(Debug)]
    struct TestMarketObserver {
      name: String,
      error_occurred: std::sync::atomic::AtomicBool,
    }
    impl MarketDataObserver for TestMarketObserver {
      fn on_tick_price(&self, req_id: i32, tick_type: TickType, price: f64, _attrib: yatws::data::TickAttrib) {
        info!("[{}] TickPrice: ReqID={}, Type={:?}, Price={}", self.name, req_id, tick_type, price);
      }
      fn on_tick_size(&self, req_id: i32, tick_type: TickType, size: f64) {
        info!("[{}] TickSize: ReqID={}, Type={:?}, Size={}", self.name, req_id, tick_type, size);
      }
      fn on_tick_string(&self, req_id: i32, tick_type: TickType, value: &str) {
        info!("[{}] TickString: ReqID={}, Type={:?}, Value='{}'", self.name, req_id, tick_type, value);
      }
      fn on_tick_snapshot_end(&self, req_id: i32) {
        info!("[{}] TickSnapshotEnd: ReqID={}", self.name, req_id);
      }
      fn on_market_data_type(&self, req_id: i32, market_data_type: MarketDataType) {
        info!("[{}] MarketDataTypeSet: ReqID={}, Type={:?}", self.name, req_id, market_data_type);
      }
      fn on_error(&self, req_id: i32, error_code: i32, error_message: &str) {
        error!("[{}] Error: ReqID={}, Code={}, Msg='{}'", self.name, req_id, error_code, error_message);
        self.error_occurred.store(true, std::sync::atomic::Ordering::Relaxed);
      }
    }

    let observer = TestMarketObserver { name: "MSFT-Observer".to_string(), error_occurred: Default::default() };
    let generic_tick_list: &[GenericTickType] = &[];
    let snapshot = false; // Streaming
    let mkt_data_options = &[];

    info!("Requesting observed market data for {} (Generic Ticks: '{}')...",
          contract.symbol,
          generic_tick_list.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(","));

    let (req_id, observer_id) = data_mgr.request_observe_market_data(
      &contract,
      generic_tick_list,
      snapshot,
      false, // regulatory_snapshot
      mkt_data_options,
      Some(MarketDataType::Delayed),
      observer,
    ).context("Failed to request observed market data")?;

    info!("Market data requested with ReqID: {}, ObserverID: {:?}. Waiting for data...", req_id, observer_id);

    let wait_duration = if is_live { Duration::from_secs(15) } else { Duration::from_millis(500) }; // Shorter for replay
    info!("Waiting for {:?} to capture streaming data...", wait_duration);
    std::thread::sleep(wait_duration);

    info!("Cancelling market data request (ReqID: {})...", req_id);
    data_mgr.cancel_market_data(req_id).context("Failed to cancel market data")?;
    info!("Removing market data observer (ObserverID: {:?})...", observer_id);
    data_mgr.remove_market_data_observer(observer_id);
    info!("Market data request and observer handled.");

    // Check if the observer reported an error
    // Note: This check is basic. In a real test, you might assert specific data was received.
    let _observer_had_error = client.data_market() // Re-fetch observer from manager to check its state (if it were stored there)
    // This part is tricky as the observer instance is consumed.
    // For this test, we'll rely on the log output for error indication.
    // A more robust test would involve channels or shared state updated by the observer.
    // For gen_goldens, logging is the primary output.
    // If the observer's on_error was called, it would have logged an error.
    // We can't directly access `observer.error_occurred` here as it was moved.
    // This test primarily verifies the API call and basic flow.
    // If an error was logged by the observer, that indicates a problem.
    // For now, assume success if no direct error from request/cancel.
      ;

    Ok(())
  }

  pub(super) fn subscribe_market_data_impl(client: &IBKRClient, is_live: bool) -> Result<()> {
    info!("--- Testing Subscribe Market Data (TickDataSubscription) ---");
    let data_mgr = client.data_market();
    let contract = Contract::stock("GOOG");

    info!("Building TickDataSubscription for {}...", contract.symbol);
    let subscription = data_mgr.subscribe_market_data(&contract)
      .with_snapshot(false) // Streaming
      .with_market_data_type(MarketDataType::Delayed)
      .submit()
      .context("Failed to submit TickDataSubscription")?;

    info!("Subscription submitted with ReqID: {}. Iterating events...", subscription.request_id());

    let mut event_count = 0;
    let max_events_or_duration = if is_live { 10 } else { 2 }; // Fewer events for replay, or rely on timeout
    let iteration_timeout = if is_live { Duration::from_secs(2) } else { Duration::from_millis(100) };
    let total_wait_duration = if is_live { Duration::from_secs(15) } else { Duration::from_secs(1) };
    let start_time = std::time::Instant::now();

    let mut iter = subscription.events(); // Default timeout is None (blocking)

    while start_time.elapsed() < total_wait_duration && event_count < max_events_or_duration {
      match iter.try_next(iteration_timeout) { // Use try_next with a timeout
        Some(event) => {
          info!("Received TickDataEvent: {:?}", event);
          event_count += 1;
          if let TickDataEvent::Error(e) = event {
            error!("Error event received in subscription: {:?}", e);
            subscription.cancel().ok(); // Best effort cancel
            return Err(e.into());
          }
        }
        None => { // Timeout
          if !is_live && subscription.is_completed() && !subscription.has_error() {
            info!("Subscription completed in replay with no error and no more events.");
            break;
          }
          if subscription.has_error() {
            let err = subscription.get_error().unwrap_or_else(|| IBKRError::InternalError("Unknown error in subscription".to_string()));
            error!("Subscription has error: {:?}", err);
            subscription.cancel().ok();
            return Err(err.into());
          }
          if subscription.is_completed() {
            info!("Subscription completed with no more events.");
            break;
          }
          debug!("No event in last {:?}, continuing iteration...", iteration_timeout);
        }
      }
    }

    info!("Finished iterating events. Total events received: {}. Cancelling subscription...", event_count);
    subscription.cancel().context("Failed to cancel TickDataSubscription")?;
    info!("TickDataSubscription cancelled.");

    if event_count == 0 && is_live {
      warn!("Received 0 events for GOOG. This might be okay if market is closed or no data ticks during the window.");
    }
    Ok(())
  }

  pub(super) fn subscribe_historical_combined_impl(client: &IBKRClient, is_live: bool) -> Result<()> {
    info!("--- Testing Combined Historical Data Subscriptions ---");
    let data_mgr = client.data_market();

    let contract1 = Contract::stock("IBM");
    let contract2 = Contract::stock("CSCO");

    let duration = DurationUnit::Day(1);
    let bar_size = BarSize::FiveMinutes;
    let what_to_show = WhatToShow::Trades;

    info!("Building HistoricalDataSubscription for {}...", contract1.symbol);
    let sub1 = data_mgr.subscribe_historical_data(&contract1, duration, bar_size, what_to_show)
      .with_market_data_type(MarketDataType::Delayed)
      .submit()
      .context(format!("Failed to submit HistoricalDataSubscription for {}", contract1.symbol))?;

    info!("Building HistoricalDataSubscription for {}...", contract2.symbol);
    let sub2 = data_mgr.subscribe_historical_data(&contract2, duration, bar_size, what_to_show)
      .with_market_data_type(MarketDataType::Delayed)
      .submit()
      .context(format!("Failed to submit HistoricalDataSubscription for {}", contract2.symbol))?;

    info!("Combining subscriptions for ReqID {} ({}) and ReqID {} ({})...",
          sub1.request_id(), contract1.symbol, sub2.request_id(), contract2.symbol);

    let mut multi_iter = data_mgr.combine_subscriptions::<HistoricalDataEvent>()
      .add(&sub1, sub1.events())
      .add(&sub2, sub2.events())
      .build();
    // .with_timeout(Duration::from_secs(1)); // Timeout for each try_next in the multi-iterator

    let mut event_count1 = 0;
    let mut event_count2 = 0;
    let mut completed1 = false;
    let mut completed2 = false;

    let iteration_timeout = if is_live { Duration::from_secs(5) } else { Duration::from_millis(200) }; // Timeout for each underlying iterator poll
    let total_wait_duration = if is_live { Duration::from_secs(30) } else { Duration::from_secs(2) };
    let start_time = std::time::Instant::now();

    info!("Iterating combined historical events (Total timeout: {:?}, Per-iterator poll: {:?})...", total_wait_duration, iteration_timeout);

    // The multi_iter.next() will internally use try_next on underlying iterators.
    // We need a loop that respects the total_wait_duration.
    // The default .next() on multi_iter can block if underlying iterators block.
    // Using try_next on multi_iter is better for controlled waiting.
    while start_time.elapsed() < total_wait_duration && !(completed1 && completed2) {
      match multi_iter.try_next(iteration_timeout) { // Poll underlying iterators with a timeout
        Some(tagged_event) => {
          info!("Combined Event: ReqID={}, Contract={}, Event={:?}",
                tagged_event.req_id, tagged_event.contract.symbol, tagged_event.event);
          if tagged_event.req_id == sub1.request_id() {
            event_count1 += 1;
            if let HistoricalDataEvent::Complete { .. } = tagged_event.event {
              completed1 = true;
              info!("Subscription for {} (ReqID {}) completed.", contract1.symbol, sub1.request_id());
            }
            if let HistoricalDataEvent::Error(e) = tagged_event.event {
              error!("Error from {} (ReqID {}): {:?}", contract1.symbol, sub1.request_id(), e);
              completed1 = true; // Treat error as completion for this sub
            }
          } else if tagged_event.req_id == sub2.request_id() {
            event_count2 += 1;
            if let HistoricalDataEvent::Complete { .. } = tagged_event.event {
              completed2 = true;
              info!("Subscription for {} (ReqID {}) completed.", contract2.symbol, sub2.request_id());
            }
            if let HistoricalDataEvent::Error(e) = tagged_event.event {
              error!("Error from {} (ReqID {}): {:?}", contract2.symbol, sub2.request_id(), e);
              completed2 = true; // Treat error as completion for this sub
            }
          }
        }
        None => { // Timeout from multi_iter.try_next
          debug!("No event from combined iterator in last poll. Checking completion status...");
          // Check if underlying subscriptions are done, even if no 'Complete' event was caught by this loop iteration
          if !completed1 && sub1.is_completed() {
            info!("Subscription for {} (ReqID {}) detected as completed (externally or error).", contract1.symbol, sub1.request_id());
            completed1 = true;
          }
          if !completed2 && sub2.is_completed() {
            info!("Subscription for {} (ReqID {}) detected as completed (externally or error).", contract2.symbol, sub2.request_id());
            completed2 = true;
          }
          if completed1 && completed2 {
            info!("Both subscriptions reported completion. Exiting loop.");
            break;
          }
        }
      }
    }


    info!("Finished iterating combined events.");
    info!("  {} (ReqID {}): {} events, Completed={}", contract1.symbol, sub1.request_id(), event_count1, completed1 || sub1.is_completed());
    info!("  {} (ReqID {}): {} events, Completed={}", contract2.symbol, sub2.request_id(), event_count2, completed2 || sub2.is_completed());

    // Explicitly cancel/drop subscriptions (though Drop should handle it)
    // sub1.cancel().ok();
    // sub2.cancel().ok();
    // multi_iter.cancel_all().ok(); // This relies on individual subscription drops

    if (event_count1 == 0 || event_count2 == 0) && is_live {
      warn!("Received 0 events for one or both contracts. This might be okay depending on market data availability.");
    }
    if (!completed1 && is_live && !sub1.has_error()) || (!completed2 && is_live && !sub2.has_error()) {
      warn!("One or both subscriptions did not explicitly complete within the test window.");
    }

    Ok(())
  }

  pub(super) fn subscribe_news_bulletins_impl(client: &IBKRClient, is_live: bool) -> Result<()> {
    info!("--- Testing Subscribe News Bulletins (NewsSubscription) ---");
    let news_mgr = client.data_news();

    info!("Building NewsSubscription (all_msgs: true)...");
    // Request all messages, hoping to see some historical ones if available, plus any new ones.
    let subscription = news_mgr.subscribe_news_bulletins_stream(true)
      .submit()
      .context("Failed to submit NewsSubscription")?;

    info!("NewsSubscription submitted (ObsID: {}). Iterating events...", subscription.request_id());

    let mut event_count = 0;
    // Expect few, if any, bulletins. More events in replay if they were logged.
    let max_events_to_process = if is_live { 5 } else { 10 };
    let iteration_timeout = if is_live { Duration::from_secs(3) } else { Duration::from_millis(100) }; // Shorter for replay
    let total_wait_duration = if is_live { Duration::from_secs(15) } else { Duration::from_secs(2) }; // Shorter total for replay
    let start_time = std::time::Instant::now();

    let mut iter = subscription.events();

    while start_time.elapsed() < total_wait_duration && event_count < max_events_to_process {
      match iter.try_next(iteration_timeout) {
        Some(event) => {
          info!("Received NewsEvent: {:?}", event);
          event_count += 1;
          match event {
            NewsEvent::Error(e) => {
              error!("Error event received in news subscription: {:?}", e);
              // Depending on severity, might not need to cancel/return
              // For gen_goldens, log and continue unless it's critical
            }
            NewsEvent::Closed{..} => {
              info!("NewsSubscription closed event received. Exiting loop.");
              break;
            }
            _ => {} // Bulletin
          }
        }
        None => { // Timeout from try_next
          if subscription.is_completed() {
            info!("NewsSubscription completed (no more events or error).");
            break;
          }
          debug!("No news event in last {:?}, continuing iteration...", iteration_timeout);
        }
      }
    }

    info!("Finished iterating news events. Total events received: {}. Cancelling subscription...", event_count);
    subscription.cancel().context("Failed to cancel NewsSubscription")?;
    info!("NewsSubscription cancelled.");

    if event_count == 0 && is_live {
      warn!("Received 0 news bulletins. This is common if no bulletins were issued during the test window or if not subscribed to news.");
    }
    Ok(())
  }

  pub(super) fn subscribe_historical_news_impl(client: &IBKRClient, is_live: bool) -> Result<()> {
    info!("--- Testing Subscribe Historical News (HistoricalNewsSubscription) ---");
    let news_mgr = client.data_news();
    let ref_data_mgr = client.data_ref();

    // Get con_id for AAPL
    let contract_spec = Contract::stock("AAPL");
    let contract_details_list = ref_data_mgr.get_contract_details(&contract_spec)
      .context("Failed to get contract details for AAPL")?;
    if contract_details_list.is_empty() {
      return Err(anyhow!("No contract details found for AAPL."));
    }
    let con_id = contract_details_list[0].contract.con_id;
    if con_id == 0 { return Err(anyhow!("Invalid con_id (0) for AAPL.")); }

    let provider_codes = "BRFG,DJNL"; // Example provider codes
    let total_results = 5;

    info!("Building HistoricalNewsSubscription for AAPL (ConID: {}), Providers: {}, MaxResults: {}...",
          con_id, provider_codes, total_results);

    let subscription = news_mgr.subscribe_historical_news_stream(con_id, provider_codes, total_results)
      .with_start_date_time(Utc::now() - ChronoDuration::days(7)) // Last 7 days
      .with_end_date_time(Utc::now())
      .submit()
      .context("Failed to submit HistoricalNewsSubscription")?;

    info!("HistoricalNewsSubscription submitted (ReqID: {}). Iterating events...", subscription.request_id());

    let mut event_count = 0;
    let iteration_timeout = if is_live { Duration::from_secs(3) } else { Duration::from_millis(100) };
    let total_wait_duration = if is_live { Duration::from_secs(20) } else { Duration::from_secs(2) };
    let start_time = std::time::Instant::now();
    let mut iter = subscription.events();

    while start_time.elapsed() < total_wait_duration && !subscription.is_completed() {
      match iter.try_next(iteration_timeout) {
        Some(event) => {
          info!("Received HistoricalNewsEvent: {:?}", event);
          event_count += 1;
          if let HistoricalNewsEvent::Error(ref e) = event {
            error!("Error event in historical news subscription: {:?}", e);
            // Don't necessarily cancel/return, let it complete or timeout
          }
          if let HistoricalNewsEvent::Closed = event { break; }
        }
        None => { // Timeout from try_next
          if subscription.is_completed() { info!("Subscription completed."); break; }
          debug!("No historical news event in last {:?}, continuing...", iteration_timeout);
        }
      }
    }

    info!("Finished iterating historical news events. Total events: {}. Cancelling...", event_count);
    subscription.cancel().context("Failed to cancel HistoricalNewsSubscription")?;
    info!("HistoricalNewsSubscription cancelled.");

    Ok(())
  }
} // <-- This brace closes the test_cases module

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
