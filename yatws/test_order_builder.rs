// yatws/test_order_builder.rs
use anyhow::{anyhow, Result};
use log::{error, info};
use std::time::Duration;
use chrono::{Duration as ChronoDuration};
use yatws::{
  IBKRError,
  IBKRClient,
  OrderBuilder,
  contract::{Contract, OptionRight},
  order::{OrderSide, OrderState, OrderRequest, TimeInForce, IBKRAlgo, AdaptivePriority, RiskAversion, TwapStrategyType},
  order_build_types::{TriggerMethod, ConditionConjunction},
};

pub(super) fn order_builder_test_impl(client: &IBKRClient, is_live: bool) -> Result<()> {
  info!("--- Testing OrderBuilder - Comprehensive Functionality with Server Validation ---");

  // Track which test categories failed
  let mut failed_categories: Vec<String> = Vec::new();
  let mut total_tests = 0;
  let mut passed_tests = 0;

  // Test Category 1: Basic Order Types
  info!("=== Testing Basic Order Types ===");
  let basic_order_results = test_basic_order_types();
  total_tests += basic_order_results.len();
  let basic_passed = basic_order_results.iter().filter(|r| r.success).count();
  passed_tests += basic_passed;
  if basic_passed < basic_order_results.len() {
    failed_categories.push("Basic Order Types".to_string());
    for result in &basic_order_results {
      if !result.success {
        error!("  ✗ {}: {}", result.name, result.error.as_deref().unwrap_or("Unknown error"));
      }
    }
  }

  // Test Category 2: Contract Types
  info!("=== Testing Contract Types ===");
  let contract_results = test_contract_types(client);
  total_tests += contract_results.len();
  let contract_passed = contract_results.iter().filter(|r| r.success).count();
  passed_tests += contract_passed;
  if contract_passed < contract_results.len() {
    failed_categories.push("Contract Types".to_string());
    for result in &contract_results {
      if !result.success {
        error!("  ✗ {}: {}", result.name, result.error.as_deref().unwrap_or("Unknown error"));
      }
    }
  }

  // Test Category 3: Order Parameters
  info!("=== Testing Order Parameters ===");
  let params_results = test_order_parameters();
  total_tests += params_results.len();
  let params_passed = params_results.iter().filter(|r| r.success).count();
  passed_tests += params_passed;
  if params_passed < params_results.len() {
    failed_categories.push("Order Parameters".to_string());
    for result in &params_results {
      if !result.success {
        error!("  ✗ {}: {}", result.name, result.error.as_deref().unwrap_or("Unknown error"));
      }
    }
  }

  // Test Category 4: Special Order Types
  info!("=== Testing Special Order Types ===");
  let special_results = test_special_order_types();
  total_tests += special_results.len();
  let special_passed = special_results.iter().filter(|r| r.success).count();
  passed_tests += special_passed;
  if special_passed < special_results.len() {
    failed_categories.push("Special Order Types".to_string());
    for result in &special_results {
      if !result.success {
        error!("  ✗ {}: {}", result.name, result.error.as_deref().unwrap_or("Unknown error"));
      }
    }
  }

  // Test Category 5: IBKR Algorithms
  info!("=== Testing IBKR Algorithms ===");
  let algo_results = test_ibkr_algorithms();
  total_tests += algo_results.len();
  let algo_passed = algo_results.iter().filter(|r| r.success).count();
  passed_tests += algo_passed;
  if algo_passed < algo_results.len() {
    failed_categories.push("IBKR Algorithms".to_string());
    for result in &algo_results {
      if !result.success {
        error!("  ✗ {}: {}", result.name, result.error.as_deref().unwrap_or("Unknown error"));
      }
    }
  }

  // Test Category 6: Order Conditions
  info!("=== Testing Order Conditions ===");
  let conditions_results = test_order_conditions();
  total_tests += conditions_results.len();
  let conditions_passed = conditions_results.iter().filter(|r| r.success).count();
  passed_tests += conditions_passed;
  if conditions_passed < conditions_results.len() {
    failed_categories.push("Order Conditions".to_string());
    for result in &conditions_results {
      if !result.success {
        error!("  ✗ {}: {}", result.name, result.error.as_deref().unwrap_or("Unknown error"));
      }
    }
  }

  // Test Category 7: Adjustable Orders
  info!("=== Testing Adjustable Orders ===");
  let adjustable_results = test_adjustable_orders();
  total_tests += adjustable_results.len();
  let adjustable_passed = adjustable_results.iter().filter(|r| r.success).count();
  passed_tests += adjustable_passed;
  if adjustable_passed < adjustable_results.len() {
    failed_categories.push("Adjustable Orders".to_string());
    for result in &adjustable_results {
      if !result.success {
        error!("  ✗ {}: {}", result.name, result.error.as_deref().unwrap_or("Unknown error"));
      }
    }
  }

  // Test Category 8: Combo Orders
  info!("=== Testing Combo Orders ===");
  let combo_results = test_combo_orders();
  total_tests += combo_results.len();
  let combo_passed = combo_results.iter().filter(|r| r.success).count();
  passed_tests += combo_passed;
  if combo_passed < combo_results.len() {
    failed_categories.push("Combo Orders".to_string());
    for result in &combo_results {
      if !result.success {
        error!("  ✗ {}: {}", result.name, result.error.as_deref().unwrap_or("Unknown error"));
      }
    }
  }

  // Test Category 9: Validation and Error Cases
  info!("=== Testing Validation and Error Cases ===");
  let validation_results = test_validation_cases();
  total_tests += validation_results.len();
  let validation_passed = validation_results.iter().filter(|r| r.success).count();
  passed_tests += validation_passed;
  if validation_passed < validation_results.len() {
    failed_categories.push("Validation Cases".to_string());
    for result in &validation_results {
      if !result.success {
        error!("  ✗ {}: {}", result.name, result.error.as_deref().unwrap_or("Unknown error"));
      }
    }
  }

  // Test Category 10: Server-Side What-If Validation (only if live connection)
  if is_live {
    info!("=== Testing Server-Side What-If Order Validation ===");
    let what_if_results = test_what_if_orders_with_server_validation(client);
    total_tests += what_if_results.len();
    let what_if_passed = what_if_results.iter().filter(|r| r.success).count();
    passed_tests += what_if_passed;
    if what_if_passed < what_if_results.len() {
      failed_categories.push("Server-Side What-If Validation".to_string());
      for result in &what_if_results {
        if !result.success {
          error!("  ✗ {}: {}", result.name, result.error.as_deref().unwrap_or("Unknown error"));
        }
      }
    }

    // Test Category 11: Specific Validation Scenarios (only if live connection)
    info!("=== Testing Specific Validation Scenarios ===");
    let scenario_results = test_specific_validation_scenarios(client);
    total_tests += scenario_results.len();
    let scenario_passed = scenario_results.iter().filter(|r| r.success).count();
    passed_tests += scenario_passed;
    if scenario_passed < scenario_results.len() {
      failed_categories.push("Specific Validation Scenarios".to_string());
      for result in &scenario_results {
        if !result.success {
          error!("  ✗ {}: {}", result.name, result.error.as_deref().unwrap_or("Unknown error"));
        }
      }
    }
  } else {
    info!("=== Skipping Server-Side What-If Tests (not connected to live TWS/Gateway) ===");
  }

  // Print summary
  info!("=== Enhanced OrderBuilder Test Summary ===");
  info!("Total tests: {}", total_tests);
  info!("Passed tests: {}", passed_tests);
  info!("Failed tests: {}", total_tests - passed_tests);
  info!("Failed categories: {}", failed_categories.len());

  if !failed_categories.is_empty() {
    error!("Failed categories:");
    for category in &failed_categories {
      error!("  - {}", category);
    }
  }

  if failed_categories.is_empty() {
    info!("OrderBuilder comprehensive test completed successfully - all categories passed!");
    Ok(())
  } else {
    let total_categories = if is_live { 11 } else { 9 };
    Err(anyhow!("OrderBuilder test failed: {}/{} categories failed: {:?}",
                failed_categories.len(), total_categories, failed_categories))
  }
}

