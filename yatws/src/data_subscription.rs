#![allow(clippy::too_many_arguments)] // Common for TWS API handlers
#![allow(clippy::type_complexity)] // For complex builder/iterator types

use crate::base::IBKRError;
use crate::contract::{Bar, BarSize, Contract, WhatToShow};
use crate::data::{
  DurationUnit, GenericTickType, MarketDataType, TickAttrib, TickAttribBidAsk,
  TickAttribLast, TickByTickRequestType, TickType, // HistoricalTick removed
  TickOptionComputationData,
};
#[cfg(not(feature = "generate_bindings"))]
use crate::data_market_manager::DataMarketManager;
#[cfg(feature = "generate_bindings")]
use crate::data_market_manager::DataMarketManagerTrait as DataMarketManager; // For testing/mocking

use crate::data_observer::{
  HistoricalDataObserver, MarketDataObserver, MarketDepthObserver, ObserverId,
  RealTimeBarsObserver, TickByTickObserver,
};
// use crate::protocol_decoder::ClientErrorCode; // Removed

use chrono::{DateTime, Utc};
use log::{error, info, trace, warn}; // debug removed
use parking_lot::{Condvar, Mutex};
use std::collections::VecDeque;
use std::sync::{
  atomic::{AtomicBool, Ordering},
  Arc, Weak,
};
use std::time::Duration;

// --- Phase 1: Core Infrastructure ---

/// Trait for all market data subscription types.
pub trait MarketDataSubscription: Send + Sync + 'static {
  type Event: Clone + Send + Sync + 'static;
  type Iterator: MarketDataIterator<Self::Event> + Send + Sync + 'static;

  fn request_id(&self) -> i32;
  fn contract(&self) -> &Contract;
  fn is_completed(&self) -> bool;
  fn has_error(&self) -> bool;
  fn get_error(&self) -> Option<IBKRError>;
  fn cancel(&self) -> Result<(), IBKRError>;
  fn events(&self) -> Self::Iterator;
}

/// Trait for iterators that produce market data events.
pub trait MarketDataIterator<T>: Iterator<Item = T> + Send + Sync + 'static {
  // fn with_timeout(self, timeout: Duration) -> Self; // Removed for dyn compatibility
  fn try_next(&mut self, timeout: Duration) -> Option<T>;
  fn collect_available(&mut self) -> Vec<T>;
}

#[derive(Debug)]
pub(crate) struct SubscriptionState<E: Clone + Send + Sync + 'static> {
  pub req_id: i32,
  pub contract: Contract,
  pub(crate) completed: AtomicBool, // True if request naturally finished or cancelled/errored
  pub(crate) active: AtomicBool,    // False if cancelled or errored out, or one-shot completed
  pub(crate) error: Mutex<Option<IBKRError>>,
  pub(crate) events_queue: Mutex<VecDeque<E>>,
  pub(crate) condvar: Condvar,
  pub(crate) manager_weak: Weak<DataMarketManager>,
  pub(crate) observer_id: Mutex<Option<ObserverId>>,
}

impl<E: Clone + Send + Sync + 'static> SubscriptionState<E> {
  pub(crate) fn new(
    req_id: i32,
    contract: Contract,
    manager_weak: Weak<DataMarketManager>,
  ) -> Self {
    Self {
      req_id,
      contract,
      completed: AtomicBool::new(false),
      active: AtomicBool::new(true),
      error: Mutex::new(None),
      events_queue: Mutex::new(VecDeque::new()),
      condvar: Condvar::new(),
      manager_weak,
      observer_id: Mutex::new(None),
    }
  }

  pub(crate) fn push_event(&self, event: E) {
    if !self.active.load(Ordering::SeqCst) {
      trace!("Subscription {} inactive, not pushing event {:?}", self.req_id, std::any::type_name::<E>());
      return;
    }
    let mut queue = self.events_queue.lock();
    queue.push_back(event);
    self.condvar.notify_one();
  }

  pub(crate) fn pop_event_with_timeout(&self, timeout: Option<Duration>) -> Option<E> {
    let mut queue = self.events_queue.lock();
    loop {
      if let Some(event) = queue.pop_front() {
        return Some(event);
      }
      // If not active (e.g. cancelled/errored) AND queue is empty, no more events will come.
      // If completed (e.g. historical data end) AND queue is empty, no more events.
      if (!self.active.load(Ordering::SeqCst) || self.completed.load(Ordering::SeqCst)) && queue.is_empty() {
        return None;
      }

      if let Some(t) = timeout {
        if self.condvar.wait_for(&mut queue, t).timed_out() {
          return None; // Timeout occurred
        }
      } else {
        self.condvar.wait(&mut queue); // Wait indefinitely
      }
      // Spurious wakeup or notified, loop to check queue again / active/completed status
    }
  }

  pub(crate) fn pop_all_available_events(&self) -> Vec<E> {
    let mut queue = self.events_queue.lock();
    queue.drain(..).collect()
  }

  pub(crate) fn set_error(&self, err: IBKRError) {
    let mut error_guard = self.error.lock();
    if error_guard.is_none() { // Only set first error
      *error_guard = Some(err);
    }
    self.active.store(false, Ordering::SeqCst);
    self.completed.store(true, Ordering::SeqCst); // Error implies completion
    self.condvar.notify_all();
  }

  // Called when a one-shot request (like historical data) finishes naturally.
  pub(crate) fn mark_completed_and_inactive(&self) {
    self.completed.store(true, Ordering::SeqCst);
    self.active.store(false, Ordering::SeqCst);
    self.condvar.notify_all();
  }

  // Called for snapshot end, where the initial snapshot is done but the stream might continue (if not snapshot-only).
  // Or for streaming subscriptions that are cancelled.
  pub(crate) fn mark_stream_completed(&self) {
    self.completed.store(true, Ordering::SeqCst);
    // Active state depends on whether it was a snapshot-only or if it was cancelled.
    // If cancelled, active would be set to false by the cancel method.
    // If snapshot-only, active should be set to false by the observer.
    self.condvar.notify_all();
  }

  pub(crate) fn set_observer_id(&self, observer_id: ObserverId) {
    *self.observer_id.lock() = Some(observer_id);
  }
}

