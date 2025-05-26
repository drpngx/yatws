// yatws/test_data_ref.rs
use anyhow::{anyhow, Result};
use log::{error, info, warn};
use yatws::{
  IBKRClient, IBKRError,
  contract::{Contract, SecType},
};

pub(super) fn contract_details_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
  info!("--- Testing Contract Details Requests ---");
  let data_ref_mgr = client.data_ref();
  let mut overall_success = true;

  // Test 1: Stock contract details
  info!("Testing contract details for stock: AAPL");
  let stock_contract = Contract::stock("AAPL");
  match data_ref_mgr.get_contract_details(&stock_contract) {
    Ok(details_list) => {
      info!("Successfully received {} contract details for AAPL", details_list.len());
      if details_list.is_empty() {
        warn!("Received empty contract details list for AAPL");
      } else {
        let first_detail = &details_list[0];
        info!("  Contract ID: {}", first_detail.contract.con_id);
        info!("  Long Name: {}", first_detail.long_name);
        info!("  Market Name: {}", first_detail.market_name);
        info!("  Min Tick: {}", first_detail.min_tick);
        info!("  Valid Exchanges: {}", first_detail.valid_exchanges);
        info!("  Trading Hours: {}", first_detail.trading_hours);
        info!("  Industry: {}", first_detail.industry);
        info!("  Category: {}", first_detail.category);
      }
    }
    Err(e) => {
      error!("Failed to get contract details for AAPL: {:?}", e);
      overall_success = false;
    }
  }

  // Test 2: Invalid contract (should handle gracefully)
  info!("Testing contract details for invalid symbol");
  let invalid_contract = Contract::stock("INVALID_SYMBOL_TEST_123");
  match data_ref_mgr.get_contract_details(&invalid_contract) {
    Ok(details_list) => {
      if details_list.is_empty() {
        info!("Correctly received empty results for invalid symbol");
      } else {
        warn!("Unexpectedly received {} results for invalid symbol", details_list.len());
      }
    }
    Err(e) => {
      info!("Expected error for invalid symbol: {:?}", e);
      // This is expected behavior for invalid symbols
    }
  }

  if overall_success {
    Ok(())
  } else {
    Err(anyhow!("One or more contract details tests failed"))
  }
}

pub(super) fn option_chain_params_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
  info!("--- Testing Option Chain Parameters ---");
  let data_ref_mgr = client.data_ref();
  let mut overall_success = true;

  // First get AAPL contract details to get the contract ID
  let stock_contract = Contract::stock("AAPL");
  let underlying_con_id = match data_ref_mgr.get_contract_details(&stock_contract) {
    Ok(details_list) => {
      if details_list.is_empty() {
        error!("Cannot get AAPL contract details for option chain test");
        return Err(anyhow!("No contract details found for AAPL"));
      }
      details_list[0].contract.con_id
    }
    Err(e) => {
      error!("Failed to get AAPL contract details for option chain test: {:?}", e);
      return Err(anyhow!("Cannot get underlying contract ID: {:?}", e));
    }
  };

  info!("Using AAPL contract ID: {} for option chain parameters", underlying_con_id);

  // Test option chain parameters
  match data_ref_mgr.get_option_chain_params("AAPL", "", SecType::Stock, underlying_con_id) {
    Ok(params_list) => {
      info!("Successfully received {} option chain parameter sets for AAPL", params_list.len());
      if params_list.is_empty() {
        warn!("Received empty option chain parameters for AAPL");
        overall_success = false;
      } else {
        for (i, params) in params_list.iter().enumerate() {
          info!("  Parameter Set {}: Exchange={}, Trading Class={}", i + 1, params.exchange, params.trading_class);
          info!("    Underlying ConID: {}", params.underlying_con_id);
          info!("    Multiplier: {}", params.multiplier);
          info!("    Number of Expirations: {}", params.expirations.len());
          info!("    Number of Strikes: {}", params.strikes.len());

          // Show sample expirations (first 5)
          if !params.expirations.is_empty() {
            let sample_expirations: Vec<_> = params.expirations.iter().take(5).collect();
            info!("    Sample Expirations: {:?}{}", sample_expirations,
                  if params.expirations.len() > 5 { "..." } else { "" });
          }

          // Show strike range
          if !params.strikes.is_empty() {
            let min_strike = params.strikes.iter().fold(f64::INFINITY, |a, &b| a.min(b));
            let max_strike = params.strikes.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
            info!("    Strike Range: ${:.2} - ${:.2}", min_strike, max_strike);
          }
        }
      }
    }
    Err(e) => {
      error!("Failed to get option chain parameters for AAPL: {:?}", e);
      overall_success = false;
    }
  }

  if overall_success {
    Ok(())
  } else {
    Err(anyhow!("Option chain parameters test failed"))
  }
}

