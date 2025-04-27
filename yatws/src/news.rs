use chrono::{DateTime, Utc};

// --- News Related Data Structures ---
/// Represents an available news provider.
#[derive(Debug, Clone)]
pub struct NewsProvider {
  pub code: String, // e.g., "BZ", "FLY"
  pub name: String, // e.g., "Benzinga", "Fly on the Wall"
}

/// Holds the data for a requested news article.
#[derive(Debug, Clone)]
pub struct NewsArticleData {
  pub req_id: i32,
  pub article_type: i32, // 0 for plain text, 1 for HTML
  pub article_text: String,
}

/// Represents a single historical news headline.
#[derive(Debug, Clone)]
pub struct HistoricalNews {
  // req_id is handled by the manager state
  pub time: String, // YYYY-MM-DD HH:MM:SS.fff format
  pub provider_code: String,
  pub article_id: String,
  pub headline: String,
}

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
