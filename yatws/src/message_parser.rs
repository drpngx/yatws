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

/// Process a message based on its type. This is the entry point for message handling.
pub fn process_message(handler: &mut MessageHandler, msg_type: i32, data: &[u8]) -> Result<(), IBKRError> {
  let mut parser = FieldParser::new(data);

  match msg_type {
    4 => process_error_message(&mut handler.client, &mut parser)?,
    9 => process_next_valid_id(&mut handler.order, &mut parser)?,
    1 => process_tick_price(&mut handler.data_market, &mut parser)?,
    2 => process_tick_size(&mut handler.data_market, &mut parser)?,
    3 => process_order_status(&mut handler.order, &mut parser)?,
    5 => process_open_order(&mut handler.order, &mut parser)?,
    6 => process_account_value(&mut handler.account, &mut parser)?,
    7 => process_portfolio_value(&mut handler.account, &mut parser)?,
    8 => process_account_update_time(&mut handler.account, &mut parser)?,
    10 => process_contract_data(&mut handler.data_ref, &mut parser)?,
    11 => process_execution_data(&mut handler.account, &mut parser)?,
    12 => process_market_depth(&mut handler.data_market, &mut parser)?,
    13 => process_market_depth_l2(&mut handler.data_market, &mut parser)?,
    14 => process_news_bulletins(&mut handler.data_news, &mut parser)?,
    15 => process_managed_accounts(&mut handler.account, &mut parser)?,
    16 => process_receive_fa(&mut handler.fin_adv, &mut parser)?,
    17 => process_historical_data(&mut handler.data_market, &mut parser)?,
    18 => process_bond_contract_data(&mut handler.data_ref, &mut parser)?,
    19 => process_scanner_parameters(&mut handler.data_market, &mut parser)?,
    20 => process_scanner_data(&mut handler.data_market, &mut parser)?,
    21 => process_tick_option_computation(&mut handler.data_ref, &mut parser)?,
    45 => process_tick_generic(&mut handler.data_market, &mut parser)?,
    46 => process_tick_string(&mut handler.data_market, &mut parser)?,
    47 => process_tick_efp(&mut handler.data_market, &mut parser)?,
    49 => process_current_time(&mut handler.client, &mut parser)?,
    50 => process_real_time_bars(&mut handler.data_market, &mut parser)?,
    51 => process_fundamental_data(&mut handler.data_fin, &mut parser)?,
    52 => process_contract_data_end(&mut handler.data_ref, &mut parser)?,
    53 => process_open_order_end(&mut handler.order, &mut parser)?,
    54 => process_account_download_end(&mut handler.account, &mut parser)?,
    55 => process_execution_data_end(&mut handler.account, &mut parser)?,
    56 => process_delta_neutral_validation(&mut handler.data_market, &mut parser)?,
    57 => process_tick_snapshot_end(&mut handler.data_market, &mut parser)?,
    58 => process_market_data_type(&mut handler.data_market, &mut parser)?,
    59 => process_commission_report(&mut handler.account, &mut parser)?,
    61 => process_position(&mut handler.account, &mut parser)?,
    62 => process_position_end(&mut handler.account, &mut parser)?,
    63 => process_account_summary(&mut handler.account, &mut parser)?,
    64 => process_account_summary_end(&mut handler.account, &mut parser)?,
    65 => process_verify_message_api(&mut handler.client, &mut parser)?,
    66 => process_verify_completed(&mut handler.client, &mut parser)?,
    67 => process_display_group_list(&mut handler.client, &mut parser)?,
    68 => process_display_group_updated(&mut handler.client, &mut parser)?,
    69 => process_verify_and_auth_message_api(&mut handler.client, &mut parser)?,
    70 => process_verify_and_auth_completed(&mut handler.client, &mut parser)?,
    71 => process_position_multi(&mut handler.account, &mut parser)?,
    72 => process_position_multi_end(&mut handler.account, &mut parser)?,
    73 => process_account_update_multi(&mut handler.account, &mut parser)?,
    74 => process_account_update_multi_end(&mut handler.account, &mut parser)?,
    75 => process_security_definition_option_parameter(&mut handler.data_ref, &mut parser)?,
    76 => process_security_definition_option_parameter_end(&mut handler.data_ref, &mut parser)?,
    77 => process_soft_dollar_tiers(&mut handler.data_ref, &mut parser)?,
    78 => process_family_codes(&mut handler.data_ref, &mut parser)?,
    79 => process_symbol_samples(&mut handler.data_ref, &mut parser)?,
    80 => process_mkt_depth_exchanges(&mut handler.data_market, &mut parser)?,
    81 => process_tick_req_params(&mut handler.data_market, &mut parser)?,
    82 => process_smart_components(&mut handler.data_ref, &mut parser)?,
    83 => process_news_article(&mut handler.data_news, &mut parser)?,
    84 => process_tick_news(&mut handler.data_news, &mut parser)?,
    85 => process_news_providers(&mut handler.data_news, &mut parser)?,
    86 => process_historical_news(&mut handler.data_news, &mut parser)?,
    87 => process_historical_news_end(&mut handler.data_news, &mut parser)?,
    88 => process_head_timestamp(&mut handler.client, &mut parser)?,
    89 => process_histogram_data(&mut handler.data_market, &mut parser)?,
    90 => process_historical_data_update(&mut handler.data_market, &mut parser)?,
    91 => process_reroute_mkt_data_req(&mut handler.data_market, &mut parser)?,
    92 => process_reroute_mkt_depth_req(&mut handler.data_market, &mut parser)?,
    93 => process_market_rule(&mut handler.data_ref, &mut parser)?,
    94 => process_pnl(&mut handler.account, &mut parser)?,
    95 => process_pnl_single(&mut handler.account, &mut parser)?,
    96 => process_historical_ticks(&mut handler.data_market, &mut parser)?,
    97 => process_historical_ticks_bid_ask(&mut handler.data_market, &mut parser)?,
    98 => process_historical_ticks_last(&mut handler.data_market, &mut parser)?,
    99 => process_tick_by_tick(&mut handler.data_market, &mut parser)?,
    100 => process_order_bound(&mut handler.order, &mut parser)?,
    101 => process_completed_order(&mut handler.order, &mut parser)?,
    102 => process_completed_orders_end(&mut handler.order, &mut parser)?,
    103 => process_replace_fa_end(&mut handler.fin_adv, &mut parser)?,
    104 => process_wsh_meta_data(&mut handler.data_fin, &mut parser)?,
    105 => process_wsh_event_data(&mut handler.data_fin, &mut parser)?,
    106 => process_historical_schedule(&mut handler.data_ref, &mut parser)?,
    107 => process_user_info(&mut handler.client, &mut parser)?,
    _ => {
      // Handle unknown message types
      log::warn!("Unknown message type: {}", msg_type);
    }
  }

  Ok(())
}