pub(super) fn soft_dollar_tiers_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
  info!("--- Testing Soft Dollar Tiers ---");
  let data_ref_mgr = client.data_ref();

  match data_ref_mgr.get_soft_dollar_tiers() {
    Ok(tiers) => {
      info!("Successfully received {} soft dollar tiers", tiers.len());
      if tiers.is_empty() {
        warn!("Received empty soft dollar tiers list");
      } else {
        for (i, tier) in tiers.iter().enumerate().take(5) { // Show first 5
          info!("  Tier {}: Name='{}', Value='{}', DisplayName='{}'",
                i + 1, tier.name, tier.value, tier.display_name);
        }
        if tiers.len() > 5 {
          info!("  ... and {} more tiers", tiers.len() - 5);
        }
      }
      Ok(())
    }
    Err(e) => {
      error!("Failed to get soft dollar tiers: {:?}", e);
      Err(anyhow!("Soft dollar tiers test failed: {:?}", e))
    }
  }
}

pub(super) fn family_codes_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
  info!("--- Testing Family Codes ---");
  let data_ref_mgr = client.data_ref();

  match data_ref_mgr.get_family_codes() {
    Ok(codes) => {
      info!("Successfully received {} family codes", codes.len());
      if codes.is_empty() {
        info!("Received empty family codes list (may be normal for some accounts)");
      } else {
        for (i, code) in codes.iter().enumerate().take(5) { // Show first 5
          info!("  Code {}: AccountID='{}', FamilyCode='{}'",
                i + 1, code.account_id, code.family_code_str);
        }
        if codes.len() > 5 {
          info!("  ... and {} more codes", codes.len() - 5);
        }
      }
      Ok(())
    }
    Err(e) => {
      error!("Failed to get family codes: {:?}", e);
      Err(anyhow!("Family codes test failed: {:?}", e))
    }
  }
}

pub(super) fn matching_symbols_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
  info!("--- Testing Matching Symbols ---");
  let data_ref_mgr = client.data_ref();
  let mut overall_success = true;

  // Test 1: Search for common symbols
  let search_patterns = vec!["APP", "MSFT", "GOOGL"];

  for pattern in search_patterns {
    info!("Searching for symbols matching: '{}'", pattern);
    match data_ref_mgr.get_matching_symbols(pattern) {
      Ok(matches) => {
        info!("Successfully received {} matching symbols for '{}'", matches.len(), pattern);
        if matches.is_empty() {
          warn!("No matches found for pattern '{}'", pattern);
        } else {
          for (i, desc) in matches.iter().enumerate().take(3) { // Show first 3
            info!("  Match {}: Symbol='{}', SecType={:?}, Exchange='{}'",
                  i + 1, desc.contract.symbol, desc.contract.sec_type, desc.contract.exchange);
            if !desc.derivative_sec_types.is_empty() {
              info!("    Derivative SecTypes: {:?}", desc.derivative_sec_types);
            }
          }
          if matches.len() > 3 {
            info!("    ... and {} more matches", matches.len() - 3);
          }
        }
      }
      Err(e) => {
        error!("Failed to get matching symbols for '{}': {:?}", pattern, e);
        overall_success = false;
      }
    }
  }

  // Test 2: Search with invalid pattern
  info!("Testing with unusual search pattern");
  match data_ref_mgr.get_matching_symbols("ZZZINVALIDPATTERN123") {
    Ok(matches) => {
      if matches.is_empty() {
        info!("Correctly received no matches for invalid pattern");
      } else {
        warn!("Unexpectedly found {} matches for invalid pattern", matches.len());
      }
    }
    Err(e) => {
      info!("Expected behavior for invalid pattern: {:?}", e);
    }
  }

  if overall_success {
    Ok(())
  } else {
    Err(anyhow!("One or more matching symbols tests failed"))
  }
}

