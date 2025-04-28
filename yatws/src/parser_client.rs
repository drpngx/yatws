// yatws/src/parser_client.rs

use std::sync::Arc;
use std::convert::TryFrom;
use crate::handler::{ClientHandler, MessageHandler}; // Import MessageHandler
use crate::protocol_decoder::ClientErrorCode; // Import the error enum
use crate::base::IBKRError;
use crate::protocol_dec_parser::FieldParser;
use log::{warn, error, info}; // Import logging levels

/// Process an error message (Type 4)
pub fn process_error_message(handler: &MessageHandler, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let version = parser.read_int()?;

  let mut id: i32 = -1; // Default for version < 2 or errors not associated with a request
  let error_code_int: i32;
  let error_msg: String;

  if version < 2 {
    // Very old format, just a single message string
    error_msg = parser.read_string()?;
    error_code_int = 0; // No code provided in this format, treat as unknown or info?
    // Let's try to map 0 to a specific code if appropriate, or handle it
  } else {
    id = parser.read_int()?;
    error_code_int = parser.read_int()?;
    error_msg = parser.read_string()?;
    // Version >= 3 adds an optional 'advanced order reject' JSON string
    // TODO: Parse advanced_order_reject if server_version supports it
    // if handler.get_server_version() >= crate::versions::ADVANCED_ORDER_REJECT {
    //     let _advanced_order_reject = parser.read_string()?;
    // }
  }

  // Convert integer code to ClientErrorCode enum
  let error_code = match ClientErrorCode::try_from(error_code_int) {
    Ok(code) => code,
    Err(_) => {
      // Handle unknown error codes - log and potentially use a generic error variant
      // TWS often uses codes >= 1000 for warnings/info related to specific situations
      if error_code_int >= 1000 && error_code_int < 2000 { // Example range for TWS info/warnings
        info!("Received TWS Info/Warning (ID: {}, Code: {}): {}", id, error_code_int, error_msg);
        // Optionally map to a generic Info/Warning enum variant if you add one
        // For now, we might skip calling the handler or call with a special code
        return Ok(()); // Or decide how to proceed
      } else {
        error!("Received unknown TWS Error Code (ID: {}, Code: {}): {}", id, error_code_int, error_msg);
        // TODO: Consider adding an Unknown variant to ClientErrorCode
        // For now, skip calling handler for truly unknown codes? Or call with NoValidId?
        // Calling with NoValidId might be misleading. Let's skip for now.
        return Ok(());
      }
    }
  };

  // Log based on severity (using the enum might simplify this later)
  // Codes >= 2000 are usually info/warnings in TWS docs, but our enum doesn't reflect that split yet.
  // We'll rely on the handler implementation to interpret the code.
  // Basic logging here:
  if error_code_int >= 2000 && error_code_int < 3000 {
    info!("TWS Info/Warning (ID: {}, Code: {:?}={}): {}", id, error_code, error_code_int, error_msg);
  } else {
    error!("TWS Error (ID: {}, Code: {:?}={}): {}", id, error_code, error_code_int, error_msg);
  }


  // --- Route to the appropriate handler based on error code ---
  // Note: This routing is based on the error code's likely origin.
  // A more robust system would track request IDs and their associated handler types.
  match error_code {
    // Order Handling Errors
    ClientErrorCode::FailSendOrder |
    ClientErrorCode::FailSendCOrder |
    ClientErrorCode::FailSendOOrder |
    ClientErrorCode::FailSendReqCompletedOrders => {
      handler.order.handle_error(id, error_code, &error_msg);
    }

    // Market Data Errors
    ClientErrorCode::FailSendReqMkt |
    ClientErrorCode::FailSendCanMkt |
    ClientErrorCode::FailSendReqMktDepth |
    ClientErrorCode::FailSendCanMktDepth |
    ClientErrorCode::FailSendReqScanner |
    ClientErrorCode::FailSendCanScanner |
    ClientErrorCode::FailSendReqScannerParameters |
    ClientErrorCode::FailSendReqHistData |
    ClientErrorCode::FailSendCanHistData | // Note: Same message as ReqHistData
    ClientErrorCode::FailSendReqRtBars |
    ClientErrorCode::FailSendCanRtBars |
    ClientErrorCode::FailSendReqCalcImpliedVolat |
    ClientErrorCode::FailSendReqCalcOptionPrice |
    ClientErrorCode::FailSendCanCalcImpliedVolat |
    ClientErrorCode::FailSendCanCalcOptionPrice |
    ClientErrorCode::FailSendReqMarketDataType |
    ClientErrorCode::FailSendReqHistogramData |
    ClientErrorCode::FailSendCancelHistogramData |
    ClientErrorCode::FailSendReqHistoricalTicks |
    ClientErrorCode::FailSendReqTickByTickData |
    ClientErrorCode::FailSendCancelTickByTickData |
    ClientErrorCode::FailSendReqHeadTimestamp |
    ClientErrorCode::FailSendCancelHeadTimestamp => {
      handler.data_market.handle_error(id, error_code, &error_msg);
    }

    // Account/Position Errors
    ClientErrorCode::FailSendAcct |
    ClientErrorCode::FailSendExec |
    ClientErrorCode::FailSendReqPositions |
    ClientErrorCode::FailSendCanPositions |
    ClientErrorCode::FailSendReqAccountData |
    ClientErrorCode::FailSendCanAccountData |
    ClientErrorCode::FailSendReqPositionsMulti |
    ClientErrorCode::FailSendCanPositionsMulti |
    ClientErrorCode::FailSendReqAccountUpdatesMulti |
    ClientErrorCode::FailSendCanAccountUpdatesMulti |
    ClientErrorCode::FailSendReqPnl |
    ClientErrorCode::FailSendCancelPnl |
    ClientErrorCode::FailSendReqPnlSingle |
    ClientErrorCode::FailSendCancelPnlSingle => {
      handler.account.handle_error(id, error_code, &error_msg);
    }

    // Reference Data Errors
    ClientErrorCode::UnknownContract |
    ClientErrorCode::FailSendReqContract |
    ClientErrorCode::FailSendReqSecDefOptParams |
    ClientErrorCode::FailSendReqSoftDollarTiers |
    ClientErrorCode::FailSendReqFamilyCodes |
    ClientErrorCode::FailSendReqMatchingSymbols |
    ClientErrorCode::FailSendReqMktDepthExchanges |
    ClientErrorCode::FailSendReqSmartComponents |
    ClientErrorCode::FailSendReqMarketRule => {
      handler.data_ref.handle_error(id, error_code, &error_msg);
    }

    // Financial Data Errors (Fundamental/WSH)
    ClientErrorCode::FailSendReqFundData |
    ClientErrorCode::FailSendCanFundData |
    ClientErrorCode::FailSendReqWshMetaData |
    ClientErrorCode::FailSendCanWshMetaData |
    ClientErrorCode::FailSendReqWshEventData |
    ClientErrorCode::FailSendCanWshEventData => {
      handler.data_fin.handle_error(id, error_code, &error_msg);
    }

    // News Data Errors
    ClientErrorCode::FailSendReqNewsProviders |
    ClientErrorCode::FailSendReqNewsArticle |
    ClientErrorCode::FailSendReqHistoricalNews => {
      handler.data_news.handle_error(id, error_code, &error_msg);
    }

    // Financial Advisor Errors
    ClientErrorCode::FailSendFaRequest |
    ClientErrorCode::FailSendFaReplace |
    ClientErrorCode::FaProfileNotSupported => {
      handler.fin_adv.handle_error(id, error_code, &error_msg);
    }

    // General Client/Connection/API Errors (Route to ClientHandler)
    ClientErrorCode::NoValidId |
    ClientErrorCode::AlreadyConnected |
    ClientErrorCode::ConnectFail |
    ClientErrorCode::UpdateTws |
    ClientErrorCode::NotConnected |
    ClientErrorCode::UnknownId |
    ClientErrorCode::UnsupportedVersion |
    ClientErrorCode::BadLength |
    ClientErrorCode::BadMessage |
    ClientErrorCode::FailSend | // Generic send failure
    ClientErrorCode::FailSendServerLogLevel |
    ClientErrorCode::FailSendReqCurrTime |
    ClientErrorCode::FailSendReqGlobalCancel |
    ClientErrorCode::FailSendVerifyRequest |
    ClientErrorCode::FailSendVerifyMessage |
    ClientErrorCode::FailSendQueryDisplayGroups |
    ClientErrorCode::FailSendSubscribeToGroupEvents |
    ClientErrorCode::FailSendUpdateDisplayGroup |
    ClientErrorCode::FailSendUnsubscribeFromGroupEvents |
    ClientErrorCode::FailSendStartApi |
    ClientErrorCode::FailSendVerifyAndAuthRequest |
    ClientErrorCode::FailSendVerifyAndAuthMessage |
    ClientErrorCode::InvalidSymbol | // Could be argued for others, but often general
    ClientErrorCode::FailSendReqUserInfo => {
      handler.client.handle_error(id, error_code, &error_msg);
    }
    // Note: If new error codes are added without updating this match,
    // they won't be routed. Consider adding a catch-all `_` case
    // that defaults to `handler.client.handle_error` if desired.
    // _ => {
    //     warn!("Error code {:?} not explicitly routed, defaulting to ClientHandler.", error_code);
    //     handler.client.handle_error(id, error_code, &error_msg);
    // }
  }

  // Example of potential future routing (requires ID mapping):
  // if id > 0 {
  //     match id_registry.get_handler_type(id) { // id_registry needs to be accessible
  //         Some(HandlerType::Order) => handler.order.handle_error(id, error_code, &error_msg),
  //         Some(HandlerType::MarketData) => handler.data_market.handle_error(id, error_code, &error_msg),
  //         // ... other handlers ...
  //         None => {
  //             warn!("Error received for unknown or inactive ID: {}. Routing to client handler.", id);
  //             handler.client.handle_error(id, error_code, &error_msg);
  //         }
  //     }
  // } else {
  //     // General error, route to client handler
  //     handler.client.handle_error(id, error_code, &error_msg);
  // }


  Ok(())
}

