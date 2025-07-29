// yatws/test_news.rs
use anyhow::{anyhow, Context, Result};
use log::{debug, error, info, warn};
use std::time::Duration;
use chrono::{Utc, Duration as ChronoDuration};
use yatws::{
  IBKRError,
  IBKRClient,
  contract::Contract,
  data_subscription::{MarketDataSubscription, MarketDataIterator},
  news_subscription::{NewsEvent, HistoricalNewsEvent},
};

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
    info!("Available provider codes: {}", provider_codes);
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

  // Free:
  // Briefing.com Analyst Actions (BRFUPDN)
  // Briefing.com General Market Columns (BRFG)
  // Dow Jones Newsletters (DJNL)
  let provider_code = "BRFG";
  info!(
    "Requesting historical news for AAPL (con_id {}), providers [{}], last {} days, max {} results.",
    con_id, provider_codes, 7, total_results
  );

  match news_mgr.get_historical_news(
    con_id,
    &provider_code,
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

pub(super) fn subscribe_news_bulletins_impl(client: &IBKRClient, is_live: bool) -> Result<()> {
  info!("--- Testing Subscribe News Bulletins (NewsSubscription) ---");
  let news_mgr = client.data_news();

  info!("Building NewsSubscription (all_msgs: true)...");
  // Request all messages, hoping to see some historical ones if available, plus any new ones.
  let mut subscription = news_mgr.subscribe_news_bulletins_stream(true)
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

  let provider_code = "BRFG"; // Briefing.com.
  let total_results = 5;

  info!("Building HistoricalNewsSubscription for AAPL (ConID: {}), Providers: {}, MaxResults: {}...",
        con_id, provider_code, total_results);

  let mut subscription = news_mgr.subscribe_historical_news_stream(con_id, provider_code, total_results)
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