// Helper struct for test results
#[derive(Debug)]
struct TestResult {
  name: String,
  success: bool,
  error: Option<String>,
}

impl TestResult {
  fn success(name: &str) -> Self {
    Self {
      name: name.to_string(),
      success: true,
      error: None,
    }
  }

  fn failure(name: &str, error: &str) -> Self {
    Self {
      name: name.to_string(),
      success: false,
      error: Some(error.to_string()),
    }
  }
}

// Helper function to test a single order build
fn test_order_build<F>(name: &str, builder_fn: F) -> TestResult
where
  F: FnOnce() -> Result<(yatws::contract::Contract, yatws::order::OrderRequest), IBKRError>,
{
  match builder_fn() {
    Ok((contract, order)) => {
      info!("  ✓ {}: Contract: {:?} {}, Order: {:?} {} @ {:?}",
            name, contract.sec_type, contract.symbol,
            order.side, order.order_type, order.limit_price);
      TestResult::success(name)
    }
    Err(e) => {
      TestResult::failure(name, &format!("{:?}", e))
    }
  }
}

// Helper function to test what-if orders with server validation
fn test_what_if_order_server<F>(name: &str, what_if_fn: F) -> TestResult
where
  F: FnOnce() -> Result<(yatws::contract::Contract, yatws::order::OrderRequest, yatws::order::OrderState), IBKRError>,
{
  match what_if_fn() {
    Ok((contract, order, order_state)) => {
      info!("  ✓ {}: Contract: {:?} {}, Order: {:?} {} @ {:?}",
            name, contract.sec_type, contract.symbol,
            order.side, order.order_type, order.limit_price);

      // Log server validation results
      if let Some(init_margin) = order_state.initial_margin_before {
        info!("    Initial Margin Required: ${:.2}", init_margin);
      }
      if let Some(maint_margin) = order_state.maintenance_margin_before {
        info!("    Maintenance Margin Required: ${:.2}", maint_margin);
      }
      if let Some(commission) = order_state.commission {
        info!("    Estimated Commission: ${:.2}", commission);
      }
      if let Some(min_commission) = order_state.min_commission {
        info!("    Minimum Commission: ${:.2}", min_commission);
      }
      if let Some(max_commission) = order_state.max_commission {
        info!("    Maximum Commission: ${:.2}", max_commission);
      }

      TestResult::success(name)
    }
    Err(e) => {
      // Some errors are expected (like invalid tickers), so we differentiate
      match &e {
        IBKRError::ApiError(error_code, error_msg) => {
          if *error_code == 200 || error_msg.contains("invalid") || error_msg.contains("not found") {
            info!("  ✓ {} (Expected server validation error): {} - {}", name, error_code, error_msg);
            TestResult::success(name) // Expected validation error
          } else {
            TestResult::failure(name, &format!("Unexpected API error: {} - {}", error_code, error_msg))
          }
        }
        _ => TestResult::failure(name, &format!("{:?}", e))
      }
    }
  }
}

