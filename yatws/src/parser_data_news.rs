// yatws/src/parser_news.rs
use std::sync::Arc;
use crate::handler::NewsDataHandler;

use crate::base::IBKRError;
use crate::protocol_dec_parser::FieldParser;

use crate::data::{NewsProvider, NewsArticleData, HistoricalNews, NewsBulletin};
use log::debug;

/// Process news article message
pub fn process_news_article(handler: &Arc<dyn NewsDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?; // Version unused currently
  let req_id = parser.read_int()?;
  let article_type = parser.read_int()?;
  let article_text = parser.read_string()?;
  debug!("News Article: ReqID={}, Type={}", req_id, article_type);
  handler.news_article(req_id, article_type, &article_text);
  Ok(())
}

/// Process news providers message
pub fn process_news_providers(handler: &Arc<dyn NewsDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?; // Version unused currently
  let num_providers = parser.read_int()?;
  let mut providers = Vec::with_capacity(num_providers as usize);
  for _ in 0..num_providers {
    providers.push(NewsProvider {
      code: parser.read_string()?,
      name: parser.read_string()?,
    });
  }
  debug!("News Providers: Count={}", providers.len());
  handler.news_providers(&providers);
  Ok(())
}

/// Process historical news message
pub fn process_historical_news(handler: &Arc<dyn NewsDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int()?;
  let time = parser.read_string()?;
  let provider_code = parser.read_string()?;
  let article_id = parser.read_string()?;
  let headline = parser.read_string()?;
  debug!("Historical News: ReqID={}, Time={}, Provider={}, Article={}", req_id, time, provider_code, article_id);
  handler.historical_news(req_id, &time, &provider_code, &article_id, &headline);
  Ok(())
}

/// Process historical news end message
pub fn process_historical_news_end(handler: &Arc<dyn NewsDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int()?;
  let has_more = parser.read_bool()?;
  debug!("Historical News End: ReqID={}, HasMore={}", req_id, has_more);
  handler.historical_news_end(req_id, has_more);
  Ok(())
}

/// Process news bulletins message
pub fn process_news_bulletins(handler: &Arc<dyn NewsDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?; // Version unused currently
  let msg_id = parser.read_int()?;
  let msg_type = parser.read_int()?;
  let news_message = parser.read_string()?;
  let origin_exch = parser.read_string()?;
  debug!("News Bulletin: ID={}, Type={}, Origin={}", msg_id, msg_type, origin_exch);
  handler.update_news_bulletin(msg_id, msg_type, &news_message, &origin_exch);
  Ok(())
}

pub fn process_tick_news(handler: &Arc<dyn NewsDataHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let req_id = parser.read_int()?; // Ticker ID maps to Req ID
  let time_stamp = parser.read_i64()?;
  let provider_code = parser.read_string()?;
  let article_id = parser.read_string()?;
  let headline = parser.read_string()?;
  let extra_data = parser.read_string()?;

  log::debug!("Tick News: ReqID={}, Time={}, Provider={}, Article={}, Headline={}", req_id, time_stamp, provider_code, article_id, headline);
  handler.tick_news(req_id, time_stamp, &provider_code, &article_id, &headline, &extra_data);
  Ok(())
}
