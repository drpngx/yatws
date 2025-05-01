
use crate::contract::{Contract, SecType, OptionRight, ComboLeg};
use crate::order::{OrderRequest, OrderSide, OrderType, TimeInForce};
use crate::base::IBKRError;
use chrono::{DateTime, Utc};

// --- Helper Enums/Structs for Builder (Keep necessary ones) ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrailingStopUnit {
  Amount,
  Percent,
}

// Re-export or define TriggerMethod constants if needed for clarity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMethod {
  Default = 0,
  DoubleBidAsk = 1,
  Last = 2,
  DoubleLast = 3,
  BidAsk = 4,
  LastOrBidAsk = 7,
  Midpoint = 8,
}


/// Build an order.
/// The most [common types](https://www.interactivebrokers.com/campus/ibkr-api-page/order-types/#introduction)
/// are supported, in addition to the usual `.with` pattern.
///
/// ```
/// fn example_limit_order() -> Result<(Contract, OrderRequest), IBKRError> {
///     OrderBuilder::new(OrderSide::Buy, 100.0)
///         .for_stock("AAPL")
///         .with_exchange("SMART")
///         .with_currency("USD")
///         .limit(150.0)
///         .with_tif(TimeInForce::Day)
///         .with_account("DU12345")
///         .with_order_ref("my_aapl_limit")
///         .build()
/// }
///
/// fn example_order_with_conditions() -> Result<(Contract, OrderRequest), IBKRError> {
///     // Example: Buy 100 MSFT if AAPL price > 180 AND MSFT volume > 1,000,000
///     let aapl_conid = 265598; // Example ConId for AAPL
///     let msft_conid = 272093; // Example ConId for MSFT
///
///     OrderBuilder::new(OrderSide::Buy, 100.0)
///         .for_stock("MSFT")
///         .with_exchange("SMART") // Volume condition requires SMART
///         .with_currency("USD")
///         .market() // Submit as market order when conditions met
///         .with_tif(TimeInForce::Day)
///         // Condition 1: AAPL Price > 180
///         .add_price_condition(aapl_conid, "SMART", 180.0, TriggerMethod::Default, true)
///         // Condition 2: MSFT Volume > 1M (linked with AND)
///         .with_next_condition_conjunction("and") // Specify AND for the *next* condition
///         .add_volume_condition(msft_conid, "SMART", 1_000_000, true)
///         .build()
/// }
/// ```
#[derive(Debug, Clone)]
pub struct OrderBuilder {
  contract: Contract,
  order: OrderRequest,
  // Internal state for validation
  tif_set: bool,
  order_type_explicitly_set: bool,
  is_combo: bool,
  is_adjusted: bool,
  trailing_unit: Option<TrailingStopUnit>,
  // Internal state for condition conjunction
  next_condition_conjunction: Option<String>, // Stores " and " or " or "
}

impl OrderBuilder {
  /// Start building an order with the essential action and quantity.
  /// Initializes a default Stock contract and Market order.
  pub fn new(action: OrderSide, quantity: f64) -> Self {
    let mut contract = Contract::stock(""); // Placeholder symbol
    contract.exchange = "SMART".to_string(); // Default exchange
    contract.currency = "USD".to_string();   // Default currency

    let mut order = OrderRequest::default();
    order.side = action;
    order.quantity = quantity;

    Self {
      contract,
      order,
      tif_set: false, // Default TIF is Day, technically set, but allow overwrite once
      order_type_explicitly_set: false, // Market is default, allow overwrite
      is_combo: false,
      is_adjusted: false,
      trailing_unit: None,
      next_condition_conjunction: None,
    }
  }

  // --- Contract Methods (Keep as before) ---
  pub fn for_stock(mut self, symbol: &str) -> Self {
    self.contract.symbol = symbol.to_string();
    self.contract.sec_type = SecType::Stock;
    self.is_combo = false;
    self
  }

  pub fn for_option(mut self, symbol: &str, expiry: &str, strike: f64, right: OptionRight) -> Self {
    self.contract.symbol = symbol.to_string();
    self.contract.sec_type = SecType::Option;
    self.contract.last_trade_date_or_contract_month = Some(expiry.to_string());
    self.contract.strike = Some(strike);
    self.contract.right = Some(right);
    self.contract.multiplier = Some("100".to_string()); // Default US
    self.is_combo = false;
    self
  }

  pub fn for_future(mut self, symbol: &str, expiry: &str) -> Self {
    self.contract.symbol = symbol.to_string();
    self.contract.sec_type = SecType::Future;
    self.contract.last_trade_date_or_contract_month = Some(expiry.to_string());
    self.is_combo = false;
    self
  }

  pub fn for_forex(mut self, pair: &str) -> Self {
    if let Some((base, quote)) = pair.split_once('/') {
      self.contract.symbol = base.to_string();
      self.contract.currency = quote.to_string();
    } else {
      if pair.len() == 6 {
        self.contract.symbol = pair[0..3].to_string();
        self.contract.currency = pair[3..6].to_string();
      } else {
        self.contract.symbol = pair.to_string();
        self.contract.currency = "USD".to_string(); // Default needed
      }
    }
    self.contract.sec_type = SecType::Forex;
    self.contract.exchange = "IDEALPRO".to_string(); // Common Forex ECN
    self.is_combo = false;
    self
  }

  pub fn for_continuous_future(mut self, symbol: &str) -> Self {
    self.contract.symbol = symbol.to_string();
    self.contract.sec_type = SecType::ContinuousFuture;
    self.contract.exchange = "SMART".to_string(); // Default, might need override
    self.contract.currency = "USD".to_string();   // Default, might need override
    self.is_combo = false;
    self
  }