pub(crate) struct BaseIterator<E: Clone + Send + Sync + 'static> {
  pub(crate) state: Arc<SubscriptionState<E>>,
  pub(crate) default_timeout: Option<Duration>,
}

impl<E: Clone + Send + Sync + 'static> Iterator for BaseIterator<E> {
  type Item = E;
  fn next(&mut self) -> Option<Self::Item> {
    self.state.pop_event_with_timeout(self.default_timeout)
  }
}

impl<E: Clone + Send + Sync + 'static> MarketDataIterator<E> for BaseIterator<E> {
  // with_timeout removed from trait, concrete types can still have it if needed
  // or rely on default_timeout being set before boxing.
  fn try_next(&mut self, timeout: Duration) -> Option<E> {
    self.state.pop_event_with_timeout(Some(timeout))
  }
  fn collect_available(&mut self) -> Vec<E> {
    self.state.pop_all_available_events()
  }
}

// --- Phase 2: TickDataSubscription ---

#[derive(Debug, Clone, Default)]
pub struct TickDataParams {
  pub generic_tick_list: Vec<GenericTickType>,
  pub snapshot: bool,
  pub regulatory_snapshot: bool,
  pub mkt_data_options: Vec<(String, String)>,
  pub market_data_type: Option<MarketDataType>,
}

#[derive(Debug, Clone)]
pub enum TickDataEvent {
  Price(TickType, f64, TickAttrib),
  Size(TickType, f64),
  String(TickType, String),
  Generic(TickType, f64),
  OptionComputation(TickOptionComputationData),
  SnapshotEnd,
  MarketDataTypeSet(MarketDataType), // Confirmation of market data type
  Error(IBKRError), // Errors specific to this subscription
}

#[derive(Debug, Clone, Default)]
struct LatestTickValues {
  bid_price: Option<f64>,
  ask_price: Option<f64>,
  last_price: Option<f64>,
}

#[derive(Debug)]
pub struct TickDataSubscription {
  state: Arc<SubscriptionState<TickDataEvent>>,
  latest_values: Arc<Mutex<LatestTickValues>>,
}

pub struct TickDataSubscriptionBuilder {
  manager_weak: Weak<DataMarketManager>,
  contract: Contract,
  params: TickDataParams,
}

impl TickDataSubscriptionBuilder {
  pub(crate) fn new(manager_weak: Weak<DataMarketManager>, contract: Contract) -> Self {
    Self { manager_weak, contract, params: Default::default() }
  }
  pub fn with_generic_ticks(mut self, ticks: &[GenericTickType]) -> Self { self.params.generic_tick_list = ticks.to_vec(); self }
  pub fn with_snapshot(mut self, snapshot: bool) -> Self { self.params.snapshot = snapshot; self }
  pub fn with_regulatory_snapshot(mut self, regulatory: bool) -> Self { self.params.regulatory_snapshot = regulatory; self }
  pub fn with_mkt_data_options(mut self, options: &[(String, String)]) -> Self { self.params.mkt_data_options = options.to_vec(); self }
  pub fn with_market_data_type(mut self, mdt: MarketDataType) -> Self { self.params.market_data_type = Some(mdt); self }

  /// Submits the request to TWS and finalizes the subscription.
  /// This method is non-blocking. The first call to `events().next()` on the
  /// returned subscription will block until data arrives or an error occurs.
  pub fn submit(self) -> Result<TickDataSubscription, IBKRError> {
    let manager = self.manager_weak.upgrade().ok_or(IBKRError::NotConnected)?;
    let req_id = manager.next_request_id();

    let state = Arc::new(SubscriptionState::new(req_id, self.contract.clone(), Arc::downgrade(&manager)));
    let latest_values = Arc::new(Mutex::new(LatestTickValues::default()));

    let observer = TickDataInternalObserver { state: Arc::clone(&state), latest_values: Arc::clone(&latest_values) };
    let observer_id = manager.observe_market_data(observer); // Assuming DataMarketManager has this
    state.set_observer_id(observer_id);

    let mdt_to_set = self.params.market_data_type.unwrap_or(MarketDataType::RealTime);
    // Attempt to set MDT, log warning on failure but proceed.
    if manager.set_market_data_type_if_needed(mdt_to_set).is_err() {
      warn!("Failed to set market data type to {:?} for req_id {}, proceeding with request.", mdt_to_set, req_id);
    }

    manager.internal_request_market_data(
      req_id, &self.contract, &self.params.generic_tick_list, self.params.snapshot,
      self.params.regulatory_snapshot, &self.params.mkt_data_options, mdt_to_set,
    )?;

    Ok(TickDataSubscription { state, latest_values })
  }
}

