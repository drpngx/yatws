use crate::contract::Contract;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// News article
#[derive(Debug, Clone)]
pub struct NewsArticle {
  pub id: String,
  pub time: DateTime<Utc>,
  pub provider_code: String,
  pub article_id: String,
  pub headline: String,
  pub summary: Option<String>,
  pub content: Option<String>,
}

/// News subscription
#[derive(Debug, Clone)]
pub struct NewsSubscription {
  pub id: String,
  pub sources: Vec<String>,
}

/// News observer trait
pub trait NewsObserver: Send + Sync {
  fn on_news_article(&self, article: &NewsArticle);
}
