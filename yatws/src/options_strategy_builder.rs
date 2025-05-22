// Builder for common options strategies with enhanced exchange handling

use crate::base::IBKRError;
use crate::contract::{Contract, SecType, OptionRight, ComboLeg};
use crate::data_ref_manager::{DataRefManager, SecDefOptParamsResult};
use crate::order::{OrderRequest, OrderSide, OrderType, TimeInForce};
use chrono::NaiveDate;
use log::{debug, info, warn, error};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

// --- New Data Structures for Exchange Handling ---

/// Contains exchange-specific option chain information with liquidity metrics
#[derive(Debug, Clone)]
pub struct OptionExchangeInfo {
  pub exchange: String,
  pub trading_class: String,
  pub multiplier: String,
  pub expirations: Vec<String>,
  pub strikes: Vec<f64>,
  pub liquidity_score: f64,           // Calculated liquidity score (0.0-1.0)
  pub completeness_score: f64,        // Based on strikes/expirations available
  pub market_share_estimate: f64,     // Estimated % of volume (0.0-1.0)
}

/// Aggregated option chain parameters across all exchanges
#[derive(Debug, Clone)]
pub struct OptionChainParams {
  pub underlying_con_id: i32,
  pub exchanges: Vec<OptionExchangeInfo>, // Multiple exchanges
  pub primary_exchange: Option<String>,    // Suggested primary
}

/// Strategy for selecting which exchange to use for options
#[derive(Debug, Clone)]
pub enum ExchangeSelectionStrategy {
  /// Use SMART routing (let IBKR choose)
  Smart,
  /// Use specific exchange
  Specific(String),
  /// Use the exchange with most available strikes/expirations
  MostComplete,
  /// Use exchange with highest estimated liquidity
  HighestLiquidity,
  /// Use exchange suggested as primary
  Primary,
}

/// Configuration for liquidity estimation
#[derive(Debug, Clone)]
pub struct LiquidityEstimationConfig {
  pub enable_spread_sampling: bool,
  pub spread_sample_timeout: Duration,
  pub max_spread_samples: usize,
  pub cache_ttl: Duration,
  pub fallback_to_smart_on_timeout: bool,
}

impl Default for LiquidityEstimationConfig {
  fn default() -> Self {
    Self {
      enable_spread_sampling: false, // Conservative default
      spread_sample_timeout: Duration::from_secs(5),
      max_spread_samples: 3,
      cache_ttl: Duration::from_secs(600), // 10 minutes
      fallback_to_smart_on_timeout: true,
    }
  }
}

// --- Helper Structs ---

#[derive(Debug, Clone)]
struct OptionLegDefinition {
  expiry: NaiveDate,
  strike: f64,
  right: OptionRight,
  action: OrderSide,
  ratio: i32,
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
  underlying_sec_type: SecType,
  underlying_exchange: String,  // This is now ONLY for the underlying asset
  underlying_currency: String,
  quantity: f64,
  legs: Vec<OptionLegDefinition>,
  strategy_name: Option<String>,

  // Order parameters for the combo order
  order_type: OrderType,
  limit_price: Option<f64>,
  aux_price: Option<f64>,
  tif: TimeInForce,
  account: Option<String>,
  order_ref: Option<String>,
  transmit: bool,
  underlying_price: Option<f64>,

  // Enhanced exchange handling
  exchange_selection_strategy: ExchangeSelectionStrategy,
  selected_option_exchange: Option<String>,
  option_chain_params: Option<OptionChainParams>,
  require_same_exchange_for_all_legs: bool,
  liquidity_config: LiquidityEstimationConfig,

  // Caches
  option_contract_cache: HashMap<(String, u64, OptionRight, String), Contract>, // Added exchange to key
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
      underlying_price: Some(underlying_price),
      underlying_con_id: None,
      underlying_sec_type,
      underlying_exchange: String::new(),
      underlying_currency: String::new(),
      quantity,
      legs: Vec::new(),
      strategy_name: None,

      // Default order params
      order_type: OrderType::Market,
      limit_price: None,
      aux_price: None,
      tif: TimeInForce::Day,
      account: None,
      order_ref: None,
      transmit: true,

      // Enhanced exchange handling
      exchange_selection_strategy: ExchangeSelectionStrategy::HighestLiquidity,
      selected_option_exchange: None,
      option_chain_params: None,
      require_same_exchange_for_all_legs: true, // Default to requiring consistency
      liquidity_config: LiquidityEstimationConfig::default(),

