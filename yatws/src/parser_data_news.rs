// yatws/src/parser_news.rs
use std::sync::Arc;
use crate::handler::NewsDataHandler;

use crate::base::IBKRError;
use crate::protocol_dec_parser::FieldParser;


/// Process news article message
pub fn process_news_article(handler: &Arc<dyn NewsDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse news article message
  Ok(())
}

/// Process news providers message
pub fn process_news_providers(handler: &Arc<dyn NewsDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse news providers message
  Ok(())
}

/// Process historical news message
pub fn process_historical_news(handler: &Arc<dyn NewsDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse historical news message
  Ok(())
}

/// Process historical news end message
pub fn process_historical_news_end(handler: &Arc<dyn NewsDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse historical news end message
  Ok(())
}

/// Process news bulletins message
pub fn process_news_bulletins(handler: &Arc<dyn NewsDataHandler>, _parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Implementation would parse news bulletins
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
  // handler.tick_news(req_id, time_stamp, &provider_code, &article_id, &headline, &extra_data);
  Ok(())
}