fn test_basic_order_types() -> Vec<TestResult> {
  let mut results = Vec::new();

  // Market Order
  results.push(test_order_build("Market Order - Stock", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .market()
      .build()
  }));

  // Limit Order
  results.push(test_order_build("Limit Order - Stock", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .limit(150.0)
      .build()
  }));

  // Stop Order
  results.push(test_order_build("Stop Order - Stock", || {
    OrderBuilder::new(OrderSide::Sell, 100.0)
      .for_stock("AAPL")
      .stop(140.0)
      .build()
  }));

  // Stop Limit Order
  results.push(test_order_build("Stop Limit Order - Stock", || {
    OrderBuilder::new(OrderSide::Sell, 100.0)
      .for_stock("AAPL")
      .stop_limit(140.0, 139.0)
      .build()
  }));

  // Market If Touched
  results.push(test_order_build("Market If Touched - Stock", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .market_if_touched(155.0)
      .build()
  }));

  // Limit If Touched
  results.push(test_order_build("Limit If Touched - Stock", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .limit_if_touched(155.0, 154.0)
      .build()
  }));

  // Trailing Stop (Absolute)
  results.push(test_order_build("Trailing Stop Absolute - Stock", || {
    OrderBuilder::new(OrderSide::Sell, 100.0)
      .for_stock("AAPL")
      .trailing_stop_abs(5.0, Some(140.0))
      .build()
  }));

  // Trailing Stop (Percentage)
  results.push(test_order_build("Trailing Stop Percentage - Stock", || {
    OrderBuilder::new(OrderSide::Sell, 100.0)
      .for_stock("AAPL")
      .trailing_stop_pct(0.05, Some(140.0))
      .build()
  }));

  // Trailing Stop Limit (Absolute)
  results.push(test_order_build("Trailing Stop Limit Absolute - Stock", || {
    OrderBuilder::new(OrderSide::Sell, 100.0)
      .for_stock("AAPL")
      .trailing_stop_limit_abs(5.0, 2.0, Some(140.0))
      .build()
  }));

  // Trailing Stop Limit (Percentage)
  results.push(test_order_build("Trailing Stop Limit Percentage - Stock", || {
    OrderBuilder::new(OrderSide::Sell, 100.0)
      .for_stock("AAPL")
      .trailing_stop_limit_pct(0.05, 2.0, Some(140.0))
      .build()
  }));

  results
}

fn test_contract_types(_client: &IBKRClient) -> Vec<TestResult> {
  let mut results = Vec::new();

  // Stock
  results.push(test_order_build("Stock Contract", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .with_exchange("SMART")
      .with_currency("USD")
      .limit(150.0)
      .build()
  }));

  // Option - Using proper third Friday expiry
  results.push(test_order_build("Option Contract", || {
    let expiry = OrderBuilder::next_monthly_option_expiry();
    info!("Using option expiry: {}", expiry);
    OrderBuilder::new(OrderSide::Buy, 10.0)
      .for_option("AAPL", expiry, 160.0, OptionRight::Call)
      .with_exchange("SMART")
      .with_currency("USD")
      .limit(2.5)
      .build()
  }));

  // Future - Using proper quarterly expiry (last Thursday)
  results.push(test_order_build("Future Contract", || {
    let expiry = OrderBuilder::next_quarterly_future_expiry();
    info!("Using future expiry: {}", expiry);
    OrderBuilder::new(OrderSide::Buy, 1.0)
      .for_future("ES", expiry)
      .with_exchange("CME")
      .with_currency("USD")
      .limit(4500.0)
      .build()
  }));

  // Forex
  results.push(test_order_build("Forex Contract", || {
    OrderBuilder::new(OrderSide::Buy, 100000.0)
      .for_forex("EUR/USD")
      .limit(1.1000)
      .build()
  }));

  // Forex with cash quantity
  results.push(test_order_build("Forex Cash Quantity", || {
    OrderBuilder::new(OrderSide::Buy, 0.0) // Quantity will be ignored
      .for_forex("EUR/USD")
      .forex_cash_quantity(10000.0, Some(1.1000))
      .build()
  }));

  // Continuous Future
  results.push(test_order_build("Continuous Future Contract", || {
    OrderBuilder::new(OrderSide::Buy, 1.0)
      .for_continuous_future("ES")
      .with_exchange("CME")
      .with_currency("USD")
      .limit(4500.0)
      .build()
  }));

  // Bond
  results.push(test_order_build("Bond Contract", || {
    OrderBuilder::new(OrderSide::Buy, 1000.0)
      .for_bond("912828XG5")
      .with_exchange("SMART")
      .with_currency("USD")
      .limit(100.0)
      .build()
  }));

  // Crypto
  results.push(test_order_build("Crypto Contract", || {
    OrderBuilder::new(OrderSide::Buy, 0.1)
      .for_crypto("BTC")
      .with_currency("USD")
      .limit(50000.0)
      .build()
  }));

  // Index
  results.push(test_order_build("Index Contract", || {
    OrderBuilder::new(OrderSide::Buy, 1.0)
      .for_index("SPX")
      .with_exchange("CBOE")
      .with_currency("USD")
      .limit(4500.0)
      .build()
  }));

  // Fund
  results.push(test_order_build("Fund Contract", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_fund("SPY")
      .with_exchange("ARCA")
      .with_currency("USD")
      .limit(450.0)
      .build()
  }));

  // Future Option - Using proper quarterly expiry
  results.push(test_order_build("Future Option Contract", || {
    let expiry = OrderBuilder::next_quarterly_future_expiry();
    info!("Using future option expiry: {}", expiry);
    OrderBuilder::new(OrderSide::Buy, 1.0)
      .for_future_option("ES", expiry, 4500.0, OptionRight::Call)
      .with_exchange("CME")
      .with_currency("USD")
      .limit(50.0)
      .build()
  }));

  // Index Option - Using proper monthly expiry
  results.push(test_order_build("Index Option Contract", || {
    let expiry = OrderBuilder::next_monthly_option_expiry();
    info!("Using index option expiry: {}", expiry);
    OrderBuilder::new(OrderSide::Buy, 1.0)
      .for_index_option("SPX", expiry, 4500.0, OptionRight::Call)
      .with_exchange("CBOE")
      .with_currency("USD")
      .limit(100.0)
      .build()
  }));

  results
}

