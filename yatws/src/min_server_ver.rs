// yatws/src/min_server_ver.rs
// Minimum server version constants for the IBKR API

/// Minimum server versions for specific functionality
#[allow(dead_code)]
pub mod min_server_ver {
  pub const REAL_TIME_BARS: i32 = 34;
  pub const SCALE_ORDERS: i32 = 35;
  pub const SNAPSHOT_MKT_DATA: i32 = 35;
  pub const SSHORT_COMBO_LEGS: i32 = 35;
  pub const WHAT_IF_ORDERS: i32 = 36;
  pub const CONTRACT_CONID: i32 = 37;
  pub const PTA_ORDERS: i32 = 39;
  pub const FUNDAMENTAL_DATA: i32 = 40;
  pub const DELTA_NEUTRAL: i32 = 40;
  pub const CONTRACT_DATA_CHAIN: i32 = 40;
  pub const SCALE_ORDERS2: i32 = 40;
  pub const ALGO_ORDERS: i32 = 41;
  pub const EXECUTION_DATA_CHAIN: i32 = 42;
  pub const NOT_HELD: i32 = 44;
  pub const SEC_ID_TYPE: i32 = 45;
  pub const PLACE_ORDER_CONID: i32 = 46;
  pub const MKT_DATA_CONID: i32 = 47;
  pub const CALC_IMPLIED_VOLAT: i32 = 49;
  pub const CALC_OPTION_PRICE: i32 = 50;
  pub const CANCEL_CALC_IMPLIED_VOLAT: i32 = 50;
  pub const CANCEL_CALC_OPTION_PRICE: i32 = 50;
  pub const SSHORTX_OLD: i32 = 51;
  pub const SSHORTX: i32 = 52;
  pub const GLOBAL_CANCEL: i32 = 53;
  pub const HEDGE_ORDERS: i32 = 54;
  pub const MARKET_DATA_TYPE: i32 = 55;
  pub const OPT_OUT_SMART_ROUTING: i32 = 56;
  pub const SMART_COMBO_ROUTING_PARAMS: i32 = 57;
  pub const DELTA_NEUTRAL_CONID: i32 = 58;
  pub const SCALE_ORDERS3: i32 = 60;
  pub const ORDER_COMBO_LEGS_PRICE: i32 = 61;
  pub const TRAILING_PERCENT: i32 = 62;
  pub const DELTA_NEUTRAL_OPEN_CLOSE: i32 = 66;
  pub const ACCT_SUMMARY: i32 = 67;
  pub const TRADING_CLASS: i32 = 68;
  pub const SCALE_TABLE: i32 = 69;
  pub const LINKING: i32 = 70;
  pub const ALGO_ID: i32 = 71;
  pub const OPTIONAL_CAPABILITIES: i32 = 72;
  pub const ORDER_SOLICITED: i32 = 73;
  pub const LINKING_AUTH: i32 = 74;
  pub const PRIMARYEXCH: i32 = 75;
  pub const RANDOMIZE_SIZE_AND_PRICE: i32 = 76;
  pub const FRACTIONAL_POSITIONS: i32 = 101;
  pub const PEGGED_TO_BENCHMARK: i32 = 102;
  pub const MODELS_SUPPORT: i32 = 103;
  pub const SEC_DEF_OPT_PARAMS_REQ: i32 = 104;
  pub const EXT_OPERATOR: i32 = 105;
  pub const SOFT_DOLLAR_TIER: i32 = 106;
  pub const FAMILY_CODES: i32 = 107;
  pub const MATCHING_SYMBOLS: i32 = 108;
  pub const PAST_LIMIT: i32 = 109;
  pub const MD_SIZE_MULTIPLIER: i32 = 110;
  pub const CASH_QTY: i32 = 111;
  pub const MKT_DEPTH_EXCHANGES: i32 = 112;
  pub const TICK_NEWS: i32 = 113;
  pub const SMART_COMPONENTS: i32 = 114;
  pub const NEWS_PROVIDERS: i32 = 115;
  pub const NEWS_ARTICLE: i32 = 116;
  pub const HISTORICAL_NEWS: i32 = 117;
  pub const HEAD_TIMESTAMP: i32 = 118;
  pub const HISTOGRAM: i32 = 119;
  pub const SERVICE_DATA_TYPE: i32 = 120;
  pub const AGG_GROUP: i32 = 121;
  pub const UNDERLYING_INFO: i32 = 122;
  pub const CANCEL_HEADTIMESTAMP: i32 = 123;
  pub const SYNT_REALTIME_BARS: i32 = 124;
  pub const CFD_REROUTE: i32 = 125;
  pub const MARKET_RULES: i32 = 126;
  pub const PNL: i32 = 127;
  pub const NEWS_QUERY_ORIGINS: i32 = 128;
  pub const UNREALIZED_PNL: i32 = 129;
  pub const HISTORICAL_TICKS: i32 = 130;
  pub const MARKET_CAP_PRICE: i32 = 131;
  pub const PRE_OPEN_BID_ASK: i32 = 132;
  pub const REAL_EXPIRATION_DATE: i32 = 134;
  pub const REALIZED_PNL: i32 = 135;
  pub const LAST_LIQUIDITY: i32 = 136;
  pub const TICK_BY_TICK: i32 = 137;
  pub const DECISION_MAKER: i32 = 138;
  pub const MIFID_EXECUTION: i32 = 139;
  pub const TICK_BY_TICK_IGNORE_SIZE: i32 = 140;
  pub const AUTO_PRICE_FOR_HEDGE: i32 = 141;
  pub const WHAT_IF_EXT_FIELDS: i32 = 142;
  pub const SCANNER_GENERIC_OPTS: i32 = 143;
  pub const API_BIND_ORDER: i32 = 144;
  pub const ORDER_CONTAINER: i32 = 145;
  pub const SMART_DEPTH: i32 = 146;
  pub const REMOVE_NULL_ALL_CASTING: i32 = 147;
  pub const D_PEG_ORDERS: i32 = 148;
  pub const MKT_DEPTH_PRIM_EXCHANGE: i32 = 149;
  pub const COMPLETED_ORDERS: i32 = 150;
  pub const PRICE_MGMT_ALGO: i32 = 151;
  pub const STOCK_TYPE: i32 = 152;
  pub const ENCODE_MSG_ASCII7: i32 = 153;
  pub const SEND_ALL_FAMILY_CODES: i32 = 154;
  pub const NO_DEFAULT_OPEN_CLOSE: i32 = 155;
  pub const PRICE_BASED_VOLATILITY: i32 = 156;
  pub const REPLACE_FA_END: i32 = 157;
  pub const DURATION: i32 = 158;
  pub const MARKET_DATA_IN_SHARES: i32 = 159;
  pub const POST_TO_ATS: i32 = 160;
  pub const WSHE_CALENDAR: i32 = 161;
  pub const AUTO_CANCEL_PARENT: i32 = 162;
  pub const FRACTIONAL_SIZE_SUPPORT: i32 = 163;
  pub const SIZE_RULES: i32 = 164;
  pub const HISTORICAL_SCHEDULE: i32 = 165;
  pub const ADVANCED_ORDER_REJECT: i32 = 166;
  pub const USER_INFO: i32 = 167;
  pub const CRYPTO_AGGREGATED_TRADES: i32 = 168;
  pub const MANUAL_ORDER_TIME: i32 = 169;
  pub const PEGBEST_PEGMID_OFFSETS: i32 = 170;
  pub const WSH_EVENT_DATA_FILTERS: i32 = 171;
  pub const IPO_PRICES: i32 = 172;
  pub const WSH_EVENT_DATA_FILTERS_DATE: i32 = 173;
  pub const INSTRUMENT_TIMEZONE: i32 = 174;
  pub const HMDS_MARKET_DATA_IN_SHARES: i32 = 175;
  pub const BOND_ISSUERID: i32 = 176;
  pub const FA_PROFILE_DESUPPORT: i32 = 177;
  pub const PENDING_PRICE_REVISION: i32 = 178;
  pub const FUND_DATA_FIELDS: i32 = 179;
  pub const MANUAL_ORDER_TIME_EXERCISE_OPTIONS: i32 = 180;
  pub const OPEN_ORDER_AD_STRATEGY: i32 = 181;
  pub const LAST_TRADE_DATE: i32 = 182;
  pub const CUSTOMER_ACCOUNT: i32 = 183;
  pub const PROFESSIONAL_CUSTOMER: i32 = 184;
  pub const BOND_ACCRUED_INTEREST: i32 = 185;
  pub const INELIGIBILITY_REASONS: i32 = 186;
  pub const RFQ_FIELDS: i32 = 187; // Added