impl TickDataSubscription {
  pub fn bid_price(&self) -> Option<f64> { self.latest_values.lock().bid_price }
  pub fn ask_price(&self) -> Option<f64> { self.latest_values.lock().ask_price }
  pub fn last_price(&self) -> Option<f64> { self.latest_values.lock().last_price }
}

impl MarketDataSubscription for TickDataSubscription {
  type Event = TickDataEvent; type Iterator = TickDataIterator;
  fn request_id(&self) -> i32 { self.state.req_id }
  fn contract(&self) -> &Contract { &self.state.contract }
  fn is_completed(&self) -> bool { self.state.completed.load(Ordering::SeqCst) }
  fn has_error(&self) -> bool { self.state.error.lock().is_some() }
  fn get_error(&self) -> Option<IBKRError> { self.state.error.lock().clone() }
  fn cancel(&self) -> Result<(), IBKRError> {
    if !self.state.active.swap(false, Ordering::SeqCst) { return Ok(()); } // Already inactive
    self.state.completed.store(true, Ordering::SeqCst); // Mark as completed due to cancel
    self.state.condvar.notify_all(); // Wake up iterators
    if let Some(manager) = self.state.manager_weak.upgrade() {
      if let Some(observer_id) = self.state.observer_id.lock().take() { manager.remove_market_data_observer(observer_id); }
      manager.cancel_market_data(self.state.req_id)?;
    } else { return Err(IBKRError::InternalError("Manager unavailable for cancel".to_string())); }
    Ok(())
  }
  fn events(&self) -> Self::Iterator { TickDataIterator { inner: BaseIterator { state: Arc::clone(&self.state), default_timeout: None } } }
}
impl Drop for TickDataSubscription { fn drop(&mut self) { if self.state.active.load(Ordering::SeqCst) { if let Err(e) = self.cancel() { error!("Drop: Error cancelling TickDataSubscription {}: {:?}", self.state.req_id, e); } } } }

pub struct TickDataIterator { inner: BaseIterator<TickDataEvent> }
impl TickDataIterator {
  pub fn with_timeout(mut self, timeout: Duration) -> Self {
    self.inner.default_timeout = Some(timeout);
    self
  }
}
impl Iterator for TickDataIterator { type Item = TickDataEvent; fn next(&mut self) -> Option<Self::Item> { self.inner.next() } }
impl MarketDataIterator<TickDataEvent> for TickDataIterator {
  // fn with_timeout removed from trait
  fn try_next(&mut self, timeout: Duration) -> Option<TickDataEvent> { self.inner.try_next(timeout) }
  fn collect_available(&mut self) -> Vec<TickDataEvent> { self.inner.collect_available() }
}

struct TickDataInternalObserver { state: Arc<SubscriptionState<TickDataEvent>>, latest_values: Arc<Mutex<LatestTickValues>> }
impl MarketDataObserver for TickDataInternalObserver {
  fn on_tick_price(&self, req_id: i32, tick_type: TickType, price: f64, attrib: TickAttrib) { if req_id != self.state.req_id { return; } { let mut latest = self.latest_values.lock(); match tick_type { TickType::BidPrice | TickType::DelayedBid => latest.bid_price = Some(price), TickType::AskPrice | TickType::DelayedAsk => latest.ask_price = Some(price), TickType::LastPrice | TickType::DelayedLast => latest.last_price = Some(price), _ => {} } } self.state.push_event(TickDataEvent::Price(tick_type, price, attrib)); }
  fn on_tick_size(&self, req_id: i32, tick_type: TickType, size: f64) { if req_id != self.state.req_id { return; } self.state.push_event(TickDataEvent::Size(tick_type, size)); }
  fn on_tick_string(&self, req_id: i32, tick_type: TickType, value: &str) { if req_id != self.state.req_id { return; } self.state.push_event(TickDataEvent::String(tick_type, value.to_string())); }
  fn on_tick_generic(&self, req_id: i32, tick_type: TickType, value: f64) { if req_id != self.state.req_id { return; } self.state.push_event(TickDataEvent::Generic(tick_type, value)); }
  // fn on_tick_option_computation(&self, req_id: i32, data: TickOptionComputationData) { if req_id != self.state.req_id { return; } self.state.push_event(TickDataEvent::OptionComputation(data)); }
  fn on_tick_snapshot_end(&self, req_id: i32) { if req_id != self.state.req_id { return; } self.state.push_event(TickDataEvent::SnapshotEnd); self.state.mark_stream_completed(); self.state.active.store(false, Ordering::SeqCst); } // Snapshot is one-shot
  fn on_market_data_type(&self, req_id: i32, market_data_type: MarketDataType) { if req_id != self.state.req_id { return; } self.state.push_event(TickDataEvent::MarketDataTypeSet(market_data_type)); }
  fn on_error(&self, req_id: i32, error_code: i32, error_message: &str) { if req_id != self.state.req_id { return; } let e = IBKRError::ApiError(error_code, error_message.to_string()); self.state.push_event(TickDataEvent::Error(e.clone())); self.state.set_error(e); }
}