fn test_order_parameters() -> Vec<TestResult> {
  let mut results = Vec::new();

  // Time In Force - Day
  results.push(test_order_build("TIF Day", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_tif(TimeInForce::Day)
      .build()
  }));

  // Time In Force - GTC
  results.push(test_order_build("TIF GTC", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_tif(TimeInForce::GoodTillCancelled)
      .build()
  }));

  // Time In Force - IOC
  results.push(test_order_build("TIF IOC", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_tif(TimeInForce::ImmediateOrCancel)
      .build()
  }));

  // Time In Force - FOK
  results.push(test_order_build("TIF FOK", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_tif(TimeInForce::FillOrKill)
      .build()
  }));

  // Good Till Date
  results.push(test_order_build("Good Till Date", || {
    let gtd_date = chrono::Utc::now() + ChronoDuration::days(7);
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_tif(TimeInForce::GoodTillDate)
      .with_good_till_date(gtd_date)
      .build()
  }));

  // Order Parameters
  results.push(test_order_build("Order Parameters", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_account("DU12345")
      .with_order_ref("test_order_123")
      .with_transmit(true)
      .with_outside_rth(false)
      .with_hidden(false)
      .build()
  }));

  // All or None
  results.push(test_order_build("All or None", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_all_or_none(true)
      .build()
  }));

  // Sweep to Fill
  results.push(test_order_build("Sweep to Fill", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .with_exchange("SMART") // Required for sweep to fill
      .limit(150.0)
      .with_sweep_to_fill(true)
      .build()
  }));

  // Block Order (Options)
  results.push(test_order_build("Block Order", || {
    let expiry = OrderBuilder::next_monthly_option_expiry();
    OrderBuilder::new(OrderSide::Buy, 50.0) // 50 contracts for block order
      .for_option("AAPL", expiry, 160.0, OptionRight::Call)
      .limit(2.5)
      .with_block_order(true)
      .build()
  }));

  // Not Held
  results.push(test_order_build("Not Held", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_not_held(true)
      .build()
  }));

  // OCA Group
  results.push(test_order_build("OCA Group", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_oca_group("test_group_1")
      .with_oca_type(1) // Cancel all remaining orders
      .build()
  }));

  // Parent ID
  results.push(test_order_build("Parent ID", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_parent_id(12345)
      .build()
  }));

  // What If Order
  results.push(test_order_build("What If Order", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .limit(150.0)
      .what_if()
      .build()
  }));

  results
}

fn test_special_order_types() -> Vec<TestResult> {
  let mut results = Vec::new();

  // Market on Close
  results.push(test_order_build("Market on Close", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .market_on_close()
      .build()
  }));

  // Market on Open
  results.push(test_order_build("Market on Open", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .market_on_open()
      .build()
  }));

  // Limit on Close
  results.push(test_order_build("Limit on Close", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .limit_on_close(150.0)
      .build()
  }));

  // Limit on Open
  results.push(test_order_build("Limit on Open", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .limit_on_open(150.0)
      .build()
  }));

  // Market to Limit
  results.push(test_order_build("Market to Limit", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .market_to_limit()
      .build()
  }));

  // Pegged to Market
  results.push(test_order_build("Pegged to Market", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .pegged_market(0.01) // 1 cent offset
      .build()
  }));

  // Relative Order
  results.push(test_order_build("Relative Pegged Primary", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .relative_pegged_primary(-0.01, Some(150.0)) // 1 cent below, cap at 150
      .build()
  }));

  // Volatility Order
  results.push(test_order_build("Volatility Order", || {
    let expiry = OrderBuilder::next_monthly_option_expiry();
    OrderBuilder::new(OrderSide::Buy, 10.0)
      .for_option("AAPL", expiry, 160.0, OptionRight::Call)
      .volatility(25.0, 2) // 25% annual volatility
      .build()
  }));

  // Discretionary Amount
  results.push(test_order_build("Discretionary Amount", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_discretionary_amount(0.50) // 50 cent discretion
      .build()
  }));

  // Box Top (BOX exchange)
  results.push(test_order_build("Box Top", || {
    let expiry = OrderBuilder::next_monthly_option_expiry();
    OrderBuilder::new(OrderSide::Buy, 10.0)
      .for_option("SPY", expiry, 450.0, OptionRight::Call)
      .box_top() // Automatically sets exchange to BOX
      .build()
  }));

  results
}