      // Caches
      option_contract_cache: HashMap::new(),
    })
  }

  // --- Exchange Configuration Methods ---

  /// Specify which exchange to use for options
  ///
  /// # Arguments
  /// * `exchange` - The specific exchange name (e.g., "CBOE", "NASDAQ", "EDGX")
  pub fn with_option_exchange(mut self, exchange: &str) -> Self {
    self.exchange_selection_strategy = ExchangeSelectionStrategy::Specific(exchange.to_string());
    self.selected_option_exchange = None; // Clear cached selection
    self
  }

  /// Use SMART routing for options
  pub fn with_smart_routing(mut self) -> Self {
    self.exchange_selection_strategy = ExchangeSelectionStrategy::Smart;
    self.selected_option_exchange = None;
    self
  }

  /// Use the exchange with most complete option chain
  pub fn with_most_complete_exchange(mut self) -> Self {
    self.exchange_selection_strategy = ExchangeSelectionStrategy::MostComplete;
    self.selected_option_exchange = None;
    self
  }

  /// Use the exchange with highest estimated liquidity
  pub fn with_highest_liquidity(mut self) -> Self {
    self.exchange_selection_strategy = ExchangeSelectionStrategy::HighestLiquidity;
    self.selected_option_exchange = None;
    self
  }

  /// Use the primary exchange suggested by the system
  pub fn with_primary_exchange(mut self) -> Self {
    self.exchange_selection_strategy = ExchangeSelectionStrategy::Primary;
    self.selected_option_exchange = None;
    self
  }

  /// Require all strategy legs to use the same exchange
  ///
  /// # Arguments
  /// * `required` - Whether to enforce exchange consistency across all legs
  pub fn require_consistent_exchange(mut self, required: bool) -> Self {
    self.require_same_exchange_for_all_legs = required;
    self
  }

  /// Configure liquidity estimation parameters
  ///
  /// # Arguments
  /// * `config` - Configuration for how liquidity is estimated and used
  pub fn with_liquidity_config(mut self, config: LiquidityEstimationConfig) -> Self {
    self.liquidity_config = config;
    self
  }

  // --- Internal Helper Methods ---

  /// Fetches and caches the underlying contract details
  fn fetch_underlying_details(&mut self) -> Result<(), IBKRError> {
    if self.underlying_con_id.is_some() {
      return Ok(());
    }

    info!(
      "Fetching underlying details for {} ({})",
      self.underlying_symbol, self.underlying_sec_type
    );

    let initial_exchange = match self.underlying_sec_type {
      SecType::Stock => "SMART".to_string(),
      SecType::Future => String::new(),
      _ => return Err(IBKRError::InvalidContract("Unsupported underlying type".to_string())),
    };

    let underlying_contract_spec = Contract {
      symbol: self.underlying_symbol.clone(),
      sec_type: self.underlying_sec_type.clone(),
      exchange: initial_exchange,
      currency: String::new(),
      ..Default::default()
    };

    let details_list = self.data_ref_manager.get_contract_details(&underlying_contract_spec)?;

    if details_list.is_empty() {
      return Err(IBKRError::InvalidContract(format!(
        "No contract details found for underlying: {}",
        self.underlying_symbol
      )));
    }

    let details = &details_list[0];
    self.underlying_con_id = Some(details.contract.con_id);
    self.underlying_exchange = details
      .contract
      .primary_exchange
      .clone()
      .unwrap_or_else(|| details.contract.exchange.clone());
    self.underlying_currency = details.contract.currency.clone();

    if self.underlying_exchange.is_empty() || self.underlying_currency.is_empty() {
      return Err(IBKRError::InvalidContract(format!(
        "Could not determine exchange or currency for underlying: {}",
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

  /// Fetches option chain parameters from all exchanges and calculates liquidity scores
  fn fetch_all_option_params(&mut self) -> Result<&OptionChainParams, IBKRError> {
    if self.option_chain_params.is_none() {
      let underlying_con_id = self.underlying_con_id.ok_or_else(|| {
        IBKRError::InvalidContract("Underlying contract ID not available".to_string())
      })?;

      info!("Fetching option chain parameters for ConID={}", underlying_con_id);

      // For stock options, use empty string for fut_fop_exchange
      // For future options, we might need to specify the exchange
      let fut_fop_exchange = match self.underlying_sec_type {
        SecType::Stock => "",
        SecType::Future => &self.underlying_exchange,
        _ => return Err(IBKRError::InvalidContract("Unsupported underlying type for options".to_string())),
      };

      let params_list = self.data_ref_manager.get_option_chain_params(
        &self.underlying_symbol,
        fut_fop_exchange,
        self.underlying_sec_type.clone(),
        underlying_con_id,
      )?;

      let mut exchanges: Vec<OptionExchangeInfo> = Vec::new();
      for p in params_list {
        let completeness_score = self.calculate_completeness_score(&p);
        let market_share = self.estimate_market_share(&p.exchange);
        let liquidity_score = self.estimate_liquidity_score(&p, completeness_score, market_share)?;

        exchanges.push(OptionExchangeInfo {
          exchange: p.exchange,
          trading_class: p.trading_class,
          multiplier: p.multiplier,
          expirations: p.expirations,
          strikes: p.strikes,
          liquidity_score,
          completeness_score,
          market_share_estimate: market_share,
        });
      }

      if exchanges.is_empty() {
        return Err(IBKRError::InvalidContract(format!(
          "No option exchanges found for underlying: {}",
          self.underlying_symbol
        )));
      }

      // Sort by liquidity score for primary selection
      exchanges.sort_by(|a, b| b.liquidity_score.partial_cmp(&a.liquidity_score).unwrap());
      let primary_exchange = exchanges.first().map(|e| e.exchange.clone());

      info!(
        "Found {} option exchanges, primary: {:?}",
        exchanges.len(),
        primary_exchange
      );

      self.option_chain_params = Some(OptionChainParams {
        underlying_con_id,
        exchanges,
        primary_exchange,
      });
    }
    Ok(self.option_chain_params.as_ref().unwrap())
  }

  /// Resolves which exchange to use for options based on the selection strategy
  fn resolve_option_exchange(&mut self) -> Result<String, IBKRError> {
    if let Some(exchange) = &self.selected_option_exchange {
      return Ok(exchange.clone());
    }

    // Clone what we need to avoid borrowing conflicts BEFORE the mutable borrow
    let strategy = self.exchange_selection_strategy.clone();
    let underlying_symbol = self.underlying_symbol.clone();

    // Now get the data (this borrows self mutably)
    let params = self.fetch_all_option_params()?;

    let selected = match &strategy {
      ExchangeSelectionStrategy::Smart => "SMART".to_string(),
      ExchangeSelectionStrategy::Specific(exchange) => {
        // Validate exchange is available
        if !params.exchanges.iter().any(|e| e.exchange == *exchange) {
          return Err(IBKRError::InvalidParameter(
            format!("Exchange {} not available for {} options", exchange, underlying_symbol)
          ));
        }
        exchange.clone()
      },
      ExchangeSelectionStrategy::Primary => {
        params.primary_exchange.clone()
          .ok_or_else(|| IBKRError::InvalidParameter("No primary exchange available".to_string()))?
      },
      ExchangeSelectionStrategy::MostComplete => {
        Self::find_most_complete_exchange_static(&params.exchanges)?
      },
      ExchangeSelectionStrategy::HighestLiquidity => {
        Self::find_highest_liquidity_exchange_static(&params.exchanges)?
      },
    };

    info!("Selected option exchange: {} (strategy: {:?})", selected, strategy);
    self.selected_option_exchange = Some(selected.clone());
    Ok(selected)
  }

  /// Calculates completeness score based on strikes and expirations available
  fn calculate_completeness_score(&self, params: &SecDefOptParamsResult) -> f64 {
    if params.strikes.is_empty() || params.expirations.is_empty() {
      return 0.0;
    }

    let strike_range = params.strikes.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap()
      - params.strikes.iter().min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
    let strike_density = if strike_range > 0.0 {
      params.strikes.len() as f64 / strike_range
    } else {
      1.0
    };
    let expiration_count = params.expirations.len() as f64;

    // Normalize to 0.0-1.0 scale
    let strike_score = (strike_density / 10.0).min(1.0); // Assume 10 strikes per point is "complete"
    let expiration_score = (expiration_count / 20.0).min(1.0); // Assume 20 expirations is "complete"

    (strike_score + expiration_score) / 2.0
  }

  /// Estimates market share for known exchanges
  fn estimate_market_share(&self, exchange: &str) -> f64 {
    // Based on historical U.S. options market share data
    match exchange {
      "CBOE" => 0.35,      // Typically largest options exchange
      "NASDAQ" => 0.25,    // NASDAQ options market
      "NYSE" => 0.15,      // NYSE options
      "EDGX" => 0.12,      // CBOE EDGX
      "BATS" => 0.08,      // CBOE BZX
      "MIAX" => 0.05,      // Miami International Securities Exchange
      _ => 0.01,           // Other/unknown exchanges: e.g. GEMINI
    }
  }

  /// Estimates liquidity score using available metrics
  fn estimate_liquidity_score(
    &self,
    _params: &SecDefOptParamsResult,
    completeness_score: f64,
    market_share: f64,
  ) -> Result<f64, IBKRError> {
    let mut weights_sum = 0.0;
    let mut weighted_score = 0.0;

    // Completeness (always available)
    weighted_score += completeness_score * 0.5;
    weights_sum += 0.5;

    // Market share (always available)
    weighted_score += market_share * 0.5;
    weights_sum += 0.5;

    // TODO: Add real-time spread sampling if enabled in config
    // This would require access to DataMarketManager and is an enhancement for Phase 3

    // Normalize by actual weights used
    Ok(weighted_score / weights_sum)
  }

  /// Finds exchange with most complete option chain
  fn find_most_complete_exchange_static(exchanges: &[OptionExchangeInfo]) -> Result<String, IBKRError> {
    exchanges
      .iter()
      .max_by(|a, b| a.completeness_score.partial_cmp(&b.completeness_score).unwrap())
      .map(|e| e.exchange.clone())
      .ok_or_else(|| IBKRError::InvalidParameter("No exchanges available".to_string()))
  }

  /// Finds exchange with highest liquidity score
  fn find_highest_liquidity_exchange_static(exchanges: &[OptionExchangeInfo]) -> Result<String, IBKRError> {
    exchanges
      .iter()
      .max_by(|a, b| a.liquidity_score.partial_cmp(&b.liquidity_score).unwrap())
      .map(|e| e.exchange.clone())
      .ok_or_else(|| IBKRError::InvalidParameter("No exchanges available".to_string()))
  }

  /// Finds strikes and expirations for the selected exchange
  fn get_strikes_and_expirations_for_exchange(&mut self, exchange: &str) -> Result<(Vec<f64>, Vec<String>), IBKRError> {
    let params = self.fetch_all_option_params()?;

    let exchange_info = params.exchanges
      .iter()
      .find(|e| e.exchange == exchange)
      .ok_or_else(|| IBKRError::InvalidParameter(format!("Exchange {} not found", exchange)))?;

    Ok((exchange_info.strikes.clone(), exchange_info.expirations.clone()))
  }

  /// Finds the nearest valid expiration date
  fn find_nearest_expiration<'a>(
    &self,
    target_expiry_date: NaiveDate,
    available_expirations: &'a [String],
  ) -> Result<&'a str, IBKRError> {
    if available_expirations.is_empty() {
      return Err(IBKRError::InvalidParameter("No expiration dates available".to_string()));
    }

    let target_expiry_str = target_expiry_date.format("%Y%m%d").to_string();

    // First try to find an exact match
    if let Some(exact_match) = available_expirations.iter().find(|&exp| exp == &target_expiry_str) {
      return Ok(exact_match);
    }

    // Find the nearest expiration on or after the target date
    let result = available_expirations
      .iter()
      .filter(|exp_str| exp_str.as_str() >= target_expiry_str.as_str())
      .min()
      .map(|s| s.as_str());

    match result {
      Some(expiry) => {
        info!(
          "Target expiry {} not found, using nearest: {}",
          target_expiry_str, expiry
        );
        Ok(expiry)
      },
      None => {
        // If no expiry on or after target, use the latest available
        let latest = available_expirations.iter().max().map(|s| s.as_str());
        match latest {
          Some(expiry) => {
            warn!(
              "No expiry on or after {} found, using latest available: {}",
              target_expiry_str, expiry
            );
            Ok(expiry)
          },
          None => Err(IBKRError::InvalidParameter(format!(
            "No valid expiration found. Target: {}, Available: {:?}",
            target_expiry_str, available_expirations
          )))
        }
      }
    }
  }

  /// Finds the nearest valid strike price
  fn find_nearest_strike(
    &self,
    target_strike: f64,
    available_strikes: &[f64],
  ) -> Result<f64, IBKRError> {
    if available_strikes.is_empty() {
      return Err(IBKRError::InvalidParameter("No strikes available".to_string()));
    }

    let result = available_strikes
      .iter()
      .min_by(|a, b| (*a - target_strike).abs().partial_cmp(&(*b - target_strike).abs()).unwrap())
      .cloned();

    match result {
      Some(strike) => {
        if (strike - target_strike).abs() > 0.01 {
          info!(
            "Target strike {} not found, using nearest: {}",
            target_strike, strike
          );
        }
        Ok(strike)
      },
      None => Err(IBKRError::InvalidParameter(format!(
        "No valid strike found. Target: {}, Available: {:?}",
        target_strike, available_strikes
      )))
    }
  }

  /// Fetches and caches the specific option contract details
  fn fetch_option_contract(
    &mut self,
    expiry_date: NaiveDate,
    strike: f64,
    right: OptionRight,
  ) -> Result<&Contract, IBKRError> {
    let expiry_str = expiry_date.format("%Y%m%d").to_string();
    let strike_bits = strike.to_bits();

    // First try with the selected exchange
    let primary_exchange = self.resolve_option_exchange()?;
    let primary_cache_key = (expiry_str.clone(), strike_bits, right, primary_exchange.clone());

    if !self.option_contract_cache.contains_key(&primary_cache_key) {
      // Validate that the strike and expiry are available on the selected exchange
      if let Err(validation_error) = self.validate_strike_and_expiry(&primary_exchange, strike, &expiry_str) {
        warn!(
          "Strike/expiry validation failed for exchange {}: {}. Will try fallback.",
          primary_exchange, validation_error
        );
      } else {
        // Try to fetch with the primary exchange
        match self.try_fetch_option_contract_for_exchange(
          &expiry_str, strike, right, &primary_exchange
        ) {
          Ok(contract) => {
            self.option_contract_cache.insert(primary_cache_key.clone(), contract);
            return Ok(self.option_contract_cache.get(&primary_cache_key).unwrap());
          },
          Err(e) => {
            warn!(
              "Failed to fetch option contract from {}: {}. Trying fallback to SMART.",
              primary_exchange, e
            );
          }
        }
      }

      // Fallback to SMART routing if primary exchange failed
      if primary_exchange != "SMART" {
        let smart_cache_key = (expiry_str.clone(), strike_bits, right, "SMART".to_string());

        if !self.option_contract_cache.contains_key(&smart_cache_key) {
          info!(
            "Attempting fallback to SMART routing for option: Exp={}, Strike={}, Right={}",
            expiry_str, strike, right
          );

          match self.try_fetch_option_contract_for_exchange(&expiry_str, strike, right, "SMART") {
            Ok(contract) => {
              self.option_contract_cache.insert(smart_cache_key.clone(), contract);
              // Update the selected exchange to SMART for consistency
              self.selected_option_exchange = Some("SMART".to_string());
              return Ok(self.option_contract_cache.get(&smart_cache_key).unwrap());
            },
            Err(e) => {
              error!(
                "Failed to fetch option contract even with SMART routing: {}",
                e
              );
              return Err(IBKRError::InvalidContract(format!(
                "No contract found for option Exp={}, Strike={}, Right={} on any exchange. \
                 Tried {} and SMART. Last error: {}",
                expiry_str, strike, right, primary_exchange, e
              )));
            }
          }
        }

        return Ok(self.option_contract_cache.get(&smart_cache_key).unwrap());
      } else {
        return Err(IBKRError::InvalidContract(format!(
          "No contract found for option Exp={}, Strike={}, Right={} on SMART routing",
          expiry_str, strike, right
        )));
      }
    }

    // Return cached contract
    let cache_key = if self.option_contract_cache.contains_key(&primary_cache_key) {
      &primary_cache_key
    } else {
      &(expiry_str, strike_bits, right, "SMART".to_string())
    };

    Ok(self.option_contract_cache.get(cache_key).unwrap())
  }

  /// Validates strategy consistency (exchange usage, etc.)
  fn validate_strike_and_expiry(
    &mut self,
    exchange: &str,
    strike: f64,
    expiry_str: &str,
  ) -> Result<(), IBKRError> {
    if exchange == "SMART" {
      // SMART routing should handle any valid strike/expiry, so skip validation
      return Ok(());
    }

    let (available_strikes, available_expiries) = self.get_strikes_and_expirations_for_exchange(exchange)?;

    // Check if the expiry is available
    if !available_expiries.contains(&expiry_str.to_string()) {
      return Err(IBKRError::InvalidParameter(format!(
        "Expiry {} not available on exchange {}. Available: {:?}",
        expiry_str, exchange,
        available_expiries.iter().take(5).collect::<Vec<_>>() // Show first 5
      )));
    }

    // Check if the strike is available (with some tolerance for floating point comparison)
    const STRIKE_TOLERANCE: f64 = 0.01;
    if !available_strikes.iter().any(|&s| (s - strike).abs() < STRIKE_TOLERANCE) {
      return Err(IBKRError::InvalidParameter(format!(
        "Strike {} not available on exchange {}. Nearest available: {:?}",
        strike, exchange,
        available_strikes
          .iter()
          .min_by(|&a, &b| (a - strike).abs().partial_cmp(&(b - strike).abs()).unwrap())
      )));
    }

    Ok(())
  }

  /// Validates strategy consistency (exchange usage, etc.)
  fn validate_strategy_consistency(&self) -> Result<(), IBKRError> {
    if self.require_same_exchange_for_all_legs {
      let exchanges: HashSet<String> = self.option_contract_cache
        .values()
        .map(|contract| contract.exchange.clone())
        .collect();

      if exchanges.len() > 1 {
        return Err(IBKRError::InvalidOrder(
          format!("Strategy requires consistent exchange but legs span: {:?}", exchanges)
        ));
      }
    }
    Ok(())
  }

  fn try_fetch_option_contract_for_exchange(
    &self,
    expiry_str: &str,
    strike: f64,
    right: OptionRight,
    exchange: &str,
  ) -> Result<Contract, IBKRError> {
    info!(
      "Fetching option contract: Exp={}, Strike={}, Right={}, Exchange={}",
      expiry_str, strike, right, exchange
    );

    let option_sec_type = match self.underlying_sec_type {
      SecType::Stock => SecType::Option,
      SecType::Future => SecType::FutureOption,
      _ => return Err(IBKRError::InvalidContract("Unsupported underlying type for options".to_string())),
    };

    let option_contract_spec = Contract {
      symbol: self.underlying_symbol.clone(),
      sec_type: option_sec_type,
      last_trade_date_or_contract_month: Some(expiry_str.to_string()),
      strike: Some(strike),
      right: Some(right),
      exchange: exchange.to_string(),
      currency: self.underlying_currency.clone(),
      ..Default::default()
    };

    let details_list = self.data_ref_manager.get_contract_details(&option_contract_spec)?;

    if details_list.is_empty() {
      return Err(IBKRError::InvalidContract(format!(
        "No contract details found for option: Exp={}, Strike={}, Right={}, Exchange={}",
        expiry_str, strike, right, exchange
      )));
    }

    if details_list.len() > 1 {
      warn!("Multiple contracts found for option spec. Using the first one (ConID={}).",
            details_list[0].contract.con_id);
    }

    let mut contract = details_list[0].contract.clone();

    // Ensure multiplier is set
    if contract.multiplier.is_none() || contract.multiplier.as_deref() == Some("") {
      let default_mult = match contract.sec_type {
        SecType::FutureOption => "50", // Common default for ES, but might need adjustment
        _ => "100", // Default for stock options
      };
      warn!("Multiplier not found for ConID {}. Using default: {}",
            contract.con_id, default_mult);
      contract.multiplier = Some(default_mult.to_string());
    }

    debug!("Found option contract: ConID={}, Exchange={}, Multiplier={:?}",
           contract.con_id, contract.exchange, contract.multiplier);

    Ok(contract)
  }

  /// Adds a leg definition to the strategy
  fn add_leg(
    &mut self,
    expiry: NaiveDate,
    strike: f64,
    right: OptionRight,
    action: OrderSide,
    ratio: i32,
  ) {
    self.legs.push(OptionLegDefinition {
      expiry,
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
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for both options
  /// * `target_strike1` - The target strike price for the lower (long) call
  /// * `target_strike2` - The target strike price for the higher (short) call
  pub fn bull_call_spread(self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64) -> Result<Self, IBKRError> {
    self.vertical_spread_internal(expiry, OptionRight::Call, target_strike1, target_strike2, OrderSide::Buy, "Bull Call Spread")
  }

  /// Defines a Bear Call Spread (Credit Call Spread). Sell lower strike call, Buy higher strike call.
  /// Strikes are approximate targets; the nearest available strikes will be used.
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for both options
  /// * `target_strike1` - The target strike price for the lower (short) call
  /// * `target_strike2` - The target strike price for the higher (long) call
  pub fn bear_call_spread(self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64) -> Result<Self, IBKRError> {
    self.vertical_spread_internal(expiry, OptionRight::Call, target_strike1, target_strike2, OrderSide::Sell, "Bear Call Spread")
  }

  /// Defines a Bull Put Spread (Credit Put Spread). Sell higher strike put, Buy lower strike put.
  /// Strikes are approximate targets; the nearest available strikes will be used.
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for both options
  /// * `target_strike1` - The target strike price for the lower (long) put
  /// * `target_strike2` - The target strike price for the higher (short) put
  pub fn bull_put_spread(self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64) -> Result<Self, IBKRError> {
    self.vertical_spread_internal(expiry, OptionRight::Put, target_strike1, target_strike2, OrderSide::Buy, "Bull Put Spread")
  }

  /// Defines a Bear Put Spread (Debit Put Spread). Buy higher strike put, Sell lower strike put.
  /// Strikes are approximate targets; the nearest available strikes will be used.
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for both options
  /// * `target_strike1` - The target strike price for the lower (short) put
  /// * `target_strike2` - The target strike price for the higher (long) put
  pub fn bear_put_spread(self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64) -> Result<Self, IBKRError> {
    self.vertical_spread_internal(expiry, OptionRight::Put, target_strike1, target_strike2, OrderSide::Sell, "Bear Put Spread")
  }

  /// Internal helper for vertical spreads. `target_strike1` < `target_strike2`. `action` applies to `strike1`.
  /// Finds nearest available strikes.
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for both options
  /// * `right` - Call or Put
  /// * `target_strike1` - The lower target strike price
  /// * `target_strike2` - The higher target strike price
  /// * `action_strike1` - Buy or Sell action for the first strike
  /// * `name` - Strategy name for logging
  fn vertical_spread_internal(
    mut self,
    expiry: NaiveDate,
    right: OptionRight,
    target_strike1: f64,
    target_strike2: f64,
    action_strike1: OrderSide,
    name: &str,
  ) -> Result<Self, IBKRError> {
    if target_strike1 >= target_strike2 {
      return Err(IBKRError::InvalidParameter(format!(
        "For vertical spread, target_strike1 ({}) must be less than target_strike2 ({})",
        target_strike1, target_strike2
      )));
    }

    // Fetch details and resolve exchange
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;

    // Find nearest actual strikes
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;

    if actual_strike1 >= actual_strike2 {
      return Err(IBKRError::InvalidParameter(format!(
        "Nearest strikes found ({}, {}) are not ordered correctly for vertical spread",
        actual_strike1, actual_strike2
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
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for both options
  /// * `target_strike` - The target strike price for both call and put
  pub fn long_straddle(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Long Straddle {}", actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Call, OrderSide::Buy, 1);
    self.add_leg(expiry, actual_strike, OptionRight::Put, OrderSide::Buy, 1);
    Ok(self)
  }

  /// Defines a Short Straddle (Sell Call and Sell Put at the same strike/expiry).
  /// Strike is an approximate target; the nearest available strike will be used.
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for both options
  /// * `target_strike` - The target strike price for both call and put
  pub fn short_straddle(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Short Straddle {}", actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Call, OrderSide::Sell, 1);
    self.add_leg(expiry, actual_strike, OptionRight::Put, OrderSide::Sell, 1);
    Ok(self)
  }

  /// Defines a Long Strangle (Buy OTM Call and Buy OTM Put, same expiry).
  /// Strikes are approximate targets; the nearest available strikes will be used.
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for both options
  /// * `target_call_strike` - The target strike price for the call (should be above underlying price)
  /// * `target_put_strike` - The target strike price for the put (should be below underlying price)
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
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_call_strike = self.find_nearest_strike(target_call_strike, &available_strikes)?;
    let actual_put_strike = self.find_nearest_strike(target_put_strike, &available_strikes)?;

    if actual_put_strike >= actual_call_strike {
      return Err(IBKRError::InvalidParameter(format!(
        "Nearest strikes found ({}, {}) are not ordered correctly for strangle",
        actual_put_strike, actual_call_strike
      )));
    }

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
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for both options
  /// * `target_call_strike` - The target strike price for the call (should be above underlying price)
  /// * `target_put_strike` - The target strike price for the put (should be below underlying price)
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
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_call_strike = self.find_nearest_strike(target_call_strike, &available_strikes)?;
    let actual_put_strike = self.find_nearest_strike(target_put_strike, &available_strikes)?;

    if actual_put_strike >= actual_call_strike {
      return Err(IBKRError::InvalidParameter(format!(
        "Nearest strikes found ({}, {}) are not ordered correctly for strangle",
        actual_put_strike, actual_call_strike
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
  ///
  /// # Arguments
  /// * `target_expiry` - The target expiration date
  /// * `target_strike1` - The lower target strike price
  /// * `target_strike2` - The higher target strike price
  pub fn box_spread_nearest_expiry(
    mut self,
    target_expiry: NaiveDate,
    target_strike1: f64,
    target_strike2: f64,
  ) -> Result<Self, IBKRError> {
    if target_strike1 >= target_strike2 {
      return Err(IBKRError::InvalidParameter(
        "For box spread, target_strike1 must be less than target_strike2".to_string(),
      ));
    }

    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, expirations) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;

    let expiry_str = self.find_nearest_expiration(target_expiry, &expirations)?;
    let expiry_date = NaiveDate::parse_from_str(expiry_str, "%Y%m%d")
      .map_err(|e| IBKRError::ParseError(format!("Failed to parse expiry string '{}': {}", expiry_str, e)))?;

    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;

    if actual_strike1 >= actual_strike2 {
      return Err(IBKRError::InvalidParameter(format!(
        "Nearest strikes found ({}, {}) are not ordered correctly for box spread",
        actual_strike1, actual_strike2
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
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for the option
  /// * `target_strike` - The target strike price
  pub fn buy_call(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Buy Call {} {}", expiry.format("%Y%m%d"), actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Call, OrderSide::Buy, 1);
    Ok(self)
  }

  /// Sell (short) a single Call option (Naked Call).
  /// Strike is an approximate target; the nearest available strike will be used.
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for the option
  /// * `target_strike` - The target strike price
  pub fn sell_call(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Sell Call {} {}", expiry.format("%Y%m%d"), actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Call, OrderSide::Sell, 1);
    Ok(self)
  }

  /// Buy a single Put option.
  /// Strike is an approximate target; the nearest available strike will be used.
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for the option
  /// * `target_strike` - The target strike price
  pub fn buy_put(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Buy Put {} {}", expiry.format("%Y%m%d"), actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Put, OrderSide::Buy, 1);
    Ok(self)
  }

  /// Sell (short) a single Put option (Naked Put).
  /// Strike is an approximate target; the nearest available strike will be used.
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for the option
  /// * `target_strike` - The target strike price
  pub fn sell_put(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
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
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for both options
  /// * `target_put_strike` - The target strike price for the protective put (should be below underlying price)
  /// * `target_call_strike` - The target strike price for the covered call (should be above underlying price)
  pub fn collar_options(mut self, expiry: NaiveDate, target_put_strike: f64, target_call_strike: f64) -> Result<Self, IBKRError> {
    if target_put_strike >= target_call_strike {
      return Err(IBKRError::InvalidParameter("For collar, target_put_strike must be less than target_call_strike".to_string()));
    }
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_put_strike = self.find_nearest_strike(target_put_strike, &available_strikes)?;
    let actual_call_strike = self.find_nearest_strike(target_call_strike, &available_strikes)?;

    if actual_put_strike >= actual_call_strike {
      return Err(IBKRError::InvalidParameter(format!(
        "Nearest strikes found ({}, {}) are not ordered correctly for collar",
        actual_put_strike, actual_call_strike
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
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for the option
  /// * `target_strike` - The target strike price for the call (should be above underlying price)
  pub fn covered_call_option(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Covered Call Option {}", actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Call, OrderSide::Sell, 1); // Short Call
    Ok(self)
  }

  /// Defines the option leg for a Covered Put (Short Put).
  /// Stock leg (Short Stock) must be handled separately.
  /// Strike is an approximate target; the nearest available strike will be used.
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for the option
  /// * `target_strike` - The target strike price for the put (should be below underlying price)
  pub fn covered_put_option(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Covered Put Option {}", actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Put, OrderSide::Sell, 1); // Short Put
    Ok(self)
  }

  /// Defines the option leg for a Protective Put (Long Put).
  /// Stock leg (Long Stock) must be handled separately.
  /// Strike is an approximate target; the nearest available strike will be used.
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for the option
  /// * `target_strike` - The target strike price for the put (should be below underlying price)
  pub fn protective_put_option(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Protective Put Option {}", actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Put, OrderSide::Buy, 1); // Long Put
    Ok(self)
  }

  /// Defines the option legs for a Stock Repair strategy (Buy 1 Call, Sell 2 Higher Strike Calls).
  /// Stock leg (Long Stock) must be handled separately.
  /// Strikes are approximate targets; the nearest available strikes will be used.
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for both option strikes
  /// * `target_strike1` - The target strike price for the long call (typically ATM or slightly OTM)
  /// * `target_strike2` - The target strike price for the short calls (higher than target_strike1)
  pub fn stock_repair_options(mut self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64) -> Result<Self, IBKRError> {
    if target_strike1 >= target_strike2 {
      return Err(IBKRError::InvalidParameter("For stock repair, target_strike1 must be less than target_strike2".to_string()));
    }
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;

    if actual_strike1 >= actual_strike2 {
      return Err(IBKRError::InvalidParameter(format!(
        "Nearest strikes found ({}, {}) are not ordered correctly for stock repair",
        actual_strike1, actual_strike2
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
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for both option strikes
  /// * `target_strike1` - The target strike price for the long calls (lower strike)
  /// * `target_strike2` - The target strike price for the short calls (higher strike)
  /// * `buy_ratio` - Number of contracts to buy at the lower strike
  /// * `sell_ratio` - Number of contracts to sell at the higher strike
  pub fn long_ratio_call_spread(mut self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64, buy_ratio: i32, sell_ratio: i32) -> Result<Self, IBKRError> {
    if target_strike1 >= target_strike2 { return Err(IBKRError::InvalidParameter("target_strike1 must be less than target_strike2".into())); }
    if buy_ratio <= 0 || sell_ratio <= 0 { return Err(IBKRError::InvalidParameter("Ratios must be positive".into())); }
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;

    if actual_strike1 >= actual_strike2 {
      return Err(IBKRError::InvalidParameter(format!(
        "Nearest strikes found ({}, {}) are not ordered correctly for ratio spread",
        actual_strike1, actual_strike2
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
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for both option strikes
  /// * `target_strike1` - The target strike price for the short puts (lower strike)
  /// * `target_strike2` - The target strike price for the long puts (higher strike)
  /// * `buy_ratio` - Number of contracts to buy at the higher strike
  /// * `sell_ratio` - Number of contracts to sell at the lower strike
  pub fn long_ratio_put_spread(mut self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64, buy_ratio: i32, sell_ratio: i32) -> Result<Self, IBKRError> {
    if target_strike1 >= target_strike2 { return Err(IBKRError::InvalidParameter("target_strike1 must be less than target_strike2".into())); }
    if buy_ratio <= 0 || sell_ratio <= 0 { return Err(IBKRError::InvalidParameter("Ratios must be positive".into())); }
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;

    if actual_strike1 >= actual_strike2 {
      return Err(IBKRError::InvalidParameter(format!(
        "Nearest strikes found ({}, {}) are not ordered correctly for ratio spread",
        actual_strike1, actual_strike2
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
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for both option strikes
  /// * `target_strike1` - The target strike price for the long puts (lower strike)
  /// * `target_strike2` - The target strike price for the short puts (higher strike)
  /// * `sell_ratio` - Number of contracts to sell at the higher strike
  /// * `buy_ratio` - Number of contracts to buy at the lower strike
  pub fn short_ratio_put_spread(mut self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64, sell_ratio: i32, buy_ratio: i32) -> Result<Self, IBKRError> {
    if target_strike1 >= target_strike2 { return Err(IBKRError::InvalidParameter("target_strike1 must be less than target_strike2".into())); }
    if buy_ratio <= 0 || sell_ratio <= 0 { return Err(IBKRError::InvalidParameter("Ratios must be positive".into())); }
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;

    if actual_strike1 >= actual_strike2 {
      return Err(IBKRError::InvalidParameter(format!(
        "Nearest strikes found ({}, {}) are not ordered correctly for ratio spread",
        actual_strike1, actual_strike2
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
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for all option strikes
  /// * `target_strike1` - The target strike price for the lower wing (long put)
  /// * `target_strike2` - The target strike price for the body (short puts)
  /// * `target_strike3` - The target strike price for the upper wing (long put)
  pub fn long_put_butterfly(mut self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64, target_strike3: f64) -> Result<Self, IBKRError> {
    if !(target_strike1 < target_strike2 && target_strike2 < target_strike3) { return Err(IBKRError::InvalidParameter("Target strikes must be ordered: strike1 < strike2 < strike3".into())); }
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;
    let actual_strike3 = self.find_nearest_strike(target_strike3, &available_strikes)?;

    if !(actual_strike1 < actual_strike2 && actual_strike2 < actual_strike3) {
      return Err(IBKRError::InvalidParameter(format!(
        "Nearest strikes found ({}, {}, {}) are not ordered correctly for butterfly",
        actual_strike1, actual_strike2, actual_strike3
      )));
    }

    self.strategy_name = Some(format!("Long Put Butterfly {}/{}/{}", actual_strike1, actual_strike2, actual_strike3));
    self.legs.clear();
    self.add_leg(expiry, actual_strike3, OptionRight::Put, OrderSide::Buy, 1);
    self.add_leg(expiry, actual_strike2, OptionRight::Put, OrderSide::Sell, 2);
    self.add_leg(expiry, actual_strike1, OptionRight::Put, OrderSide::Buy, 1);
    Ok(self)
  }

  /// Defines a Short Call Butterfly (Sell 1 Low Call, Buy 2 Mid Calls, Sell 1 High Call).
  /// Strikes are approximate targets; the nearest available strikes will be used.
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for all option strikes
  /// * `target_strike1` - The target strike price for the lower wing (short call)
  /// * `target_strike2` - The target strike price for the body (long calls)
  /// * `target_strike3` - The target strike price for the upper wing (short call)
  pub fn short_call_butterfly(mut self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64, target_strike3: f64) -> Result<Self, IBKRError> {
    if !(target_strike1 < target_strike2 && target_strike2 < target_strike3) { return Err(IBKRError::InvalidParameter("Target strikes must be ordered: strike1 < strike2 < strike3".into())); }
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;
    let actual_strike3 = self.find_nearest_strike(target_strike3, &available_strikes)?;

    if !(actual_strike1 < actual_strike2 && actual_strike2 < actual_strike3) {
      return Err(IBKRError::InvalidParameter(format!(
        "Nearest strikes found ({}, {}, {}) are not ordered correctly for butterfly",
        actual_strike1, actual_strike2, actual_strike3
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
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for all option strikes
  /// * `target_strike1` - The target strike price for the OTM put
  /// * `target_strike2` - The target strike price for the ATM strikes (both put and call)
  /// * `target_strike3` - The target strike price for the OTM call
  pub fn long_iron_butterfly(mut self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64, target_strike3: f64) -> Result<Self, IBKRError> {
    if !(target_strike1 < target_strike2 && target_strike2 < target_strike3) { return Err(IBKRError::InvalidParameter("Target strikes must be ordered: strike1 < strike2 < strike3".into())); }
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;
    let actual_strike3 = self.find_nearest_strike(target_strike3, &available_strikes)?;

    if !(actual_strike1 < actual_strike2 && actual_strike2 < actual_strike3) {
      return Err(IBKRError::InvalidParameter(format!(
        "Nearest strikes found ({}, {}, {}) are not ordered correctly for iron butterfly",
        actual_strike1, actual_strike2, actual_strike3
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
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for all option strikes
  /// * `target_strike1` - The target strike price for the lowest put (long)
  /// * `target_strike2` - The target strike price for the lower-middle put (short)
  /// * `target_strike3` - The target strike price for the upper-middle put (short)
  /// * `target_strike4` - The target strike price for the highest put (long)
  pub fn long_put_condor(mut self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64, target_strike3: f64, target_strike4: f64) -> Result<Self, IBKRError> {
    if !(target_strike1 < target_strike2 && target_strike2 < target_strike3 && target_strike3 < target_strike4) { return Err(IBKRError::InvalidParameter("Target strikes must be ordered: strike1 < strike2 < strike3 < strike4".into())); }
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;
    let actual_strike3 = self.find_nearest_strike(target_strike3, &available_strikes)?;
    let actual_strike4 = self.find_nearest_strike(target_strike4, &available_strikes)?;

    if !(actual_strike1 < actual_strike2 && actual_strike2 < actual_strike3 && actual_strike3 < actual_strike4) {
      return Err(IBKRError::InvalidParameter(format!(
        "Nearest strikes found ({}, {}, {}, {}) are not ordered correctly for condor",
        actual_strike1, actual_strike2, actual_strike3, actual_strike4
      )));
    }

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
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for all option strikes
  /// * `target_strike1` - The target strike price for the lowest put (long)
  /// * `target_strike2` - The target strike price for the lower-middle put (short)
  /// * `target_strike3` - The target strike price for the upper-middle call (short)
  /// * `target_strike4` - The target strike price for the highest call (long)
  pub fn short_condor(mut self, expiry: NaiveDate, target_strike1: f64, target_strike2: f64, target_strike3: f64, target_strike4: f64) -> Result<Self, IBKRError> {
    if !(target_strike1 < target_strike2 && target_strike2 < target_strike3 && target_strike3 < target_strike4) { return Err(IBKRError::InvalidParameter("Target strikes must be ordered: strike1 < strike2 < strike3 < strike4".into())); }
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike1 = self.find_nearest_strike(target_strike1, &available_strikes)?;
    let actual_strike2 = self.find_nearest_strike(target_strike2, &available_strikes)?;
    let actual_strike3 = self.find_nearest_strike(target_strike3, &available_strikes)?;
    let actual_strike4 = self.find_nearest_strike(target_strike4, &available_strikes)?;

    if !(actual_strike1 < actual_strike2 && actual_strike2 < actual_strike3 && actual_strike3 < actual_strike4) {
      return Err(IBKRError::InvalidParameter(format!(
        "Nearest strikes found ({}, {}, {}, {}) are not ordered correctly for condor",
        actual_strike1, actual_strike2, actual_strike3, actual_strike4
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
  ///
  /// # Arguments
  /// * `target_strike` - The target strike price for both puts
  /// * `near_expiry` - The expiration date for the short put (nearer term)
  /// * `far_expiry` - The expiration date for the long put (farther term)
  pub fn long_put_calendar_spread(mut self, target_strike: f64, near_expiry: NaiveDate, far_expiry: NaiveDate) -> Result<Self, IBKRError> {
    if near_expiry >= far_expiry { return Err(IBKRError::InvalidParameter("near_expiry must be before far_expiry".into())); }
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Long Put Calendar {} {}/{}", actual_strike, near_expiry.format("%Y%m%d"), far_expiry.format("%Y%m%d")));
    self.legs.clear();
    self.add_leg(near_expiry, actual_strike, OptionRight::Put, OrderSide::Sell, 1);
    self.add_leg(far_expiry, actual_strike, OptionRight::Put, OrderSide::Buy, 1);
    Ok(self)
  }

  /// Defines a Short Call Calendar Spread (Buy Near Call, Sell Far Call, same strike).
  /// Strike is an approximate target; the nearest available strike will be used.
  ///
  /// # Arguments
  /// * `target_strike` - The target strike price for both calls
  /// * `near_expiry` - The expiration date for the long call (nearer term)
  /// * `far_expiry` - The expiration date for the short call (farther term)
  pub fn short_call_calendar_spread(mut self, target_strike: f64, near_expiry: NaiveDate, far_expiry: NaiveDate) -> Result<Self, IBKRError> {
    if near_expiry >= far_expiry { return Err(IBKRError::InvalidParameter("near_expiry must be before far_expiry".into())); }
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
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
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for the call option
  /// * `target_strike` - The target strike price for the call
  pub fn synthetic_long_put_option(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Synthetic Long Put Option {}", actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Call, OrderSide::Buy, 1); // Long Call
    Ok(self)
  }

  /// Defines a Synthetic Long Stock (Long Call + Short Put).
  /// Strike is an approximate target; the nearest available strike will be used.
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for both options
  /// * `target_strike` - The target strike price for both call and put
  pub fn synthetic_long_stock(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Synthetic Long Stock {}", actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Call, OrderSide::Buy, 1);
    self.add_leg(expiry, actual_strike, OptionRight::Put, OrderSide::Sell, 1);
    Ok(self)
  }

  /// Defines a Synthetic Short Stock (Short Call + Long Put).
  /// Strike is an approximate target; the nearest available strike will be used.
  ///
  /// # Arguments
  /// * `expiry` - The expiration date for both options
  /// * `target_strike` - The target strike price for both call and put
  pub fn synthetic_short_stock(mut self, expiry: NaiveDate, target_strike: f64) -> Result<Self, IBKRError> {
    self.fetch_underlying_details()?;
    let option_exchange = self.resolve_option_exchange()?;
    let (available_strikes, _) = self.get_strikes_and_expirations_for_exchange(&option_exchange)?;
    let actual_strike = self.find_nearest_strike(target_strike, &available_strikes)?;

    self.strategy_name = Some(format!("Synthetic Short Stock {}", actual_strike));
    self.legs.clear();
    self.add_leg(expiry, actual_strike, OptionRight::Call, OrderSide::Sell, 1);
    self.add_leg(expiry, actual_strike, OptionRight::Put, OrderSide::Buy, 1);
    Ok(self)
  }

  // --- Order Parameter Methods ---

  /// Set the net limit price for the combo order (Debit or Credit).
  /// Positive for Debit, Negative for Credit.
  ///
  /// # Arguments
  /// * `price` - The net limit price for the entire strategy
  pub fn with_limit_price(mut self, price: f64) -> Self {
    self.limit_price = Some(price);
    self.order_type = OrderType::Limit;
    self
  }

  /// Set the order type for the combo (e.g., Market). Default is Limit.
  ///
  /// # Arguments
  /// * `order_type` - The order type to use for the combo order
  pub fn with_order_type(mut self, order_type: OrderType) -> Self {
    self.order_type = order_type;
    if order_type != OrderType::Limit {
      self.limit_price = None;
    }
    self
  }

  /// Set the Time-In-Force for the combo order. Default is Day.
  ///
  /// # Arguments
  /// * `tif` - The time-in-force setting (Day, GTC, IOC, etc.)
  pub fn with_tif(mut self, tif: TimeInForce) -> Self {
    self.tif = tif;
    self
  }

  /// Set the account for the combo order.
  ///
  /// # Arguments
  /// * `account` - The account identifier to use for the order
  pub fn with_account(mut self, account: &str) -> Self {
    self.account = Some(account.to_string());
    self
  }

  /// Set the order reference for the combo order.
  ///
  /// # Arguments
  /// * `order_ref` - A reference string for the order
  pub fn with_order_ref(mut self, order_ref: &str) -> Self {
    self.order_ref = Some(order_ref.to_string());
    self
  }

  /// Set whether the combo order should be transmitted immediately. Default is true.
  ///
  /// # Arguments
  /// * `transmit` - Whether to transmit the order immediately upon placement
  pub fn with_transmit(mut self, transmit: bool) -> Self {
    self.transmit = transmit;
    self
  }

  /// Set the underlying price for strike selection logic (e.g., OTM).
  ///
  /// # Arguments
  /// * `price` - The current price of the underlying asset
  pub fn with_underlying_price(mut self, price: f64) -> Self {
    self.underlying_price = Some(price);
    self
  }

  // --- Build Method ---

  /// Finalize the strategy, fetch contracts, and build the combo order.
  /// This method resolves the exchange to use, fetches all required option contracts,
  /// validates strategy consistency, and creates the final combo contract and order request.
  ///
  /// # Returns
  /// A tuple containing the combo contract (BAG type) and the order request
  ///
  /// # Errors
  /// - `IBKRError::InvalidOrder` if no strategy legs are defined
  /// - `IBKRError::InvalidContract` if contract details cannot be fetched
  /// - `IBKRError::InvalidParameter` if exchange validation fails
  /// - Other `IBKRError` variants for various failure modes
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

    // Ensure underlying details are fetched
    self.fetch_underlying_details()?;

    // Collect leg definitions to avoid borrow checker issues
    let leg_definitions: Vec<OptionLegDefinition> = self.legs.clone();

    // Fetch all required option contracts
    let mut final_legs: Vec<StrategyLeg> = Vec::with_capacity(leg_definitions.len());

    for leg_def in leg_definitions.iter() {
      let option_contract = self.fetch_option_contract(
        leg_def.expiry,
        leg_def.strike,
        leg_def.right
      )?;

      final_legs.push(StrategyLeg {
        contract: option_contract.clone(),
        action: leg_def.action,
        ratio: leg_def.ratio,
      });
    }

    // Validate strategy consistency
    self.validate_strategy_consistency()?;

    // Create the Combo Contract (BAG)
    let mut combo_contract = Contract {
      symbol: self.underlying_symbol.clone(),
      sec_type: SecType::Combo,
      currency: self.underlying_currency.clone(),
      exchange: self.selected_option_exchange.clone().unwrap_or_else(|| "SMART".to_string()),
      combo_legs: Vec::with_capacity(final_legs.len()),
      ..Default::default()
    };

    for leg in &final_legs {
      combo_contract.combo_legs.push(ComboLeg {
        con_id: leg.contract.con_id,
        ratio: leg.ratio,
        action: leg.action.to_string(),
        exchange: leg.contract.exchange.clone(),
        open_close: 0,
        short_sale_slot: 0,
        designated_location: "".to_string(),
        exempt_code: -1,
        price: None,
      });
    }

    // Create the Combo OrderRequest
    let order_request = OrderRequest {
      side: OrderSide::Buy, // Direction determined by net price sign
      quantity: self.quantity,
      order_type: self.order_type,
      limit_price: self.limit_price,
      aux_price: self.aux_price,
      time_in_force: self.tif,
      account: self.account.clone(),
      order_ref: self.order_ref.clone(),
      transmit: self.transmit,
      ..Default::default()
    };

    // Final validation
    if order_request.order_type == OrderType::Limit && order_request.limit_price.is_none() {
      warn!("Combo order type is LMT but no limit_price set. Order might be rejected.");
    }

    info!(
      "Strategy build complete. Exchange: {}, Legs: {}, Type: {}, Qty: {}, LmtPx: {:?}",
      combo_contract.exchange,
      combo_contract.combo_legs.len(),
      order_request.order_type,
      order_request.quantity,
      order_request.limit_price
    );

    Ok((combo_contract, order_request))
  }
}