// --- RealTimeBarSubscription ---
#[derive(Debug, Clone, Default)]
pub struct RealTimeBarParams { pub what_to_show: WhatToShow, pub use_rth: bool, pub rt_bar_options: Vec<(String, String)> }
pub type RealTimeBarEvent = Bar;
#[derive(Debug)]
pub struct RealTimeBarSubscription { state: Arc<SubscriptionState<RealTimeBarEvent>> }
pub struct RealTimeBarSubscriptionBuilder { manager_weak: Weak<DataMarketManager>, contract: Contract, params: RealTimeBarParams }
impl RealTimeBarSubscriptionBuilder {
  pub(crate) fn new(manager_weak: Weak<DataMarketManager>, contract: Contract, what_to_show: WhatToShow) -> Self { Self { manager_weak, contract, params: RealTimeBarParams { what_to_show, use_rth: true, ..Default::default() } } }
  pub fn with_use_rth(mut self, use_rth: bool) -> Self { self.params.use_rth = use_rth; self }
  pub fn with_options(mut self, options: &[(String, String)]) -> Self { self.params.rt_bar_options = options.to_vec(); self }
  pub fn submit(self) -> Result<RealTimeBarSubscription, IBKRError> {
    let manager = self.manager_weak.upgrade().ok_or(IBKRError::NotConnected)?;
    let req_id = manager.next_request_id();
    let state = Arc::new(SubscriptionState::new(req_id, self.contract.clone(), Arc::downgrade(&manager)));
    let observer = RealTimeBarInternalObserver { state: Arc::clone(&state) };
    let observer_id = manager.observe_realtime_bars(observer);
    state.set_observer_id(observer_id);
    manager.internal_request_real_time_bars(req_id, &self.contract, self.params.what_to_show, self.params.use_rth, &self.params.rt_bar_options, 5)?;
    Ok(RealTimeBarSubscription { state })
  }
}
impl MarketDataSubscription for RealTimeBarSubscription {
  type Event = RealTimeBarEvent; type Iterator = RealTimeBarIterator;
  fn request_id(&self) -> i32 { self.state.req_id }
  fn contract(&self) -> &Contract { &self.state.contract }
  fn is_completed(&self) -> bool { self.state.completed.load(Ordering::SeqCst) } // Streaming, completed on error/cancel
  fn has_error(&self) -> bool { self.state.error.lock().is_some() }
  fn get_error(&self) -> Option<IBKRError> { self.state.error.lock().clone() }
  fn cancel(&self) -> Result<(), IBKRError> {
    if !self.state.active.swap(false, Ordering::SeqCst) { return Ok(()); }
    self.state.completed.store(true, Ordering::SeqCst); self.state.condvar.notify_all();
    if let Some(manager) = self.state.manager_weak.upgrade() {
      if let Some(observer_id) = self.state.observer_id.lock().take() { manager.remove_realtime_bars_observer(observer_id); }
      manager.cancel_real_time_bars(self.state.req_id)?;
    } else { return Err(IBKRError::InternalError("Manager unavailable".to_string())); }
    Ok(())
  }
  fn events(&self) -> Self::Iterator { RealTimeBarIterator { inner: BaseIterator { state: Arc::clone(&self.state), default_timeout: None } } }
}
impl Drop for RealTimeBarSubscription { fn drop(&mut self) { if self.state.active.load(Ordering::SeqCst) { if let Err(e) = self.cancel() { error!("Drop: Error cancelling RealTimeBarSubscription {}: {:?}", self.state.req_id, e); } } } }
pub struct RealTimeBarIterator { inner: BaseIterator<RealTimeBarEvent> }
impl RealTimeBarIterator {
  pub fn with_timeout(mut self, timeout: Duration) -> Self {
    self.inner.default_timeout = Some(timeout);
    self
  }
}
impl Iterator for RealTimeBarIterator { type Item = RealTimeBarEvent; fn next(&mut self) -> Option<Self::Item> { self.inner.next() } }
impl MarketDataIterator<RealTimeBarEvent> for RealTimeBarIterator {
  // fn with_timeout removed from trait
  fn try_next(&mut self, timeout: Duration) -> Option<RealTimeBarEvent> { self.inner.try_next(timeout) }
  fn collect_available(&mut self) -> Vec<RealTimeBarEvent> { self.inner.collect_available() }
}
struct RealTimeBarInternalObserver { state: Arc<SubscriptionState<RealTimeBarEvent>> }
impl RealTimeBarsObserver for RealTimeBarInternalObserver {
  fn on_bar_update(&self, req_id: i32, bar: &Bar) { if req_id != self.state.req_id { return; } self.state.push_event(bar.clone()); }
  fn on_error(&self, req_id: i32, error_code: i32, error_message: &str) { if req_id != self.state.req_id { return; } self.state.set_error(IBKRError::ApiError(error_code, error_message.to_string())); }
}