fn test_ibkr_algorithms() -> Vec<TestResult> {
  let mut results = Vec::new();

  // Adaptive Algorithm
  results.push(test_order_build("Adaptive Algorithm", || {
    OrderBuilder::new(OrderSide::Buy, 1000.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_ibkr_algo(IBKRAlgo::Adaptive {
        priority: AdaptivePriority::Normal
      })
      .build()
  }));

  // VWAP Algorithm
  results.push(test_order_build("VWAP Algorithm", || {
    let start_time = chrono::Utc::now() + ChronoDuration::hours(1);
    let end_time = start_time + ChronoDuration::hours(6);
    OrderBuilder::new(OrderSide::Buy, 5000.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_ibkr_algo(IBKRAlgo::VWAP {
        max_pct_vol: 0.10,
        start_time: Some(start_time),
        end_time: Some(end_time),
        allow_past_end_time: false,
        no_take_liq: false,
        speed_up: false,
      })
      .build()
  }));

  // TWAP Algorithm
  results.push(test_order_build("TWAP Algorithm", || {
    let start_time = chrono::Utc::now() + ChronoDuration::hours(1);
    let end_time = start_time + ChronoDuration::hours(4);
    OrderBuilder::new(OrderSide::Buy, 2000.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_ibkr_algo(IBKRAlgo::TWAP {
        strategy_type: TwapStrategyType::Marketable,
        start_time: Some(start_time),
        end_time: Some(end_time),
        allow_past_end_time: false,
      })
      .build()
  }));

  // Arrival Price Algorithm
  results.push(test_order_build("Arrival Price Algorithm", || {
    let start_time = chrono::Utc::now() + ChronoDuration::hours(1);
    let end_time = start_time + ChronoDuration::hours(3);
    OrderBuilder::new(OrderSide::Buy, 3000.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_ibkr_algo(IBKRAlgo::ArrivalPrice {
        max_pct_vol: 0.15,
        risk_aversion: RiskAversion::Neutral,
        start_time: Some(start_time),
        end_time: Some(end_time),
        allow_past_end_time: false,
        force_completion: false,
      })
      .build()
  }));

  // Dark Ice Algorithm
  results.push(test_order_build("Dark Ice Algorithm", || {
    let start_time = chrono::Utc::now() + ChronoDuration::hours(1);
    let end_time = start_time + ChronoDuration::hours(4);
    OrderBuilder::new(OrderSide::Buy, 1000.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_ibkr_algo(IBKRAlgo::DarkIce {
        display_size: 100,
        start_time: Some(start_time),
        end_time: Some(end_time),
        allow_past_end_time: false,
      })
      .build()
  }));

  // Percentage of Volume Algorithm
  results.push(test_order_build("Percentage of Volume Algorithm", || {
    let start_time = chrono::Utc::now() + ChronoDuration::hours(1);
    let end_time = start_time + ChronoDuration::hours(6);
    OrderBuilder::new(OrderSide::Buy, 2000.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_ibkr_algo(IBKRAlgo::PercentageOfVolume {
        pct_vol: 0.05,
        start_time: Some(start_time),
        end_time: Some(end_time),
        no_take_liq: false,
      })
      .build()
  }));

  // Accumulate/Distribute Algorithm
  results.push(test_order_build("Accumulate/Distribute Algorithm", || {
    let active_start = chrono::Utc::now() + ChronoDuration::hours(1);
    let active_end = active_start + ChronoDuration::hours(6);
    OrderBuilder::new(OrderSide::Buy, 5000.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_ibkr_algo(IBKRAlgo::AccumulateDistribute {
        component_size: 100,
        time_between_orders: 300, // 5 minutes
        randomize_time_20pct: true,
        randomize_size_55pct: true,
        give_up: Some(30), // Give up after 30 minutes
        catch_up_in_time: true,
        wait_for_fill: false,
        active_time_start: Some(active_start),
        active_time_end: Some(active_end),
      })
      .build()
  }));

  // Balance Impact Risk Algorithm
  results.push(test_order_build("Balance Impact Risk Algorithm", || {
    OrderBuilder::new(OrderSide::Buy, 3000.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_ibkr_algo(IBKRAlgo::BalanceImpactRisk {
        max_pct_vol: 0.12,
        risk_aversion: RiskAversion::Aggressive,
        force_completion: true,
      })
      .build()
  }));

  // Minimize Impact Algorithm
  results.push(test_order_build("Minimize Impact Algorithm", || {
    OrderBuilder::new(OrderSide::Buy, 4000.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_ibkr_algo(IBKRAlgo::MinimiseImpact {
        max_pct_vol: 0.08,
      })
      .build()
  }));

  // Custom Algorithm
  results.push(test_order_build("Custom Algorithm", || {
    OrderBuilder::new(OrderSide::Buy, 1000.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_ibkr_algo(IBKRAlgo::Custom {
        strategy: "CustomAlgo".to_string(),
        params: vec![
          ("param1".to_string(), "value1".to_string()),
          ("param2".to_string(), "123".to_string()),
        ],
      })
      .build()
  }));

  results
}

fn test_order_conditions() -> Vec<TestResult> {
  let mut results = Vec::new();

  // Price Condition
  results.push(test_order_build("Price Condition", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("MSFT")
      .market()
      .add_price_condition(265598, "SMART", 180.0, TriggerMethod::Default, true) // AAPL > 180
      .build()
  }));

  // Time Condition
  results.push(test_order_build("Time Condition", || {
    let trigger_time = chrono::Utc::now() + ChronoDuration::hours(2);
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .market()
      .add_time_condition(trigger_time, true) // After specific time
      .build()
  }));

  // Margin Condition
  results.push(test_order_build("Margin Condition", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .market()
      .add_margin_condition(30, true) // Margin cushion > 30%
      .build()
  }));

  // Execution Condition
  results.push(test_order_build("Execution Condition", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("MSFT")
      .market()
      .add_execution_condition("AAPL", "STK", "SMART") // After AAPL execution
      .build()
  }));

  // Volume Condition (requires SMART routing)
  results.push(test_order_build("Volume Condition", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("MSFT")
      .with_exchange("SMART") // Required for volume conditions
      .market()
      .add_volume_condition(272093, "SMART", 1000000, true) // MSFT volume > 1M
      .build()
  }));

  // Percent Change Condition
  results.push(test_order_build("Percent Change Condition", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("MSFT")
      .market()
      .add_percent_change_condition(265598, "SMART", 5.0, true) // AAPL +5%
      .build()
  }));

  // Multiple Conditions with AND
  results.push(test_order_build("Multiple Conditions AND", || {
    let trigger_time = chrono::Utc::now() + ChronoDuration::hours(1);
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("MSFT")
      .with_exchange("SMART") // Required for volume condition
      .market()
      .add_price_condition(265598, "SMART", 180.0, TriggerMethod::Default, true) // AAPL > 180
      .with_next_condition_conjunction(ConditionConjunction::And)
      .add_volume_condition(272093, "SMART", 1000000, true) // AND MSFT volume > 1M
      .with_next_condition_conjunction(ConditionConjunction::And)
      .add_time_condition(trigger_time, true) // AND after time
      .build()
  }));

  // Multiple Conditions with OR
  results.push(test_order_build("Multiple Conditions OR", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("MSFT")
      .market()
      .add_price_condition(265598, "SMART", 180.0, TriggerMethod::Default, true) // AAPL > 180
      .with_next_condition_conjunction(ConditionConjunction::Or)
      .add_price_condition(265598, "SMART", 150.0, TriggerMethod::Default, false) // OR AAPL < 150
      .build()
  }));

  // Conditions Cancel Order
  results.push(test_order_build("Conditions Cancel Order", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .limit(150.0)
      .add_price_condition(265598, "SMART", 160.0, TriggerMethod::Default, false) // AAPL < 160
      .with_conditions_cancel_order(true) // Cancel instead of submit
      .build()
  }));

  // Conditions Ignore RTH
  results.push(test_order_build("Conditions Ignore RTH", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .market()
      .add_price_condition(265598, "SMART", 180.0, TriggerMethod::Default, true)
      .with_conditions_ignore_rth(true) // Check conditions outside RTH
      .build()
  }));

  results
}

fn test_adjustable_orders() -> Vec<TestResult> {
  let mut results = Vec::new();

  // Adjust to Stop
  results.push(test_order_build("Adjust to Stop", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_trigger_price(155.0)
      .adjust_to_stop(152.0)
      .build()
  }));

  // Adjust to Stop Limit
  results.push(test_order_build("Adjust to Stop Limit", || {
    OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_trigger_price(155.0)
      .adjust_to_stop_limit(152.0, 151.0)
      .build()
  }));

  // Adjust to Trail Absolute
  results.push(test_order_build("Adjust to Trail Absolute", || {
    OrderBuilder::new(OrderSide::Sell, 100.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_trigger_price(145.0)
      .adjust_to_trail_abs(148.0, 3.0) // Trail 3 points
      .build()
  }));

  // Adjust to Trail Percentage
  results.push(test_order_build("Adjust to Trail Percentage", || {
    OrderBuilder::new(OrderSide::Sell, 100.0)
      .for_stock("AAPL")
      .limit(150.0)
      .with_trigger_price(145.0)
      .adjust_to_trail_pct(148.0, 0.02) // Trail 2%
      .build()
  }));

  results
}

fn test_combo_orders() -> Vec<TestResult> {
  let mut results = Vec::new();

  // Basic Combo Order
  results.push(test_order_build("Basic Combo Order", || {
    OrderBuilder::new(OrderSide::Buy, 1.0)
      .for_combo()
      .add_combo_leg(265598, 1, OrderSide::Buy, "SMART") // AAPL long
      .add_combo_leg(272093, 1, OrderSide::Sell, "SMART") // MSFT short
      .with_currency("USD")
      .limit(5.0) // Net debit of $5
      .build()
  }));

  // Combo Order with Leg Prices
  results.push(test_order_build("Combo Order with Leg Prices", || {
    OrderBuilder::new(OrderSide::Buy, 1.0)
      .for_combo()
      .add_combo_leg(265598, 1, OrderSide::Buy, "SMART")
      .add_combo_leg(272093, 1, OrderSide::Sell, "SMART")
      .with_combo_leg_price(0, 150.0) // AAPL leg price
      .with_combo_leg_price(1, 145.0) // MSFT leg price
      .with_currency("USD")
      .market() // Market order for combo
      .build()
  }));

  // Multi-leg Combo
  results.push(test_order_build("Multi-leg Combo", || {
    OrderBuilder::new(OrderSide::Buy, 1.0)
      .for_combo()
      .add_combo_leg(265598, 2, OrderSide::Buy, "SMART") // 2x AAPL long
      .add_combo_leg(272093, 1, OrderSide::Sell, "SMART") // 1x MSFT short
      .add_combo_leg(76483726, 1, OrderSide::Buy, "SMART") // 1x GOOGL long
      .with_currency("USD")
      .limit(10.0)
      .build()
  }));

  results
}

fn test_validation_cases() -> Vec<TestResult> {
  let mut results = Vec::new();

  // Test validation success cases (should pass)

  // Valid Limit Order
  match OrderBuilder::new(OrderSide::Buy, 100.0)
    .for_stock("AAPL")
    .limit(150.0)
    .build()
  {
    Ok(_) => results.push(TestResult::success("Valid Limit Order")),
    Err(e) => results.push(TestResult::failure("Valid Limit Order", &format!("{:?}", e))),
  }

  // Test validation failure cases (should fail)

  // Missing Symbol
  match OrderBuilder::new(OrderSide::Buy, 100.0)
    .for_stock("") // Empty symbol
    .limit(150.0)
    .build()
  {
    Ok(_) => results.push(TestResult::failure("Missing Symbol", "Should have failed validation")),
    Err(_) => results.push(TestResult::success("Missing Symbol")),
  }

  // Zero Quantity
  match OrderBuilder::new(OrderSide::Buy, 0.0) // Zero quantity
    .for_stock("AAPL")
    .limit(150.0)
    .build()
  {
    Ok(_) => results.push(TestResult::failure("Zero Quantity", "Should have failed validation")),
    Err(_) => results.push(TestResult::success("Zero Quantity")),
  }

  // GTD without Date
  match OrderBuilder::new(OrderSide::Buy, 100.0)
    .for_stock("AAPL")
    .limit(150.0)
    .with_tif(TimeInForce::GoodTillDate) // GTD without date
    .build()
  {
    Ok(_) => results.push(TestResult::failure("GTD No Date", "Should have failed validation")),
    Err(_) => results.push(TestResult::success("GTD No Date")),
  }

  // Volatility Order on Stock (should fail - requires option)
  match OrderBuilder::new(OrderSide::Buy, 100.0)
    .for_stock("AAPL") // Stock contract
    .volatility(25.0, 2) // Volatility order
    .build()
  {
    Ok(_) => results.push(TestResult::failure("Vol Order on Stock", "Should have failed validation")),
    Err(_) => results.push(TestResult::success("Vol Order on Stock")),
  }

  // Box Top on non-option (should fail)
  match OrderBuilder::new(OrderSide::Buy, 100.0)
    .for_stock("AAPL") // Stock contract
    .box_top() // Box top order
    .build()
  {
    Ok(_) => results.push(TestResult::failure("Box Top on Stock", "Should have failed validation")),
    Err(_) => results.push(TestResult::success("Box Top on Stock")),
  }

  // Sweep to Fill on non-SMART exchange
  match OrderBuilder::new(OrderSide::Buy, 100.0)
    .for_stock("AAPL")
    .with_exchange("NYSE") // Non-SMART exchange
    .limit(150.0)
    .with_sweep_to_fill(true) // Should fail
    .build()
  {
    Ok(_) => results.push(TestResult::failure("Sweep Non-SMART", "Should have failed validation")),
    Err(_) => results.push(TestResult::success("Sweep Non-SMART")),
  }

  // Block Order on non-option
  match OrderBuilder::new(OrderSide::Buy, 100.0)
    .for_stock("AAPL") // Stock contract
    .limit(150.0)
    .with_block_order(true) // Should fail
    .build()
  {
    Ok(_) => results.push(TestResult::failure("Block Order on Stock", "Should have failed validation")),
    Err(_) => results.push(TestResult::success("Block Order on Stock")),
  }

  // Cash Quantity on non-Forex
  match OrderBuilder::new(OrderSide::Buy, 100.0)
    .for_stock("AAPL") // Stock contract
    .forex_cash_quantity(10000.0, Some(150.0)) // Should fail
    .build()
  {
    Ok(_) => results.push(TestResult::failure("Cash Qty on Stock", "Should have failed validation")),
    Err(_) => results.push(TestResult::success("Cash Qty on Stock")),
  }

  // Combo without legs
  match OrderBuilder::new(OrderSide::Buy, 1.0)
    .for_combo() // Combo without legs
    .limit(5.0)
    .build()
  {
    Ok(_) => results.push(TestResult::failure("Combo No Legs", "Should have failed validation")),
    Err(_) => results.push(TestResult::success("Combo No Legs")),
  }

  // Adjusted order without trigger price
  match OrderBuilder::new(OrderSide::Buy, 100.0)
    .for_stock("AAPL")
    .limit(150.0)
    .adjust_to_stop(145.0) // Adjusted without trigger
    .build()
  {
    Ok(_) => results.push(TestResult::failure("Adjusted No Trigger", "Should have failed validation")),
    Err(_) => results.push(TestResult::success("Adjusted No Trigger")),
  }

  // Volume condition without SMART routing
  match OrderBuilder::new(OrderSide::Buy, 100.0)
    .for_stock("AAPL")
    .with_exchange("NYSE") // Non-SMART
    .market()
    .add_volume_condition(265598, "NYSE", 1000000, true) // Should fail
    .build()
  {
    Ok(_) => results.push(TestResult::failure("Vol Condition Non-SMART", "Should have failed validation")),
    Err(_) => results.push(TestResult::success("Vol Condition Non-SMART")),
  }

  results
}

// Server-side what-if validation tests
fn test_what_if_orders_with_server_validation(client: &IBKRClient) -> Vec<TestResult> {
  let mut results = Vec::new();
  let timeout = Duration::from_secs(20);

  // Test 1: Basic Stock What-If Order
  results.push(test_what_if_order_server("Stock What-If Order", || {
    let (contract, order) = OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .with_exchange("SMART")
      .with_currency("USD")
      .limit(150.0)
      .build()?;

    // Send to server for validation
    client.orders().check_what_if_order(&contract, &order, timeout)
      .map(|order_state| (contract, order, order_state))
  }));

  // Test 2: Option What-If Order
  // Options not enabled in this account.
  // results.push(test_what_if_order_server("Option What-If Order", || {
  //   let expiry = OrderBuilder::next_monthly_option_expiry();
  //   let (contract, order) = OrderBuilder::new(OrderSide::Buy, 10.0)
  //     .for_option("AAPL", expiry, 160.0, OptionRight::Call)
  //     .with_exchange("SMART")
  //     .with_currency("USD")
  //     .limit(2.5)
  //     .build()?;

  //   client.orders().check_what_if_order(&contract, &order, timeout)
  //     .map(|order_state| (contract, order, order_state))
  // }));

  // Test 3: Future What-If Order
  // Futures not enabled in this account.
  // results.push(test_what_if_order_server("Future What-If Order", || {
  //   let expiry = OrderBuilder::next_quarterly_future_expiry();
  //   let (contract, order) = OrderBuilder::new(OrderSide::Buy, 1.0)
  //     .for_future("ES", expiry)
  //     .with_exchange("CME")
  //     .with_currency("USD")
  //     .limit(4500.0)
  //     .build()?;

  //   client.orders().check_what_if_order(&contract, &order, timeout)
  //     .map(|order_state| (contract, order, order_state))
  // }));

  // Test 4: Forex What-If Order
  results.push(test_what_if_order_server("Forex What-If Order", || {
    let (contract, order) = OrderBuilder::new(OrderSide::Buy, 100000.0)
      .for_forex("EUR/USD")
      .limit(1.1000)
      .build()?;

    client.orders().check_what_if_order(&contract, &order, timeout)
      .map(|order_state| (contract, order, order_state))
  }));

  // Test 5: Complex Order with Algorithm What-If
  results.push(test_what_if_order_server("VWAP Algorithm What-If Order", || {
    let start_time = chrono::Utc::now() + ChronoDuration::hours(1);
    let end_time = start_time + ChronoDuration::hours(6);
    let (contract, order) = OrderBuilder::new(OrderSide::Buy, 5000.0)
      .for_stock("AAPL")
      .with_exchange("SMART")
      .with_currency("USD")
      .limit(150.0)
      .with_ibkr_algo(IBKRAlgo::VWAP {
        max_pct_vol: 0.10,
        start_time: Some(start_time),
        end_time: Some(end_time),
        allow_past_end_time: false,
        no_take_liq: false,
        speed_up: false,
      })
      .build()?;

    client.orders().check_what_if_order(&contract, &order, timeout)
      .map(|order_state| (contract, order, order_state))
  }));

  // Test 6: Margin-intensive Order What-If
  results.push(test_what_if_order_server("High Margin What-If Order", || {
    let (contract, order) = OrderBuilder::new(OrderSide::Buy, 10000.0) // Large quantity
      .for_stock("AAPL")
      .with_exchange("SMART")
      .with_currency("USD")
      .limit(150.0)
      .build()?;

    client.orders().check_what_if_order(&contract, &order, timeout)
      .map(|order_state| (contract, order, order_state))
  }));

  // Test 7: Invalid Contract What-If (should fail validation)
  results.push(test_what_if_order_server("Invalid Contract What-If", || {
    let (contract, order) = OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("INVALIDTICKER")
      .with_exchange("SMART")
      .with_currency("USD")
      .limit(150.0)
      .build()?;

    match client.orders().check_what_if_order(&contract, &order, timeout)
      .map(|order_state| (contract, order, order_state)) {
        Ok(state) => Err(IBKRError::InvalidOrder(format!("Invalid success with invalid ticker: {:?}", state))),
        Err(IBKRError::ApiError(code, msg)) =>
          if code == 200 { Ok((Contract::default(), OrderRequest::default(), OrderState::default())) } else {
            Err(IBKRError::InvalidOrder(format!("Unexpected API error: {}, {}", code, msg))) },
        Err(e) => Err(e),
      }
  }));

  results
}

// Additional utility function to test specific order validation scenarios
fn test_specific_validation_scenarios(client: &IBKRClient) -> Vec<TestResult> {
  let mut results = Vec::new();
  let timeout = Duration::from_secs(20);

  // Test options margin requirements
  results.push(test_what_if_order_server("Options Margin Requirements", || {
    let expiry = OrderBuilder::next_monthly_option_expiry();
    let (contract, order) = OrderBuilder::new(OrderSide::Sell, 100.0) // Naked call sell
      .for_option("AAPL", expiry, 160.0, OptionRight::Call)
      .with_exchange("SMART")
      .with_currency("USD")
      .limit(2.5)
      .build()?;

    // We expect and error: invert the condition of Ok.
    match client.orders().check_what_if_order(&contract, &order, timeout)
      .map(|order_state| (contract, order, order_state)) {
        Ok(state) => Err(IBKRError::InvalidOrder(format!("Invalid success selling naked call: {:?}", state))),
        Err(IBKRError::ApiError(code, msg)) =>
          if code == 460 { Ok((Contract::default(), OrderRequest::default(), OrderState::default())) } else {
            Err(IBKRError::InvalidOrder(format!("Unexpected API error: {}, {}", code, msg))) },
        Err(e) => Err(e),
      }
  }));

  // Test commission calculation
  results.push(test_what_if_order_server("Commission Calculation", || {
    let (contract, order) = OrderBuilder::new(OrderSide::Buy, 1000.0) // Medium quantity
      .for_stock("AAPL")
      .with_exchange("SMART")
      .with_currency("USD")
      .limit(150.0)
      .build()?;

    client.orders().check_what_if_order(&contract, &order, timeout)
      .map(|order_state| (contract, order, order_state))
  }));

  // Test pre-market order validation
  results.push(test_what_if_order_server("Pre-Market Order Validation", || {
    let (contract, order) = OrderBuilder::new(OrderSide::Buy, 100.0)
      .for_stock("AAPL")
      .with_exchange("SMART")
      .with_currency("USD")
      .limit(150.0)
      .with_outside_rth(true) // Allow outside regular trading hours
      .build()?;

    client.orders().check_what_if_order(&contract, &order, timeout)
      .map(|order_state| (contract, order, order_state))
  }));

  results
}