  // ---- TWS versions ----
  // TWS sends the current time as the first message upon connection. The time format is YYYYMMDD{ }hh:mm:ss{ }zzz. Requires TWS v40+.
  pub const TIME_MSG: i32 = 20;
  // TWS can receive Financial Advisor data. Requires TWS v89+.
  pub const FINANCIAL_ADVISOR: i32 = 18;
  // TWS supports the reqMktDataStream request. Requires TWS v90+.
  pub const MARKET_DATA_STREAMING: i32 = 59; // Corresponds to MARKET_DATA_TYPE
  // TWS supports requesting market depth data. Requires TWS v6.05+.
  pub const MARKET_DEPTH: i32 = 10;
  // TWS supports requesting news bulletins. Requires TWS v??+.
  pub const NEWS_BULLETINS: i32 = 12;
  // TWS supports requesting auto open orders. Requires TWS v??+.
  pub const AUTO_OPEN_ORDERS: i32 = 15;
  // TWS supports requesting all open orders. Requires TWS v??+.
  pub const ALL_OPEN_ORDERS: i32 = 16;
  // TWS supports requesting managed accounts. Requires TWS v??+.
  pub const MANAGED_ACCOUNTS: i32 = 17;
  // TWS supports replacing FA data. Requires TWS v??+.
  pub const REPLACE_FA: i32 = 19;
  // TWS supports exercising options. Requires TWS v21+.
  pub const EXERCISE_OPTIONS: i32 = 21;
  // TWS supports scanner subscriptions. Requires TWS v24+.
  pub const SCANNER_SUBSCRIPTION: i32 = 22;
  // TWS supports requesting scanner parameters. Requires TWS v24+.
  pub const SCANNER_PARAMETERS: i32 = 24;
  // TWS supports canceling historical data requests. Requires TWS v24+.
  pub const CANCEL_HISTORICAL_DATA: i32 = 25;
  // TWS supports requesting current time. Requires TWS v33+.
  pub const CURRENT_TIME: i32 = 49;
  // TWS supports canceling real time bars. Requires TWS v34+.
  pub const CANCEL_REAL_TIME_BARS: i32 = 51;
  // TWS supports canceling fundamental data requests. Requires TWS v40+.
  pub const CANCEL_FUNDAMENTAL_DATA: i32 = 53;
  // TWS supports requesting positions multi. Requires TWS v103+.
  pub const POSITIONS_MULTI: i32 = 74;
  // TWS supports canceling positions multi. Requires TWS v103+.
  pub const CANCEL_POSITIONS_MULTI: i32 = 75;
  // TWS supports requesting account updates multi. Requires TWS v103+.
  pub const ACCOUNT_UPDATES_MULTI: i32 = 76;
  // TWS supports canceling account updates multi. Requires TWS v103+.
  pub const CANCEL_ACCOUNT_UPDATES_MULTI: i32 = 77;
  // TWS supports verifying requests. Requires TWS v70+.
  pub const VERIFY_REQUEST: i32 = 65;
  // TWS supports verifying messages. Requires TWS v70+.
  pub const VERIFY_MESSAGE: i32 = 66;
  // TWS supports querying display groups. Requires TWS v70+.
  pub const QUERY_DISPLAY_GROUPS: i32 = 67;
  // TWS supports subscribing to group events. Requires TWS v70+.
  pub const SUBSCRIBE_TO_GROUP_EVENTS: i32 = 68;
  // TWS supports updating display groups. Requires TWS v70+.
  pub const UPDATE_DISPLAY_GROUP: i32 = 69;
  // TWS supports unsubscribing from group events. Requires TWS v70+.
  pub const UNSUBSCRIBE_FROM_GROUP_EVENTS: i32 = 70;
  // TWS supports verifying and authenticating requests. Requires TWS v74+.
  pub const VERIFY_AND_AUTH_REQUEST: i32 = 72;
  // TWS supports verifying and authenticating messages. Requires TWS v74+.
  pub const VERIFY_AND_AUTH_MESSAGE: i32 = 73;
  // TWS supports canceling histogram data requests. Requires TWS v119+.
  pub const CANCEL_HISTOGRAM_DATA: i32 = 89;


  // If you add anything, please edit the MAX_SUPPORTED_VERSION.
  // MAX_SUPPORTED_VERSION should match the highest constant defined above.
  pub const MAX_SUPPORTED_VERSION: i32 = 187;
}