// --- TickByTickSubscription ---
#[derive(Debug, Clone)]
pub struct TickByTickParams { pub tick_type: TickByTickRequestType, pub number_of_ticks: i32, pub ignore_size: bool }
#[derive(Debug, Clone)]
pub enum TickByTickEvent { Last { time: i64, price: f64, size: f64, tick_attrib_last: TickAttribLast, exchange: String, special_conditions: String }, AllLast { time: i64, price: f64, size: f64, tick_attrib_last: TickAttribLast, exchange: String, special_conditions: String }, BidAsk { time: i64, bid_price: f64, ask_price: f64, bid_size: f64, ask_size: f64, tick_attrib_bid_ask: TickAttribBidAsk }, MidPoint { time: i64, mid_point: f64 }, Error(IBKRError) }
#[derive(Debug)]
pub struct TickByTickSubscription { state: Arc<SubscriptionState<TickByTickEvent>> }
pub struct TickByTickSubscriptionBuilder { manager_weak: Weak<DataMarketManager>, contract: Contract, params: TickByTickParams }
impl TickByTickSubscriptionBuilder {
  pub(crate) fn new(manager_weak: Weak<DataMarketManager>, contract: Contract, tick_type: TickByTickRequestType) -> Self { Self { manager_weak, contract, params: TickByTickParams { tick_type, number_of_ticks: 0, ignore_size: false } } }
  pub fn with_number_of_ticks(mut self, num_ticks: i32) -> Self { self.params.number_of_ticks = num_ticks; self }
  pub fn with_ignore_size(mut self, ignore: bool) -> Self { self.params.ignore_size = ignore; self }
  pub fn submit(self) -> Result<TickByTickSubscription, IBKRError> {
    let manager = self.manager_weak.upgrade().ok_or(IBKRError::NotConnected)?;
    let req_id = manager.next_request_id();
    let state = Arc::new(SubscriptionState::new(req_id, self.contract.clone(), Arc::downgrade(&manager)));
    let observer = TickByTickInternalObserver { state: Arc::clone(&state) };
    let observer_id = manager.observe_tick_by_tick(observer);
    state.set_observer_id(observer_id);
    manager.internal_request_tick_by_tick_data(req_id, &self.contract, self.params.tick_type, self.params.number_of_ticks, self.params.ignore_size)?;
    Ok(TickByTickSubscription { state })
  }
}
impl MarketDataSubscription for TickByTickSubscription {
  type Event = TickByTickEvent; type Iterator = TickByTickIterator;
  fn request_id(&self) -> i32 { self.state.req_id }
  fn contract(&self) -> &Contract { &self.state.contract }
  fn is_completed(&self) -> bool { self.state.completed.load(Ordering::SeqCst) }
  fn has_error(&self) -> bool { self.state.error.lock().is_some() }
  fn get_error(&self) -> Option<IBKRError> { self.state.error.lock().clone() }
  fn cancel(&self) -> Result<(), IBKRError> {
    if !self.state.active.swap(false, Ordering::SeqCst) { return Ok(()); }
    self.state.completed.store(true, Ordering::SeqCst); self.state.condvar.notify_all();
    if let Some(manager) = self.state.manager_weak.upgrade() {
      if let Some(observer_id) = self.state.observer_id.lock().take() { manager.remove_tick_by_tick_observer(observer_id); }
      manager.cancel_tick_by_tick_data(self.state.req_id)?;
    } else { return Err(IBKRError::InternalError("Manager unavailable".to_string())); }
    Ok(())
  }
  fn events(&self) -> Self::Iterator { TickByTickIterator { inner: BaseIterator { state: Arc::clone(&self.state), default_timeout: None } } }
}
impl Drop for TickByTickSubscription { fn drop(&mut self) { if self.state.active.load(Ordering::SeqCst) { if let Err(e) = self.cancel() { error!("Drop: Error cancelling TickByTickSubscription {}: {:?}", self.state.req_id, e); } } } }
pub struct TickByTickIterator { inner: BaseIterator<TickByTickEvent> }
impl TickByTickIterator {
  pub fn with_timeout(mut self, timeout: Duration) -> Self {
    self.inner.default_timeout = Some(timeout);
    self
  }
}
impl Iterator for TickByTickIterator { type Item = TickByTickEvent; fn next(&mut self) -> Option<Self::Item> { self.inner.next() } }
impl MarketDataIterator<TickByTickEvent> for TickByTickIterator {
  // fn with_timeout removed from trait
  fn try_next(&mut self, timeout: Duration) -> Option<TickByTickEvent> { self.inner.try_next(timeout) }
  fn collect_available(&mut self) -> Vec<TickByTickEvent> { self.inner.collect_available() }
}
struct TickByTickInternalObserver { state: Arc<SubscriptionState<TickByTickEvent>> }
impl TickByTickObserver for TickByTickInternalObserver {
  fn on_tick_by_tick_all_last(&self, req_id: i32, tick_type_val: i32, time: i64, price: f64, size: f64, tick_attrib_last: &TickAttribLast, exchange: &str, special_conditions: &str) { if req_id != self.state.req_id { return; } let event = if tick_type_val == 1 { TickByTickEvent::Last { time, price, size, tick_attrib_last: tick_attrib_last.clone(), exchange: exchange.to_string(), special_conditions: special_conditions.to_string() } } else { TickByTickEvent::AllLast { time, price, size, tick_attrib_last: tick_attrib_last.clone(), exchange: exchange.to_string(), special_conditions: special_conditions.to_string() } }; self.state.push_event(event); }
  fn on_tick_by_tick_bid_ask(&self, req_id: i32, time: i64, bid_price: f64, ask_price: f64, bid_size: f64, ask_size: f64, tick_attrib_bid_ask: &TickAttribBidAsk) { if req_id != self.state.req_id { return; } self.state.push_event(TickByTickEvent::BidAsk { time, bid_price, ask_price, bid_size, ask_size, tick_attrib_bid_ask: tick_attrib_bid_ask.clone() }); }
  fn on_tick_by_tick_mid_point(&self, req_id: i32, time: i64, mid_point: f64) { if req_id != self.state.req_id { return; } self.state.push_event(TickByTickEvent::MidPoint { time, mid_point }); }
  fn on_error(&self, req_id: i32, error_code: i32, error_message: &str) { if req_id != self.state.req_id { return; } let e = IBKRError::ApiError(error_code, error_message.to_string()); self.state.push_event(TickByTickEvent::Error(e.clone())); self.state.set_error(e); }
}