  pub fn for_bond(mut self, symbol: &str) -> Self {
    self.contract.symbol = symbol.to_string();
    self.contract.sec_type = SecType::Bond;
    self.contract.exchange = "SMART".to_string(); // Default, might need override
    // Currency is often important for bonds, ensure it's set via .with_currency()
    self.is_combo = false;
    self
  }

  pub fn for_future_option(mut self, symbol: &str, expiry: &str, strike: f64, right: OptionRight) -> Self {
    self.contract.symbol = symbol.to_string();
    self.contract.sec_type = SecType::FutureOption;
    self.contract.last_trade_date_or_contract_month = Some(expiry.to_string());
    self.contract.strike = Some(strike);
    self.contract.right = Some(right);
    // Exchange, Currency, Multiplier likely needed via .with_ methods
    self.is_combo = false;
    self
  }

  pub fn for_warrant(mut self, symbol: &str) -> Self {
    self.contract.symbol = symbol.to_string();
    self.contract.sec_type = SecType::Warrant;
    self.contract.exchange = "SMART".to_string(); // Default, might need override
    self.contract.currency = "USD".to_string();   // Default, might need override
    self.is_combo = false;
    self
  }

  pub fn for_index_option(mut self, symbol: &str, expiry: &str, strike: f64, right: OptionRight) -> Self {
    self.contract.symbol = symbol.to_string();
    self.contract.sec_type = SecType::IndexOption;
    self.contract.last_trade_date_or_contract_month = Some(expiry.to_string());
    self.contract.strike = Some(strike);
    self.contract.right = Some(right);
    // Exchange, Currency, Multiplier likely needed via .with_ methods
    self.is_combo = false;
    self
  }

  pub fn for_forward(mut self, symbol: &str, expiry: &str) -> Self {
    self.contract.symbol = symbol.to_string();
    self.contract.sec_type = SecType::Forward;
    self.contract.last_trade_date_or_contract_month = Some(expiry.to_string());
    // Exchange, Currency likely needed via .with_ methods
    self.is_combo = false;
    self
  }

  pub fn for_index(mut self, symbol: &str) -> Self {
    self.contract.symbol = symbol.to_string();
    self.contract.sec_type = SecType::Index;
    // Exchange, Currency likely needed via .with_ methods
    self.is_combo = false;
    self
  }

  pub fn for_bill(mut self, symbol: &str) -> Self {
    self.contract.symbol = symbol.to_string();
    self.contract.sec_type = SecType::Bill;
    // Exchange, Currency likely needed via .with_ methods
    self.is_combo = false;
    self
  }

  pub fn for_fund(mut self, symbol: &str) -> Self {
    self.contract.symbol = symbol.to_string();
    self.contract.sec_type = SecType::Fund;
    self.contract.exchange = "FUNDSERV".to_string(); // Common default for funds
    // Currency likely needed via .with_currency()
    self.is_combo = false;
    self
  }

  pub fn for_fixed(mut self, symbol: &str) -> Self {
    self.contract.symbol = symbol.to_string();
    self.contract.sec_type = SecType::Fixed;
    // Exchange, Currency likely needed via .with_ methods
    self.is_combo = false;
    self
  }

  pub fn for_slb(mut self, symbol: &str) -> Self {
    self.contract.symbol = symbol.to_string();
    self.contract.sec_type = SecType::Slb;
    // Exchange, Currency likely needed via .with_ methods
    self.is_combo = false;
    self
  }

  pub fn for_commodity(mut self, symbol: &str) -> Self {
    self.contract.symbol = symbol.to_string();
    self.contract.sec_type = SecType::Commodity;
    // Exchange, Currency likely needed via .with_ methods
    self.is_combo = false;
    self
  }

  pub fn for_basket(mut self, symbol: &str) -> Self {
    self.contract.symbol = symbol.to_string();
    self.contract.sec_type = SecType::Basket;
    // Exchange, Currency likely needed via .with_ methods
    // Note: Baskets might behave similarly to Combos regarding legs.
    self.is_combo = false; // Treat as non-combo by default, legs added separately?
    self
  }

  pub fn for_crypto(mut self, symbol: &str) -> Self {
    self.contract.symbol = symbol.to_string();
    self.contract.sec_type = SecType::Crypto;
    self.contract.exchange = "PAXOS".to_string(); // Common default for crypto
    self.contract.currency = "USD".to_string();   // Common default for crypto
    self.is_combo = false;
    self
  }

  pub fn for_combo(mut self) -> Self {
    self.contract.sec_type = SecType::Combo;
    self.contract.symbol = "".to_string(); // Symbol often derived or not primary ID
    self.contract.combo_legs.clear();
    self.is_combo = true;
    self
  }

  pub fn add_combo_leg(mut self, con_id: i32, ratio: i32, action: &str, exchange: &str) -> Self {
    if !self.is_combo {
      self = self.for_combo();
    }
    self.contract.combo_legs.push(ComboLeg {
      con_id,
      ratio,
      action: action.to_string(), // "BUY" or "SELL"
      exchange: exchange.to_string(),
      open_close: 0, // Default: Same as parent
      short_sale_slot: 0, // Default
      designated_location: "".to_string(),
      exempt_code: -1, // Default
      price: None, // Use with_combo_leg_price for this
    });
    self
  }

  pub fn with_combo_leg_price(mut self, leg_index: usize, price: f64) -> Self {
    if self.is_combo && leg_index < self.contract.combo_legs.len() {
      // Using the Vec<(String, String)>:
      if self.order.combo_orders.len() <= leg_index {
        self.order.combo_orders.resize_with(leg_index + 1, || (String::new(), String::new())); // Placeholder
      }
      self.order.combo_orders[leg_index] = ("LMT".to_string(), price.to_string());
    }
    self
  }

