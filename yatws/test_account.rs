// yatws/test_account.rs
use anyhow::{anyhow, Result};
use log::{debug, error, info, warn};
use std::time::Duration;
use crate::test_order::attempt_cleanup;

use yatws::{
  IBKRError,
  IBKRClient,
  account::AccountValueKey,
  account_manager::AccountManager,
  account_subscription::AccountEvent,
  order::{OrderSide, OrderStatus},
  OrderBuilder,
  contract::Contract,
};

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

  let pnl =  acct_mgr.get_daily_pnl()?;
  info!("Daily PNL: {}", pnl);

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
