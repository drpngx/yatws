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

/// Represents a news article, typically received via streaming updates (tick_news).
#[derive(Debug, Clone)]
pub struct NewsArticle {
  /// A unique identifier, possibly combining provider and article ID.
  pub id: String,
  /// Timestamp of the article.
  pub time: DateTime<Utc>,
  /// News provider code (e.g., "BZ", "FLY").
  pub provider_code: String,
  /// The provider's unique ID for the article.
  pub article_id: String,
  /// The headline text.
  pub headline: String,
  // --- Fields below are typically NOT available from tick_news or historical_news headlines ---
  // pub summary: Option<String>, // Would require parsing article_text if available
  /// Full content, only available via get_news_article (in NewsArticleData).
  pub content: Option<String>, // Usually None for streamed headlines
  /// Type of article (0=text, 1=HTML), only from get_news_article.
  pub article_type: Option<i32>, // Usually None for streamed headlines
  // Add extra_data from tick_news if needed
  pub extra_data: Option<String>,
}

/// News observer trait
pub trait NewsObserver: Send + Sync {
  fn on_news_article(&self, article: &NewsArticle);
}