pub(super) fn market_depth_exchanges_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
  info!("--- Testing Market Depth Exchanges ---");
  let data_ref_mgr = client.data_ref();

  match data_ref_mgr.get_mkt_depth_exchanges() {
    Ok(exchanges) => {
      info!("Successfully received {} market depth exchanges", exchanges.len());
      if exchanges.is_empty() {
        warn!("Received empty market depth exchanges list");
      } else {
        for (i, exchange) in exchanges.iter().enumerate().take(10) { // Show first 10
          info!("  Exchange {}: '{}', SecType='{}', ListingExch='{}', ServiceDataType='{}'",
                i + 1, exchange.exchange, exchange.sec_type, exchange.listing_exch, exchange.service_data_type);
          if let Some(agg_group) = exchange.agg_group {
            info!("    AggGroup: {}", agg_group);
          }
        }
        if exchanges.len() > 10 {
          info!("  ... and {} more exchanges", exchanges.len() - 10);
        }
      }
      Ok(())
    }
    Err(e) => {
      error!("Failed to get market depth exchanges: {:?}", e);
      Err(anyhow!("Market depth exchanges test failed: {:?}", e))
    }
  }
}

pub(super) fn smart_components_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
  info!("--- Testing SMART Components (with tickReqParams workflow) ---");
  let data_ref_mgr = client.data_ref();
  let mut overall_success = true;

  // Test with a common stock contract to get SMART components
  // This follows the correct workflow: market data request -> tickReqParams -> reqSmartComponents
  info!("Testing SMART components for AAPL stock contract");
  let test_contract = Contract::stock("AAPL"); // Use AAPL as test contract

  match data_ref_mgr.get_smart_components(&test_contract) {
    Ok(components) => {
      info!("Successfully received {} SMART components for AAPL", components.len());
      if components.is_empty() {
        warn!("Received empty SMART components for AAPL");
        // This might be normal if markets are closed or no SMART routing available
      } else {
        let mut sorted_components: Vec<_> = components.iter().collect();
        sorted_components.sort_by_key(|(bit_number, _)| *bit_number);

        info!("SMART Components for AAPL:");
        for (bit_number, (exchange_name, exchange_letter)) in sorted_components.iter().take(10) {
          info!("  Bit {}: Exchange='{}', Letter='{}'", bit_number, exchange_name, exchange_letter);
        }
        if components.len() > 10 {
          info!("  ... and {} more components", components.len() - 10);
        }

        // Show summary statistics
        let exchanges: std::collections::HashSet<_> = components.values().map(|(exchange, _)| exchange).collect();
        info!("  Total unique exchanges: {}", exchanges.len());

        // Show first few unique exchange names
        let mut exchange_names: Vec<_> = exchanges.into_iter().collect();
        exchange_names.sort();
        let sample_exchanges: Vec<_> = exchange_names.iter().take(5).collect();
        info!("  Sample exchanges: {:?}{}", sample_exchanges,
              if exchange_names.len() > 5 { "..." } else { "" });

        // Verify that common exchanges are present (if any components received)
        let has_nasdaq = components.values().any(|(exchange, _)| exchange.contains("NASDAQ"));
        let has_nyse = components.values().any(|(exchange, _)| exchange.contains("NYSE"));
        let has_arca = components.values().any(|(exchange, _)| exchange.contains("ARCA"));

        info!("  Contains NASDAQ: {}, NYSE: {}, ARCA: {}", has_nasdaq, has_nyse, has_arca);
      }
    }
    Err(e) => {
      error!("Failed to get SMART components for AAPL: {:?}", e);
      overall_success = false;

      // Check if this is a specific known error that might be expected
      match &e {
        IBKRError::ApiError(code, msg) => {
          if msg.contains("No market data during weekend") ||
            msg.contains("market data not available") ||
            msg.contains("outside of trading hours") {
              warn!("SMART components unavailable due to market hours/data availability: {} - {}", code, msg);
              // Don't mark as failure if it's just a timing/market hours issue
              overall_success = true;
            }
        }
        IBKRError::Timeout(_) => {
          warn!("SMART components request timed out - this might be due to market conditions");
          // Timeouts during off-market hours might be expected
        }
        _ => {
          // Other errors are unexpected
        }
      }
    }
  }

  // Additional test: Try with a different contract type if the first one worked
  if overall_success {
    info!("Testing SMART components for SPY (ETF) contract");
    let spy_contract = Contract::stock("SPY");

    match data_ref_mgr.get_smart_components(&spy_contract) {
      Ok(components) => {
        info!("Successfully received {} SMART components for SPY", components.len());
        if !components.is_empty() {
          info!("  SPY has {} routing components available", components.len());
        }
      }
      Err(e) => {
        error!("SPY SMART components request failed: {:?}", e);
        overall_success = false;
      }
    }
  }

  if overall_success {
    Ok(())
  } else {
    Err(anyhow!("SMART components test had significant issues"))
  }
}