// --- MarketDepthSubscription ---
#[derive(Debug, Clone)]
pub struct MarketDepthParams { pub num_rows: i32, pub is_smart_depth: bool, pub mkt_depth_options: Vec<(String, String)> }
#[derive(Debug, Clone)]
pub enum MarketDepthEvent { UpdateL1 { position: i32, operation: i32, side: i32, price: f64, size: f64 }, UpdateL2 { position: i32, market_maker: String, operation: i32, side: i32, price: f64, size: f64, is_smart_depth: bool }, Error(IBKRError) }
#[derive(Debug)]
pub struct MarketDepthSubscription { state: Arc<SubscriptionState<MarketDepthEvent>>, params: MarketDepthParams }
pub struct MarketDepthSubscriptionBuilder { manager_weak: Weak<DataMarketManager>, contract: Contract, params: MarketDepthParams }
impl MarketDepthSubscriptionBuilder {
  pub(crate) fn new(manager_weak: Weak<DataMarketManager>, contract: Contract, num_rows: i32) -> Self { Self { manager_weak, contract, params: MarketDepthParams { num_rows, is_smart_depth: false, mkt_depth_options: Vec::new() } } }
  pub fn with_smart_depth(mut self, smart: bool) -> Self { self.params.is_smart_depth = smart; self }
  pub fn with_options(mut self, options: &[(String, String)]) -> Self { self.params.mkt_depth_options = options.to_vec(); self }
  pub fn submit(self) -> Result<MarketDepthSubscription, IBKRError> {
    let manager = self.manager_weak.upgrade().ok_or(IBKRError::NotConnected)?;
    let req_id = manager.next_request_id();
    let state = Arc::new(SubscriptionState::new(req_id, self.contract.clone(), Arc::downgrade(&manager)));
    let observer = MarketDepthInternalObserver { state: Arc::clone(&state) };
    let observer_id = manager.observe_market_depth(observer);
    state.set_observer_id(observer_id);
    manager.internal_request_market_depth(req_id, &self.contract, self.params.num_rows, self.params.is_smart_depth, &self.params.mkt_depth_options)?;
    Ok(MarketDepthSubscription { state, params: self.params })
  }
}
impl MarketDataSubscription for MarketDepthSubscription {
  type Event = MarketDepthEvent; type Iterator = MarketDepthIterator;
  fn request_id(&self) -> i32 { self.state.req_id }
  fn contract(&self) -> &Contract { &self.state.contract }
  fn is_completed(&self) -> bool { self.state.completed.load(Ordering::SeqCst) }
  fn has_error(&self) -> bool { self.state.error.lock().is_some() }
  fn get_error(&self) -> Option<IBKRError> { self.state.error.lock().clone() }
  fn cancel(&self) -> Result<(), IBKRError> {
    if !self.state.active.swap(false, Ordering::SeqCst) { return Ok(()); }
    self.state.completed.store(true, Ordering::SeqCst); self.state.condvar.notify_all();
    if let Some(manager) = self.state.manager_weak.upgrade() {
      if let Some(observer_id) = self.state.observer_id.lock().take() { manager.remove_market_depth_observer(observer_id); }
      manager._cancel_market_depth_internal(self.state.req_id, self.params.is_smart_depth)?;
    } else { return Err(IBKRError::InternalError("Manager unavailable".to_string())); }
    Ok(())
  }
  fn events(&self) -> Self::Iterator { MarketDepthIterator { inner: BaseIterator { state: Arc::clone(&self.state), default_timeout: None } } }
}
impl Drop for MarketDepthSubscription { fn drop(&mut self) { if self.state.active.load(Ordering::SeqCst) { if let Err(e) = self.cancel() { error!("Drop: Error cancelling MarketDepthSubscription {}: {:?}", self.state.req_id, e); } } } }
pub struct MarketDepthIterator { inner: BaseIterator<MarketDepthEvent> }
impl MarketDepthIterator {
  pub fn with_timeout(mut self, timeout: Duration) -> Self {
    self.inner.default_timeout = Some(timeout);
    self
  }
}
impl Iterator for MarketDepthIterator { type Item = MarketDepthEvent; fn next(&mut self) -> Option<Self::Item> { self.inner.next() } }
impl MarketDataIterator<MarketDepthEvent> for MarketDepthIterator {
  // fn with_timeout removed from trait
  fn try_next(&mut self, timeout: Duration) -> Option<MarketDepthEvent> { self.inner.try_next(timeout) }
  fn collect_available(&mut self) -> Vec<MarketDepthEvent> { self.inner.collect_available() }
}
struct MarketDepthInternalObserver { state: Arc<SubscriptionState<MarketDepthEvent>> }
impl MarketDepthObserver for MarketDepthInternalObserver {
  fn on_update_mkt_depth(&self, req_id: i32, position: i32, operation: i32, side: i32, price: f64, size: f64) { if req_id != self.state.req_id { return; } self.state.push_event(MarketDepthEvent::UpdateL1 { position, operation, side, price, size }); }
  fn on_update_mkt_depth_l2(&self, req_id: i32, position: i32, market_maker: &str, operation: i32, side: i32, price: f64, size: f64, is_smart_depth: bool) { if req_id != self.state.req_id { return; } self.state.push_event(MarketDepthEvent::UpdateL2 { position, market_maker: market_maker.to_string(), operation, side, price, size, is_smart_depth }); }
  fn on_error(&self, req_id: i32, error_code: i32, error_message: &str) { if req_id != self.state.req_id { return; } let e = IBKRError::ApiError(error_code, error_message.to_string()); self.state.push_event(MarketDepthEvent::Error(e.clone())); self.state.set_error(e); }
}