/// Process current time message (Type 49)
pub fn process_current_time(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Version is unused for this message type according to docs
  let _version = parser.read_int()?;
  let time_unix = parser.read_int()? as i64; // TWS sends Unix timestamp

  // --- Call the handler ---
  handler.current_time(time_unix);
  // ---

  Ok(())
}

/// Process verify message API message (Type 65)
pub fn process_verify_message_api(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let api_data = parser.read_string()?;

  // --- Call the handler ---
  handler.verify_message_api(&api_data);
  // ---

  warn!("Processing logic for VerifyMessageAPI not fully implemented.");
  Ok(())
}

/// Process verify completed message (Type 66)
pub fn process_verify_completed(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let is_successful_str = parser.read_string()?; // "true" or "false"
  let error_text = parser.read_string()?;

  let is_successful = is_successful_str.eq_ignore_ascii_case("true");

  // --- Call the handler ---
  handler.verify_completed(is_successful, &error_text);
  // ---

  Ok(())
}

/// Process verify and auth message API message (Type 69)
pub fn process_verify_and_auth_message_api(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let api_data = parser.read_string()?;
  let xyz_challenge = parser.read_string()?;

  // --- Call the handler ---
  handler.verify_and_auth_message_api(&api_data, &xyz_challenge);
  // ---

  warn!("Processing logic for VerifyAndAuthMessageAPI not fully implemented (challenge response needed).");
  Ok(())
}

