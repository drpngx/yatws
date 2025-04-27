// yatws/src/data_wsh.rs
// Transcoded from:
// https://www.interactivebrokers.com/campus/wp-content/uploads/sites/2/2023/09/WSHEclassesandfieldsforIBAPI2022-12-23.pdf
#![allow(dead_code)] // Allow unused fields for now as parsing is external

use serde::Deserialize;

// --- Helper Types ---
type Amount = f64; // Using f64 for simplicity, consider rust_decimal for precision
type DateTime1 = String; // Represents date/time strings, parsing handled externally
type Date = String;      // Represents date strings (YYYY-MM-DD), parsing handled externally
type Time = String;      // Represents time strings (HH:MM:SS), parsing handled externally

// --- Base Structure (Conceptual - for understanding common fields) ---
// The actual deserialization will likely target the WshEventData enum directly.
// struct WshEventBase {
//     event_type: String,
//     contract: String, // IBKR Contract ID (Numeric String?)
//     isin: String,
//     contract_description: String,
// }

// --- Enum to Represent All Possible WSH Event Data Payloads ---
// This is the target for deserializing the JSON string from message 105
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event_type", rename_all = "camelCase")] // Assuming event_type dictates structure
pub enum WshEventData {
  #[serde(rename = "wshe_bod")]
  Bod(WsheBod),
  #[serde(rename = "wshe_bybk")]
  Bybk(WsheBybk),
  #[serde(rename = "wshe_bybkmod")]
  BybkMod(WsheBybkMod),
  #[serde(rename = "wshe_cc")]
  Cc(WsheCc),
  #[serde(rename = "wshe_div")]
  Div(WsheDiv),
  #[serde(rename = "wshe_divsr")]
  DivSr(WsheDivSr),
  #[serde(rename = "wshe_ed")]
  Ed(WsheEd),
  #[serde(rename = "wshe_eps")]
  Eps(WsheEps),
  #[serde(rename = "wshe_fda_adv_comm")]
  FdaAdvComm(WsheFdaAdvComm),
  #[serde(rename = "wshe_fq")]
  Fq(WsheFq),
  #[serde(rename = "wshe_ic")] // This type has subtypes in the schema
  Ic(WsheIc), // Represents both single and multi-company based on fields present? Needs sample JSON.
  #[serde(rename = "wshe_ic:PRESENTER")] // How presenters are represented needs clarification (nested? separate msg?)
  IcPresenter(WsheIcPresenter), // Assuming separate event type for now
  #[serde(rename = "wshe_idx")]
  Idx(WsheIdx),
  #[serde(rename = "wshe_interim_dates")]
  InterimDates(WsheInterimDates),
  #[serde(rename = "wshe_ipo")]
  Ipo(WsheIpo),
  #[serde(rename = "wshe_merg_acq")]
  MergAcq(WsheMergAcq),
  #[serde(rename = "wshe_movies")]
  Movies(WsheMovies),
  #[serde(rename = "wshe_option")]
  Option(WsheOption),
  #[serde(rename = "wshe_qe")]
  Qe(WsheQe),
  #[serde(rename = "wsh_sec")] // Note the underscore vs hyphen difference
  Sec(WshSec),
  #[serde(rename = "wshe_secondary")]
  Secondary(WsheSecondary),
  #[serde(rename = "wshe_sh")]
  Sh(WsheSh),
  #[serde(rename = "wshe_spinoffs")]
  Spinoffs(WsheSpinoffs),
  #[serde(rename = "wshe_splits")]
  Splits(WsheSplits),
  #[serde(rename = "wshe_videos")]
  Videos(WsheVideos),
  // Add unknown variant if needed:
  // #[serde(other)]
  // Unknown,
}

