// yatws/test_order.rs
use anyhow::{anyhow, Context, Result};
use log::{debug, error, info, warn};
use std::time::Duration;

use yatws::{
  IBKRError,
  IBKRClient,
  order::{OrderRequest, OrderSide, OrderType, TimeInForce, OrderStatus, OrderUpdates},
  OrderBuilder,
  OrderEvent,
  contract::{Contract, SecType, OptionRight, DateOrMonth},
  data::MarketDataType,
};

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

pub(super) fn order_modify_impl(client: &IBKRClient, is_live: bool) -> Result<()> {
  info!("--- Testing Modify Stop-Limit Order ---");
  if is_live {
    warn!("This test will fetch the price of MSFT, place a STP LMT order, modify it, then cancel.");
    warn!("Prices are calculated based on market price to avoid immediate fills.");
    std::thread::sleep(Duration::from_secs(3));
  }

  let order_mgr = client.orders();
  let market_data_mgr = client.data_market();
  let contract = Contract::stock("MSFT");

  // 1. Request the current price of MSFT to use as a base for order prices.
  info!("Requesting market data quote for MSFT...");
  let (_bid, ask, last) = market_data_mgr.get_quote(
    &contract,
    Some(MarketDataType::Delayed), // Use delayed data for testing
    Duration::from_secs(10)
  ).context("Failed to get quote for MSFT")?;

  let current_price = last.or(ask)
    .ok_or_else(|| anyhow!("Could not retrieve a valid last or ask price for MSFT"))?;
  info!("Retrieved current price for MSFT: {}", current_price);

  // Calculate prices safely away from the market to avoid execution.
  let initial_stop_price = (current_price * 0.95).round(); // 5% below
  let initial_limit_price = (current_price * 0.94).round(); // 6% below

  // 2. Create and place a Stop-Limit order using the calculated prices.
  let order_req = OrderRequest {
    side: OrderSide::Sell, // Use SELL to test with prices below market
    order_type: OrderType::StopLimit,
    quantity: 1.0,
    limit_price: Some(initial_limit_price),
    aux_price: Some(initial_stop_price),
    time_in_force: TimeInForce::Day,
    transmit: true,
    ..Default::default()
  };
  let order_id = order_mgr.place_order(contract.clone(), order_req)
    .context("Failed to place STP LMT order")?;
  info!("STP LMT order placed with ID: {} (STP: {}, LMT: {})", order_id, initial_stop_price, initial_limit_price);

  // 3. Wait for the order to be submitted.
  let submit_timeout = Duration::from_secs(10);
  info!("Waiting for order {} to be submitted...", order_id);
  match order_mgr.try_wait_order_submitted(&order_id, submit_timeout) {
    Ok(status) => info!("Order {} submitted with status: {:?}", order_id, status),
    Err(e) => {
      //let _ = order_mgr.cancel_order(&order_id);
      //return Err(e).context("Waiting for order submission failed");
      log::warn!("order not submitted, continuing");
    }
  }

  // 4. Modify the order with new prices.
  let modified_stop_price = (current_price * 0.93).round(); // 7% below
  let modified_limit_price = (current_price * 0.92).round(); // 8% below
  info!("Modifying order {} with new prices (STP: {}, LMT: {})...", order_id, modified_stop_price, modified_limit_price);

  let updates = OrderUpdates {
    limit_price: Some(modified_limit_price),
    aux_price: Some(modified_stop_price),
    ..Default::default()
  };
  order_mgr.modify_order(&order_id, updates).context("Failed to send order modification request")?;

  // 5. Verify the modification.
  std::thread::sleep(Duration::from_secs(2));
  info!("Verifying modification for order {}...", order_id);
  let order = order_mgr.get_order(&order_id).ok_or_else(|| anyhow!("Failed to get order after modification"))?;

  let updated_limit = order.request.limit_price.ok_or_else(|| anyhow!("Order has no limit price"))?;
  let updated_stop = order.request.aux_price.ok_or_else(|| anyhow!("Order has no stop price"))?;

  if (updated_limit - modified_limit_price).abs() < f64::EPSILON && (updated_stop - modified_stop_price).abs() < f64::EPSILON {
    info!("Verified order prices updated successfully to LMT: {}, STP: {}", updated_limit, updated_stop);
  } else {
    return Err(anyhow!("Order prices incorrect. LMT: {}, STP: {}. Expected LMT: {}, STP: {}", updated_limit, updated_stop, modified_limit_price, modified_stop_price));
  }

  // 6. Cancel the order.
  info!("Cancelling order {}...", order_id);
  order_mgr.cancel_order(&order_id).context("Failed to cancel order")?;

  // 7. Wait for cancellation confirmation.
  let cancel_timeout = Duration::from_secs(10);
  match order_mgr.try_wait_order_canceled(&order_id, cancel_timeout) {
    Ok(status) => info!("Order {} cancelled with status: {:?}", order_id, status),
    Err(e) => return Err(e).context("Waiting for order cancellation failed"),
  }

  info!("Stop-Limit order modify test completed successfully.");
  Ok(())
}


