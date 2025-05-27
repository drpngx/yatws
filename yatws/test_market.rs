// yatws/test_market.rs
use anyhow::{anyhow, Context, Result};
use log::{error, info, warn};
use std::time::Duration;
use chrono::{Utc, Duration as ChronoDuration};
use yatws::{
  IBKRError,
  IBKRClient,
  contract::{Contract, WhatToShow, SecType},
  data::{MarketDataType, TickType, GenericTickType, TickByTickRequestType, DurationUnit, TimePeriodUnit},
};

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
  let timeout = Duration::from_secs(30); // Timeout for receiving ticks

  info!(
    "Requesting blocking tick-by-tick data for {} (Type={}, Timeout={:?}). Waiting for 5 ticks...",
    contract.symbol, tick_type, timeout // tick_type will use Display
  );

  let target_ticks = 3;

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
  // FX market depth is free.
  let contract = Contract {
    symbol: "EUR".to_string(),
    exchange: "IDEALPRO".to_string(),
    sec_type: SecType::Forex,
    currency: "GBP".to_string(),
    ..Default::default()
  };
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
