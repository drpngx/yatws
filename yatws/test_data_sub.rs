// yatws/test_data_sub.rs
use anyhow::{Context, Result, anyhow};
use log::{debug, error, info, warn};
use std::time::Duration;
use chrono::{TimeZone, Utc};
use yatws::{
  IBKRError,
  IBKRClient,
  contract::{Contract, WhatToShow, BarSize, SecType},
  data::{MarketDataType, DurationUnit, TickByTickRequestType},
  data_subscription::{
    HistoricalDataEvent,
    MarketDataIterator,
    MarketDataSubscription,
    TickDataEvent,
    TickByTickEvent,
    MarketDepthEvent
  },
};

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

pub(super) fn subscribe_historical_keep_up_to_date_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
  info!("--- Testing Subscribe Historical Data with keep_up_to_date ---");
  let symbol = "GOOG";
  let seconds_ago = 180;

  let contract = Contract::stock(symbol);
  let bars_event = client
    .data_market()
    .subscribe_historical_data(
      &contract,
      yatws::data::DurationUnit::Second(seconds_ago),
      yatws::contract::BarSize::FifteenSeconds,
      yatws::contract::WhatToShow::Trades,
    )
    .with_use_rth(true)
    .with_keep_up_to_date(true)
    .with_market_data_type(MarketDataType::RealTime)
    .submit()?;

  {
    let mut completed_bars = false;
    let mut bars = bars_event.events();
    // The udpates will come at 5 second intervals. We are asking for 15 seconds bars.
    // So, if we ask for 16 bars, we will get:
    //  - a complete signal to show that we are done with the past
    //  - at most 3 updates for the current bar, and
    //  - at least 12 updates in the future.
    // This is only true if the stock actually trades in all bars, otherwise the bar may not
    // close. There is no marker for a complete bar, you have to figure it out when the time changes.
    const NUM_BAR_UPDATES: usize = 16;
    const MIN_NUM_FUTURE_BAR_UPDATES: usize = 12;
    let mut bar_updates = 0;
    let mut future_bar_updates = 0;
    let mut num_complete_signals = 0;
    let now = Utc::now();
    while bar_updates < NUM_BAR_UPDATES {
      match bars.next() {
        Some(evt) => match evt {
          HistoricalDataEvent::Bar(bar) => {
            assert!(!completed_bars, "Got a historical bar after complete");
            log::info!("Got history bar: {:?}", bar);
          }
          HistoricalDataEvent::UpdateBar(bar) => {
            assert!(completed_bars);
            log::info!("Future bar: {:?}", bar);
            bar_updates += 1;
            if bar.time < now {
              // We are completing the first bar.
              assert!(num_complete_signals == 1, "Bar time in the past but we have received a Complete: bar.time = {}", bar.time);
            } else {
              future_bar_updates += 1;
            }
          }
          HistoricalDataEvent::Complete { .. } => {
            if completed_bars {
              log::error!("Something is wrong: multiple Complete signals");
            }
            completed_bars = true;
            num_complete_signals += 1;
            log::info!("History complete, further updates will be current bars");
          }
          HistoricalDataEvent::Error(e) => {
            log::info!("Error: {}", e);
          }
        },
        None => {
          log::info!("Ignoring timeout");
        }
      }
    }

    let mut err = String::new();
    if future_bar_updates < MIN_NUM_FUTURE_BAR_UPDATES {
      err.push_str(&format!("Not enough future updates: expected {}, got: {}",
                            MIN_NUM_FUTURE_BAR_UPDATES, future_bar_updates));
    }
    if num_complete_signals > 1 {
      if !err.is_empty() {
        err.push_str("\n");
      }
      err.push_str(&format!("Too many Completed events: {}", num_complete_signals));
    }
    if !err.is_empty() {
      return Err(anyhow::anyhow!(err));
    }
  }

  Ok(())
}