// --- Helper for Order Test ---
pub(crate) fn attempt_cleanup(client: &IBKRClient, contract: &Contract) -> Result<()> {
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
  if client.client_id() != 0 {
    return Err(anyhow!("Must cleanup from client 0, got {}", client.client_id()));
  }
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
  let submit_timeout = Duration::from_secs(20);
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
  let cancel_timeout = Duration::from_secs(30);
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
    warn!("This test will BUY 2 IBM call option contracts and then EXERCISE 1 and LAPSE 1.");
    warn!("The test will select the lowest strike (most in-the-money) call option for the current month.");
    warn!("Ensure you have sufficient buying power for IBM options.");
    std::thread::sleep(Duration::from_secs(5));
  }

  let order_mgr = client.orders();
  let acct_mgr = client.account();
  let ref_data_mgr = client.data_ref();

  // 1. Get account ID
  let account_id = acct_mgr.get_account_info()?.account_id;
  info!("Using account ID: {}", account_id);

  // 2. Find IBM call options for the current month
  let mut option_search_contract = Contract::new();
  option_search_contract.symbol = "IBM".to_string();
  option_search_contract.sec_type = SecType::Option;
  option_search_contract.currency = "USD".to_string();
  option_search_contract.exchange = "SMART".to_string();
  option_search_contract.last_trade_date_or_contract_month = Some(DateOrMonth::Date(OrderBuilder::next_monthly_option_expiry()));
  option_search_contract.right = Some(OptionRight::Call);

  let option_details_list = ref_data_mgr.get_contract_details(&option_search_contract)
    .context("Failed to get option contract details for IBM")?;

  if option_details_list.is_empty() {
    return Err(anyhow!("No IBM call options found for current month. Ensure market data subscription for IBM options."));
  }

  // 3. Find the lowest strike call option (most likely to be in-the-money)
  let mut best_option = None;
  let mut lowest_strike = f64::MAX;

  for details in &option_details_list {
    if let Some(strike) = details.contract.strike {
      if strike < lowest_strike {
        lowest_strike = strike;
        best_option = Some(details.contract.clone());
      }
    }
  }

  let option_contract = best_option.ok_or_else(|| anyhow!("No valid IBM call option found with strike price"))?;

  info!("Selected lowest strike call option: ConID={}, Symbol={}, Expiry={}, Strike={}, Right={:?}",
        option_contract.con_id, option_contract.symbol,
        option_contract.last_trade_date_or_contract_month.clone().map(|x| x.to_string()).unwrap_or("N/A".to_string()),
        option_contract.strike.unwrap_or(0.0), option_contract.right);

  // 4. Buy 2 option contracts at market price
  info!("Placing market order to BUY 2 {} call option contracts...", option_contract.symbol);

  let option_buy_request = OrderRequest {
    side: OrderSide::Buy,
    order_type: OrderType::Market,
    quantity: 2.0, // Buy 2 contracts so we can exercise 1 and lapse 1
    time_in_force: TimeInForce::Day,
    transmit: true,
    ..Default::default()
  };

  let buy_order_id = order_mgr.place_order(option_contract.clone(), option_buy_request)
    .context("Failed to place option buy order")?;

  info!("Option BUY order placed with ID: {}. Waiting for execution...", buy_order_id);

  // 5. Wait for the option purchase to fill
  let buy_timeout = Duration::from_secs(30);
  match order_mgr.try_wait_order_executed(&buy_order_id, buy_timeout) {
    Ok(OrderStatus::Filled) => {
      info!("Option BUY order {} filled successfully.", buy_order_id);
    }
    Ok(status) => {
      error!("Option BUY order {} did not fill. Final status: {:?}", buy_order_id, status);
      return Err(anyhow!("Option BUY order {} failed to fill. Status: {:?}", buy_order_id, status));
    }
    Err(e) => {
      error!("Error waiting for option BUY order {}: {:?}", buy_order_id, e);
      return Err(e).context(format!("Waiting for option BUY order {} failed", buy_order_id));
    }
  }

  // 6. Verify we have the option position
  info!("Verifying option position exists...");
  std::thread::sleep(Duration::from_secs(3)); // Allow position data to update

  let positions = acct_mgr.list_open_positions()?;
  let option_position = positions.iter().find(|p| {
    p.contract.symbol == option_contract.symbol &&
      p.contract.sec_type == SecType::Option &&
      p.contract.strike == option_contract.strike &&
      p.contract.right == option_contract.right &&
      p.contract.last_trade_date_or_contract_month == option_contract.last_trade_date_or_contract_month
  });

  match option_position {
    Some(pos) if pos.quantity >= 2.0 => {
      info!("Verified option position: {} has quantity {}", pos.symbol, pos.quantity);
    }
    Some(pos) => {
      warn!("Option position exists but quantity {} is less than expected 2.0", pos.quantity);
      // Continue anyway, adjust quantities
    }
    None => {
      error!("Option position not found after purchase!");
      return Err(anyhow!("Option position not found after successful purchase"));
    }
  }

  // 7. Exercise 1 contract
  let exercise_req_id = 9001;
  let exercise_qty = 1;
  let override_exercise = false;

  info!("Attempting to EXERCISE {} contract(s) of {} with ReqID {}",
        exercise_qty, option_contract.local_symbol.as_deref().unwrap_or(&option_contract.symbol), exercise_req_id);

  match order_mgr.exercise_option(exercise_req_id, &option_contract, yatws::order::ExerciseAction::Exercise, exercise_qty, &account_id, override_exercise) {
    Ok(_) => info!("Exercise request sent for ReqID {}. Monitor account updates.", exercise_req_id),
    Err(e) => {
      error!("Failed to send EXERCISE request for ReqID {}: {:?}", exercise_req_id, e);
      return Err(e).context("Exercise option request failed");
    }
  }

  // Allow time for processing
  std::thread::sleep(Duration::from_secs(5));

  // 8. Lapse 1 contract (if we still have position)
  let lapse_req_id = 9002;
  let lapse_qty = 1;
  let override_lapse = false;

  info!("Attempting to LAPSE {} contract(s) of {} with ReqID {}",
        lapse_qty, option_contract.local_symbol.as_deref().unwrap_or(&option_contract.symbol), lapse_req_id);

  match order_mgr.exercise_option(lapse_req_id, &option_contract, yatws::order::ExerciseAction::Lapse, lapse_qty, &account_id, override_lapse) {
    Ok(_) => info!("Lapse request sent for ReqID {}. Monitor account updates.", lapse_req_id),
    Err(e) => {
      warn!("Failed to send LAPSE request for ReqID {}: {:?}. This might be expected if position was reduced by exercise.", lapse_req_id, e);
    }
  }

  if is_live {
    info!("Exercise/Lapse requests completed. Waiting 15 seconds for processing...");
    std::thread::sleep(Duration::from_secs(15));

    // Check final positions
    info!("Checking final positions after exercise/lapse operations...");
    let final_positions = acct_mgr.list_open_positions()?;

    // Look for the underlying stock position (from exercise)
    if let Some(stock_pos) = final_positions.iter().find(|p| p.contract.symbol == "IBM" && p.contract.sec_type == SecType::Stock) {
      info!("Found IBM stock position from exercise: quantity = {}", stock_pos.quantity);
    }

    // Look for remaining option position
    if let Some(opt_pos) = final_positions.iter().find(|p| {
      p.contract.symbol == option_contract.symbol &&
        p.contract.sec_type == SecType::Option &&
        p.contract.strike == option_contract.strike
    }) {
      info!("Remaining {} option position: quantity = {}", opt_pos.symbol, opt_pos.quantity);
    }

    info!("Manual verification of exercise/lapse operations and resulting positions is recommended.");
  } else {
    info!("Exercise/Lapse test sequence completed for replay mode.");
  }

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

