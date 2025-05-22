// Builder for common options strategies

use crate::base::IBKRError;
use crate::contract::{Contract, SecType, OptionRight, ComboLeg};
use crate::data_ref_manager::{DataRefManager, SecDefOptParamsResult};
use crate::order::{OrderRequest, OrderSide, OrderType, TimeInForce};
use chrono::NaiveDate; // Removed unused Datelike

use log::{debug, info, warn};
use std::collections::HashMap;
use std::sync::Arc;

// --- Helper Structs ---

#[derive(Debug, Clone)]
struct OptionLegDefinition {
  expiry: NaiveDate, // Use NaiveDate internally
  strike: f64,
  right: OptionRight,
  action: OrderSide, // BUY/SELL for this leg
  ratio: i32,        // Typically 1 for standard strategies
}

#[derive(Debug, Clone)]
struct StrategyLeg {
  contract: Contract,
  action: OrderSide,
  ratio: i32,
}

// --- Builder ---

pub struct OptionsStrategyBuilder {
  data_ref_manager: Arc<DataRefManager>,
  underlying_symbol: String,
  underlying_con_id: Option<i32>,
  underlying_sec_type: SecType, // STK or FUT usually
  underlying_exchange: String,  // Primary exchange for underlying
  underlying_currency: String,
  quantity: f64, // Number of strategy units (e.g., number of spreads)
  legs: Vec<OptionLegDefinition>,
  strategy_name: Option<String>, // For logging/reference
  // Order parameters for the *combo* order
  order_type: OrderType,
  limit_price: Option<f64>, // Net debit/credit limit for the combo
  aux_price: Option<f64>,   // For stop triggers etc. on the combo
  tif: TimeInForce,
  account: Option<String>,
  order_ref: Option<String>,
  transmit: bool,
  underlying_price: Option<f64>, // Added for OTM calculations etc.
  // Cache for option chain params
  option_params_cache: HashMap<String, SecDefOptParamsResult>, // Keyed by exchange
  // Cache for fetched option contracts
  // Key: (expiry_str, strike_bits, right) - Cache key still uses YYYYMMDD string
  option_contract_cache: HashMap<(String, u64, OptionRight), Contract>,
}

impl OptionsStrategyBuilder {
  /// Start building an options strategy.
  ///
  /// # Arguments
  /// * `data_ref_manager` - Reference to the DataRefManager for fetching contract details.
  /// * `underlying_symbol` - The symbol of the underlying asset (e.g., "AAPL", "ES").
  /// * `underlying_price` - The current price of the underlying.
  /// * `quantity` - The number of strategy units to trade (e.g., 10 spreads). Must be positive.
  /// * `underlying_sec_type` - The security type of the underlying (Stock or Future).
  pub fn new(
    data_ref_manager: Arc<DataRefManager>,
    underlying_symbol: &str,
    underlying_price: f64,
    quantity: f64,
    underlying_sec_type: SecType,
  ) -> Result<Self, IBKRError> {
    if quantity <= 0.0 {
      return Err(IBKRError::InvalidParameter(
        "Strategy quantity must be positive".to_string(),
      ));
    }
    if underlying_sec_type != SecType::Stock && underlying_sec_type != SecType::Future {
      return Err(IBKRError::InvalidParameter(
        "Underlying security type must be Stock or Future".to_string(),
      ));
    }

    Ok(Self {
      data_ref_manager,
      underlying_symbol: underlying_symbol.to_string(),
      underlying_price: Some(underlying_price), // Wrap in Some()
      underlying_con_id: None, // Will be fetched
      underlying_sec_type,
      underlying_exchange: String::new(), // Will be fetched
      underlying_currency: String::new(), // Will be fetched
      quantity,
      legs: Vec::new(),
      strategy_name: None,
      // Default order params for the combo
      order_type: OrderType::Market,
      limit_price: None,            // User must set net price
      aux_price: None,
      tif: TimeInForce::Day,
      account: None,
      order_ref: None,
      transmit: true,
      option_params_cache: HashMap::new(),
      option_contract_cache: HashMap::new(),
    })
  }

  // --- Internal Helper Methods ---

  /// Fetches and caches the underlying contract details (conId, exchange, currency).
  fn fetch_underlying_details(&mut self) -> Result<(), IBKRError> {
    if self.underlying_con_id.is_some() {
      return Ok(()); // Already fetched
    }
    info!(
      "Fetching underlying details for {} ({})",
      self.underlying_symbol, self.underlying_sec_type
    );
    // Use SMART for Stocks, but leave empty for Futures to let TWS resolve exchange
    let initial_exchange = match self.underlying_sec_type {
        SecType::Stock => "SMART".to_string(),
        SecType::Future => String::new(), // Let TWS find the exchange for Futures
        _ => return Err(IBKRError::InvalidContract("Unsupported underlying type for fetching details".to_string())),
    };
    let underlying_contract_spec = Contract {
      symbol: self.underlying_symbol.clone(),
      sec_type: self.underlying_sec_type.clone(),
      exchange: initial_exchange,
      currency: String::new(),      // Allow API to determine currency
      ..Default::default()
    };

    let details_list = self
      .data_ref_manager
      .get_contract_details(&underlying_contract_spec)?; // Now sync

    if details_list.is_empty() {
      return Err(IBKRError::InvalidContract(format!(
        "No contract details found for underlying: {}",
        self.underlying_symbol
      )));
    }

    // Use the first result (or implement logic to choose if multiple)
    let details = &details_list[0];
    self.underlying_con_id = Some(details.contract.con_id);
    // Use primary_exchange if available, otherwise the main exchange from details
    self.underlying_exchange = details
      .contract
      .primary_exchange
      .clone()
      .unwrap_or_else(|| details.contract.exchange.clone());
    self.underlying_currency = details.contract.currency.clone();

    if self.underlying_exchange.is_empty() || self.underlying_currency.is_empty() {
      return Err(IBKRError::InvalidContract(format!(
        "Could not determine primary exchange or currency for underlying: {}",
        self.underlying_symbol
      )));
    }

    info!(
      "Found underlying details: ConID={}, Exchange={}, Currency={}",
      self.underlying_con_id.unwrap(),
      self.underlying_exchange,
      self.underlying_currency
    );
    Ok(())
  }