pub(super) fn subscribe_number_of_ticks_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
  info!("--- Testing Subscribe number of ticks ---");
  let contract = Contract::stock("AAPL");

  let num_historical = 10;
  let min_historical = 8;
  let now = Utc::now();
  let sub_l1 = client
    .data_market()
    .subscribe_tick_by_tick(&contract, yatws::data::TickByTickRequestType::BidAsk)
    .with_ignore_size(true)
    .with_number_of_ticks(num_historical)
    .submit()
    .expect("Failed to subscribe to l1 data");

  let mut num_received = 0;
  let mut num_historical_ticks = 0;
  for tick in sub_l1.events() {
    match tick {
      TickByTickEvent::BidAsk {
        time,
        bid_price,
        ask_price,
        ..
      } => {
        let tick_time = Utc.timestamp_opt(time, 0).single().unwrap();
        log::info!("[{num_received}] Received L1 tick at {tick_time}: {bid_price}:{ask_price}");
        if tick_time < now {
          num_historical_ticks += 1;
        }
        num_received += 1;
        if num_received >= num_historical + 3 {
          break;
        }
      }
      TickByTickEvent::Error(e) => return Err(anyhow!(e.to_string())),
      _ => log::info!("Other tick received!"),
    }
  }

  if num_historical_ticks < min_historical {
    return Err(anyhow!(
      "Expected at least {min_historical} historical ticks, but got {num_historical_ticks}"
    ));
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

pub(super) fn subscribe_real_time_bars_impl(client: &IBKRClient, is_live: bool) -> Result<()> {
  info!("--- Testing Subscribe Real Time Bars ---");
  let data_mgr = client.data_market();
  let contract = Contract::stock("AAPL");
  let what_to_show = WhatToShow::Trades;

  info!("Building RealTimeBarSubscription for {}...", contract.symbol);
  let subscription = data_mgr.subscribe_real_time_bars(&contract, what_to_show)
    .with_use_rth(true)
    .submit()
    .context("Failed to submit RealTimeBarSubscription")?;

  info!("Subscription submitted with ReqID: {}. Iterating events...", subscription.request_id());

  let mut event_count = 0;
  let max_events = if is_live { 3 } else { 1 }; // 5-second bars, so fewer events needed
  let iteration_timeout = if is_live { Duration::from_secs(6) } else { Duration::from_millis(100) };
  let total_wait_duration = if is_live { Duration::from_secs(20) } else { Duration::from_secs(1) };
  let start_time = std::time::Instant::now();

  let mut iter = subscription.events();

  while start_time.elapsed() < total_wait_duration && event_count < max_events {
    match iter.try_next(iteration_timeout) {
      Some(bar) => {
        info!("Received Bar: Time={}, O={}, H={}, L={}, C={}, V={}",
              bar.time.format("%H:%M:%S"), bar.open, bar.high, bar.low, bar.close, bar.volume);
        event_count += 1;
      }
      None => {
        if subscription.is_completed() {
          info!("Subscription completed with no more events.");
          break;
        }
        if subscription.has_error() {
          let err = subscription.get_error().unwrap_or_else(||
                                                            IBKRError::InternalError("Unknown error in subscription".to_string()));
          error!("Subscription has error: {:?}", err);
          subscription.cancel().ok();
          return Err(err.into());
        }
        debug!("No event in last {:?}, continuing iteration...", iteration_timeout);
      }
    }
  }

  info!("Finished iterating events. Total events received: {}. Cancelling subscription...", event_count);
  subscription.cancel().context("Failed to cancel RealTimeBarSubscription")?;
  info!("RealTimeBarSubscription cancelled.");

  if event_count == 0 && is_live {
    warn!("Received 0 real-time bars for AAPL. This might be okay if market is closed.");
  }
  Ok(())
}

// 2. Test for subscribe_tick_by_tick
pub(super) fn subscribe_tick_by_tick_impl(client: &IBKRClient, is_live: bool) -> Result<()> {
  info!("--- Testing Subscribe Tick By Tick ---");
  let data_mgr = client.data_market();
  let contract = Contract::stock_with_exchange("MSFT", "SMART", "USD");
  let tick_type = TickByTickRequestType::BidAsk;

  info!("Building TickByTickSubscription for {}...", contract.symbol);
  let subscription = data_mgr.subscribe_tick_by_tick(&contract, tick_type)
    .with_number_of_ticks(0) // Streaming
    .with_market_data_type(MarketDataType::Delayed)
    .submit()
    .context("Failed to submit TickByTickSubscription")?;

  info!("Subscription submitted with ReqID: {}. Iterating events...", subscription.request_id());

  let mut event_count = 0;
  let max_events = if is_live { 10 } else { 2 };
  let iteration_timeout = if is_live { Duration::from_secs(2) } else { Duration::from_millis(100) };
  let total_wait_duration = if is_live { Duration::from_secs(15) } else { Duration::from_secs(1) };
  let start_time = std::time::Instant::now();

  let mut iter = subscription.events();

  while start_time.elapsed() < total_wait_duration && event_count < max_events {
    match iter.try_next(iteration_timeout) {
      Some(event) => {
        info!("Received TickByTickEvent: {:?}", event);
        event_count += 1;
        if let TickByTickEvent::Error(e) = event {
          error!("Error event received in subscription: {:?}", e);
          subscription.cancel().ok();
          return Err(e.into());
        }
      }
      None => {
        if subscription.is_completed() {
          info!("Subscription completed with no more events.");
          break;
        }
        if subscription.has_error() {
          let err = subscription.get_error().unwrap_or_else(||
                                                            IBKRError::InternalError("Unknown error in subscription".to_string()));
          error!("Subscription has error: {:?}", err);
          subscription.cancel().ok();
          return Err(err.into());
        }
        debug!("No event in last {:?}, continuing iteration...", iteration_timeout);
      }
    }
  }

  info!("Finished iterating events. Total events received: {}. Cancelling subscription...", event_count);
  subscription.cancel().context("Failed to cancel TickByTickSubscription")?;
  info!("TickByTickSubscription cancelled.");

  if event_count == 0 && is_live {
    warn!("Received 0 tick-by-tick events for MSFT. This might be okay if market is closed.");
  }
  Ok(())
}

// 3. Test for subscribe_market_depth
pub(super) fn subscribe_market_depth_impl(client: &IBKRClient, is_live: bool) -> Result<()> {
  info!("--- Testing Subscribe Market Depth ---");
  let data_mgr = client.data_market();
  // FX market depth is free.
  let contract = Contract {
    symbol: "EUR".to_string(),
    exchange: "IDEALPRO".to_string(),
    sec_type: SecType::Forex,
    currency: "GBP".to_string(),
    ..Default::default()
  };
  let num_rows = 5;

  info!("Building MarketDepthSubscription for {}...", contract.symbol);
  let subscription = data_mgr.subscribe_market_depth(&contract, num_rows)
    .with_smart_depth(false)
    .submit()
    .context("Failed to submit MarketDepthSubscription")?;

  info!("Subscription submitted with ReqID: {}. Iterating events...", subscription.request_id());

  let mut event_count = 0;
  let max_events = if is_live { 10 } else { 2 };
  let iteration_timeout = if is_live { Duration::from_secs(2) } else { Duration::from_millis(100) };
  let total_wait_duration = if is_live { Duration::from_secs(15) } else { Duration::from_secs(1) };
  let start_time = std::time::Instant::now();

  let mut iter = subscription.events();

  while start_time.elapsed() < total_wait_duration && event_count < max_events {
    match iter.try_next(iteration_timeout) {
      Some(event) => {
        info!("Received MarketDepthEvent: {:?}", event);
        event_count += 1;
        if let MarketDepthEvent::Error(e) = event {
          error!("Error event received in subscription: {:?}", e);
          subscription.cancel().ok();
          return Err(e.into());
        }
      }
      None => {
        if subscription.is_completed() {
          info!("Subscription completed with no more events.");
          break;
        }
        if subscription.has_error() {
          let err = subscription.get_error().unwrap_or_else(||
                                                            IBKRError::InternalError("Unknown error in subscription".to_string()));
          error!("Subscription has error: {:?}", err);
          subscription.cancel().ok();
          return Err(err.into());
        }
        debug!("No event in last {:?}, continuing iteration...", iteration_timeout);
      }
    }
  }

  info!("Finished iterating events. Total events received: {}. Cancelling subscription...", event_count);
  subscription.cancel().context("Failed to cancel MarketDepthSubscription")?;
  info!("MarketDepthSubscription cancelled.");

  if event_count == 0 && is_live {
    warn!("Received 0 market depth events for IBM. This might be okay if market is closed.");
  }
  Ok(())
}

pub(super) fn multi_subscription_mixed_impl(client: &IBKRClient, is_live: bool) -> Result<()> {
  info!("--- Testing Multi-Subscription for Same Event Types ---");
  let data_mgr = client.data_market();

  // Create two market data subscriptions for different symbols
  info!("Creating market data subscriptions for AAPL and MSFT...");
  let aapl_sub = data_mgr.subscribe_market_data(&Contract::stock("AAPL"))
    .with_market_data_type(MarketDataType::Delayed)
    .submit()
    .context("Failed to submit AAPL market data subscription")?;

  let msft_sub = data_mgr.subscribe_market_data(&Contract::stock("MSFT"))
    .with_market_data_type(MarketDataType::Delayed)
    .submit()
    .context("Failed to submit MSFT market data subscription")?;

  info!("Setting up multi-subscription iterator...");

  // Create the multi-subscription iterator, properly adding both subscriptions
  let mut multi_iter = data_mgr.combine_subscriptions::<TickDataEvent>()
    .add(&aapl_sub, aapl_sub.events())
    .add(&msft_sub, msft_sub.events())
    .build()
    .with_timeout(Duration::from_millis(100));

  info!("Successfully created multi-subscription with both market data subscriptions");
  info!("Consuming events from the combined iterator...");

  let mut event_count = 0;
  let mut aapl_events = 0;
  let mut msft_events = 0;
  let max_events = if is_live { 10 } else { 4 };
  let wait_duration = if is_live { Duration::from_secs(15) } else { Duration::from_secs(1) };
  let start_time = std::time::Instant::now();

  while start_time.elapsed() < wait_duration && event_count < max_events {
    // Use the multi-iterator to get events from either subscription
    match multi_iter.try_next(Duration::from_millis(200)) {
      Some(tagged_event) => {
        info!("Got event from symbol {} (ReqID: {}): {:?}",
              tagged_event.contract.symbol,
              tagged_event.req_id,
              tagged_event.event);

        event_count += 1;

        // Track which symbol we got the event for
        if tagged_event.req_id == aapl_sub.request_id() {
          aapl_events += 1;
        } else if tagged_event.req_id == msft_sub.request_id() {
          msft_events += 1;
        }
      }
      None => {
        // No event available, check if both subscriptions are completed
        if aapl_sub.is_completed() && msft_sub.is_completed() {
          info!("Both subscriptions are completed. Exiting event loop.");
          break;
        }
        debug!("No event available in latest poll. Continuing...");
      }
    }
  }

  info!("Cancelling subscriptions...");
  aapl_sub.cancel().ok();
  msft_sub.cancel().ok();

  info!("Multi-subscription test completed:");
  info!("  Total Events: {}", event_count);
  info!("  AAPL Events: {}", aapl_events);
  info!("  MSFT Events: {}", msft_events);

  if event_count == 0 && is_live {
    warn!("Received 0 events from both subscriptions. This might indicate market is closed.");
  }

  Ok(())
}
