// yatws/src/parser_fin_adv.rs
use std::sync::Arc;
use crate::handler::FinancialAdvisorHandler;

use crate::base::IBKRError;
use crate::protocol_dec_parser::FieldParser;

use log::debug;

/// Process replace FA end message
pub fn process_replace_fa_end(handler: &Arc<dyn FinancialAdvisorHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Message: REPLACE_FA_END (reqId, text)
  // Version is not part of this message structure from TWS.
  let req_id_str = parser.read_string()?;
  let req_id = req_id_str.parse::<i32>().map_err(|e| IBKRError::ParseError(format!("Invalid reqId in REPLACE_FA_END: '{}', {}", req_id_str, e)))?;
  let text = parser.read_string()?;
  debug!("Parsed REPLACE_FA_END: ReqID={}, Text='{}'", req_id, text);
  handler.replace_fa_end(req_id, &text);
  Ok(())
}

/// Process receive FA message
pub fn process_receive_fa(handler: &Arc<dyn FinancialAdvisorHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Message: RECEIVE_FA (faDataType, xmlData)
  // Version is not part of this message structure from TWS.
  let fa_data_type_str = parser.read_string()?;
  let fa_data_type = fa_data_type_str.parse::<i32>().map_err(|e| IBKRError::ParseError(format!("Invalid faDataType in RECEIVE_FA: '{}', {}", fa_data_type_str, e)))?;
  let xml_data = parser.read_string()?;
  // XML data can be very long, so only log its length or a snippet in debug.
  debug!("Parsed RECEIVE_FA: DataType={}, XMLDataLen={}", fa_data_type, xml_data.len());
  log::trace!("RECEIVE_FA XML Data: {}", xml_data);
  handler.receive_fa(fa_data_type, &xml_data);
  Ok(())
}