pub(super) fn order_subscription_impl(client: &IBKRClient, is_live: bool) -> Result<()> {
  info!("--- Testing Order Subscription ---");

  if is_live {
    warn!("This test will place a market order for 1 share of SPY that should execute immediately");
    warn!("Ensure market is open for this test to work properly");
    std::thread::sleep(Duration::from_secs(3));
  }

  let order_mgr = client.orders();

  // Place a market order that should execute quickly
  let (contract, order_request) = OrderBuilder::new(OrderSide::Buy, 1.0)
    .market()
    .for_stock("SPY")
    .build()?;

  info!("Creating order subscription for: {} {} {} (market order)",
        order_request.side, order_request.quantity, contract.symbol);

  // Create the subscription
  let subscription = order_mgr.subscribe_new_order(contract.clone(), order_request)
    .context("Failed to create order subscription")?;

  info!("Order subscription created for ID: {}", subscription.order_id());

  // Collect events
  let mut events_received = 0;
  let mut saw_pending_submit = false;
  let mut saw_submitted = false;
  let mut saw_filled = false;
  let mut final_order_state = None;

  info!("Collecting order events...");
  let start_time = std::time::Instant::now();
  let max_wait_time = if is_live { Duration::from_secs(30) } else { Duration::from_secs(5) };

  for event in subscription.events() {
    events_received += 1;

    match event {
      OrderEvent::Update(order) => {
        info!("Event {}: Order {} status: {:?}, filled: {}, remaining: {}",
              events_received, order.id, order.state.status,
              order.state.filled_quantity, order.state.remaining_quantity);

        match order.state.status {
          OrderStatus::PendingSubmit => saw_pending_submit = true,
          OrderStatus::Submitted => saw_submitted = true,
          OrderStatus::Filled => {
            saw_filled = true;
            info!("Order filled! Filled quantity: {}", order.state.filled_quantity);
          }
          _ => {}
        }

        final_order_state = Some(order.state.clone());

        // Stop if we reach a terminal state
        if order.state.is_terminal() {
          info!("Order reached terminal state: {:?}", order.state.status);
          break;
        }
      }
      OrderEvent::Error(err) => {
        info!("Event {}: Order error: {:?}", events_received, err);
        return Err(anyhow!("Order subscription received error: {:?}", err));
      }
    }

    // Safety timeout
    if start_time.elapsed() > max_wait_time {
      warn!("Stopping event collection after {:?}", max_wait_time);
      break;
    }
  }

  info!("Received {} events total", events_received);

  // Always attempt to clean up position, regardless of what we observed
  info!("Attempting to clean up any SPY position created by this test...");
  let cleanup_result = attempt_cleanup(client, &contract);
  match cleanup_result {
    Ok(()) => info!("Position cleanup completed successfully"),
    Err(e) => {
      warn!("Position cleanup encountered issues: {:?}", e);
      warn!("Manual intervention may be required to close SPY position");
    }
  }

  // Validate we received the expected events
  if events_received == 0 {
    return Err(anyhow!("No events received from order subscription"));
  }

  if !saw_pending_submit && !saw_submitted {
    return Err(anyhow!("Expected to see PendingSubmit or Submitted status, but didn't"));
  }

  // Check if the order filled (this is expected in live mode, but not required for test to pass)
  if is_live && !saw_filled {
    warn!("Market order did not fill in live mode - this may indicate markets are closed or illiquid");
    warn!("For SPY, this is unusual during market hours");
  }

  info!("Order subscription test completed successfully");
  info!("  - Received {} events", events_received);
  info!("  - Saw PendingSubmit: {}", saw_pending_submit);
  info!("  - Saw Submitted: {}", saw_submitted);
  info!("  - Saw Filled: {}", saw_filled);

  if let Some(final_state) = final_order_state {
    info!("  - Final order status: {:?}", final_state.status);
    info!("  - Final filled quantity: {}", final_state.filled_quantity);

    // Additional validation - check that filled + remaining = original quantity
    let total_accounted = final_state.filled_quantity + final_state.remaining_quantity;
    if (total_accounted - 1.0).abs() > f64::EPSILON {
      warn!("Quantity accounting mismatch: filled ({}) + remaining ({}) != original (1.0)",
            final_state.filled_quantity, final_state.remaining_quantity);
    }
  }

  Ok(())
}
