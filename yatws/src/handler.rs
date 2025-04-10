// yatws/src/handler.rs
// Handlers for events parsed from the server.


/// Meta messages such as errors and time.
pub trait ClientHandler {
}

/// Order processing.
pub trait OrderHandler {
}

/// Information about the account such as open positions and cash balance.
pub trait AccountHandler {
}

/// Contract types etc.
pub trait ReferenceDataHandler {
}

/// Micro-structure: quotes etc.
pub trait MarketDataHandler {
}

/// Fundamentals.
pub trait FinancialDataHandler {
}

pub trait NewsDataHandler {
}

pub trait FinancialAdvisorHandler {
}

pub struct MessageHandler {
  pub client: Box<dyn ClientHandler>,
  pub order: Box<dyn OrderHandler>,
  pub account: Box<dyn AccountHandler>,
  pub fin_adv: Box<dyn FinancialAdvisorHandler>,
  pub data_ref: Box<dyn ReferenceDataHandler>,
  pub data_market: Box<dyn MarketDataHandler>,
  pub data_news: Box<dyn NewsDataHandler>,
  pub data_fin: Box<dyn FinancialDataHandler>,
}

unsafe impl Send for MessageHandler {}
