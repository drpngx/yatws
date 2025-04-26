// yatws/src/message_parser.rs
//
// Parse the messages and dispatch to the appropriate handler.
use crate::handler::MessageHandler;
use crate::base::IBKRError;
use crate::protocol_dec_parser::FieldParser;

use crate::parser_client::*;
use crate::parser_order::*;
use crate::parser_account::*;
use crate::parser_fin_adv::*;
use crate::parser_data_ref::*;
use crate::parser_data_market::*;
use crate::parser_data_fin::*;
use crate::parser_data_news::*;

fn msg_to_string(data: &[u8]) -> String {
  match std::str::from_utf8(data) {
    Ok(s) => s.replace('\0', "·"),
    Err(_) => "Non-UTF8/Binary Data".to_string(),
  }
}

/// Process a message based on its type. This is the entry point for message handling.
pub fn process_message(handler: &mut MessageHandler, data: &[u8]) -> Result<(), IBKRError> {
  let mut parser = FieldParser::new(data);
  let msg_type_str = parser.read_string()?; // Read the first field (message ID)
  let msg_type = msg_type_str.parse::<i32>().map_err(|e| {
    log::error!("Failed to parse message type string '{}': {}", msg_type_str, e);
    IBKRError::ParseError(format!(
      "Invalid message type string '{}': {}",
      msg_type_str, e
    ))
  })?;
  log::debug!("parse message: {} [{}]", msg_type, msg_to_string(data));
  let sver = handler.get_server_version();
  match msg_type {
    4 => process_error_message(&handler.client, &mut parser)?,
    9 => process_next_valid_id(&handler.order, &mut parser)?,
    1 => process_tick_price(&handler.data_market, &mut parser)?,
    2 => process_tick_size(&handler.data_market, &mut parser)?,
    3 => process_order_status(&handler.order, &mut parser, sver)?,
    5 => process_open_order(&handler.order, &mut parser, sver)?,
    6 => process_account_value(&handler.account, &mut parser)?,
    7 => process_portfolio_value(&handler.account, &mut parser)?,
    8 => process_account_update_time(&handler.account, &mut parser)?,
    10 => process_contract_data(&handler.data_ref, &mut parser, sver)?,
    11 => process_execution_data(&handler.account, &mut parser, sver)?,
    12 => process_market_depth(&handler.data_market, &mut parser)?,
    13 => process_market_depth_l2(&handler.data_market, &mut parser)?,
    14 => process_news_bulletins(&handler.data_news, &mut parser)?,
    15 => process_managed_accounts(&handler.account, &mut parser)?,
    16 => process_receive_fa(&handler.fin_adv, &mut parser)?,
    17 => process_historical_data(&handler.data_market, &mut parser)?,
    18 => process_bond_contract_data(&handler.data_ref, &mut parser, sver)?,
    19 => process_scanner_parameters(&handler.data_market, &mut parser)?,
    20 => process_scanner_data(&handler.data_market, &mut parser)?,
    21 => process_tick_option_computation(&handler.data_market, &mut parser)?,
    45 => process_tick_generic(&handler.data_market, &mut parser)?,
    46 => process_tick_string(&handler.data_market, &mut parser)?,
    47 => process_tick_efp(&handler.data_market, &mut parser)?,
    49 => process_current_time(&handler.client, &mut parser)?,
    50 => process_real_time_bars(&handler.data_market, &mut parser)?,
    51 => process_fundamental_data(&handler.data_fin, &mut parser)?,
    52 => process_contract_data_end(&handler.data_ref, &mut parser)?,
    53 => process_open_order_end(&handler.order, &mut parser)?,
    54 => process_account_download_end(&handler.account, &mut parser)?,
    55 => process_execution_data_end(&handler.account, &mut parser)?,
    56 => process_delta_neutral_validation(&handler.data_market, &mut parser)?,
    57 => process_tick_snapshot_end(&handler.data_market, &mut parser)?,
    58 => process_market_data_type(&handler.data_market, &mut parser)?,
    59 => process_commission_report(&handler.account, &mut parser)?,
    61 => process_position(&handler.account, &mut parser)?,
    62 => process_position_end(&handler.account, &mut parser)?,
    63 => process_account_summary(&handler.account, &mut parser)?,
    64 => process_account_summary_end(&handler.account, &mut parser)?,
    65 => process_verify_message_api(&handler.client, &mut parser)?,
    66 => process_verify_completed(&handler.client, &mut parser)?,
    67 => process_display_group_list(&handler.client, &mut parser)?,
    68 => process_display_group_updated(&handler.client, &mut parser)?,
    69 => process_verify_and_auth_message_api(&handler.client, &mut parser)?,
    70 => process_verify_and_auth_completed(&handler.client, &mut parser)?,
    71 => process_position_multi(&handler.account, &mut parser)?,
    72 => process_position_multi_end(&handler.account, &mut parser)?,
    73 => process_account_update_multi(&handler.account, &mut parser)?,
    74 => process_account_update_multi_end(&handler.account, &mut parser)?,
    75 => process_security_definition_option_parameter(&handler.data_ref, &mut parser)?,
    76 => process_security_definition_option_parameter_end(&handler.data_ref, &mut parser)?,
    77 => process_soft_dollar_tiers(&handler.data_ref, &mut parser)?,
    78 => process_family_codes(&handler.data_ref, &mut parser)?,
    79 => process_symbol_samples(&handler.data_ref, &mut parser, sver)?,
    80 => process_mkt_depth_exchanges(&handler.data_ref, &mut parser, sver)?,
    81 => process_tick_req_params(&handler.data_market, &mut parser)?,
    82 => process_smart_components(&handler.data_ref, &mut parser)?,
    83 => process_news_article(&handler.data_news, &mut parser)?,
    84 => process_tick_news(&handler.data_news, &mut parser)?,
    85 => process_news_providers(&handler.data_news, &mut parser)?,
    86 => process_historical_news(&handler.data_news, &mut parser)?,
    87 => process_historical_news_end(&handler.data_news, &mut parser)?,
    88 => process_head_timestamp(&handler.client, &mut parser)?,
    89 => process_histogram_data(&handler.data_market, &mut parser)?,
    90 => process_historical_data_update(&handler.data_market, &mut parser)?,
    91 => process_reroute_mkt_data_req(&handler.data_market, &mut parser)?,
    92 => process_reroute_mkt_depth_req(&handler.data_market, &mut parser)?,
    93 => process_market_rule(&handler.data_ref, &mut parser)?,
    94 => process_pnl(&handler.account, &mut parser)?,
    95 => process_pnl_single(&handler.account, &mut parser)?,
    96 => process_historical_ticks(&handler.data_market, &mut parser)?,
    97 => process_historical_ticks_bid_ask(&handler.data_market, &mut parser)?,
    98 => process_historical_ticks_last(&handler.data_market, &mut parser)?,
    99 => process_tick_by_tick(&handler.data_market, &mut parser)?,
    100 => process_order_bound(&handler.order, &mut parser)?,
    101 => process_completed_order(&handler.order, &mut parser, sver)?,
    102 => process_completed_orders_end(&handler.order, &mut parser)?,
    103 => process_replace_fa_end(&handler.fin_adv, &mut parser)?,
    104 => process_wsh_meta_data(&handler.data_fin, &mut parser)?,
    105 => process_wsh_event_data(&handler.data_fin, &mut parser)?,
    106 => process_historical_schedule(&handler.data_ref, &mut parser)?,
    107 => process_user_info(&handler.client, &mut parser)?,
    _ => {
      // Handle unknown message types
      log::warn!("Unknown message type: {}", msg_type);
    }
  }

  Ok(())
}