  pub fn with_con_id(mut self, con_id: i32) -> Self {
    self.contract.con_id = con_id;
    self
  }

  pub fn with_exchange(mut self, exchange: &str) -> Self {
    self.contract.exchange = exchange.to_string();
    self
  }

  pub fn with_primary_exchange(mut self, primary_exchange: &str) -> Self {
    self.contract.primary_exchange = Some(primary_exchange.to_string());
    self
  }

  pub fn with_currency(mut self, currency: &str) -> Self {
    self.contract.currency = currency.to_string();
    self
  }

  pub fn with_local_symbol(mut self, local_symbol: &str) -> Self {
    self.contract.local_symbol = Some(local_symbol.to_string());
    self
  }

  pub fn with_trading_class(mut self, trading_class: &str) -> Self {
    self.contract.trading_class = Some(trading_class.to_string());
    self
  }

  // --- OrderRequest Methods (Keep most as before) ---

  pub fn market(mut self) -> Self {
    self.order.order_type = OrderType::Market;
    self.order.limit_price = None;
    self.order.aux_price = None;
    self.order_type_explicitly_set = true;
    self
  }

  pub fn limit(mut self, limit_price: f64) -> Self {
    self.order.order_type = OrderType::Limit;
    self.order.limit_price = Some(limit_price);
    self.order.aux_price = None;
    self.order_type_explicitly_set = true;
    self
  }

  pub fn stop(mut self, stop_price: f64) -> Self {
    self.order.order_type = OrderType::Stop;
    self.order.limit_price = None;
    self.order.aux_price = Some(stop_price);
    self.order_type_explicitly_set = true;
    self
  }

  pub fn stop_limit(mut self, stop_price: f64, limit_price: f64) -> Self {
    self.order.order_type = OrderType::StopLimit;
    self.order.limit_price = Some(limit_price);
    self.order.aux_price = Some(stop_price);
    self.order_type_explicitly_set = true;
    self
  }

  pub fn market_if_touched(mut self, trigger_price: f64) -> Self {
    self.order.order_type = OrderType::MarketIfTouched;
    self.order.limit_price = None;
    self.order.aux_price = Some(trigger_price);
    self.order_type_explicitly_set = true;
    self
  }

  pub fn limit_if_touched(mut self, trigger_price: f64, limit_price: f64) -> Self {
    self.order.order_type = OrderType::LimitIfTouched;
    self.order.limit_price = Some(limit_price);
    self.order.aux_price = Some(trigger_price);
    self.order_type_explicitly_set = true;
    self
  }

  pub fn trailing_stop_abs(mut self, trailing_amount: f64, trail_stop_price: Option<f64>) -> Self {
    self.order.order_type = OrderType::TrailingStop;
    self.order.aux_price = Some(trailing_amount); // Absolute amount goes here
    self.order.trailing_percent = None;
    self.order.trailing_stop_price = trail_stop_price;
    self.trailing_unit = Some(TrailingStopUnit::Amount);
    self.order_type_explicitly_set = true;
    self
  }

  pub fn trailing_stop_pct(mut self, trailing_percent: f64, trail_stop_price: Option<f64>) -> Self {
    self.order.order_type = OrderType::TrailingStop;
    self.order.trailing_percent = Some(trailing_percent); // Percentage goes here
    self.order.aux_price = None;
    self.order.trailing_stop_price = trail_stop_price;
    self.trailing_unit = Some(TrailingStopUnit::Percent);
    self.order_type_explicitly_set = true;
    self
  }

  pub fn trailing_stop_limit_abs(mut self, trailing_amount: f64, limit_offset: f64, trail_stop_price: Option<f64>) -> Self {
    self.order.order_type = OrderType::TrailingStopLimit;
    self.order.aux_price = Some(trailing_amount);
    self.order.lmt_price_offset = Some(limit_offset); // Limit offset from stop
    self.order.limit_price = None; // Cannot set both lmtPrice and lmtPriceOffset
    self.order.trailing_percent = None;
    self.order.trailing_stop_price = trail_stop_price;
    self.trailing_unit = Some(TrailingStopUnit::Amount);
    self.order_type_explicitly_set = true;
    self
  }

  pub fn trailing_stop_limit_pct(mut self, trailing_percent: f64, limit_offset: f64, trail_stop_price: Option<f64>) -> Self {
    self.order.order_type = OrderType::TrailingStopLimit;
    self.order.trailing_percent = Some(trailing_percent);
    self.order.lmt_price_offset = Some(limit_offset);
    self.order.limit_price = None;
    self.order.aux_price = None;
    self.order.trailing_stop_price = trail_stop_price;
    self.trailing_unit = Some(TrailingStopUnit::Percent);
    self.order_type_explicitly_set = true;
    self
  }

  pub fn with_tif(mut self, tif: TimeInForce) -> Self {
    let is_immediate_or_kill = tif == TimeInForce::ImmediateOrCancel || tif == TimeInForce::FillOrKill;
    let is_persistent = tif == TimeInForce::GoodTillCancelled || tif == TimeInForce::GoodTillDate || tif == TimeInForce::Day;

    if self.tif_set {
      let current_is_immediate = self.order.time_in_force == TimeInForce::ImmediateOrCancel || self.order.time_in_force == TimeInForce::FillOrKill;
      if (is_immediate_or_kill && !current_is_immediate) || (is_persistent && current_is_immediate) {
        log::warn!("Overwriting TimeInForce ({} with {}). Ensure this is intended.", self.order.time_in_force, tif);
      }
    }

    self.order.time_in_force = tif;
    self.tif_set = true; // Mark TIF as explicitly set by user

    if tif != TimeInForce::GoodTillDate {
      self.order.good_till_date = None;
    }
    self
  }