pub(super) fn market_rule_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
  info!("--- Testing Market Rule (Dynamic Discovery) ---");
  let data_ref_mgr = client.data_ref();
  let mut overall_success = true;
  let mut discovered_rule_ids = std::collections::HashSet::new();

  // Define test contracts for different instrument types
  let test_contracts = vec![
    ("NYSE Stock", Contract::stock_with_exchange("IBM", "NYSE", "USD")),
    ("NASDAQ Stock", Contract::stock_with_exchange("AAPL", "NASDAQ", "USD")),
    ("Forex", {
      let mut forex = Contract::new();
      forex.symbol = "EUR".to_string();
      forex.sec_type = SecType::Forex;
      forex.exchange = "IDEALPRO".to_string();
      forex.currency = "USD".to_string();
      forex
    }),
    ("Bond", {
      let mut bond = Contract::new();
      bond.symbol = "912828C57".to_string(); // US Treasury bond CUSIP
      bond.sec_type = SecType::Bond;
      bond.exchange = "SMART".to_string();
      bond.currency = "USD".to_string();
      bond
    }),
  ];

  // Discover market rule IDs from contract details
  info!("Discovering market rule IDs from contract details...");
  for (contract_type, contract) in &test_contracts {
    info!("Getting contract details for {}: {}", contract_type, contract.symbol);

    match data_ref_mgr.get_contract_details(contract) {
      Ok(details_list) => {
        if details_list.is_empty() {
          warn!("No contract details found for {} ({})", contract_type, contract.symbol);
          continue;
        }

        for (i, details) in details_list.iter().enumerate() {
          let market_rule_ids_str = &details.market_rule_ids;
          info!("  Contract {}: ConID={}, MarketRuleIds='{}'",
                i + 1, details.contract.con_id, market_rule_ids_str);

          if !market_rule_ids_str.is_empty() {
            // Parse comma-separated market rule IDs
            for rule_id_str in market_rule_ids_str.split(',') {
              let rule_id_str = rule_id_str.trim();
              if !rule_id_str.is_empty() {
                match rule_id_str.parse::<i32>() {
                  Ok(rule_id) => {
                    discovered_rule_ids.insert(rule_id);
                    info!("    Discovered market rule ID: {}", rule_id);
                  }
                  Err(e) => {
                    warn!("    Failed to parse market rule ID '{}': {:?}", rule_id_str, e);
                  }
                }
              }
            }
          }
        }
      }
      Err(e) => {
        warn!("Failed to get contract details for {} ({}): {:?}",
              contract_type, contract.symbol, e);
        // Continue with other contracts
      }
    }

    // Small delay between contract detail requests
    std::thread::sleep(std::time::Duration::from_millis(200));
  }

  if discovered_rule_ids.is_empty() {
    error!("No market rule IDs discovered from contract details");
    return Err(anyhow!("No market rule IDs found to test"));
  }

  info!("Discovered {} unique market rule IDs: {:?}",
        discovered_rule_ids.len(),
        {
          let mut sorted_ids: Vec<_> = discovered_rule_ids.iter().collect();
          sorted_ids.sort();
          sorted_ids
        });

  // Test discovered market rule IDs
  info!("Testing discovered market rule IDs...");
  let mut successful_tests = 0;
  let mut total_tests = 0;

  // Test up to 5 rule IDs to avoid overwhelming the API
  for &rule_id in discovered_rule_ids.iter().take(5) {
    total_tests += 1;
    info!("Testing market rule ID: {}", rule_id);

    match data_ref_mgr.get_market_rule(rule_id) {
      Ok(market_rule) => {
        info!("✓ Successfully received market rule {}", market_rule.market_rule_id);
        info!("  Number of price increments: {}", market_rule.price_increments.len());

        if market_rule.price_increments.is_empty() {
          warn!("  Warning: Market rule {} has no price increments", rule_id);
        } else {
          // Show first few price increments
          for (i, increment) in market_rule.price_increments.iter().enumerate().take(3) {
            info!("    Increment {}: LowEdge=${:.6}, Increment=${:.6}",
                  i + 1, increment.low_edge, increment.increment);
          }
          if market_rule.price_increments.len() > 3 {
            info!("    ... and {} more increments", market_rule.price_increments.len() - 3);
          }
        }
        successful_tests += 1;
      }
      Err(e) => {
        error!("✗ Failed to get market rule {}: {:?}", rule_id, e);
        overall_success = false;
      }
    }

    // Small delay between market rule requests
    std::thread::sleep(std::time::Duration::from_millis(200));
  }

  info!("Market rule test results: {}/{} successful", successful_tests, total_tests);

  if overall_success && successful_tests > 0 {
    Ok(())
  } else if successful_tests == 0 {
    Err(anyhow!("No market rule requests succeeded"))
  } else {
    Err(anyhow!("Market rule test had some failures ({}/{} successful)", successful_tests, total_tests))
  }
}