pub fn identify_incoming_type(msg_data: &[u8]) -> Option<&'static str> {
  if msg_data.is_empty() {
    return None;
  }

  // Find the first null terminator to isolate the type ID string
  let end_pos = msg_data.iter().position(|&b| b == 0).unwrap_or(0); // Find first '\0'
  if end_pos == 0 {
    return None; // No null terminator or empty message ID
  }

  let type_id_bytes = &msg_data[0..end_pos];

  // Convert bytes to string slice
  let type_id_str = match std::str::from_utf8(type_id_bytes) {
    Ok(s) => s,
    Err(_) => return None, // Not valid UTF-8
  };

  // Parse the string to an integer (assuming base 10)
  let type_id: i32 = match type_id_str.parse() {
    Ok(id) => id,
    Err(_) => return None, // Failed to parse as integer
  };

  // Map the integer ID to a static string name
  match type_id {
    1 => Some("TICK_PRICE"),
    2 => Some("TICK_SIZE"),
    3 => Some("ORDER_STATUS"),
    4 => Some("ERR_MSG"),
    5 => Some("OPEN_ORDER"),
    6 => Some("ACCT_VALUE"),
    7 => Some("PORTFOLIO_VALUE"),
    8 => Some("ACCT_UPDATE_TIME"),
    9 => Some("NEXT_VALID_ID"),
    10 => Some("CONTRACT_DATA"),
    11 => Some("EXECUTION_DATA"),
    12 => Some("MARKET_DEPTH"),
    13 => Some("MARKET_DEPTH_L2"),
    14 => Some("NEWS_BULLETINS"),
    15 => Some("MANAGED_ACCTS"),
    16 => Some("RECEIVE_FA"), // Financial Advisor types
    17 => Some("HISTORICAL_DATA"),
    18 => Some("BOND_CONTRACT_DATA"),
    19 => Some("SCANNER_PARAMETERS"),
    20 => Some("SCANNER_DATA"),
    21 => Some("TICK_OPTION_COMPUTATION"),
    45 => Some("TICK_GENERIC"),
    46 => Some("TICK_STRING"),
    47 => Some("TICK_EFP"), // Exchange For Physical
    49 => Some("CURRENT_TIME"),
    50 => Some("REAL_TIME_BARS"),
    51 => Some("FUNDAMENTAL_DATA"),
    52 => Some("CONTRACT_DATA_END"),
    53 => Some("OPEN_ORDER_END"),
    54 => Some("ACCT_DOWNLOAD_END"),
    55 => Some("EXECUTION_DATA_END"),
    56 => Some("DELTA_NEUTRAL_VALIDATION"),
    57 => Some("TICK_SNAPSHOT_END"),
    58 => Some("MARKET_DATA_TYPE"),
    59 => Some("COMMISSION_REPORT"),
    61 => Some("POSITION_DATA"),
    62 => Some("POSITION_END"),
    63 => Some("ACCOUNT_SUMMARY"),
    64 => Some("ACCOUNT_SUMMARY_END"),
    65 => Some("VERIFY_MESSAGE_API"),
    66 => Some("VERIFY_COMPLETED"),
    67 => Some("DISPLAY_GROUP_LIST"),
    68 => Some("DISPLAY_GROUP_UPDATED"),
    69 => Some("VERIFY_AND_AUTH_MESSAGE_API"),
    70 => Some("VERIFY_AND_AUTH_COMPLETED"),
    71 => Some("POSITION_MULTI"),
    72 => Some("POSITION_MULTI_END"),
    73 => Some("ACCOUNT_UPDATE_MULTI"),
    74 => Some("ACCOUNT_UPDATE_MULTI_END"),
    75 => Some("SECURITY_DEFINITION_OPTION_PARAMETER"),
    76 => Some("SECURITY_DEFINITION_OPTION_PARAMETER_END"),
    77 => Some("SOFT_DOLLAR_TIERS"),
    78 => Some("FAMILY_CODES"),
    79 => Some("SYMBOL_SAMPLES"),
    80 => Some("MKT_DEPTH_EXCHANGES"),
    81 => Some("TICK_REQ_PARAMS"), // Response to reqMktData with snapshot=true
    82 => Some("SMART_COMPONENTS"), // Response to reqSmartComponents
    83 => Some("NEWS_ARTICLE"), // Response to reqNewsArticle
    84 => Some("TICK_NEWS"),
    85 => Some("NEWS_PROVIDERS"), // Response to reqNewsProviders
    86 => Some("HISTORICAL_NEWS"), // Response to reqHistoricalNews
    87 => Some("HISTORICAL_NEWS_END"), // Response to reqHistoricalNews
    88 => Some("HEAD_TIMESTAMP"), // Response to reqHeadTimestamp
    89 => Some("HISTOGRAM_DATA"), // Response to reqHistogramData
    90 => Some("HISTORICAL_DATA_UPDATE"), // Realtime update after HISTORICAL_DATA end
    91 => Some("REROUTE_MKT_DATA_REQ"), // Notification
    92 => Some("REROUTE_MKT_DEPTH_REQ"), // Notification
    93 => Some("MARKET_RULE"), // Response to reqMarketRule
    94 => Some("PNL"), // Response to reqPnL
    95 => Some("PNL_SINGLE"), // Response to reqPnLSingle
    96 => Some("HISTORICAL_TICKS"), // Response to reqHistoricalTicks
    97 => Some("HISTORICAL_TICKS_BID_ASK"), // Response to reqHistoricalTicks
    98 => Some("HISTORICAL_TICKS_LAST"), // Response to reqHistoricalTicks
    99 => Some("TICK_BY_TICK"), // Response to reqTickByTickData
    100 => Some("ORDER_BOUND"), // Response to placeOrder with transmit=false
    101 => Some("COMPLETED_ORDER"), // Response to reqCompletedOrders
    102 => Some("COMPLETED_ORDERS_END"), // Response to reqCompletedOrders
    103 => Some("REPLACE_FA_END"), // Financial Advisor
    104 => Some("WSH_META_DATA"), // Response to reqWshMetaData
    105 => Some("WSH_EVENT_DATA"), // Response to reqWshEventData
    106 => Some("HISTORICAL_SCHEDULE"),
    107 => Some("USER_INFO"), // Response to reqUserInfo
    // Add any missing incoming message IDs here
    _ => None, // Unknown type ID
  }
}