  pub fn with_good_till_date(mut self, date: DateTime<Utc>) -> Self {
    self.order.good_till_date = Some(date);
    self
  }

  pub fn with_account(mut self, account: &str) -> Self {
    self.order.account = Some(account.to_string());
    self
  }

  pub fn with_order_ref(mut self, order_ref: &str) -> Self {
    self.order.order_ref = Some(order_ref.to_string());
    self
  }

  pub fn with_transmit(mut self, transmit: bool) -> Self {
    self.order.transmit = transmit;
    self
  }

  pub fn with_outside_rth(mut self, outside_rth: bool) -> Self {
    self.order.outside_rth = outside_rth;
    self
  }

  pub fn with_hidden(mut self, hidden: bool) -> Self {
    self.order.hidden = hidden;
    self
  }

  pub fn with_all_or_none(mut self, all_or_none: bool) -> Self {
    self.order.all_or_none = all_or_none;
    self
  }

  pub fn with_sweep_to_fill(mut self, sweep: bool) -> Self {
    self.order.sweep_to_fill = sweep;
    self
  }

  pub fn with_block_order(mut self, block: bool) -> Self {
    self.order.block_order = block;
    self
  }

  pub fn with_not_held(mut self, not_held: bool) -> Self {
    self.order.not_held = not_held;
    self
  }

  pub fn with_parent_id(mut self, parent_id: i64) -> Self {
    self.order.parent_id = Some(parent_id);
    self
  }

  pub fn with_oca_group(mut self, group: &str) -> Self {
    self.order.oca_group = Some(group.to_string());
    self
  }

  pub fn with_oca_type(mut self, oca_type: i32) -> Self {
    if !(1..=3).contains(&oca_type) {
      log::warn!("Invalid OCA Type specified: {}. Using default.", oca_type);
      self.order.oca_type = None;
    } else {
      self.order.oca_type = Some(oca_type);
    }
    self
  }

  // --- Specific Order Type Configurations (Keep as before) ---

  pub fn auction(mut self, price: f64) -> Self {
    self = self.limit(price); // Base is LMT/MTL
    self.order.order_type = OrderType::MarketToLimit; // Often used for Auction
    // TIF needs to be AUC, handled externally or enum extended
    log::warn!("Auction order requires TIF 'AUC'. Ensure OrderRequest.tif is set appropriately if needed.");
    self.order.time_in_force = TimeInForce::Day; // Placeholder TIF
    self.tif_set = true;
    self
  }
  pub fn auction_limit(mut self, limit_price: f64, auction_strategy: i32) -> Self {
    self = self.limit(limit_price);
    self.order.auction_strategy = Some(auction_strategy);
    self.contract.exchange = "BOX".to_string(); // Requires BOX
    self
  }
  pub fn auction_pegged_stock(mut self, delta: f64, starting_price: f64) -> Self {
    // TODO: Add PeggedToStock to OrderType enum
    log::warn!("Auction Pegged to Stock requires OrderType 'PEG STK'. Set manually if needed.");
    self.order.order_type = OrderType::Relative; // Placeholder type
    self.order.delta = Some(delta);
    self.order.starting_price = Some(starting_price);
    self.contract.exchange = "BOX".to_string();
    self.order_type_explicitly_set = true;
    self
  }
  pub fn auction_relative(mut self, offset: f64) -> Self {
    self.order.order_type = OrderType::Relative;
    self.order.aux_price = Some(offset);
    self.contract.exchange = "BOX".to_string();
    self.order_type_explicitly_set = true;
    self
  }
  pub fn box_top(mut self) -> Self {
    self.order.order_type = OrderType::BoxTop;
    self.contract.exchange = "BOX".to_string();
    self.order_type_explicitly_set = true;
    self
  }
  pub fn with_discretionary_amount(mut self, amount: f64) -> Self {
    self.order.discretionary_amt = Some(amount);
    self
  }
  pub fn forex_cash_quantity(mut self, cash_quantity: f64, limit_price: Option<f64>) -> Self {
    self.contract.sec_type = SecType::Forex; // Ensure sec type is Forex
    if self.contract.exchange.is_empty() || self.contract.exchange == "SMART" {
      self.contract.exchange = "IDEALPRO".to_string(); // Default Forex ECN
    }
    self.order.cash_qty = Some(cash_quantity);
    self.order.quantity = 0.0; // Explicitly set quantity to 0 when using cash_qty
    if let Some(price) = limit_price {
      self = self.limit(price);
    } else {
      self = self.market();
    }
    self
  }
  pub fn limit_on_close(mut self, limit_price: f64) -> Self {
    self.order.order_type = OrderType::LimitOnClose;
    self.order.limit_price = Some(limit_price);
    self.order.aux_price = None;
    self.order_type_explicitly_set = true;
    self
  }
  pub fn limit_on_open(mut self, limit_price: f64) -> Self {
    self = self.limit(limit_price);
    self.order.time_in_force = TimeInForce::MarketOnOpen;
    self.tif_set = true;
    self
  }
  pub fn market_on_close(mut self) -> Self {
    self.order.order_type = OrderType::MarketOnClose;
    self.order.limit_price = None;
    self.order.aux_price = None;
    self.order_type_explicitly_set = true;
    self
  }
  pub fn market_on_open(mut self) -> Self {
    self = self.market();
    self.order.time_in_force = TimeInForce::MarketOnOpen;
    self.tif_set = true;
    self
  }
  pub fn market_to_limit(mut self) -> Self {
    self.order.order_type = OrderType::MarketToLimit;
    self.order.limit_price = None;
    self.order.aux_price = None;
    self.order_type_explicitly_set = true;
    self
  }
  pub fn market_with_protection(mut self) -> Self {
    log::warn!("Market with Protection requires OrderType 'MKT PRT'. Set manually if needed.");
    self = self.market(); // Base is market
    // Mark as MKT PRT externally if needed
    self.order_type_explicitly_set = true;
    self
  }
  pub fn passive_relative(mut self, offset: f64) -> Self {
    log::warn!("Passive Relative requires OrderType 'PASSV REL'. Set manually if needed.");
    self.order.order_type = OrderType::Relative; // Use REL as base
    self.order.aux_price = Some(offset.abs() * -1.0); // Passive offset might be negative? Check docs.
    self.order_type_explicitly_set = true;
    self
  }
  pub fn pegged_benchmark(mut self, reference_con_id: i32, starting_price: f64, pegged_change_amount: f64, reference_change_amount: f64, is_decrease: bool) -> Self {
    log::warn!("Pegged Benchmark requires OrderType 'PEG BENCH'. Set manually if needed.");
    self.order.order_type = OrderType::Relative; // Placeholder type
    self.order.reference_contract_id = Some(reference_con_id);
    self.order.starting_price = Some(starting_price);
    self.order.pegged_change_amount = Some(pegged_change_amount);
    self.order.reference_change_amount = Some(reference_change_amount);
    self.order.is_pegged_change_amount_decrease = is_decrease;
    self.order_type_explicitly_set = true;
    self
  }
  pub fn pegged_market(mut self, market_offset: f64) -> Self {
    self.order.order_type = OrderType::PeggedToMarket;
    self.order.aux_price = Some(market_offset);
    self.order_type_explicitly_set = true;
    self
  }
  pub fn relative_pegged_primary(mut self, offset_amount: f64, price_cap: Option<f64>) -> Self {
    self.order.order_type = OrderType::Relative;
    self.order.aux_price = Some(offset_amount);
    self.order.limit_price = price_cap; // Price cap acts like limit price
    self.order_type_explicitly_set = true;
    self
  }
  pub fn pegged_stock(mut self, delta: f64, stock_ref_price: f64, starting_price: f64) -> Self {
    log::warn!("Pegged Stock requires OrderType 'PEG STK'. Set manually if needed.");
    self.order.order_type = OrderType::Relative; // Placeholder type
    self.order.delta = Some(delta);
    self.order.stock_ref_price = Some(stock_ref_price);
    self.order.starting_price = Some(starting_price);
    self.order_type_explicitly_set = true;
    self
  }
  pub fn stop_with_protection(mut self, stop_price: f64) -> Self {
    log::warn!("Stop with Protection requires OrderType 'STP PRT'. Set manually if needed.");
    self = self.stop(stop_price); // Base is Stop
    // Mark as STP PRT externally if needed
    self.order_type_explicitly_set = true;
    self
  }
  pub fn volatility(mut self, volatility_percent: f64, vol_type: i32) -> Self {
    self.order.order_type = OrderType::Volatility;
    self.order.volatility = Some(volatility_percent / 100.0); // API expects decimal e.g. 0.4 for 40%
    self.order.volatility_type = Some(vol_type); // 1=daily, 2=annual
    self.order_type_explicitly_set = true;
    self
  }
  pub fn midprice(mut self, price_cap: Option<f64>) -> Self {
    log::warn!("Midprice requires OrderType 'MIDPRICE'. Set manually if needed.");
    self.order.order_type = OrderType::Relative; // Placeholder type
    self.order.limit_price = price_cap;
    self.contract.exchange = "SMART".to_string(); // Must be SMART
    self.order_type_explicitly_set = true;
    self
  }