// --- HistoricalDataSubscription ---
#[derive(Debug, Clone)]
pub struct HistoricalDataParams { pub end_date_time: Option<DateTime<Utc>>, pub duration: DurationUnit, pub bar_size_setting: BarSize, pub what_to_show: WhatToShow, pub use_rth: bool, pub format_date: i32, pub keep_up_to_date: bool, pub market_data_type: Option<MarketDataType>, pub chart_options: Vec<(String, String)> }
#[derive(Debug, Clone)]
pub enum HistoricalDataEvent { Bar(Bar), UpdateBar(Bar), Complete { start_date: String, end_date: String }, Error(IBKRError) }
#[derive(Debug)]
pub struct HistoricalDataSubscription { state: Arc<SubscriptionState<HistoricalDataEvent>> }
pub struct HistoricalDataSubscriptionBuilder { manager_weak: Weak<DataMarketManager>, contract: Contract, params: HistoricalDataParams }
impl HistoricalDataSubscriptionBuilder {
  pub(crate) fn new(manager_weak: Weak<DataMarketManager>, contract: Contract, duration: DurationUnit, bar_size: BarSize, what_to_show: WhatToShow) -> Self { Self { manager_weak, contract, params: HistoricalDataParams { duration, bar_size_setting: bar_size, what_to_show, use_rth: true, format_date: 1, keep_up_to_date: false, market_data_type: None, chart_options: Vec::new(), end_date_time: None } } }
  pub fn with_end_date_time(mut self, edt: DateTime<Utc>) -> Self { self.params.end_date_time = Some(edt); self }
  pub fn with_use_rth(mut self, use_rth: bool) -> Self { self.params.use_rth = use_rth; self }
  pub fn with_format_date(mut self, fmt: i32) -> Self { self.params.format_date = fmt; self }
  pub fn with_keep_up_to_date(mut self, keep_up: bool) -> Self { self.params.keep_up_to_date = keep_up; self }
  pub fn with_market_data_type(mut self, mdt: MarketDataType) -> Self { self.params.market_data_type = Some(mdt); self }
  pub fn with_chart_options(mut self, opts: &[(String, String)]) -> Self { self.params.chart_options = opts.to_vec(); self }
  pub fn submit(self) -> Result<HistoricalDataSubscription, IBKRError> {
    let manager = self.manager_weak.upgrade().ok_or(IBKRError::NotConnected)?;
    let req_id = manager.next_request_id();
    let state = Arc::new(SubscriptionState::new(req_id, self.contract.clone(), Arc::downgrade(&manager)));
    let observer = HistoricalDataInternalObserver { state: Arc::clone(&state) };
    let observer_id = manager.observe_historical_data(observer);
    state.set_observer_id(observer_id);
    let mdt_to_set = self.params.market_data_type.unwrap_or(MarketDataType::RealTime);
    if manager.set_market_data_type_if_needed(mdt_to_set).is_err() {
      warn!("Failed to set market data type to {:?} for hist_data req_id {}, proceeding.", mdt_to_set, req_id);
    }
    manager.internal_request_historical_data( req_id, &self.contract, self.params.end_date_time, &self.params.duration.to_string(), &self.params.bar_size_setting.to_string(), &self.params.what_to_show.to_string(), self.params.use_rth, self.params.format_date, self.params.keep_up_to_date, &self.params.chart_options, mdt_to_set)?;
    Ok(HistoricalDataSubscription { state })
  }
}
impl MarketDataSubscription for HistoricalDataSubscription {
  type Event = HistoricalDataEvent; type Iterator = HistoricalDataIterator;
  fn request_id(&self) -> i32 { self.state.req_id }
  fn contract(&self) -> &Contract { &self.state.contract }
  fn is_completed(&self) -> bool { self.state.completed.load(Ordering::SeqCst) }
  fn has_error(&self) -> bool { self.state.error.lock().is_some() }
  fn get_error(&self) -> Option<IBKRError> { self.state.error.lock().clone() }
  fn cancel(&self) -> Result<(), IBKRError> {
    if !self.state.active.swap(false, Ordering::SeqCst) { return Ok(()); }
    self.state.completed.store(true, Ordering::SeqCst); self.state.condvar.notify_all();
    if let Some(manager) = self.state.manager_weak.upgrade() {
      if let Some(observer_id) = self.state.observer_id.lock().take() { manager.remove_historical_data_observer(observer_id); }
      manager.cancel_historical_data(self.state.req_id)?;
    } else { return Err(IBKRError::InternalError("Manager unavailable".to_string())); }
    Ok(())
  }
  fn events(&self) -> Self::Iterator { HistoricalDataIterator { inner: BaseIterator { state: Arc::clone(&self.state), default_timeout: None } } }
}
impl Drop for HistoricalDataSubscription { fn drop(&mut self) { if self.state.active.load(Ordering::SeqCst) { if let Err(e) = self.cancel() { error!("Drop: Error cancelling HistoricalDataSubscription {}: {:?}", self.state.req_id, e); } } } }
pub struct HistoricalDataIterator { inner: BaseIterator<HistoricalDataEvent> }
impl HistoricalDataIterator {
  pub fn with_timeout(mut self, timeout: Duration) -> Self {
    self.inner.default_timeout = Some(timeout);
    self
  }
}
impl Iterator for HistoricalDataIterator { type Item = HistoricalDataEvent; fn next(&mut self) -> Option<Self::Item> { self.inner.next() } }
impl MarketDataIterator<HistoricalDataEvent> for HistoricalDataIterator {
  // fn with_timeout removed from trait
  fn try_next(&mut self, timeout: Duration) -> Option<HistoricalDataEvent> { self.inner.try_next(timeout) }
  fn collect_available(&mut self) -> Vec<HistoricalDataEvent> { self.inner.collect_available() }
}
struct HistoricalDataInternalObserver { state: Arc<SubscriptionState<HistoricalDataEvent>> }
impl HistoricalDataObserver for HistoricalDataInternalObserver {
  fn on_historical_data(&self, req_id: i32, bar: &Bar) { if req_id != self.state.req_id { return; } self.state.push_event(HistoricalDataEvent::Bar(bar.clone())); }
  fn on_historical_data_update(&self, req_id: i32, bar: &Bar) { if req_id != self.state.req_id { return; } self.state.push_event(HistoricalDataEvent::UpdateBar(bar.clone())); }
  fn on_historical_data_end(&self, req_id: i32, start_date: &str, end_date: &str) { if req_id != self.state.req_id { return; } self.state.push_event(HistoricalDataEvent::Complete { start_date: start_date.to_string(), end_date: end_date.to_string() }); self.state.mark_completed_and_inactive(); }
  fn on_error(&self, req_id: i32, error_code: i32, error_message: &str) { if req_id != self.state.req_id { return; } let e = IBKRError::ApiError(error_code, error_message.to_string()); self.state.push_event(HistoricalDataEvent::Error(e.clone())); self.state.set_error(e); }
}