  /// Fetches and caches option chain parameters for a given exchange.
  fn fetch_option_params(&mut self, exchange: &str) -> Result<&SecDefOptParamsResult, IBKRError> {
    if !self.option_params_cache.contains_key(exchange) {
      info!(
        "Fetching option chain params for Underlying ConID={} on Exchange={}",
        self.underlying_con_id.unwrap(), exchange
      );
      // Note: get_option_chain_params expects fut_fop_exchange, which is usually "" for STK options
      let fut_fop_exchange = if self.underlying_sec_type == SecType::Future { exchange } else { "" };

      let params_list = self.data_ref_manager.get_option_chain_params(
        &self.underlying_symbol, // Symbol might not be strictly needed if conId is correct
        fut_fop_exchange,
        self.underlying_sec_type.clone(),
        self.underlying_con_id.unwrap(),
      )?; // Now sync

      // Find the params matching the requested exchange
      if let Some(params) = params_list.into_iter().find(|p| p.exchange == exchange) {
        debug!("Found option params for exchange {}: {} expirations, {} strikes",
               exchange, params.expirations.len(), params.strikes.len());
        self.option_params_cache.insert(exchange.to_string(), params);
      } else {
        return Err(IBKRError::InvalidContract(format!(
          "No option parameters found for Underlying ConID={} on Exchange={}",
          self.underlying_con_id.unwrap(), exchange
        )));
      }
    }
    // Return immutable reference from cache
    Ok(self.option_params_cache.get(exchange).unwrap()) // Safe unwrap due to contains_key check
  }