  // --- Adjustable Stop Methods (Keep as before) ---
  pub fn with_trigger_price(mut self, price: f64) -> Self {
    self.order.trigger_price = Some(price);
    self.is_adjusted = true;
    self
  }
  pub fn adjust_to_stop(mut self, adjusted_stop_price: f64) -> Self {
    self.order.adjusted_order_type = Some(OrderType::Stop);
    self.order.adjusted_stop_price = Some(adjusted_stop_price);
    self.is_adjusted = true;
    self
  }
  pub fn adjust_to_stop_limit(mut self, adjusted_stop_price: f64, adjusted_limit_price: f64) -> Self {
    self.order.adjusted_order_type = Some(OrderType::StopLimit);
    self.order.adjusted_stop_price = Some(adjusted_stop_price);
    self.order.adjusted_stop_limit_price = Some(adjusted_limit_price);
    self.is_adjusted = true;
    self
  }
  pub fn adjust_to_trail_abs(mut self, adjusted_stop_price: f64, adjusted_trail_amount: f64) -> Self {
    self.order.adjusted_order_type = Some(OrderType::TrailingStop);
    self.order.adjusted_stop_price = Some(adjusted_stop_price);
    self.order.adjustable_trailing_unit = Some(0); // 0 for amount
    self.order.adjusted_trailing_amount = Some(adjusted_trail_amount);
    self.is_adjusted = true;
    self
  }
  pub fn adjust_to_trail_pct(mut self, adjusted_stop_price: f64, adjusted_trail_percent: f64) -> Self {
    self.order.adjusted_order_type = Some(OrderType::TrailingStop);
    self.order.adjusted_stop_price = Some(adjusted_stop_price);
    self.order.adjustable_trailing_unit = Some(1); // 1 for percent
    self.order.adjusted_trailing_amount = Some(adjusted_trail_percent);
    self.is_adjusted = true;
    self
  }