// --- Phase 4: Multiple Subscription Support ---

#[derive(Debug, Clone)]
pub struct TaggedEvent<T: Clone + Send + Sync + 'static> {
  pub req_id: i32,
  pub contract: Contract,
  pub event: T,
}

pub struct MultiSubscriptionBuilder<T: Clone + Send + Sync + 'static> {
  iterators: Vec<(i32, Contract, Box<dyn MarketDataIterator<T>>)>,
}

impl<T: Clone + Send + Sync + 'static> Default for MultiSubscriptionBuilder<T> {
  fn default() -> Self { Self::new() }
}

impl<T: Clone + Send + Sync + 'static> MultiSubscriptionBuilder<T> {
  pub(crate) fn new() -> Self { Self { iterators: Vec::new() } }
  pub fn add<S>(mut self, subscription: &S, iterator: S::Iterator) -> Self
  where S: MarketDataSubscription<Event = T>,
  {
    self.iterators.push((subscription.request_id(), subscription.contract().clone(), Box::new(iterator)));
    self
  }
  pub fn build(self) -> MultiSubscriptionIterator<T> {
    MultiSubscriptionIterator { iterators: self.iterators, current_timeout_per_iterator: None, last_polled_idx: 0 }
  }
}

pub struct MultiSubscriptionIterator<T: Clone + Send + Sync + 'static> {
  iterators: Vec<(i32, Contract, Box<dyn MarketDataIterator<T>>)>,
  current_timeout_per_iterator: Option<Duration>,
  last_polled_idx: usize,
}

impl<T: Clone + Send + Sync + 'static> MultiSubscriptionIterator<T> {
  pub fn with_timeout(mut self, timeout_per_iterator: Duration) -> Self {
    self.current_timeout_per_iterator = Some(timeout_per_iterator);
    self
  }
  pub fn try_next(&mut self, timeout_per_iterator: Duration) -> Option<TaggedEvent<T>> {
    if self.iterators.is_empty() { return None; }
    let num_iterators = self.iterators.len();
    let effective_timeout = timeout_per_iterator / (num_iterators as u32).max(1);

    for i in 0..num_iterators {
      let current_idx = (self.last_polled_idx + i) % num_iterators;
      let (req_id, contract, iter) = &mut self.iterators[current_idx];
      if let Some(event) = iter.try_next(effective_timeout) {
        self.last_polled_idx = (current_idx + 1) % num_iterators;
        return Some(TaggedEvent { req_id: *req_id, contract: contract.clone(), event });
      }
    }
    self.last_polled_idx = (self.last_polled_idx + 1) % num_iterators; // Ensure progress
    None
  }
  pub fn cancel_all(self) -> Result<(), IBKRError> {
    // Drop handles individual cancellations.
    info!("MultiSubscriptionIterator cancel_all: Relies on Drop of individual subscriptions.");
    Ok(())
  }
}

impl<T: Clone + Send + Sync + 'static> Iterator for MultiSubscriptionIterator<T> {
  type Item = TaggedEvent<T>;
  fn next(&mut self) -> Option<Self::Item> {
    let timeout_per_iterator = self.current_timeout_per_iterator.unwrap_or_else(|| Duration::from_millis(100));
    loop {
      if self.iterators.is_empty() { return None; }
      if let Some(event) = self.try_next(timeout_per_iterator) {
        return Some(event);
      }
      // If a timeout was set for the MultiSubscriptionIterator itself, and one round of try_next yielded nothing,
      // then the "overall" try_next for the multi-iterator has timed out.
      if self.current_timeout_per_iterator.is_some() {
        return None;
      }
      // If no timeout on MultiSubscriptionIterator, continue polling.
      // This is a simple polling `next()`. A truly blocking `next()` on multiple sources is more complex.
      std::thread::yield_now(); // Be nice if polling indefinitely
    }
  }
}