  /// Finds the nearest valid expiration date (as YYYYMMDD string) >= the target date.
  fn find_nearest_expiration<'a>(
    &self,
    target_expiry_date: NaiveDate,
    available_expirations: &'a [String], // These are YYYYMMDD strings from IBKR
  ) -> Result<&'a str, IBKRError> {
    let target_expiry_str = target_expiry_date.format("%Y%m%d").to_string();
    available_expirations
      .iter()
      .filter(|exp_str| exp_str.as_str() >= target_expiry_str.as_str()) // Compare strings
      .min() // Find the smallest string >= target
      .map(|s| s.as_str()) // Return as &str
      .ok_or_else(|| {
        IBKRError::InvalidParameter(format!(
          "No valid expiration found on or after {}",
          target_expiry_str
        ))
      })
  }

  /// Finds the nearest valid strike price to the target price.
  fn find_nearest_strike(
    &self,
    target_strike: f64,
    available_strikes: &[f64],
  ) -> Result<f64, IBKRError> {
    available_strikes
      .iter()
      .min_by(|a, b| (*a - target_strike).abs().partial_cmp(&(*b - target_strike).abs()).unwrap()) // Dereferenced a and b
      .cloned()
      .ok_or_else(|| IBKRError::InvalidParameter("No available strikes found".to_string()))
  }

  /// Fetches and caches the specific option contract details.
  fn fetch_option_contract(
    &mut self,
    expiry_date: NaiveDate,
    strike: f64,
    right: OptionRight,
  ) -> Result<&Contract, IBKRError> {
    // Format date to YYYYMMDD string for API call and cache key
    let expiry_str = expiry_date.format("%Y%m%d").to_string();
    let strike_bits = strike.to_bits();
    let cache_key = (expiry_str.clone(), strike_bits, right); // Cache key uses string

    if !self.option_contract_cache.contains_key(&cache_key) {
      info!(
        "Fetching option contract details: Exp={}, Strike={}, Right={}, UnderlyingConID={}",
        expiry_str, strike, right, self.underlying_con_id.unwrap()
      );

      // Determine option secType and exchange based on underlying
      let (option_sec_type, option_exchange) = match self.underlying_sec_type {
          SecType::Stock => (SecType::Option, "SMART".to_string()), // Use SMART for stock options
          SecType::Future => (SecType::FutureOption, self.underlying_exchange.clone()), // Use underlying's exchange for FOP
          _ => return Err(IBKRError::InvalidContract("Unsupported underlying type for options".to_string())),
      };

      let option_contract_spec = Contract {
        symbol: self.underlying_symbol.clone(),
        sec_type: option_sec_type, // Use determined SecType (OPT or FOP)
        last_trade_date_or_contract_month: Some(expiry_str.clone()), // Use formatted string
        strike: Some(strike),
        right: Some(right),
        exchange: option_exchange.to_string(),
        currency: self.underlying_currency.clone(),
        // Multiplier will be filled in from details below
        ..Default::default()
      };

      let details_list = self
        .data_ref_manager
        .get_contract_details(&option_contract_spec)?; // Now sync

      if details_list.is_empty() {
        return Err(IBKRError::InvalidContract(format!(
          "No contract details found for option: Exp={}, Strike={}, Right={}",
          expiry_str, strike, right
        )));
      }
      if details_list.len() > 1 {
        warn!("Multiple contracts found for option spec ({}, {}, {}). Using the first one (ConID={}).",
              expiry_str, strike, right, details_list[0].contract.con_id);
      }

      let mut contract = details_list[0].contract.clone(); // Clone the contract part

      // Ensure the multiplier is set correctly from the details
      if contract.multiplier.is_none() || contract.multiplier.as_deref() == Some("") {
          let detail_multiplier = details_list[0].contract.multiplier.clone(); // Multiplier from details
          if detail_multiplier.is_some() && detail_multiplier.as_deref() != Some("") {
              debug!("Updating contract multiplier from details: {:?}", detail_multiplier);
              contract.multiplier = detail_multiplier;
          } else {
              // Fallback or error if multiplier still missing
              let default_mult = match contract.sec_type {
                  SecType::FutureOption => "50", // Common default for ES, but might be wrong
                  _ => "100", // Default for STK options
              };
              warn!("Multiplier not found in contract details for ConID {}. Using default: {}", contract.con_id, default_mult);
              contract.multiplier = Some(default_mult.to_string());
          }
      }


      debug!("Found option contract: ConID={}, Multiplier={:?}", contract.con_id, contract.multiplier);
      // Insert using the original cache_key with strike_bits
      self.option_contract_cache.insert(cache_key.clone(), contract);
    }

    // Retrieve using the original cache_key with strike_bits
    Ok(self.option_contract_cache.get(&cache_key).unwrap()) // Safe unwrap
  }

  /// Adds a leg definition to the strategy.
  fn add_leg(
    &mut self,
    expiry: NaiveDate, // Accept NaiveDate
    strike: f64,
    right: OptionRight,
    action: OrderSide,
    ratio: i32,
  ) {
    self.legs.push(OptionLegDefinition {
      expiry, // Store NaiveDate
      strike,
      right,
      action,
      ratio,
    });
  }

  // --- Strategy Definition Methods ---

  // --- Vertical Spreads ---

  /// Defines a Bull Call Spread (Debit Call Spread). Buy lower strike call, Sell higher strike call.
  /// Strikes are approximate targets; the nearest available strikes will be used.
  pub fn bull_call_spread(self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64) -> Result<Self, IBKRError> { // Add mut
    self.vertical_spread_internal(expiry, OptionRight::Call, target_strike1, target_strike2, OrderSide::Buy, "Bull Call Spread")
  }

  /// Defines a Bear Call Spread (Credit Call Spread). Sell lower strike call, Buy higher strike call.
  /// Strikes are approximate targets; the nearest available strikes will be used.
  pub fn bear_call_spread(self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64) -> Result<Self, IBKRError> { // Add mut
    self.vertical_spread_internal(expiry, OptionRight::Call, target_strike1, target_strike2, OrderSide::Sell, "Bear Call Spread")
  }

  /// Defines a Bull Put Spread (Credit Put Spread). Sell higher strike put, Buy lower strike put.
  /// Strikes are approximate targets; the nearest available strikes will be used.
  pub fn bull_put_spread(self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64) -> Result<Self, IBKRError> { // Add mut
    // Note: Action is on the *lower* strike (strike1) in vertical_spread_internal
    self.vertical_spread_internal(expiry, OptionRight::Put, target_strike1, target_strike2, OrderSide::Buy, "Bull Put Spread")
  }

  /// Defines a Bear Put Spread (Debit Put Spread). Buy higher strike put, Sell lower strike put.
  /// Strikes are approximate targets; the nearest available strikes will be used.
  pub fn bear_put_spread(self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64) -> Result<Self, IBKRError> { // Add mut
    // Note: Action is on the *lower* strike (strike1) in vertical_spread_internal
    self.vertical_spread_internal(expiry, OptionRight::Put, target_strike1, target_strike2, OrderSide::Sell, "Bear Put Spread")
  }

  /// Internal helper for vertical spreads. `target_strike1` < `target_strike2`. `action` applies to `strike1`.
  /// Finds nearest available strikes.
  fn vertical_spread_internal(
    mut self, // Add mut self back
    expiry: NaiveDate, // Add expiry back
    right: OptionRight,
    target_strike1: f64,
    target_strike2: f64,
    action_strike1: OrderSide,
    name: &str,
  ) -> Result<Self, IBKRError> {
    if target_strike1 >= target_strike2 {
      return Err(IBKRError::InvalidParameter(format!(
        "For vertical spread, target_strike1 ({}) must be less than target_strike2 ({})", target_strike1, target_strike2
      )));
    }

    // Fetch details and params
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = {
        let params = self.fetch_option_params(&exch)?;
        params.strikes.clone()
    };

    // Find nearest actual strikes
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;

    // Ensure actual strikes maintain order after snapping (could become equal)
    if actual_strike1 >= actual_strike2 {
        return Err(IBKRError::InvalidParameter(format!(
            "Nearest strikes found ({}, {}) are not ordered correctly for vertical spread (target: {}/{})",
            actual_strike1, actual_strike2, target_strike1, target_strike2
        )));
    }


    let action_strike2 = match action_strike1 {
      OrderSide::Buy => OrderSide::Sell,
      OrderSide::Sell | OrderSide::SellShort => OrderSide::Buy,
    };

    self.strategy_name = Some(format!("{} {}/{}", name, actual_strike1, actual_strike2));
    self.legs.clear();
    self.add_leg(expiry, actual_strike1, right, action_strike1, 1);
    self.add_leg(expiry, actual_strike2, right, action_strike2, 1);
    Ok(self)
  }

  // --- Straddles / Strangles ---

  /// Defines a Long Straddle (Buy Call and Buy Put at the same strike/expiry).
  /// Strike is an approximate target; the nearest available strike will be used.
  pub fn long_straddle(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Long Straddle {}", actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Call, OrderSide::Buy, 1);
    self.add_leg(expiry, actual_strike, OptionRight::Put, OrderSide::Buy, 1);
    Ok(self)
  }

  /// Defines a Short Straddle (Sell Call and Sell Put at the same strike/expiry).
  /// Strike is an approximate target; the nearest available strike will be used.
  pub fn short_straddle(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Short Straddle {}", actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Call, OrderSide::Sell, 1);
    self.add_leg(expiry, actual_strike, OptionRight::Put, OrderSide::Sell, 1);
    Ok(self)
  }

  /// Defines a Long Strangle (Buy OTM Call and Buy OTM Put, same expiry).
  /// Strikes are approximate targets; the nearest available strikes will be used.
  pub fn long_strangle(
    mut self,
    expiry: NaiveDate,
    target_call_strike: f64,
    target_put_strike: f64,
  ) -> Result<Self, IBKRError> {
    if target_put_strike >= target_call_strike {
      return Err(IBKRError::InvalidParameter(
        "For strangle, target_put_strike must be less than target_call_strike".to_string(),
      ));
    }
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_call_strike = self.find_nearest_strike(target_call_strike, &available_strikes)?;
    let actual_put_strike = self.find_nearest_strike(target_put_strike, &available_strikes)?;

    if actual_put_strike >= actual_call_strike {
        return Err(IBKRError::InvalidParameter(format!(
            "Nearest strikes found ({}, {}) are not ordered correctly for strangle (target: {}/{})",
            actual_put_strike, actual_call_strike, target_put_strike, target_call_strike
        )));
    }

    // Optional: Add validation using self.underlying_price if available
    self.strategy_name = Some(format!(
      "Long Strangle {}/{}",
      actual_put_strike, actual_call_strike
    ));
    self.legs.clear();
    self.add_leg(expiry, actual_call_strike, OptionRight::Call, OrderSide::Buy, 1);
    self.add_leg(expiry, actual_put_strike, OptionRight::Put, OrderSide::Buy, 1);
    Ok(self)
  }

  /// Defines a Short Strangle (Sell OTM Call and Sell OTM Put, same expiry).
  /// Strikes are approximate targets; the nearest available strikes will be used.
  pub fn short_strangle(
    mut self,
    expiry: NaiveDate,
    target_call_strike: f64,
    target_put_strike: f64,
  ) -> Result<Self, IBKRError> {
    if target_put_strike >= target_call_strike {
      return Err(IBKRError::InvalidParameter(
        "For strangle, target_put_strike must be less than target_call_strike".to_string(),
      ));
    }
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_call_strike = self.find_nearest_strike(target_call_strike, &available_strikes)?;
    let actual_put_strike = self.find_nearest_strike(target_put_strike, &available_strikes)?;

    if actual_put_strike >= actual_call_strike {
        return Err(IBKRError::InvalidParameter(format!(
            "Nearest strikes found ({}, {}) are not ordered correctly for strangle (target: {}/{})",
            actual_put_strike, actual_call_strike, target_put_strike, target_call_strike
        )));
    }

    self.strategy_name = Some(format!(
      "Short Strangle {}/{}",
      actual_put_strike, actual_call_strike
    ));
    self.legs.clear();
    self.add_leg(expiry, actual_call_strike, OptionRight::Call, OrderSide::Sell, 1);
    self.add_leg(expiry, actual_put_strike, OptionRight::Put, OrderSide::Sell, 1);
    Ok(self)
  }

  // --- Box Spread ---

  /// Defines a Box Spread using the nearest available expiration >= `target_expiry`.
  /// Buys a Bull Call Spread and Buys a Bear Put Spread.
  /// Strikes are approximate targets; the nearest available strikes will be used.
  pub fn box_spread_nearest_expiry(
    mut self,
    target_expiry: NaiveDate,
    target_strike1: f64, // Lower target strike
    target_strike2: f64, // Higher target strike
  ) -> Result<Self, IBKRError> {
    if target_strike1 >= target_strike2 {
      return Err(IBKRError::InvalidParameter(
        "For box spread, target_strike1 must be less than target_strike2".to_string(),
      ));
    }
    // 1. Fetch underlying details if needed
    self.fetch_underlying_details()?;
    // 2. Fetch option params for the primary exchange
    let exch = self.underlying_exchange.clone();
    let (expirations, available_strikes) = { // Fetch params and clone data
        let params = self.fetch_option_params(&exch)?;
        (params.expirations.clone(), params.strikes.clone())
    };
    // 3. Find nearest expiry string using the cloned list
    let expiry_str = self.find_nearest_expiration(target_expiry, &expirations)?;
    // 4. Parse the found expiry string back to NaiveDate for internal use
    let expiry_date = NaiveDate::parse_from_str(expiry_str, "%Y%m%d")
      .map_err(|e| IBKRError::ParseError(format!("Failed to parse expiry string '{}': {}", expiry_str, e)))?;
    // 5. Find nearest actual strikes
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;

    if actual_strike1 >= actual_strike2 {
        return Err(IBKRError::InvalidParameter(format!(
            "Nearest strikes found ({}, {}) are not ordered correctly for box spread (target: {}/{})",
            actual_strike1, actual_strike2, target_strike1, target_strike2
        )));
    }

    self.strategy_name = Some(format!("Box Spread {}/{} (Exp: {})", actual_strike1, actual_strike2, expiry_str));
    self.legs.clear();
    // Bull Call Spread part
    self.add_leg(expiry_date, actual_strike1, OptionRight::Call, OrderSide::Buy, 1);
    self.add_leg(expiry_date, actual_strike2, OptionRight::Call, OrderSide::Sell, 1);
    // Bear Put Spread part
    self.add_leg(expiry_date, actual_strike2, OptionRight::Put, OrderSide::Buy, 1);
    self.add_leg(expiry_date, actual_strike1, OptionRight::Put, OrderSide::Sell, 1);
    Ok(self)
  }

  // --- Single Leg Options ---

  /// Buy a single Call option.
  /// Strike is an approximate target; the nearest available strike will be used.
  pub fn buy_call(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Buy Call {} {}", expiry.format("%Y%m%d"), actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Call, OrderSide::Buy, 1);
    Ok(self)
  }

  /// Sell (short) a single Call option (Naked Call).
  /// Strike is an approximate target; the nearest available strike will be used.
  pub fn sell_call(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Sell Call {} {}", expiry.format("%Y%m%d"), actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Call, OrderSide::Sell, 1);
    Ok(self)
  }

  /// Buy a single Put option.
  /// Strike is an approximate target; the nearest available strike will be used.
  pub fn buy_put(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Buy Put {} {}", expiry.format("%Y%m%d"), actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Put, OrderSide::Buy, 1);
    Ok(self)
  }

  /// Sell (short) a single Put option (Naked Put).
  /// Strike is an approximate target; the nearest available strike will be used.
  pub fn sell_put(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Sell Put {} {}", expiry.format("%Y%m%d"), actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Put, OrderSide::Sell, 1);
    Ok(self)
  }

  // --- Strategies involving Stock (Option Legs Only) ---
  // Note: The stock leg needs to be handled separately by the caller.

  /// Defines the option legs for a Collar (Long Put + Short Call).
  /// Stock leg (Long Stock) must be handled separately.
  /// Strikes are approximate targets; the nearest available strikes will be used.
  pub fn collar_options(mut self, expiry: NaiveDate, target_put_strike: f64, target_call_strike: f64) -> Result<Self, IBKRError> {
    if target_put_strike >= target_call_strike {
      return Err(IBKRError::InvalidParameter("For collar, target_put_strike must be less than target_call_strike".to_string()));
    }
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_put_strike = self.find_nearest_strike(target_put_strike, &available_strikes)?;
    let actual_call_strike = self.find_nearest_strike(target_call_strike, &available_strikes)?;

    if actual_put_strike >= actual_call_strike {
        return Err(IBKRError::InvalidParameter(format!(
            "Nearest strikes found ({}, {}) are not ordered correctly for collar (target: {}/{})",
            actual_put_strike, actual_call_strike, target_put_strike, target_call_strike
        )));
    }

    self.strategy_name = Some(format!("Collar Options {}/{}", actual_put_strike, actual_call_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_put_strike, OptionRight::Put, OrderSide::Buy, 1); // Long Put
    self.add_leg(expiry, actual_call_strike, OptionRight::Call, OrderSide::Sell, 1); // Short Call
    Ok(self)
  }

  /// Defines the option leg for a Covered Call (Short Call).
  /// Stock leg (Long Stock) must be handled separately.
  /// Strike is an approximate target; the nearest available strike will be used.
  pub fn covered_call_option(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Covered Call Option {}", actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Call, OrderSide::Sell, 1); // Short Call
    Ok(self)
  }

  /// Defines the option leg for a Covered Put (Short Put).
  /// Stock leg (Short Stock) must be handled separately.
  /// Strike is an approximate target; the nearest available strike will be used.
  pub fn covered_put_option(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Covered Put Option {}", actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Put, OrderSide::Sell, 1); // Short Put
    Ok(self)
  }

  /// Defines the option leg for a Protective Put (Long Put).
  /// Stock leg (Long Stock) must be handled separately.
  /// Strike is an approximate target; the nearest available strike will be used.
  pub fn protective_put_option(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Protective Put Option {}", actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Put, OrderSide::Buy, 1); // Long Put
    Ok(self)
  }

  /// Defines the option legs for a Stock Repair strategy (Buy 1 Call, Sell 2 Higher Strike Calls).
  /// Stock leg (Long Stock) must be handled separately.
  /// Strikes are approximate targets; the nearest available strikes will be used.
  pub fn stock_repair_options(mut self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64) -> Result<Self, IBKRError> {
    if target_strike1 >= target_strike2 {
      return Err(IBKRError::InvalidParameter("For stock repair, target_strike1 must be less than target_strike2".to_string()));
    }
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;

    if actual_strike1 >= actual_strike2 {
        return Err(IBKRError::InvalidParameter(format!(
            "Nearest strikes found ({}, {}) are not ordered correctly for stock repair (target: {}/{})",
            actual_strike1, actual_strike2, target_strike1, target_strike2
        )));
    }

    self.strategy_name = Some(format!("Stock Repair Options {}/{}", actual_strike1, actual_strike2));
    self.legs.clear();
    self.add_leg(expiry, actual_strike1, OptionRight::Call, OrderSide::Buy, 1);  // Buy 1 Call
    self.add_leg(expiry, actual_strike2, OptionRight::Call, OrderSide::Sell, 2); // Sell 2 Calls
    Ok(self)
  }

  // --- Ratio Spreads ---

  /// Defines a Long Ratio Call Spread (e.g., Buy 1 Lower Call, Sell N Higher Calls).
  /// Strikes are approximate targets; the nearest available strikes will be used.
  pub fn long_ratio_call_spread(mut self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64, buy_ratio: i32, sell_ratio: i32) -> Result<Self, IBKRError> {
    if target_strike1 >= target_strike2 { return Err(IBKRError::InvalidParameter("target_strike1 must be less than target_strike2".into())); }
    if buy_ratio <= 0 || sell_ratio <= 0 { return Err(IBKRError::InvalidParameter("Ratios must be positive".into())); }
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;

    if actual_strike1 >= actual_strike2 {
        return Err(IBKRError::InvalidParameter(format!(
            "Nearest strikes found ({}, {}) are not ordered correctly for ratio spread (target: {}/{})",
            actual_strike1, actual_strike2, target_strike1, target_strike2
        )));
    }

    self.strategy_name = Some(format!("Long Ratio Call Spread {}/{} ({}:{})", actual_strike1, actual_strike2, buy_ratio, sell_ratio));
    self.legs.clear();
    self.add_leg(expiry, actual_strike1, OptionRight::Call, OrderSide::Buy, buy_ratio);
    self.add_leg(expiry, actual_strike2, OptionRight::Call, OrderSide::Sell, sell_ratio);
    Ok(self)
  }

  /// Defines a Long Ratio Put Spread (e.g., Buy N Higher Puts, Sell 1 Lower Put).
  /// Strikes are approximate targets; the nearest available strikes will be used.
  pub fn long_ratio_put_spread(mut self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64, buy_ratio: i32, sell_ratio: i32) -> Result<Self, IBKRError> {
    if target_strike1 >= target_strike2 { return Err(IBKRError::InvalidParameter("target_strike1 must be less than target_strike2".into())); }
    if buy_ratio <= 0 || sell_ratio <= 0 { return Err(IBKRError::InvalidParameter("Ratios must be positive".into())); }
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;

    if actual_strike1 >= actual_strike2 {
        return Err(IBKRError::InvalidParameter(format!(
            "Nearest strikes found ({}, {}) are not ordered correctly for ratio spread (target: {}/{})",
            actual_strike1, actual_strike2, target_strike1, target_strike2
        )));
    }

    self.strategy_name = Some(format!("Long Ratio Put Spread {}/{} ({}:{})", actual_strike1, actual_strike2, buy_ratio, sell_ratio));
    self.legs.clear();
    self.add_leg(expiry, actual_strike2, OptionRight::Put, OrderSide::Buy, buy_ratio);
    self.add_leg(expiry, actual_strike1, OptionRight::Put, OrderSide::Sell, sell_ratio);
    Ok(self)
  }

  /// Defines a Short Ratio Put Spread (e.g., Sell N Higher Puts, Buy 1 Lower Put).
  /// Strikes are approximate targets; the nearest available strikes will be used.
  pub fn short_ratio_put_spread(mut self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64, sell_ratio: i32, buy_ratio: i32) -> Result<Self, IBKRError> {
    if target_strike1 >= target_strike2 { return Err(IBKRError::InvalidParameter("target_strike1 must be less than target_strike2".into())); }
    if buy_ratio <= 0 || sell_ratio <= 0 { return Err(IBKRError::InvalidParameter("Ratios must be positive".into())); }
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;

    if actual_strike1 >= actual_strike2 {
        return Err(IBKRError::InvalidParameter(format!(
            "Nearest strikes found ({}, {}) are not ordered correctly for ratio spread (target: {}/{})",
            actual_strike1, actual_strike2, target_strike1, target_strike2
        )));
    }

    self.strategy_name = Some(format!("Short Ratio Put Spread {}/{} ({}:{})", actual_strike1, actual_strike2, sell_ratio, buy_ratio));
    self.legs.clear();
    self.add_leg(expiry, actual_strike2, OptionRight::Put, OrderSide::Sell, sell_ratio);
    self.add_leg(expiry, actual_strike1, OptionRight::Put, OrderSide::Buy, buy_ratio);
    Ok(self)
  }

  // --- Butterflies ---
  // target_strike1 < target_strike2 < target_strike3

  /// Defines a Long Put Butterfly (Buy 1 High Put, Sell 2 Mid Puts, Buy 1 Low Put).
  /// Strikes are approximate targets; the nearest available strikes will be used.
  pub fn long_put_butterfly(mut self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64, target_strike3: f64) -> Result<Self, IBKRError> {
    if !(target_strike1 < target_strike2 && target_strike2 < target_strike3) { return Err(IBKRError::InvalidParameter("Target strikes must be ordered: strike1 < strike2 < strike3".into())); }
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;
    let actual_strike3 = self.find_nearest_strike(target_strike3, &available_strikes)?;

    if !(actual_strike1 < actual_strike2 && actual_strike2 < actual_strike3) {
        return Err(IBKRError::InvalidParameter(format!(
            "Nearest strikes found ({}, {}, {}) are not ordered correctly for butterfly (target: {}/{}/{})",
            actual_strike1, actual_strike2, actual_strike3, target_strike1, target_strike2, target_strike3
        )));
    }
    // Optional: Check for equal distance: strike2-strike1 == strike3-strike2
    self.strategy_name = Some(format!("Long Put Butterfly {}/{}/{}", actual_strike1, actual_strike2, actual_strike3));
    self.legs.clear();
    self.add_leg(expiry, actual_strike3, OptionRight::Put, OrderSide::Buy, 1);
    self.add_leg(expiry, actual_strike2, OptionRight::Put, OrderSide::Sell, 2);
    self.add_leg(expiry, actual_strike1, OptionRight::Put, OrderSide::Buy, 1);
    Ok(self)
  }

  /// Defines a Short Call Butterfly (Sell 1 Low Call, Buy 2 Mid Calls, Sell 1 High Call).
  /// Strikes are approximate targets; the nearest available strikes will be used.
  pub fn short_call_butterfly(mut self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64, target_strike3: f64) -> Result<Self, IBKRError> {
    if !(target_strike1 < target_strike2 && target_strike2 < target_strike3) { return Err(IBKRError::InvalidParameter("Target strikes must be ordered: strike1 < strike2 < strike3".into())); }
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;
    let actual_strike3 = self.find_nearest_strike(target_strike3, &available_strikes)?;

    if !(actual_strike1 < actual_strike2 && actual_strike2 < actual_strike3) {
        return Err(IBKRError::InvalidParameter(format!(
            "Nearest strikes found ({}, {}, {}) are not ordered correctly for butterfly (target: {}/{}/{})",
            actual_strike1, actual_strike2, actual_strike3, target_strike1, target_strike2, target_strike3
        )));
    }
    self.strategy_name = Some(format!("Short Call Butterfly {}/{}/{}", actual_strike1, actual_strike2, actual_strike3));
    self.legs.clear();
    self.add_leg(expiry, actual_strike1, OptionRight::Call, OrderSide::Sell, 1);
    self.add_leg(expiry, actual_strike2, OptionRight::Call, OrderSide::Buy, 2);
    self.add_leg(expiry, actual_strike3, OptionRight::Call, OrderSide::Sell, 1);
    Ok(self)
  }

  /// Defines a Long Iron Butterfly (Buy OTM Put, Sell ATM Put, Sell ATM Call, Buy OTM Call).
  /// Assumes target_strike2 is the ATM strike. target_strike1 < target_strike2 < target_strike3.
  /// Strikes are approximate targets; the nearest available strikes will be used.
  pub fn long_iron_butterfly(mut self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64, target_strike3: f64) -> Result<Self, IBKRError> {
    if !(target_strike1 < target_strike2 && target_strike2 < target_strike3) { return Err(IBKRError::InvalidParameter("Target strikes must be ordered: strike1 < strike2 < strike3".into())); }
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;
    let actual_strike3 = self.find_nearest_strike(target_strike3, &available_strikes)?;

    if !(actual_strike1 < actual_strike2 && actual_strike2 < actual_strike3) {
        return Err(IBKRError::InvalidParameter(format!(
            "Nearest strikes found ({}, {}, {}) are not ordered correctly for iron butterfly (target: {}/{}/{})",
            actual_strike1, actual_strike2, actual_strike3, target_strike1, target_strike2, target_strike3
        )));
    }
    self.strategy_name = Some(format!("Long Iron Butterfly {}/{}/{}", actual_strike1, actual_strike2, actual_strike3));
    self.legs.clear();
    self.add_leg(expiry, actual_strike1, OptionRight::Put, OrderSide::Buy, 1);  // Buy Low Put
    self.add_leg(expiry, actual_strike2, OptionRight::Put, OrderSide::Sell, 1); // Sell Mid Put
    self.add_leg(expiry, actual_strike2, OptionRight::Call, OrderSide::Sell, 1); // Sell Mid Call
    self.add_leg(expiry, actual_strike3, OptionRight::Call, OrderSide::Buy, 1);  // Buy High Call
    Ok(self)
  }

  // --- Condors ---
  // target_strike1 < target_strike2 < target_strike3 < target_strike4

  /// Defines a Long Put Condor (Buy High Put, Sell Mid-High Put, Sell Mid-Low Put, Buy Low Put).
  /// Strikes are approximate targets; the nearest available strikes will be used.
  pub fn long_put_condor(mut self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64, target_strike3: f64, target_strike4: f64) -> Result<Self, IBKRError> {
    if !(target_strike1 < target_strike2 && target_strike2 < target_strike3 && target_strike3 < target_strike4) { return Err(IBKRError::InvalidParameter("Target strikes must be ordered: strike1 < strike2 < strike3 < strike4".into())); }
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;
    let actual_strike3 = self.find_nearest_strike(target_strike3, &available_strikes)?;
    let actual_strike4 = self.find_nearest_strike(target_strike4, &available_strikes)?;

    if !(actual_strike1 < actual_strike2 && actual_strike2 < actual_strike3 && actual_strike3 < actual_strike4) {
        return Err(IBKRError::InvalidParameter(format!(
            "Nearest strikes found ({}, {}, {}, {}) are not ordered correctly for condor (target: {}/{}/{}/{})",
            actual_strike1, actual_strike2, actual_strike3, actual_strike4, target_strike1, target_strike2, target_strike3, target_strike4
        )));
    }
    // Optional: Check for equal wing widths
    self.strategy_name = Some(format!("Long Put Condor {}/{}/{}/{}", actual_strike1, actual_strike2, actual_strike3, actual_strike4));
    self.legs.clear();
    self.add_leg(expiry, actual_strike4, OptionRight::Put, OrderSide::Buy, 1);  // Buy Highest Put
    self.add_leg(expiry, actual_strike3, OptionRight::Put, OrderSide::Sell, 1); // Sell Mid-High Put
    self.add_leg(expiry, actual_strike2, OptionRight::Put, OrderSide::Sell, 1); // Sell Mid-Low Put
    self.add_leg(expiry, actual_strike1, OptionRight::Put, OrderSide::Buy, 1);  // Buy Lowest Put
    Ok(self)
  }

  /// Defines a Short Condor / Iron Condor (Sell OTM Put Spread, Sell OTM Call Spread).
  /// Sell Low Put, Buy Lower Put, Sell High Call, Buy Higher Call.
  /// Strikes are approximate targets; the nearest available strikes will be used.
  pub fn short_condor(mut self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64, target_strike3: f64, target_strike4: f64) -> Result<Self, IBKRError> {
    if !(target_strike1 < target_strike2 && target_strike2 < target_strike3 && target_strike3 < target_strike4) { return Err(IBKRError::InvalidParameter("Target strikes must be ordered: strike1 < strike2 < strike3 < strike4".into())); }
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;
    let actual_strike3 = self.find_nearest_strike(target_strike3, &available_strikes)?;
    let actual_strike4 = self.find_nearest_strike(target_strike4, &available_strikes)?;

    if !(actual_strike1 < actual_strike2 && actual_strike2 < actual_strike3 && actual_strike3 < actual_strike4) {
        return Err(IBKRError::InvalidParameter(format!(
            "Nearest strikes found ({}, {}, {}, {}) are not ordered correctly for condor (target: {}/{}/{}/{})",
            actual_strike1, actual_strike2, actual_strike3, actual_strike4, target_strike1, target_strike2, target_strike3, target_strike4
        )));
    }
    self.strategy_name = Some(format!("Short Condor (Iron) {}/{}/{}/{}", actual_strike1, actual_strike2, actual_strike3, actual_strike4));
    self.legs.clear();
    self.add_leg(expiry, actual_strike1, OptionRight::Put, OrderSide::Buy, 1);  // Buy Lowest Put
    self.add_leg(expiry, actual_strike2, OptionRight::Put, OrderSide::Sell, 1); // Sell Mid-Low Put
    self.add_leg(expiry, actual_strike3, OptionRight::Call, OrderSide::Sell, 1); // Sell Mid-High Call
    self.add_leg(expiry, actual_strike4, OptionRight::Call, OrderSide::Buy, 1);  // Buy Highest Call
    Ok(self)
  }

  // --- Calendar Spreads ---

  /// Defines a Long Put Calendar Spread (Sell Near Put, Buy Far Put, same strike).
  /// Strike is an approximate target; the nearest available strike will be used.
  pub fn long_put_calendar_spread(mut self, target_strike: f64, near_expiry: NaiveDate, far_expiry: NaiveDate) -> Result<Self, IBKRError> {
    if near_expiry >= far_expiry { return Err(IBKRError::InvalidParameter("near_expiry must be before far_expiry".into())); }
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    // Assume strikes are available for both expiries on the same exchange
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Long Put Calendar {} {}/{}", actual_strike, near_expiry.format("%Y%m%d"), far_expiry.format("%Y%m%d")));
    self.legs.clear();
    self.add_leg(near_expiry, actual_strike, OptionRight::Put, OrderSide::Sell, 1);
    self.add_leg(far_expiry, actual_strike, OptionRight::Put, OrderSide::Buy, 1);
    Ok(self)
  }

  /// Defines a Short Call Calendar Spread (Buy Near Call, Sell Far Call, same strike).
  /// Strike is an approximate target; the nearest available strike will be used.
  pub fn short_call_calendar_spread(mut self, target_strike: f64, near_expiry: NaiveDate, far_expiry: NaiveDate) -> Result<Self, IBKRError> {
    if near_expiry >= far_expiry { return Err(IBKRError::InvalidParameter("near_expiry must be before far_expiry".into())); }
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Short Call Calendar {} {}/{}", actual_strike, near_expiry.format("%Y%m%d"), far_expiry.format("%Y%m%d")));
    self.legs.clear();
    self.add_leg(near_expiry, actual_strike, OptionRight::Call, OrderSide::Buy, 1);
    self.add_leg(far_expiry, actual_strike, OptionRight::Call, OrderSide::Sell, 1);
    Ok(self)
  }

  // --- Synthetics ---

  /// Defines a Synthetic Long Put (Short Stock + Long Call). Option leg only.
  /// Strike is an approximate target; the nearest available strike will be used.
  pub fn synthetic_long_put_option(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Synthetic Long Put Option {}", actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Call, OrderSide::Buy, 1); // Long Call
    Ok(self)
  }

  /// Defines a Synthetic Long Stock (Long Call + Short Put).
  /// Strike is an approximate target; the nearest available strike will be used.
  pub fn synthetic_long_stock(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Synthetic Long Stock {}", actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Call, OrderSide::Buy, 1);
    self.add_leg(expiry, actual_strike, OptionRight::Put, OrderSide::Sell, 1);
    Ok(self)
  }

  /// Defines a Synthetic Short Stock (Short Call + Long Put).
  /// Strike is an approximate target; the nearest available strike will be used.
  pub fn synthetic_short_stock(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let exch = self.underlying_exchange.clone();
    let available_strikes = { let params = self.fetch_option_params(&exch)?; params.strikes.clone() };
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Synthetic Short Stock {}", actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Call, OrderSide::Sell, 1);
    self.add_leg(expiry, actual_strike, OptionRight::Put, OrderSide::Buy, 1);
    Ok(self)
  }

  // --- Unsupported / Notes ---

  // Note: Cash-Backed Call/Put are conceptual, not specific order types.
  // Note: Long/Short Stock are single-leg orders, use OrderBuilder.
  // Note: Covered Ratio Spread involves stock leg.
  // Note: Covered Strangle involves stock leg.
  // Note: Bear Spread Spread / Double Bear Spread / Combination Bear Spread are ambiguous. Use specific types like Bear Call/Put Spread.
  // Note: Double Bull Spread is ambiguous. Use specific types like Bull Call/Put Spread.
  // Note: Buying Index Calls/Puts are single-leg orders, use buy_call/buy_put.


  // --- Order Parameter Methods ---

  /// Set the net limit price for the combo order (Debit or Credit).
  /// Positive for Debit, Negative for Credit.
  pub fn with_limit_price(mut self, price: f64) -> Self {
    self.limit_price = Some(price);
    self.order_type = OrderType::Limit; // Ensure LMT type
    self
  }

  /// Set the order type for the combo (e.g., Market). Default is Limit.
  pub fn with_order_type(mut self, order_type: OrderType) -> Self {
    self.order_type = order_type;
    if order_type != OrderType::Limit {
      self.limit_price = None; // Clear limit price if not LMT
    }
    self
  }

  /// Set the Time-In-Force for the combo order. Default is Day.
  pub fn with_tif(mut self, tif: TimeInForce) -> Self {
    self.tif = tif;
    self
  }

  /// Set the account for the combo order.
  pub fn with_account(mut self, account: &str) -> Self {
    self.account = Some(account.to_string());
    self
  }

  /// Set the order reference for the combo order.
  pub fn with_order_ref(mut self, order_ref: &str) -> Self {
    self.order_ref = Some(order_ref.to_string());
    self
  }

  /// Set whether the combo order should be transmitted immediately. Default is true.
  pub fn with_transmit(mut self, transmit: bool) -> Self {
    self.transmit = transmit;
    self
  }

  /// Set the underlying price, used for strike selection logic (e.g., OTM).
  pub fn with_underlying_price(mut self, price: f64) -> Self {
    self.underlying_price = Some(price);
    self
  }

  // --- Build Method ---

  /// Finalize the strategy, fetch contracts, and build the combo order.
  pub fn build(mut self) -> Result<(Contract, OrderRequest), IBKRError> {
    info!(
      "Building strategy: {} for {}",
      self.strategy_name.as_deref().unwrap_or("Unnamed"),
      self.underlying_symbol
    );

    if self.legs.is_empty() {
      return Err(IBKRError::InvalidOrder(
        "No strategy legs defined.".to_string(),
      ));
    }

    // 1. Ensure underlying details are fetched
    self.fetch_underlying_details()?;

    // 2. Collect leg definitions to avoid borrow checker issues
    let leg_definitions: Vec<OptionLegDefinition> = self.legs.clone(); // Clone the definitions

    // 3. Fetch all required option contracts (now iterating over cloned data)
    let mut final_legs: Vec<StrategyLeg> = Vec::with_capacity(leg_definitions.len());
    let mut combo_symbol = String::new(); // Build combo symbol (optional)

    for (i, leg_def) in leg_definitions.iter().enumerate() {
      // TODO: Add logic to fetch option params if needed (e.g., for multiplier)
      // let params = self.fetch_option_params("SMART").await?; // Assuming SMART

      // Now `self` can be borrowed mutably here
      let option_contract = self
        .fetch_option_contract(leg_def.expiry, leg_def.strike, leg_def.right)?; // Pass NaiveDate

      final_legs.push(StrategyLeg {
        contract: option_contract.clone(), // Clone the fetched contract
        action: leg_def.action,
        ratio: leg_def.ratio,
      });

      // Build a simple combo symbol representation (e.g., AAPL C170/P160)
      if i > 0 { combo_symbol.push('/'); }
      combo_symbol.push_str(&format!("{}{}{}", leg_def.right, leg_def.strike, leg_def.action.to_string().chars().next().unwrap() ));
    }

    // 4. Create the Combo Contract (BAG)
    let mut combo_contract = Contract {
      symbol: self.underlying_symbol.clone(), // Use underlying symbol for combo
      sec_type: SecType::Combo,
      currency: self.underlying_currency.clone(),
      exchange: self.underlying_exchange.clone(), // Route combo via underlying's exchange? Or SMART? Use underlying's for now.
      combo_legs: Vec::with_capacity(final_legs.len()),
      ..Default::default()
    };

    for leg in &final_legs {
      combo_contract.combo_legs.push(ComboLeg {
        con_id: leg.contract.con_id,
        ratio: leg.ratio,
        action: leg.action.to_string(),
        exchange: leg.contract.exchange.clone(), // Use the specific option leg's exchange
        // Defaults for other ComboLeg fields
        open_close: 0,
        short_sale_slot: 0,
        designated_location: "".to_string(),
        exempt_code: -1,
        price: None, // Not setting individual leg prices here
      });
    }

    // 4. Create the Combo OrderRequest
    let order_request = OrderRequest { // Removed mut
      side: OrderSide::Buy, // Action for combo is determined by net price (Debit=BUY, Credit=SELL) - API requires one, but price sign matters most. Let's default to BUY and rely on price sign.
      quantity: self.quantity,
      order_type: self.order_type,
      limit_price: self.limit_price,
      aux_price: self.aux_price,
      time_in_force: self.tif,
      account: self.account.clone(),
      order_ref: self.order_ref.clone(),
      transmit: self.transmit,
      // Set other relevant fields from builder state
      ..Default::default()
    };

    // Validation for combo order
    if order_request.order_type == OrderType::Limit && order_request.limit_price.is_none() {
      warn!("Combo order type is LMT but no limit_price (net debit/credit) was set. Order might be rejected or behave unexpectedly.");
      // Consider returning error? Or let TWS reject? Let TWS reject for now.
      // return Err(IBKRError::InvalidOrder("Limit price (net debit/credit) must be set for LMT combo orders.".to_string()));
    }
    if order_request.order_type != OrderType::Limit && order_request.order_type != OrderType::Market {
      // Only LMT and MKT are typically supported for BAG combos directly. REL might work sometimes.
      warn!("Order type {} may not be supported for direct combo (BAG) orders. Consider LMT or MKT.", order_request.order_type);
    }


    info!(
      "Strategy build complete. Combo Contract: Symbol={}, Legs={}, Order: Type={}, Qty={}, LmtPx={:?}",
      combo_contract.symbol,
      combo_contract.combo_legs.len(),
      order_request.order_type,
      order_request.quantity,
      order_request.limit_price
    );

    Ok((combo_contract, order_request))
  }
}


// --- TODO ---
// - Implement remaining strategy methods.
// - Add support for fetching underlying price for strike selection (requires DataMarketManager).
// - Add more robust error handling and validation (e.g., check available strikes/expirations).
// - Handle strategies involving stock legs (e.g., Covered Call) - might need separate orders or different combo setup.
// - Refine exchange handling for options and combos.
// - Add tests.