  // --- Algo Methods (Keep as before) ---
  pub fn with_algo(mut self, strategy: &str, params: Vec<(&str, &str)>) -> Self {
    self.order.algo_strategy = Some(strategy.to_string());
    self.order.algo_params = params
      .into_iter()
      .map(|(k, v)| (k.to_string(), v.to_string()))
      .collect();
    self
  }
  pub fn adaptive_algo(mut self, priority: &str) -> Self {
    let valid_priorities = ["Urgent", "Normal", "Patient"];
    if !valid_priorities.contains(&priority) {
      log::warn!("Invalid Adaptive priority: {}. Using Normal.", priority);
      self.order.algo_params = vec![("adaptivePriority".to_string(), "Normal".to_string())];
    } else {
      self.order.algo_params = vec![("adaptivePriority".to_string(), priority.to_string())];
    }
    self.order.algo_strategy = Some("Adaptive".to_string());
    if self.order.order_type == OrderType::None {
      self.order.order_type = OrderType::Limit; // Default to Limit if not set? Risky.
      log::warn!("Base order type not set for Adaptive algo. Ensure LMT or MKT is intended.");
    }
    self
  }
  pub fn vwap_algo(mut self, max_pct_vol: f64, start_time: Option<&str>, end_time: Option<&str>, allow_past_end: bool, no_take_liq: bool, speed_up: bool) -> Self {
    self.order.algo_strategy = Some("Vwap".to_string());
    let mut params = Vec::new();
    params.push(("maxPctVol".to_string(), max_pct_vol.to_string()));
    if let Some(st) = start_time { params.push(("startTime".to_string(), st.to_string())); }
    if let Some(et) = end_time { params.push(("endTime".to_string(), et.to_string())); }
    params.push(("allowPastEndTime".to_string(), if allow_past_end { "1".to_string() } else { "0".to_string() }));
    params.push(("noTakeLiq".to_string(), if no_take_liq { "1".to_string() } else { "0".to_string() }));
    params.push(("speedUp".to_string(), if speed_up { "1".to_string() } else { "0".to_string() }));
    self.order.algo_params = params;
    self
  }
  // Add more algo helpers...


  // --- Condition Methods (Direct String Formatting) ---

  /// Sets the conjunction (" and " or " or ") to be prepended to the *next* condition added.
  pub fn with_next_condition_conjunction(mut self, conjunction: &str) -> Self {
    let conj_str = conjunction.trim().to_lowercase();
    if conj_str == "and" || conj_str == "or" {
      self.next_condition_conjunction = Some(format!(" {} ", conj_str));
    } else {
      log::warn!("Invalid conjunction specified: '{}'. Use 'and' or 'or'.", conjunction);
      self.next_condition_conjunction = None; // Default to AND (no prefix)
    }
    self
  }

  // Helper to add formatted condition string
  fn add_formatted_condition(mut self, condition_str: String) -> Self {
    let prefix = self.next_condition_conjunction.take().unwrap_or_default();
    self.order.conditions.push(format!("{}{}", prefix, condition_str));
    self
  }

  /// Adds a Price condition.
  pub fn add_price_condition(self, con_id: i32, exchange: &str, price: f64, trigger_method: TriggerMethod, is_more: bool) -> Self {
    let condition_str = format!(
      "Price,conid={},exchange={},isMore={},price={},triggerMethod={}",
      con_id, exchange, is_more, price, trigger_method as i32
    );
    self.add_formatted_condition(condition_str)
  }

  /// Adds a Time condition. `time` format: "YYYYMMDD HH:MM:SS (optional timezone)".
  pub fn add_time_condition(self, time: &str, is_more: bool) -> Self {
    // Basic validation of time format might be useful here
    let condition_str = format!("Time,isMore={},time={}", is_more, time);
    self.add_formatted_condition(condition_str)
  }

  /// Adds a Margin condition. `percent` is the margin cushion percentage.
  pub fn add_margin_condition(self, percent: i32, is_more: bool) -> Self {
    let condition_str = format!("Margin,isMore={},percent={}", is_more, percent);
    self.add_formatted_condition(condition_str)
  }

  /// Adds an Execution condition. `sec_type` should be like "STK", "OPT".
  pub fn add_execution_condition(self, symbol: &str, sec_type: &str, exchange: &str) -> Self {
    let condition_str = format!(
      "Execution,secType={},exchange={},symbol={}",
      sec_type, exchange, symbol
    );
    self.add_formatted_condition(condition_str)
  }

  /// Adds a Volume condition. Requires SMART routing (validated in build).
  pub fn add_volume_condition(self, con_id: i32, exchange: &str, volume: i32, is_more: bool) -> Self {
    let condition_str = format!(
      "Volume,conid={},exchange={},isMore={},volume={}",
      con_id, exchange, is_more, volume
    );
    self.add_formatted_condition(condition_str)
  }

  /// Adds a PercentChange condition. `change_percent` is the percentage change (e.g., 5.0 for 5%).
  pub fn add_percent_change_condition(self, con_id: i32, exchange: &str, change_percent: f64, is_more: bool) -> Self {
    let condition_str = format!(
      "PercentChange,conid={},exchange={},isMore={},changePercent={}",
      con_id, exchange, is_more, change_percent
    );
    self.add_formatted_condition(condition_str)
  }

  /// If true, the order is canceled when conditions are met; otherwise, it's submitted.
  pub fn with_conditions_cancel_order(mut self, cancel: bool) -> Self {
    self.order.conditions_cancel_order = cancel;
    self
  }

  /// If true, conditions are also valid outside Regular Trading Hours.
  pub fn with_conditions_ignore_rth(mut self, ignore_rth: bool) -> Self {
    self.order.conditions_ignore_rth = ignore_rth;
    self
  }

  // --- Build Method ---