/// Process verify and auth completed message (Type 70)
pub fn process_verify_and_auth_completed(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let is_successful_str = parser.read_string()?; // "true" or "false"
  let error_text = parser.read_string()?;

  let is_successful = is_successful_str.eq_ignore_ascii_case("true");

  // --- Call the handler ---
  handler.verify_and_auth_completed(is_successful, &error_text);
  // ---

  Ok(())
}

/// Process head timestamp message (Type 88)
pub fn process_head_timestamp(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Version is unused according to docs
  // let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let timestamp_str = parser.read_string()?; // Can be Unix timestamp or "yyyyMMdd HH:mm:ss"

  // --- Call the handler ---
  handler.head_timestamp(req_id, &timestamp_str);
  // ---

  // Parsing the timestamp string could happen here or in the handler
  Ok(())
}

/// Process user info message (Type 107)
pub fn process_user_info(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Version is unused according to docs
  // let _version = parser.read_int()?;
  let req_id = parser.read_int()?; // Will always be 0 from TWS
  let white_branding_id = parser.read_string()?;

  // --- Call the handler ---
  handler.user_info(req_id, &white_branding_id);
  // ---

  Ok(())
}

/// Process display group list message (Type 67)
pub fn process_display_group_list(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let groups = parser.read_string()?; // Comma-separated list of group IDs

  // --- Call the handler ---
  handler.display_group_list(req_id, &groups);
  // ---

  Ok(())
}

/// Process display group updated message (Type 68)
pub fn process_display_group_updated(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let contract_info = parser.read_string()?; // Encoded contract string (needs parsing if used)

  // --- Call the handler ---
  handler.display_group_updated(req_id, &contract_info);
  // ---

  warn!("Parsing of contract_info in DisplayGroupUpdated not implemented.");
  Ok(())
}