pub(super) fn reference_data_impl(client: &IBKRClient, is_live: bool) -> Result<()> {
  info!("--- Running Comprehensive Reference Data Tests ---");
  let mut results = Vec::new();
  let mut overall_success = true;

  // Define all the test functions to run
  let tests = vec![
    ("Contract Details", contract_details_impl as fn(&IBKRClient, bool) -> Result<()>),
    ("Option Chain Parameters", option_chain_params_impl),
    ("Soft Dollar Tiers", soft_dollar_tiers_impl),
    ("Family Codes", family_codes_impl),
    ("Matching Symbols", matching_symbols_impl),
    ("Market Depth Exchanges", market_depth_exchanges_impl),
    ("SMART Components", smart_components_impl),
    ("Market Rule", market_rule_impl),
  ];

  for (test_name, test_func) in tests {
    info!("Starting test: {}", test_name);
    match test_func(client, is_live) {
      Ok(()) => {
        info!("✓ Test PASSED: {}", test_name);
        results.push((test_name, true));
      }
      Err(e) => {
        error!("✗ Test FAILED: {}: {:?}", test_name, e);
        results.push((test_name, false));
        overall_success = false;
      }
    }

    // Small delay between tests to avoid overwhelming the API
    std::thread::sleep(std::time::Duration::from_millis(500));
  }

  // Summary
  info!("=== Reference Data Tests Summary ===");
  let passed = results.iter().filter(|(_, success)| *success).count();
  let total = results.len();

  for (test_name, success) in &results {
    let status = if *success { "PASSED" } else { "FAILED" };
    info!("  {}: {}", test_name, status);
  }

  info!("Total: {}/{} tests passed", passed, total);

  if overall_success {
    Ok(())
  } else {
    Err(anyhow!("{}/{} reference data tests failed", total - passed, total))
  }
}