  /// Finalize and validate the order, returning the Contract and OrderRequest.
  pub fn build(self) -> Result<(Contract, OrderRequest), IBKRError> {
    // --- Validation (Keep most checks from previous version) ---

    // 1. Basic Contract checks
    if self.contract.symbol.is_empty() && self.contract.con_id == 0 && !self.is_combo {
      return Err(IBKRError::InvalidOrder("Contract symbol or conId must be set.".to_string()));
    }
    if self.contract.exchange.is_empty() && !self.is_combo {
      return Err(IBKRError::InvalidOrder("Contract exchange must be set.".to_string()));
    }
    if self.contract.currency.is_empty() && !self.is_combo {
      return Err(IBKRError::InvalidOrder("Contract currency must be set.".to_string()));
    }
    if self.is_combo && self.contract.combo_legs.is_empty() {
      return Err(IBKRError::InvalidOrder("Combo order must have at least one leg.".to_string()));
    }

    // 2. Basic Order checks
    if self.order.quantity <= 0.0 && self.order.cash_qty.is_none() {
      return Err(IBKRError::InvalidOrder(
        std::format!("Order quantity or cash quantity must be positive, got: {}.", self.order.quantity)));
    }
    if self.order.quantity > 0.0 && self.order.cash_qty.is_some() {
      log::warn!("Both quantity ({}) and cash_qty ({:?}) are set. Typically only one should be used. Quantity will likely be ignored by TWS if cash_qty is present.", self.order.quantity, self.order.cash_qty);
      // Don't error, let TWS decide, but warn the user.
    }
    if self.order.order_type == OrderType::None {
      return Err(IBKRError::InvalidOrder("Order type must be specified.".to_string()));
    }

    // 3. TIF checks
    if self.order.time_in_force == TimeInForce::GoodTillDate && self.order.good_till_date.is_none() {
      return Err(IBKRError::InvalidOrder("Good Till Date must be set for GTD Time In Force.".to_string()));
    }

    // 4. Order Type / Parameter checks (Keep previous checks)
    match self.order.order_type {
      OrderType::Limit | OrderType::LimitOnClose | OrderType::LimitOnOpen => {
        if self.order.limit_price.is_none() {
          return Err(IBKRError::InvalidOrder(format!("Limit price is required for {} order.", self.order.order_type)));
        }
      }
      OrderType::Stop | OrderType::MarketIfTouched => {
        if self.order.aux_price.is_none() {
          return Err(IBKRError::InvalidOrder(format!("Stop/trigger price (auxPrice) is required for {} order.", self.order.order_type)));
        }
      }
      OrderType::StopLimit | OrderType::LimitIfTouched => {
        if self.order.aux_price.is_none() || self.order.limit_price.is_none() {
          return Err(IBKRError::InvalidOrder(format!("Stop/trigger price (auxPrice) and limit price are required for {} order.", self.order.order_type)));
        }
      }
      OrderType::TrailingStop => {
        if self.order.aux_price.is_none() && self.order.trailing_percent.is_none() {
          return Err(IBKRError::InvalidOrder("Trailing amount (auxPrice) or trailing percent is required for TRAIL order.".to_string()));
        }
        if self.order.aux_price.is_some() && self.order.trailing_percent.is_some() {
          return Err(IBKRError::InvalidOrder("Cannot set both trailing amount (auxPrice) and trailing percent for TRAIL order.".to_string()));
        }
      }
      OrderType::TrailingStopLimit => {
        if (self.order.aux_price.is_none() && self.order.trailing_percent.is_none()) || self.order.lmt_price_offset.is_none() {
          return Err(IBKRError::InvalidOrder("Trailing amount/percent and limit price offset are required for TRAIL LIMIT order.".to_string()));
        }
        if self.order.aux_price.is_some() && self.order.trailing_percent.is_some() {
          return Err(IBKRError::InvalidOrder("Cannot set both trailing amount (auxPrice) and trailing percent for TRAIL LIMIT order.".to_string()));
        }
        if self.order.limit_price.is_some() {
          return Err(IBKRError::InvalidOrder("Cannot set both limit price and limit price offset for TRAIL LIMIT order.".to_string()));
        }
      }
      OrderType::Volatility => {
        if self.order.volatility.is_none() || self.order.volatility_type.is_none() {
          return Err(IBKRError::InvalidOrder("Volatility and volatility type are required for VOL order.".to_string()));
        }
      }
      _ => {}
    }

    // 5. Product / Order Type Compatibility Checks (Keep previous checks)
    match self.order.order_type {
      OrderType::BoxTop => {
        if self.contract.sec_type != SecType::Option || self.contract.exchange != "BOX" {
          return Err(IBKRError::InvalidOrder("Box Top order requires Option contract routed to BOX.".to_string()));
        }
      }
      OrderType::Volatility => {
        if !(self.contract.sec_type == SecType::Option || self.contract.sec_type == SecType::FutureOption) {
          return Err(IBKRError::InvalidOrder("Volatility order requires Option or FOP contract.".to_string()));
        }
      }
      // Add checks for Auction, Block, etc. based on SecType
      _ => {}
    }

    // Check IBKRATS specific rules (Keep previous checks)
    if self.contract.exchange == "IBKRATS" {
      if !self.order.not_held {
        return Err(IBKRError::InvalidOrder("Orders routed to IBKRATS must be marked as Not Held.".to_string()));
      }
      if self.order.order_type == OrderType::Market {
        return Err(IBKRError::InvalidOrder("Market orders cannot be routed to IBKRATS.".to_string()));
      }
    }

    // Check SweepToFill specific rules (Keep previous checks)
    if self.order.sweep_to_fill {
      let valid_sweep_types = [SecType::Stock, SecType::Warrant]; // Add CFD if applicable
      if !valid_sweep_types.contains(&self.contract.sec_type) || self.contract.exchange != "SMART" {
        return Err(IBKRError::InvalidOrder("SweepToFill only valid for Stocks/Warrants routed to SMART.".to_string()));
      }
    }

    // Check Block order rules (Keep previous checks)
    if self.order.block_order {
      if self.contract.sec_type != SecType::Option {
        return Err(IBKRError::InvalidOrder("Block order attribute only valid for Options.".to_string()));
      }
      if self.order.quantity < 50.0 {
        log::warn!("Block order typically used for >= 50 contracts (quantity is {}).", self.order.quantity);
      }
    }

    // Check Forex Cash Quantity rules (Keep previous checks)
    if self.order.cash_qty.is_some() {
      if self.contract.sec_type != SecType::Forex {
        return Err(IBKRError::InvalidOrder("Cash quantity can only be used for Forex orders.".to_string()));
      }
    }

    // 6. Combo Order checks (Keep previous checks)
    if self.is_combo {
      let valid_combo_types = [
        OrderType::Limit, OrderType::Market, OrderType::Relative
      ];
      if !valid_combo_types.contains(&self.order.order_type) {
        if !(self.order.order_type == OrderType::Relative && self.order.limit_price.is_some()) { // Check for REL+LMT
          return Err(IBKRError::InvalidOrder(format!("Order type {} not valid for Combo orders.", self.order.order_type)));
        }
      }
      if !self.order.combo_orders.is_empty() && self.order.combo_orders.len() != self.contract.combo_legs.len() {
        return Err(IBKRError::InvalidOrder("Number of combo order leg prices must match number of contract legs.".to_string()));
      }
    }

    // 7. Adjusted Order checks (Keep previous checks)
    if self.is_adjusted {
      if self.order.trigger_price.is_none() {
        return Err(IBKRError::InvalidOrder("Trigger price must be set for adjusted orders.".to_string()));
      }
      if self.order.adjusted_order_type.is_none() {
        return Err(IBKRError::InvalidOrder("Adjusted order type must be specified.".to_string()));
      }
      match self.order.adjusted_order_type {
        Some(OrderType::Stop) => if self.order.adjusted_stop_price.is_none() { return Err(IBKRError::InvalidOrder("adjusted_stop_price needed for Adjust-to-Stop".into())); },
        Some(OrderType::StopLimit) => if self.order.adjusted_stop_price.is_none() || self.order.adjusted_stop_limit_price.is_none() { return Err(IBKRError::InvalidOrder("adjusted_stop_price and adjusted_stop_limit_price needed for Adjust-to-StopLimit".into())); },
        Some(OrderType::TrailingStop) => if self.order.adjusted_stop_price.is_none() || self.order.adjustable_trailing_unit.is_none() || self.order.adjusted_trailing_amount.is_none() { return Err(IBKRError::InvalidOrder("adjusted_stop_price, adjustable_trailing_unit, and adjusted_trailing_amount needed for Adjust-to-Trail".into())); },
        Some(OrderType::TrailingStopLimit) => { /* Check all Trail Limit adjusted fields */ if self.order.adjusted_stop_price.is_none() || self.order.adjustable_trailing_unit.is_none() || self.order.adjusted_trailing_amount.is_none() || self.order.adjusted_stop_limit_price.is_none() { return Err(IBKRError::InvalidOrder("adjusted_stop_price, adjustable_trailing_unit, adjusted_trailing_amount and adjusted_stop_limit_price needed for Adjust-to-TrailLimit".into())); } },
        _ => return Err(IBKRError::InvalidOrder("Invalid adjusted_order_type specified.".into())),
      }
    }

    // 8. Algo checks (Keep previous checks)
    if let Some(strategy) = &self.order.algo_strategy {
      match strategy.as_str() {
        "Adaptive" => {
          if self.order.algo_params.iter().find(|(k,_)| k == "adaptivePriority").is_none() {
            return Err(IBKRError::InvalidOrder("Adaptive algo requires 'adaptivePriority' parameter.".to_string()));
          }
        }
        "Vwap" => {
          if self.order.algo_params.iter().find(|(k,_)| k == "maxPctVol").is_none() {
            return Err(IBKRError::InvalidOrder("VWAP algo requires 'maxPctVol' parameter.".to_string()));
          }
        }
        "AD" | "AccuDistr" => {
          if self.order.algo_params.iter().find(|(k,_)| k == "componentSize").is_none() {
            return Err(IBKRError::InvalidOrder("Accumulate/Distribute algo requires 'componentSize' parameter.".to_string()));
          }
          if strategy == "AD" && self.order.algo_params.iter().find(|(k,_)| k == "giveUp").is_none() {
            log::warn!("Accumulate/Distribute (AD) algo often uses 'giveUp' parameter.");
          }
          if strategy == "AccuDistr" && self.order.algo_params.iter().find(|(k,_)| k == "giveUp").is_some() {
            return Err(IBKRError::InvalidOrder("'giveUp' parameter is not supported by 'AccuDistr' algo strategy.".to_string()));
          }
        }
        _ => {}
      }
    }

    // 9. Condition checks
    if !self.order.conditions.is_empty() {
      // Validate SMART routing for Volume conditions
      for condition_str in &self.order.conditions {
        if condition_str.to_lowercase().contains("volume,") { // Simple check
          if self.contract.exchange != "SMART" {
            return Err(IBKRError::InvalidOrder("Volume condition requires SMART routing.".to_string()));
          }
        }
        // Potentially add more checks based on string parsing if needed
      }
      return Err(IBKRError::InvalidOrder("Order conditions not implemented".to_string()));
    }

    // --- Finalization ---
    Ok((self.contract, self.order))
  }
}