// --- Specific Event Structs ---

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheBod {
  #[serde(default)] pub announce_date: Option<DateTime1>,
  #[serde(default)] pub start_date: Option<Date>, // Marked with *
  #[serde(default)] pub end_date: Option<Date>,
  #[serde(default)] pub wshe_event_status: Option<String>, // PENDING, INPROCESS, HELD, CANCEL, POSTPONED
  // Common fields (must be present in the JSON alongside these)
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheBybk {
  #[serde(default)] pub record_type: Option<String>, // INITIAL
  #[serde(default)] pub wshe_buyback_status: Option<String>, // OPEN, COMPLETED, CANCELED, RETIRED
  #[serde(default)] pub buyback_method: Option<String>, // OPEN MARKET, TENDER OFFER
  #[serde(default)] pub announce_date: Option<Date>, // Marked with *
  #[serde(default)] pub approval_date: Option<Date>, // Marked with *
  #[serde(default)] pub number_of_shares: Option<i64>, // Use i64 for potentially large numbers
  #[serde(default)] pub shares_different: Option<String>, // Y/N?
  #[serde(default)] pub percent_of_shares: Option<Amount>,
  #[serde(default)] pub share_value: Option<i64>, // Use i64
  #[serde(default)] pub share_value_currency: Option<String>,
  #[serde(default)] pub value_different: Option<String>, // Y/N?
  #[serde(default)] pub end_date: Option<Date>, // Marked with *
  #[serde(default)] pub price_from: Option<Amount>,
  #[serde(default)] pub price_to: Option<Amount>,
  #[serde(default)] pub tender_result: Option<Amount>,
  #[serde(default)] pub tender_expiration: Option<DateTime1>, // Marked with *
  #[serde(default)] pub news_references: Option<String>,
  #[serde(default)] pub external_notes: Option<String>,
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheBybkMod {
  #[serde(default)] pub event_id: Option<String>, // Changed to String, could be alphanumeric
  #[serde(default)] pub record_type: Option<String>, // MODIFICATION
  #[serde(default)] pub announce_date: Option<Date>, // Marked with *
  #[serde(default)] pub news_references: Option<String>,
  #[serde(default)] pub external_notes: Option<String>,
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheCc {
  #[serde(default)] pub quarter: Option<String>, // Q1–Q4, H1, H2, Q1T–Q4T, H1T, H2T
  #[serde(default)] pub fiscal_year: Option<i32>,
  #[serde(default)] pub live_call_datetime: Option<DateTime1>, // Marked with *
  #[serde(default)] pub live_number: Option<String>,
  #[serde(default)] pub live_passcode: Option<String>,
  #[serde(default)] pub live_intl_number: Option<String>,
  #[serde(default)] pub live_intl_passcode: Option<String>,
  #[serde(default)] pub live_pwebsite: Option<String>,
  #[serde(default)] pub replay_start_datetime: Option<DateTime1>,
  #[serde(default)] pub replay_end_date: Option<Date>,
  #[serde(default)] pub replay_number: Option<String>,
  #[serde(default)] pub replay_passcode: Option<String>,
  #[serde(default)] pub replay_pwebsite: Option<String>,
  #[serde(default)] pub rebroadcast_enddate: Option<Date>,
  #[serde(default)] pub country: Option<String>,
  #[serde(default)] pub time_zone: Option<String>,
  #[serde(default)] pub iso_country_code: Option<String>,
  #[serde(default)] pub transcript_url: Option<String>,
  #[serde(default)] pub external_notes: Option<String>,
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheDiv {
  #[serde(default)] pub stock_exchange: Option<String>,
  #[serde(default)] pub wshe_approval_status: Option<String>, // Approved, Pending, Cancelled
  #[serde(default)] pub dividend_amount: Option<Amount>, // USD
  #[serde(default)] pub dividend_oc: Option<Amount>,
  #[serde(default)] pub dividend_currency: Option<String>,
  #[serde(default)] pub mixed_dividend: Option<String>, // Y/N?
  #[serde(default)] pub frequency: Option<i32>, // 1, 2, 4, 12, -1, 0
  #[serde(default)] pub announce_datetime: Option<DateTime1>, // Marked with *
  #[serde(default)] pub ex_div_date: Option<Date>, // Marked with *
  #[serde(default)] pub record_date: Option<Date>, // Marked with *
  #[serde(default)] pub pay_date: Option<Date>, // Marked with *
  #[serde(default)] pub fiscal_year: Option<i32>,
  #[serde(default)] pub time_period: Option<String>, // A01, M01–M12, Q01–Q04, S01, S02, SP, UK
  #[serde(default)] pub increase_decrease_code: Option<String>, // I, D, N
  #[serde(default)] pub change_amount: Option<Amount>,
  #[serde(default)] pub change_percent: Option<Amount>,
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheDivSr {
  #[serde(default)] pub wshe_dividend_status: Option<String>, // S, R
  #[serde(default)] pub dividend_status_date: Option<Date>, // Marked with *
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheEd {
  #[serde(default)] pub stock_exchange: Option<String>,
  #[serde(default)] pub earnings_date: Option<Date>, // Marked with *
  #[serde(default)] pub quarter: Option<String>, // Q1–Q4, H1, H2, Q1T–Q4T, H1T, H2T
  #[serde(default)] pub fiscal_year: Option<i32>,
  #[serde(default)] pub wshe_earnings_date_status: Option<String>, // CONFIRMED, UNCONFIRMED, INFERRED
  #[serde(default)] pub time_of_day: Option<String>, // BEFORE MARKET, DURING MARKET, AFTER MARKET, UNSPECIFIED
  #[serde(default)] pub prelim_earnings_date: Option<Date>, // Marked with *
  #[serde(default)] pub quarter_end_date: Option<Date>,
  #[serde(default)] pub audit_source: Option<String>, // RESEARCH, NEWS, BASE
  #[serde(default)] pub filing_due_date: Option<Date>,
  #[serde(default)] pub announcement_url: Option<String>,
  #[serde(default)] pub announce_datetime: Option<DateTime1>, // Marked with *
  #[serde(default)] pub disclaimer: Option<String>, // Y/N?
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheEps {
  #[serde(default)] pub announce_datetime: Option<DateTime1>, // Marked with *
  #[serde(default)] pub fiscal_year: Option<i32>,
  #[serde(default)] pub quarter1: Option<String>, // Q1–Q4, H1, H2, Q1T–Q4T, H1T, H2T (Note: name clash with WsheEd quarter, used quarter1)
  #[serde(default)] pub quarter_end_date: Option<Date>,
  #[serde(default)] pub currency: Option<String>,
  #[serde(default)] pub amount_oc: Option<Amount>,
  #[serde(default)] pub estimated_eps: Option<Amount>,
  #[serde(default)] pub purl: Option<String>,
  #[serde(default)] pub prelim_earnings_date: Option<Date>, // Marked with *
  #[serde(default)] pub prelim_amount_from: Option<Amount>,
  #[serde(default)] pub prelim_amount_to: Option<Amount>,
  #[serde(default)] pub prelim_earnings_link: Option<String>,
  #[serde(default)] pub increase_decrease_code: Option<String>, // I, D, N
  #[serde(default)] pub change_amount: Option<Amount>,
  #[serde(default)] pub change_percent: Option<Amount>,
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheFdaAdvComm {
  #[serde(default)] pub organizer: Option<String>, // FDA
  #[serde(default)] pub event_desc: Option<String>,
  #[serde(default)] pub wshe_event_status: Option<String>, // CONFIRMED, UNCONFIRMED
  #[serde(default)] pub start_date: Option<Date>, // Marked with *
  #[serde(default)] pub end_date: Option<Date>,
  #[serde(default)] pub local_time_start: Option<Time>,
  #[serde(default)] pub time_zone: Option<String>,
  #[serde(default)] pub venue: Option<String>,
  #[serde(default)] pub venue_address: Option<String>,
  #[serde(default)] pub venue_city: Option<String>,
  #[serde(default)] pub venue_state: Option<String>,
  #[serde(default)] pub venue_country: Option<String>,
  #[serde(default)] pub venue_country_iso: Option<String>,
  #[serde(default)] pub external_note: Option<String>,
  #[serde(default)] pub reference_link: Option<String>,
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheFq {
  #[serde(default)] pub stock_exchange: Option<String>,
  #[serde(default)] pub confidence_indicator: Option<String>, // 3 chars
  #[serde(default)] pub earnings_date: Option<Date>, // Marked with *
  #[serde(default)] pub quarter: Option<String>, // Q1-Q4, H1-H2, Q1T-Q4T, H1T-H2T
  #[serde(default)] pub fiscal_year: Option<i32>,
  #[serde(default)] pub wshe_earnings_date_status: Option<String>, // CONFIRMED, UNCONFIRMED, INFERRED
  #[serde(default)] pub time_of_day: Option<String>, // BEFORE MARKET, DURING MARKET, AFTER MARKET, UNSPECIFIED
  #[serde(default)] pub disclaimer: Option<String>, // Y/N?
  #[serde(default)] pub audit_source: Option<String>, // RESEARCH, NEWS, BASE
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheIc {
  #[serde(default)] pub ic_type: Option<String>, // Analyst Day, Business Update, etc. OR Forum, General Conference, etc.
  #[serde(default)] pub announce_date: Option<DateTime1>,
  #[serde(default)] pub organizer: Option<String>,
  #[serde(default)] pub event_desc: Option<String>,
  #[serde(default)] pub wshe_event_status: Option<String>, // PENDING, INPROCESS, CANCEL (Note: mismatch w/ schema, using wshe_)
  #[serde(default)] pub event_status: Option<String>, // PENDING, INPROCESS, CANCEL (From multi-company)
  #[serde(default)] pub sector_names: Option<String>,
  #[serde(default)] pub start_date: Option<Date>, // Marked with * in single-company
  #[serde(default)] pub end_date: Option<Date>,
  #[serde(default)] pub local_time_start: Option<Time>,
  #[serde(default)] pub time_zone: Option<String>,
  #[serde(default)] pub venue: Option<String>,
  #[serde(default)] pub venue_address: Option<String>,
  #[serde(default)] pub venue_city: Option<String>,
  #[serde(default)] pub venue_state: Option<String>,
  #[serde(default)] pub venue_country: Option<String>,
  #[serde(default)] pub venue_country_iso: Option<String>,
  #[serde(default)] pub purl: Option<String>,
  #[serde(default)] pub invite_url: Option<String>,
  #[serde(default)] pub schedule_url: Option<String>,
  #[serde(default)] pub additional_info_url: Option<String>,
  #[serde(default)] pub external_note: Option<String>,
  #[serde(default)] pub reference_link: Option<String>,
  #[serde(default)] pub virtual_meeting: Option<String>, // Y/N?
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
  // Presenter fields might appear here if nested, or via WsheIcPresenter type
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheIcPresenter {
  #[serde(default)] pub event_id: Option<String>, // Changed to String
  #[serde(default)] pub stock_symbol: Option<String>,
  #[serde(default)] pub company_id: Option<i32>, // Assuming WSH ID is numeric
  #[serde(default)] pub isin: Option<String>, // ISIN specific to presenter company
  #[serde(default)] pub company_name: Option<String>,
  #[serde(default)] pub start_date: Option<Date>, // Marked with *
  #[serde(default)] pub end_date: Option<Date>,
  #[serde(default)] pub time: Option<Time>,
  #[serde(default)] pub announce_date: Option<Date>,
  #[serde(default)] pub presenter_name: Option<String>,
  #[serde(default)] pub presenter_title: Option<String>,
  #[serde(default)] pub presenter_status: Option<String>, // PENDING, CANCEL
  // Common fields (May not be present if this is nested or separate)
  #[serde(default)] pub contract: Option<String>,
  // isin covered above
  #[serde(default)] pub contract_description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheIdx {
  #[serde(default)] pub change_type: Option<String>, // ADD, REMOVE, REPLACE
  #[serde(default)] pub index_name: Option<String>, // DJIA, SP500, etc.
  #[serde(default)] pub announce_date: Option<Date>, // Marked with *
  #[serde(default)] pub announce_url: Option<String>,
  #[serde(default)] pub effective_date: Option<Date>, // Marked with *
  #[serde(default)] pub replace_company_id: Option<i32>, // Assuming WSH ID is numeric
  #[serde(default)] pub replace_ticker: Option<String>,
  #[serde(default)] pub replace_isin: Option<String>,
  #[serde(default)] pub replace_ticker_name: Option<String>,
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheInterimDates {
  #[serde(default)] pub stock_exchange: Option<String>,
  #[serde(default)] pub interim_date: Option<Date>, // Marked with *
  #[serde(default)] pub wshe_interim_date_status: Option<String>, // Confirmed, Inferred, Unconfirmed
  #[serde(default)] pub interim_description: Option<String>,
  #[serde(default)] pub interim_type: Option<String>, // interim_statement, sales, etc.
  #[serde(default)] pub interim_frequency: Option<String>, // Monthly, Quarterly, etc.
  #[serde(default)] pub interim_fiscal_year: Option<i32>,
  #[serde(default)] pub interim_time_period: Option<String>, // M01-M12, Q01-Q04, H01-H02, A01
  #[serde(default)] pub interim_announce_datetime: Option<DateTime1>, // Marked with *
  #[serde(default)] pub interim_url: Option<String>,
  #[serde(default)] pub interim_source: Option<String>,
  #[serde(default)] pub call_datetime: Option<DateTime1>,
  #[serde(default)] pub call_time_zone: Option<String>,
  #[serde(default)] pub call_utc_offset: Option<Amount>,
  #[serde(default)] pub call_domestic_phone: Option<String>,
  #[serde(default)] pub call_intl_phone: Option<String>,
  #[serde(default)] pub call_passcode: Option<String>,
  #[serde(default)] pub call_live_url: Option<String>,
  #[serde(default)] pub call_source: Option<String>,
  #[serde(default)] pub publish_datetime: Option<DateTime1>, // Marked with *
  #[serde(default)] pub publish_url: Option<String>,
  #[serde(default)] pub publish_source: Option<String>,
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheIpo {
  #[serde(default)] pub wshe_ipo_status: Option<String>, // Priced, Withdrawn, Postponed, Week of, etc.
  #[serde(default)] pub modified: Option<DateTime1>,
  #[serde(default)] pub iposcoop_id: Option<i32>,
  #[serde(default)] pub iposcoop_company_name: Option<String>,
  #[serde(default)] pub industry: Option<String>,
  #[serde(default)] pub fileddate: Option<Date>, // Marked with *
  #[serde(default)] pub offeringdate: Option<Date>, // Marked with *
  #[serde(default)] pub offerprice: Option<Amount>, // USD
  #[serde(default)] pub firstdayclose: Option<Amount>, // USD
  #[serde(default)] pub currentprice: Option<Amount>, // USD
  #[serde(rename = "return", default)] pub return_percent: Option<Amount>, // Renamed from "return"
  #[serde(default)] pub shares: Option<Amount>, // millions
  #[serde(default)] pub pricerangelow: Option<Amount>,
  #[serde(default)] pub pricerangehigh: Option<Amount>,
  #[serde(default)] pub volume: Option<Amount>, // $ millions
  #[serde(default)] pub managers: Option<String>,
  #[serde(default)] pub quietperiod: Option<Date>, // Marked with *
  #[serde(default)] pub lockupperiod: Option<Date>, // Marked with *
  #[serde(default)] pub rating: Option<i32>, // 0-5
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheMergAcq {
  #[serde(default)] pub action_type: Option<String>, // Merger, 100% Acquisition, Partial Acquisition
  #[serde(default)] pub wshe_action_status: Option<String>, // Proposed, Announced, Completed, Rejected, Canceled
  #[serde(default)] pub acquirer_company_id: Option<i32>,
  #[serde(default)] pub acquirer_symbol: Option<String>,
  #[serde(default)] pub acquirer_isin: Option<String>,
  #[serde(default)] pub acquirer_name: Option<String>,
  #[serde(default)] pub target_company_id: Option<i32>,
  #[serde(default)] pub target_symbol: Option<String>,
  #[serde(default)] pub target_isin: Option<String>,
  #[serde(default)] pub target_name: Option<String>,
  #[serde(default)] pub announce_date: Option<Date>, // Marked with *
  #[serde(default)] pub approval_date: Option<Date>, // Marked with *
  #[serde(default)] pub close_date: Option<Date>, // Marked with *
  #[serde(default)] pub percent_interest: Option<Amount>, // 1-99
  #[serde(default)] pub price_per_share_currency: Option<String>,
  #[serde(default)] pub purchase_price_cash: Option<Amount>,
  #[serde(default)] pub purchase_price_currency: Option<String>,
  #[serde(default)] pub purchase_price_per_share: Option<Amount>,
  #[serde(default)] pub purchase_price_shares_number: Option<Amount>, // Number of shares? Using Amount for consistency
  #[serde(default)] pub et_flag: Option<String>, // Y/N?
  #[serde(default)] pub et_date: Option<Date>,
  #[serde(default)] pub sr_flag: Option<String>, // Y/N?
  #[serde(default)] pub sr_date: Option<Date>,
  #[serde(default)] pub sec_references: Option<String>,
  #[serde(default)] pub news_references: Option<String>,
  #[serde(default)] pub action_notes: Option<String>,
  #[serde(default)] pub close_time_period: Option<String>, // Q1-Q4, H1-H2, January–December
  #[serde(default)] pub close_year: Option<i32>, // Assuming i32 year
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheMovies {
  #[serde(default)] pub release_date: Option<Date>, // Marked with *
  #[serde(default)] pub release_title: Option<String>,
  #[serde(default)] pub region: Option<String>,
  #[serde(default)] pub distributor: Option<String>,
  #[serde(default)] pub release_pattern: Option<String>, // WIDE, LIMITED, EXCLUSIVE, etc.
  #[serde(default)] pub accuracy: Option<String>, // DAY, MONTH, FALL, SUMMER, YEAR
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheOption {
  #[serde(default)] pub option_symbol: Option<String>, // 1-5 chars
  #[serde(default)] pub us_stock_exchange: Option<String>,
  #[serde(default)] pub expiration_date: Option<Date>, // Marked with *
  #[serde(default)] pub expiration_frequency: Option<String>, // W1-W4, M, Y
  #[serde(default)] pub wshe_strike_status: Option<i32>, // 1-5
  #[serde(default)] pub option_type: Option<String>, // EO, L
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheQe {
  #[serde(default)] pub quarter_end_date: Option<Date>, // Marked with *
  #[serde(default)] pub quarter: Option<String>, // Q1–Q4, H1, H2, Q1T–Q4T, H1T, H2T
  #[serde(default)] pub fiscal_year: Option<i32>,
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WshSec { // Renamed struct to avoid clash with sec keyword
  #[serde(default)] pub filing_due_date: Option<Date>, // Marked with *
  #[serde(default)] pub quarter: Option<String>, // Q1–Q4, H1, H2, Q1T–Q4T, H1T, H2T
  #[serde(default)] pub fiscal_year: Option<i32>,
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheSecondary {
  #[serde(default)] pub wshe_offering_stage: Option<String>, // Open, Closed, Postponed, Canceled
  #[serde(default)] pub selling_shareholder: Option<String>,
  #[serde(default)] pub number_of_shares: Option<Amount>, // Using Amount (f64)
  #[serde(default)] pub share_price: Option<Amount>,
  #[serde(default)] pub currency: Option<String>,
  #[serde(default)] pub shares_different: Option<String>, // Y/N?
  #[serde(default)] pub announce_date: Option<Date>, // Marked with *
  #[serde(default)] pub effective_date: Option<Date>, // Marked with *
  #[serde(default)] pub closing_date: Option<Date>, // Marked with *
  #[serde(default)] pub offering_proceeds: Option<String>, // Y/N? Interpretation needed
  #[serde(default)] pub underwriter_manager: Option<String>,
  #[serde(default)] pub prospectus_link: Option<String>,
  #[serde(default)] pub link_to_publication: Option<String>,
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheSh {
  #[serde(default)] pub announce_date: Option<DateTime1>,
  #[serde(default)] pub start_date: Option<Date>, // Marked with *
  #[serde(default)] pub end_date: Option<Date>,
  #[serde(default)] pub local_time_start: Option<Time>,
  #[serde(default)] pub time_zone: Option<String>,
  #[serde(default)] pub record_date: Option<Date>, // Marked with *
  #[serde(default)] pub venue: Option<String>,
  #[serde(default)] pub venue_address: Option<String>,
  #[serde(default)] pub venue_city: Option<String>,
  #[serde(default)] pub venue_state: Option<String>,
  #[serde(default)] pub venue_country: Option<String>,
  #[serde(default)] pub venue_country_iso: Option<String>,
  #[serde(default)] pub purl: Option<String>, // Virtual meeting link
  #[serde(default)] pub reference_link: Option<String>,
  #[serde(default)] pub shm_meeting_type: Option<String>, // ANNUAL, SPECIAL, EXTRAORDINARY
  #[serde(default)] pub event_status: Option<String>, // Pending, In Process, Held, Postponed, Cancel
  #[serde(default)] pub virtual_meeting: Option<String>, // Y/N?
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheSpinoffs {
  #[serde(default)] pub parent_stock_symbol: Option<String>,
  #[serde(default)] pub parent_company_id: Option<i32>,
  #[serde(default)] pub parent_company_isin: Option<String>,
  #[serde(default)] pub parent_company_name: Option<String>,
  #[serde(default)] pub spinoff_stock_symbol: Option<String>,
  #[serde(default)] pub spinoff_company_id: Option<i32>,
  #[serde(default)] pub spinoff_company_isin: Option<String>,
  #[serde(default)] pub spinoff_company_name: Option<String>,
  #[serde(default)] pub wshe_spinoff_stage: Option<String>, // Pending, Approved, Completed, Canceled
  #[serde(default)] pub parent_shares: Option<Amount>,
  #[serde(default)] pub spinoff_shares: Option<Amount>,
  #[serde(default)] pub taxable: Option<String>, // Y/N/U?
  #[serde(default)] pub fractional_payment_type: Option<String>, // CASH, SHARES, NONE
  #[serde(default)] pub estimated_market_capitalization: Option<Amount>,
  #[serde(default)] pub announce_date: Option<Date>, // Marked with *
  #[serde(default)] pub record_date: Option<Date>, // Marked with *
  #[serde(default)] pub distribution_date: Option<Date>,
  #[serde(default)] pub trade_date: Option<Date>, // Marked with *
  #[serde(default)] pub parent_wi_from_date: Option<Date>,
  #[serde(default)] pub parent_wi_to_date: Option<Date>,
  #[serde(default)] pub spinoff_wi_from_date: Option<Date>,
  #[serde(default)] pub spinoff_wi_to_date: Option<Date>,
  #[serde(default)] pub approval_date: Option<Date>, // Marked with *
  #[serde(default)] pub news_references: Option<String>,
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheSplits {
  #[serde(default)] pub split_type: Option<String>, // REVERSE, DIVIDEND, STOCK SPLIT
  #[serde(default)] pub wshe_split_status: Option<String>, // APPROVED, PENDING
  #[serde(default)] pub ratio: Option<String>, // "NEW:ORIGINAL"
  #[serde(default)] pub announce_date: Option<DateTime1>, // Marked with * (Schema has DateTime1, name implies Date)
  #[serde(default)] pub ex_date: Option<Date>, // Marked with *
  #[serde(default)] pub record_date: Option<Date>, // Marked with *
  #[serde(default)] pub effective_date: Option<Date>, // Marked with *
  #[serde(default)] pub url: Option<String>,
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsheVideos {
  #[serde(default)] pub release_date: Option<Date>, // Marked with *
  #[serde(default)] pub release_title: Option<String>,
  #[serde(default)] pub region: Option<String>,
  #[serde(default)] pub distributor: Option<String>,
  #[serde(default)] pub accuracy: Option<String>, // DAY, MONTH, FALL, SUMMER, YEAR
  // Common fields
  pub contract: String,
  pub isin: String,
  pub contract_description: String,
}

// --- Struct for Encoder Input ---
/// Represents the filters for a reqWshEventData request.
#[derive(Debug, Clone, Default)]
pub struct WshEventDataRequest {
  pub con_id: Option<i32>,
  pub filter: Option<String>,
  pub fill_watchlist: bool,
  pub fill_portfolio: bool,
  pub fill_competitors: bool,
  pub start_date: Option<String>, // Format "yyyy-MM-dd" ? Check API docs
  pub end_date: Option<String>,   // Format "yyyy-MM-dd" ? Check API docs
  pub total_limit: Option<i32>, // Use i32::MAX for no limit
}
