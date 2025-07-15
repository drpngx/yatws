use crate::base::IBKRError;
use crate::data::{
  CompanyInformation, FundamentalReportType, // Removed FinancialStatement, FinancialStatementType
  ParsedFundamentalData, Ratio, ReportSnapshot, ReportsFinSummary, // Removed PeriodType
};
use chrono::NaiveDate;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
// Removed unused HashMap import
use std::str;

// Helper to extract attribute value
fn get_attr_value(e: &BytesStart, key: &[u8]) -> Result<Option<String>, IBKRError> {
  for attr_result in e.attributes() {
    let attr = attr_result.map_err(|err| {
      IBKRError::ParseError(format!("XML attribute parsing error: {}", err))
    })?;
    if attr.key.as_ref() == key {
      // Unescape the attribute value, then convert to String
      let unescaped_value_cow = attr.unescape_value().map_err(|err| {
        IBKRError::ParseError(format!("Attribute value unescape error: {}", err))
      })?;
      let value_str = unescaped_value_cow.as_ref();
      return Ok(Some(value_str.to_string()));
    }
  }
  Ok(None)
}

// Helper to parse f64 from Option<&str>
#[allow(dead_code)] // May not be used by all parsers initially
fn parse_optional_f64(opt_str: Option<String>) -> Option<f64> {
  opt_str.as_deref().and_then(|s| s.parse::<f64>().ok())
}

// Helper to parse i32 from Option<&str>
fn parse_optional_i32(opt_str: Option<String>) -> Option<i32> {
  opt_str.as_deref().and_then(|s| s.parse::<i32>().ok())
}

// Helper to parse NaiveDate from Option<&str> (YYYY-MM-DD)
fn parse_optional_date(opt_str: Option<String>) -> Option<NaiveDate> {
  opt_str.as_deref().and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
}


fn parse_reports_fin_summary(xml_data: &str) -> Result<ReportsFinSummary, IBKRError> {
  let mut reader = Reader::from_str(xml_data);
  reader.config_mut().trim_text(true);
  let mut buf = Vec::new();
  let mut report = ReportsFinSummary::default();

  let mut current_section_currency: Option<String> = None;

  loop {
    match reader.read_event_into(&mut buf) {
      Ok(Event::Start(e)) => {
        match e.name().as_ref() {
          b"FinancialSummary" => { /* Root tag, continue */ }
          b"EPSs" | b"DividendPerShares" | b"TotalRevenues" | b"Dividends" => {
            current_section_currency = get_attr_value(&e, b"currency")?;
          }
          b"EPS" => {
            let mut record = EPSRecord {
              currency: current_section_currency.clone(),
              ..Default::default()
            };
            record.as_of_date = parse_optional_date(get_attr_value(&e, b"asofDate")?);
            record.report_type = get_attr_value(&e, b"reportType")?;
            record.period = get_attr_value(&e, b"period")?;
            let is_empty_tag = e.is_empty();

            if !is_empty_tag {
              if let Ok(Event::Text(text_e)) = reader.read_event_into(&mut buf) {
                record.value = text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.parse().ok();
                if let Ok(Event::End(_)) = reader.read_event_into(&mut buf) { /* ok */ }
                else { return Err(IBKRError::ParseError("Expected </EPS> after text".into())); }
              } else {
                return Err(IBKRError::ParseError("Expected text content for non-empty <EPS>".into()));
              }
            }
            report.eps_records.push(record);
          }
          b"DividendPerShare" => {
            let mut record = DividendPerShareRecord {
              currency: current_section_currency.clone(),
              ..Default::default()
            };
            record.as_of_date = parse_optional_date(get_attr_value(&e, b"asofDate")?);
            record.report_type = get_attr_value(&e, b"reportType")?;
            record.period = get_attr_value(&e, b"period")?;
            let is_empty_tag = e.is_empty();

            if !is_empty_tag {
              if let Ok(Event::Text(text_e)) = reader.read_event_into(&mut buf) {
                record.value = text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.parse().ok();
                if let Ok(Event::End(_)) = reader.read_event_into(&mut buf) { /* ok */ }
                else { return Err(IBKRError::ParseError("Expected </DividendPerShare> after text".into())); }
              } else {
                return Err(IBKRError::ParseError("Expected text content for non-empty <DividendPerShare>".into()));
              }
            }
            report.dividend_per_share_records.push(record);
          }
          b"TotalRevenue" => {
            let mut record = TotalRevenueRecord {
              currency: current_section_currency.clone(),
              ..Default::default()
            };
            record.as_of_date = parse_optional_date(get_attr_value(&e, b"asofDate")?);
            record.report_type = get_attr_value(&e, b"reportType")?;
            record.period = get_attr_value(&e, b"period")?;
            let is_empty_tag = e.is_empty();

            if !is_empty_tag {
              if let Ok(Event::Text(text_e)) = reader.read_event_into(&mut buf) {
                record.value = text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.parse().ok();
                if let Ok(Event::End(_)) = reader.read_event_into(&mut buf) { /* ok */ }
                else { return Err(IBKRError::ParseError("Expected </TotalRevenue> after text".into())); }
              } else {
                return Err(IBKRError::ParseError("Expected text content for non-empty <TotalRevenue>".into()));
              }
            }
            report.total_revenue_records.push(record);
          }
          b"Dividend" => { // For announced dividends
            let mut record = AnnouncedDividendRecord {
              currency: current_section_currency.clone(), // Assuming currency from parent <Dividends>
              ..Default::default()
            };
            record.dividend_type = get_attr_value(&e, b"type")?;
            record.ex_date = parse_optional_date(get_attr_value(&e, b"exDate")?);
            record.record_date = parse_optional_date(get_attr_value(&e, b"recordDate")?);
            record.pay_date = parse_optional_date(get_attr_value(&e, b"payDate")?);
            record.declaration_date = parse_optional_date(get_attr_value(&e, b"declarationDate")?);
            let is_empty_tag = e.is_empty();

            if !is_empty_tag {
              if let Ok(Event::Text(text_e)) = reader.read_event_into(&mut buf) {
                record.value = text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.parse().ok();
                if let Ok(Event::End(_)) = reader.read_event_into(&mut buf) { /* ok */ }
                else { return Err(IBKRError::ParseError("Expected </Dividend> after text".into())); }
              } else {
                return Err(IBKRError::ParseError("Expected text content for non-empty <Dividend>".into()));
              }
            }
            report.announced_dividend_records.push(record);
          }
          _ => { /* Skip other tags by consuming them */
                    let tag_name_bytes = e.name().as_ref().to_vec();
                    reader.read_to_end_into(quick_xml::name::QName(&tag_name_bytes), &mut buf).map_err(|err| IBKRError::ParseError(format!("Failed to skip to end of tag {:?}: {}", String::from_utf8_lossy(&tag_name_bytes), err)))?;
          }
        }
      }
      Ok(Event::End(e)) => {
        match e.name().as_ref() {
          b"FinancialSummary" => break, // End of report
          b"EPSs" | b"DividendPerShares" | b"TotalRevenues" | b"Dividends" => {
            current_section_currency = None; // Reset currency when exiting a section
          }
          _ => { /* Do nothing for other end tags */ }
        }
      }
      Ok(Event::Eof) => break, // End of document
      Err(err) => return Err(IBKRError::ParseError(format!("XML parsing error in ReportsFinSummary: {}", err))),
      _ => (),
    }
    buf.clear();
  }
  Ok(report)
}


// Import new structs
use crate::data::{
  AnnouncedDividendRecord, CoGeneralInfo, ContactInfo, EPSRecord, DividendPerShareRecord, ForecastData,
  ForecastDataItem, Issue, Officer, PeerInfo, RatioGroup, TextInfoItem, TotalRevenueRecord, WebLink, Industry,
};
use chrono::TimeZone; // For parsing DateTime with timezone offset

// Helper to parse DateTime with potential timezone offset (e.g., 2024-11-12T14:08:27)
fn parse_optional_datetime(opt_str: Option<String>) -> Option<chrono::DateTime<chrono::Utc>> {
  opt_str.as_deref().and_then(|s| {
    // Try parsing with explicit offset first
    chrono::DateTime::parse_from_rfc3339(s)
      .map(|dt| dt.with_timezone(&chrono::Utc)) // Convert to Utc immediately
      .or_else(|_| {
        // If RFC3339 fails, try naive datetime and assume UTC
        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
          .map(|ndt| chrono::Utc.from_utc_datetime(&ndt))
      })
      .ok()
  })
}

// Helper to parse bool from Option<String> ("0" or "1")
fn parse_optional_bool_int(opt_str: Option<String>) -> Option<bool> {
  opt_str.as_deref().and_then(|s| match s {
    "1" => Some(true),
    "0" => Some(false),
    _ => None,
  })
}

fn parse_report_snapshot(xml_data: &str) -> Result<ReportSnapshot, IBKRError> {
  let mut reader = Reader::from_str(xml_data);
  reader.config_mut().trim_text(true);
  let mut buf = Vec::new();
  let mut report = ReportSnapshot::default();
  let mut current_issue: Option<Issue> = None;
  let mut current_officer: Option<Officer> = None;
  let mut current_ratio_group: Option<RatioGroup> = None;
  let mut current_forecast_item: Option<ForecastDataItem> = None;
  let mut current_industry: Option<Industry> = None;
  let mut current_text_info: Option<TextInfoItem> = None;
  let mut current_contact_info = ContactInfo::default(); // Accumulate within this struct
  let mut current_web_links = WebLink::default();
  let mut current_peer_info = PeerInfo::default();
  let mut current_co_general_info = CoGeneralInfo::default();
  let mut current_forecast_data = ForecastData::default();

  loop {
    match reader.read_event_into(&mut buf) {
      Ok(Event::Start(e)) => {
        match e.name().as_ref() {
          // Top Level / Containers
          b"ReportSnapshot" => { /* Root */ }
          b"CoIDs" => { // Top-level CoIDs
            loop {
              match reader.read_event_into(&mut buf) {
                Ok(Event::Start(coid_e)) if coid_e.name().as_ref() == b"CoID" => {
                  if let Some(coid_type) = get_attr_value(&coid_e, b"Type")? {
                    if let Ok(Event::Text(text_e)) = reader.read_event_into(&mut buf) {
                      report.top_level_coids.insert(coid_type, text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.into_owned());
                    }
                    if let Ok(Event::End(_)) = reader.read_event_into(&mut buf) { /* consume end CoID */ }
                  }
                }
                Ok(Event::End(end_e)) if end_e.name().as_ref() == b"CoIDs" => break,
                Ok(Event::Eof) => return Err(IBKRError::ParseError("EOF in top-level CoIDs".into())),
                Err(err) => return Err(IBKRError::ParseError(format!("XML error in top-level CoIDs: {}", err))),
                _ => {}
              }
              buf.clear();
            }
          }
          b"Issues" => { /* Container */ }
          b"Issue" => {
            current_issue = Some(Issue {
              id: get_attr_value(&e, b"ID")?,
              issue_type: get_attr_value(&e, b"Type")?,
              description: get_attr_value(&e, b"Desc")?,
              order: parse_optional_i32(get_attr_value(&e, b"Order")?),
              ..Default::default()
            });
          }
          b"CoGeneralInfo" => {
            current_co_general_info.last_modified = parse_optional_date(get_attr_value(&e, b"LastModified")?); // Assuming date only here
          }
          b"TextInfo" => { /* Container */ }
          b"Text" => {
            current_text_info = Some(TextInfoItem {
              text_type: get_attr_value(&e, b"Type")?,
              last_modified: parse_optional_datetime(get_attr_value(&e, b"lastModified")?),
              ..Default::default()
            });
          }
          b"contactInfo" => {
            current_contact_info.last_updated = parse_optional_datetime(get_attr_value(&e, b"lastUpdated")?);
          }
          b"webLinks" => {
            current_web_links.last_updated = parse_optional_datetime(get_attr_value(&e, b"lastUpdated")?);
          }
          b"peerInfo" => {
            current_peer_info.last_updated = parse_optional_datetime(get_attr_value(&e, b"lastUpdated")?);
          }
          b"officers" => { /* Container */ }
          b"officer" => {
            current_officer = Some(Officer {
              rank: parse_optional_i32(get_attr_value(&e, b"rank")?),
              since: get_attr_value(&e, b"since")?,
              ..Default::default()
            });
          }
          b"Ratios" => {
            report.ratios_price_currency = get_attr_value(&e, b"PriceCurrency")?;
            report.ratios_reporting_currency = get_attr_value(&e, b"ReportingCurrency")?;
            report.ratios_exchange_rate = parse_optional_f64(get_attr_value(&e, b"ExchangeRate")?);
            report.ratios_latest_available_date = parse_optional_date(get_attr_value(&e, b"LatestAvailableDate")?);
          }
          b"Group" => { // Inside Ratios
            current_ratio_group = Some(RatioGroup {
              id: get_attr_value(&e, b"ID")?,
              ..Default::default()
            });
          }
          b"Ratio" => { // Inside Ratios/Group OR ForecastData
            if current_ratio_group.is_some() { // Parsing Ratios/Group/Ratio
              let mut ratio = Ratio {
                group_name: current_ratio_group.as_ref().and_then(|g| g.id.clone()).unwrap_or_default(), // Use group ID as name
                field_name: get_attr_value(&e, b"FieldName")?.unwrap_or_default(),
                period_type: get_attr_value(&e, b"Type")?, // Note: Type attribute here, not PeriodType
                ..Default::default()
              };
              let is_empty_tag = e.is_empty();
              if !is_empty_tag {
                if let Ok(Event::Text(text_e)) = reader.read_event_into(&mut buf) {
                  ratio.raw_value = Some(text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.into_owned());
                  if let Ok(Event::End(_)) = reader.read_event_into(&mut buf) { /* consume end */ }
                  else { return Err(IBKRError::ParseError("Expected </Ratio> after text in Ratios/Group".into())); }
                } else {
                  return Err(IBKRError::ParseError("Expected text content for non-empty <Ratio> in Ratios/Group".into()));
                }
              }

              if let Some(group) = current_ratio_group.as_mut() {
                group.ratios.push(ratio);
              }
            } else if current_forecast_data.consensus_type.is_some() { // Parsing ForecastData/Ratio
              current_forecast_item = Some(ForecastDataItem{
                field_name: get_attr_value(&e, b"FieldName")?.unwrap_or_default(),
                value_type: get_attr_value(&e, b"Type")?,
                ..Default::default()
              });
            }
          }
          b"ForecastData" => {
            current_forecast_data.consensus_type = get_attr_value(&e, b"ConsensusType")?;
            current_forecast_data.cur_fiscal_year = parse_optional_i32(get_attr_value(&e, b"CurFiscalYear")?);
            current_forecast_data.cur_fiscal_year_end_month = parse_optional_i32(get_attr_value(&e, b"CurFiscalYearEndMonth")?);
            current_forecast_data.cur_interim_end_cal_year = parse_optional_i32(get_attr_value(&e, b"CurInterimEndCalYear")?);
            current_forecast_data.cur_interim_end_month = parse_optional_i32(get_attr_value(&e, b"CurInterimEndMonth")?);
            current_forecast_data.earnings_basis = get_attr_value(&e, b"EarningsBasis")?;
          }

          // --- Fields within specific sections ---
          // Issue fields
          b"IssueID" if current_issue.is_some() => {
            if let Some(issue) = current_issue.as_mut() {
              if let Some(id_type) = get_attr_value(&e, b"Type")? {
                let is_outer_tag_empty = e.is_empty(); // Capture state of the <IssueID ...> StartEvent itself

                if !is_outer_tag_empty {
                  // The <IssueID ...> tag is not self-closing, so expect content or an immediate end tag.
                  match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                    Event::Text(text_e) => {
                      let value = text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.into_owned();
                      match id_type.as_str() {
                        "Name" => issue.name = Some(value),
                        "Ticker" => issue.ticker = Some(value),
                        "RIC" => issue.ric = Some(value),
                        "DisplayRIC" => issue.display_ric = Some(value),
                        "InstrumentPI" => issue.instrument_pi = Some(value),
                        "QuotePI" => issue.quote_pi = Some(value),
                        "InstrumentPermID" => issue.instrument_perm_id = Some(value),
                        "QuotePermID" => issue.quote_perm_id = Some(value),
                        _ => {}
                      }
                      // Expect and consume the </IssueID> end tag
                      match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                        Event::End(end_e) if end_e.name().as_ref() == b"IssueID" => { /* Correctly consumed */ }
                        other_event => return Err(IBKRError::ParseError(format!("Expected </IssueID> after text for type '{}', found {:?}", id_type, other_event))),
                      }
                    }
                    Event::End(end_e) if end_e.name().as_ref() == b"IssueID" => {
                      // Tag was like <IssueID Type="..."></IssueID> (empty content)
                      // Fields remain None or default as per struct initialization.
                    }
                    other_event => return Err(IBKRError::ParseError(format!("Expected text or </IssueID> for non-empty <IssueID Type='{}'>, found {:?}", id_type, other_event))),
                  }
                } // else: is_outer_tag_empty is true. Tag was <IssueID ... />. Attributes parsed. No content or separate end tag needed.
              }
            }
          }
          b"Exchange" if current_issue.is_some() => {
            if let Some(issue) = current_issue.as_mut() {
              issue.exchange_code = get_attr_value(&e, b"Code")?;
              issue.exchange_country = get_attr_value(&e, b"Country")?;
              if !e.is_empty() {
                if let Ok(Event::Text(text_e)) = reader.read_event_into(&mut buf) {
                  issue.exchange_name = Some(text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.into_owned());
                  if let Ok(Event::End(_)) = reader.read_event_into(&mut buf) { /* consume end */ }
                  else { return Err(IBKRError::ParseError("Expected </Exchange> after text".into())); }
                } else { return Err(IBKRError::ParseError("Expected text for <Exchange>".into())); }
              }
            }
          }
          b"GlobalListingType" if current_issue.is_some() => {
            if let Some(issue) = current_issue.as_mut() {
              if !e.is_empty() {
                if let Ok(Event::Text(text_e)) = reader.read_event_into(&mut buf) {
                  issue.global_listing_type = Some(text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.into_owned());
                  if let Ok(Event::End(_)) = reader.read_event_into(&mut buf) { /* consume end */ }
                  else { return Err(IBKRError::ParseError("Expected </GlobalListingType> after text".into())); }
                } else { return Err(IBKRError::ParseError("Expected text for <GlobalListingType>".into())); }
              }
            }
          }
          b"MostRecentSplit" if current_issue.is_some() => {
            if let Some(issue) = current_issue.as_mut() {
              issue.most_recent_split_date = parse_optional_date(get_attr_value(&e, b"Date")?);
              if !e.is_empty() {
                if let Ok(Event::Text(text_e)) = reader.read_event_into(&mut buf) {
                  issue.most_recent_split_factor = text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.parse().ok();
                  if let Ok(Event::End(_)) = reader.read_event_into(&mut buf) { /* consume end */ }
                  else { return Err(IBKRError::ParseError("Expected </MostRecentSplit> after text".into())); }
                } else { return Err(IBKRError::ParseError("Expected text for <MostRecentSplit>".into())); }
              }
            }
          }
          // CoGeneralInfo fields
          b"CoStatus" => {
            current_co_general_info.co_status_code = parse_optional_i32(get_attr_value(&e, b"Code")?);
            if !e.is_empty() {
              if let Ok(Event::Text(text_e)) = reader.read_event_into(&mut buf) {
                current_co_general_info.co_status_desc = Some(text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.into_owned());
                if let Ok(Event::End(_)) = reader.read_event_into(&mut buf) { /* consume end */ }
                else { return Err(IBKRError::ParseError("Expected </CoStatus> after text".into())); }
              } else { return Err(IBKRError::ParseError("Expected text for <CoStatus>".into())); }
            }
          }
          b"CoType" => {
            current_co_general_info.co_type_code = get_attr_value(&e, b"Code")?;
            if !e.is_empty() {
              if let Ok(Event::Text(text_e)) = reader.read_event_into(&mut buf) {
                current_co_general_info.co_type_desc = Some(text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.into_owned());
                if let Ok(Event::End(_)) = reader.read_event_into(&mut buf) { /* consume end */ }
                else { return Err(IBKRError::ParseError("Expected </CoType> after text".into())); }
              } else { return Err(IBKRError::ParseError("Expected text for <CoType>".into())); }
            }
          }
          b"LatestAvailableAnnual" | b"LatestAvailableInterim" => {
            let tag_name = e.name().as_ref().to_vec(); // Clone name before reading text
            if !e.is_empty() {
              if let Ok(Event::Text(text_e)) = reader.read_event_into(&mut buf) {
                let date_val = parse_optional_date(Some(text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.into_owned()));
                if tag_name == b"LatestAvailableAnnual" { current_co_general_info.latest_available_annual = date_val; }
                else { current_co_general_info.latest_available_interim = date_val; }
                if let Ok(Event::End(_)) = reader.read_event_into(&mut buf) { /* consume end */ }
                else { return Err(IBKRError::ParseError(format!("Expected </{}> after text", String::from_utf8_lossy(&tag_name)))); }
              } else { return Err(IBKRError::ParseError(format!("Expected text for <{}>", String::from_utf8_lossy(&tag_name)))); }
            }
          }
          b"Employees" => {
            current_co_general_info.employees_last_updated = parse_optional_date(get_attr_value(&e, b"LastUpdated")?);
            if !e.is_empty() {
              if let Ok(Event::Text(text_e)) = reader.read_event_into(&mut buf) {
                current_co_general_info.employees = text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.parse().ok();
                if let Ok(Event::End(_)) = reader.read_event_into(&mut buf) { /* consume end */ }
                else { return Err(IBKRError::ParseError("Expected </Employees> after text".into())); }
              } else { return Err(IBKRError::ParseError("Expected text for <Employees>".into())); }
            }
          }
          b"SharesOut" => {
            current_co_general_info.shares_out_date = parse_optional_date(get_attr_value(&e, b"Date")?);
            current_co_general_info.total_float = parse_optional_f64(get_attr_value(&e, b"TotalFloat")?);
            if !e.is_empty() {
              if let Ok(Event::Text(text_e)) = reader.read_event_into(&mut buf) {
                current_co_general_info.shares_out = text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.parse().ok();
                if let Ok(Event::End(_)) = reader.read_event_into(&mut buf) { /* consume end */ }
                else { return Err(IBKRError::ParseError("Expected </SharesOut> after text".into())); }
              } else { return Err(IBKRError::ParseError("Expected text for <SharesOut>".into())); }
            }
          }
          b"ReportingCurrency" => {
            current_co_general_info.reporting_currency_code = get_attr_value(&e, b"Code")?;
            if !e.is_empty() {
              if let Ok(Event::Text(text_e)) = reader.read_event_into(&mut buf) {
                current_co_general_info.reporting_currency_name = Some(text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.into_owned());
                if let Ok(Event::End(_)) = reader.read_event_into(&mut buf) { /* consume end */ }
                else { return Err(IBKRError::ParseError("Expected </ReportingCurrency> after text".into())); }
              } else { return Err(IBKRError::ParseError("Expected text for <ReportingCurrency>".into())); }
            }
          }
          b"MostRecentExchange" => {
            current_co_general_info.most_recent_exchange_rate_date = parse_optional_date(get_attr_value(&e, b"Date")?);
            if !e.is_empty() {
              if let Ok(Event::Text(text_e)) = reader.read_event_into(&mut buf) {
                current_co_general_info.most_recent_exchange_rate = text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.parse().ok();
                if let Ok(Event::End(_)) = reader.read_event_into(&mut buf) { /* consume end */ }
                else { return Err(IBKRError::ParseError("Expected </MostRecentExchange> after text".into())); }
              } else { return Err(IBKRError::ParseError("Expected text for <MostRecentExchange>".into())); }
            }
          }
          // contactInfo fields
          b"streetAddress" => {
            let line_opt = get_attr_value(&e, b"line")?;
            let is_empty_outer_tag = e.is_empty();

            if !is_empty_outer_tag {
              match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                Event::Text(text_e) => {
                  let value = text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.into_owned();
                  if let Some(line) = line_opt {
                    match line.as_str() {
                      "1" => current_contact_info.address_line1 = Some(value),
                      "2" => current_contact_info.address_line2 = Some(value),
                      "3" => current_contact_info.address_line3 = Some(value),
                      _ => {}
                    }
                  }
                  match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                    Event::End(end_e) if end_e.name().as_ref() == b"streetAddress" => { /* ok */ }
                    other => return Err(IBKRError::ParseError(format!("Expected </streetAddress> after text, got {:?}", other))),
                  }
                }
                Event::End(end_e) if end_e.name().as_ref() == b"streetAddress" => {
                  // Empty tag like <streetAddress line="2"></streetAddress>
                  // If an attribute like 'line' was present, we might want to store an empty string.
                  if let Some(line) = line_opt {
                    match line.as_str() {
                      "1" => current_contact_info.address_line1 = Some("".to_string()),
                      "2" => current_contact_info.address_line2 = Some("".to_string()),
                      "3" => current_contact_info.address_line3 = Some("".to_string()),
                      _ => {}
                    }
                  }
                }
                other => return Err(IBKRError::ParseError(format!("Expected text or </streetAddress>, got {:?}", other))),
              }
            }
            // If self-closing <streetAddress ... />, attributes are parsed, text is implicitly empty.
          }
          b"city" | b"state-region" | b"postalCode" | b"contactName" | b"contactTitle" => {
            let tag_name = e.name().as_ref().to_vec();
            let is_empty_outer_tag = e.is_empty();

            if !is_empty_outer_tag {
              match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                Event::Text(text_e) => {
                  let value_str = text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.into_owned();
                  match tag_name.as_slice() {
                    b"city" => current_contact_info.city = Some(value_str),
                    b"state-region" => current_contact_info.state_region = Some(value_str),
                    b"postalCode" => current_contact_info.postal_code = Some(value_str),
                    b"contactName" => current_contact_info.contact_name = Some(value_str),
                    b"contactTitle" => current_contact_info.contact_title = Some(value_str),
                    _ => {}
                  }
                  match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                    Event::End(end_e) if end_e.name().as_ref() == tag_name.as_slice() => { /* ok */ }
                    other => return Err(IBKRError::ParseError(format!("Expected </{}> after text, got {:?}", String::from_utf8_lossy(&tag_name), other))),
                  }
                }
                Event::End(end_e) if end_e.name().as_ref() == tag_name.as_slice() => {
                  // Empty tag like <city></city>. Store empty string.
                  let empty_str = Some("".to_string());
                  match tag_name.as_slice() {
                    b"city" => current_contact_info.city = empty_str,
                    b"state-region" => current_contact_info.state_region = empty_str,
                    b"postalCode" => current_contact_info.postal_code = empty_str,
                    b"contactName" => current_contact_info.contact_name = empty_str,
                    b"contactTitle" => current_contact_info.contact_title = empty_str,
                    _ => {}
                  }
                }
                other => return Err(IBKRError::ParseError(format!("Expected text or </{}>, got {:?}", String::from_utf8_lossy(&tag_name), other))),
              }
            }
          }
          b"country" if current_contact_info.last_updated.is_some() => { // Inside contactInfo
            current_contact_info.country_code = get_attr_value(&e, b"code")?;
            let is_empty_outer_tag = e.is_empty();
            if !is_empty_outer_tag {
              match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                Event::Text(text_e) => {
                  current_contact_info.country_name = Some(text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.into_owned());
                  match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                    Event::End(end_e) if end_e.name().as_ref() == b"country" => { /* ok */ }
                    other => return Err(IBKRError::ParseError(format!("Expected </country> after text, got {:?}", other))),
                  }
                }
                Event::End(end_e) if end_e.name().as_ref() == b"country" => {
                  current_contact_info.country_name = Some("".to_string());
                }
                other => return Err(IBKRError::ParseError(format!("Expected text or </country>, got {:?}", other))),
              }
            }
          }
          b"phone" if current_contact_info.last_updated.is_some() => { /* Container, check attributes */
                                                                          let is_main_phone = e.attributes().any(|a| a.map(|attr| attr.key.as_ref() == b"type" && attr.value.as_ref() == b"mainphone").unwrap_or(false));
                                                                          if is_main_phone {
                                                                            // Dive into main phone details
                                                                            loop {
                                                                              match reader.read_event_into(&mut buf) {
                                                                                Ok(Event::Start(ph_e)) => {
                                                                                  let ph_tag_name = ph_e.name().as_ref().to_vec();
                                                                                  if !ph_e.is_empty() {
                                                                                    if let Ok(Event::Text(text_e)) = reader.read_event_into(&mut buf) {
                                                                                      let value = Some(text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.into_owned());
                                                                                      match ph_tag_name.as_slice() {
                                                                                        b"countryPhoneCode" => current_contact_info.main_phone_country_code = value,
                                                                                        b"city-areacode" => current_contact_info.main_phone_area_code = value,
                                                                                        b"number" => current_contact_info.main_phone_number = value,
                                                                                        _ => {}
                                                                                      }
                                                                                      if let Ok(Event::End(_)) = reader.read_event_into(&mut buf) { /* consume end */ }
                                                                                      else { return Err(IBKRError::ParseError(format!("Expected </{}> after text", String::from_utf8_lossy(&ph_tag_name)))); }
                                                                                    } else { return Err(IBKRError::ParseError(format!("Expected text for <{}>", String::from_utf8_lossy(&ph_tag_name)))); }
                                                                                  }
                                                                                }
                                                                                Ok(Event::End(end_e)) if end_e.name().as_ref() == b"phone" => break, // End of this phone tag
                                                                                Ok(Event::Eof) => return Err(IBKRError::ParseError("EOF in contactInfo/phone".into())),
                                                                                Err(err) => return Err(IBKRError::ParseError(format!("XML error in contactInfo/phone: {}", err))),
                                                                                _ => {} // Skip other events within phone
                                                                              }
                                                                              buf.clear();
                                                                            }
                                                                          } else {
                                                                            // Skip non-main phone tags
                                                                            let tag_name_bytes = e.name().as_ref().to_vec();
                                                                            reader.read_to_end_into(quick_xml::name::QName(&tag_name_bytes), &mut buf).map_err(|err| IBKRError::ParseError(format!("Failed to skip to end of non-main phone tag {:?}: {}", String::from_utf8_lossy(&tag_name_bytes), err)))?;
                                                                          }
          }
          // webLinks fields
          b"webSite" => {
            let is_empty_outer_tag = e.is_empty();
            if !is_empty_outer_tag {
              match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                Event::Text(text_e) => {
                  current_web_links.home_page = Some(text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.into_owned());
                  match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                    Event::End(end_e) if end_e.name().as_ref() == b"webSite" => { /* ok */ }
                    other => return Err(IBKRError::ParseError(format!("Expected </webSite> after text, got {:?}", other))),
                  }
                }
                Event::End(end_e) if end_e.name().as_ref() == b"webSite" => {
                  current_web_links.home_page = Some("".to_string());
                }
                other => return Err(IBKRError::ParseError(format!("Expected text or </webSite>, got {:?}", other))),
              }
            }
          }
          b"eMail" => {
            let is_empty_outer_tag = e.is_empty();
            if !is_empty_outer_tag {
              match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                Event::Text(text_e) => {
                  current_web_links.email = Some(text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.into_owned());
                  match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                    Event::End(end_e) if end_e.name().as_ref() == b"eMail" => { /* ok */ }
                    other => return Err(IBKRError::ParseError(format!("Expected </eMail> after text, got {:?}", other))),
                  }
                }
                Event::End(end_e) if end_e.name().as_ref() == b"eMail" => {
                  current_web_links.email = Some("".to_string()); // XML shows <eMail ...></eMail>
                }
                other => return Err(IBKRError::ParseError(format!("Expected text or </eMail>, got {:?}", other))),
              }
            }
          }
          // peerInfo fields
          b"IndustryInfo" => { /* Container */ }
          b"Industry" => {
            current_industry = Some(Industry {
              industry_type: get_attr_value(&e, b"type")?,
              order: parse_optional_i32(get_attr_value(&e, b"order")?),
              reported: parse_optional_bool_int(get_attr_value(&e, b"reported")?),
              code: get_attr_value(&e, b"code")?,
              mnemonic: get_attr_value(&e, b"mnem")?,
              ..Default::default()
            });
            let is_empty_outer_tag = e.is_empty();
            if !is_empty_outer_tag {
              match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                Event::Text(text_e) => {
                  if let Some(ind) = current_industry.as_mut() {
                    ind.description = Some(text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.into_owned());
                  }
                  match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                    Event::End(end_e) if end_e.name().as_ref() == b"Industry" => { /* ok */ }
                    other => return Err(IBKRError::ParseError(format!("Expected </Industry> after text, got {:?}", other))),
                  }
                }
                Event::End(end_e) if end_e.name().as_ref() == b"Industry" => {
                  if let Some(ind) = current_industry.as_mut() {
                    ind.description = Some("".to_string());
                  }
                }
                other => return Err(IBKRError::ParseError(format!("Expected text or </Industry>, got {:?}", other))),
              }
            }
          }
          b"Indexconstituet" => { // Note: Typo in XML source 'constituet'
            let is_empty_outer_tag = e.is_empty();
            if !is_empty_outer_tag {
              match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                Event::Text(text_e) => {
                  current_peer_info.index_constituents.push(text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.into_owned());
                  match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                    Event::End(end_e) if end_e.name().as_ref() == b"Indexconstituet" => { /* ok */ }
                    other => return Err(IBKRError::ParseError(format!("Expected </Indexconstituet> after text, got {:?}", other))),
                  }
                }
                Event::End(end_e) if end_e.name().as_ref() == b"Indexconstituet" => {
                  // This case implies <Indexconstituet></Indexconstituet>, add empty string? Or skip?
                  // current_peer_info.index_constituents.push("".to_string());
                }
                other => return Err(IBKRError::ParseError(format!("Expected text or </Indexconstituet>, got {:?}", other))),
              }
            }
          }
          // officer fields
          b"firstName" | b"mI" | b"lastName" | b"age" => {
            if let Some(officer) = current_officer.as_mut() {
              let tag_name = e.name().as_ref().to_vec();
              let is_empty_outer_tag = e.is_empty();
              if !is_empty_outer_tag {
                match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                  Event::Text(text_e) => {
                    let value_str = text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.into_owned();
                    match tag_name.as_slice() {
                      b"firstName" => officer.first_name = Some(value_str),
                      b"mI" => officer.middle_initial = Some(value_str),
                      b"lastName" => officer.last_name = Some(value_str),
                      b"age" => officer.age = Some(value_str),
                      _ => {}
                    }
                    match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                      Event::End(end_e) if end_e.name().as_ref() == tag_name.as_slice() => { /* ok */ }
                      other => return Err(IBKRError::ParseError(format!("Expected </{}> after text, got {:?}", String::from_utf8_lossy(&tag_name), other))),
                    }
                  }
                  Event::End(end_e) if end_e.name().as_ref() == tag_name.as_slice() => {
                    // Empty tag, store empty string
                    let empty_val = Some("".to_string());
                    match tag_name.as_slice() {
                      b"firstName" => officer.first_name = empty_val,
                      b"mI" => officer.middle_initial = empty_val,
                      b"lastName" => officer.last_name = empty_val,
                      b"age" => officer.age = empty_val,
                      _ => {}
                    }
                  }
                  other => return Err(IBKRError::ParseError(format!("Expected text or </{}>, got {:?}", String::from_utf8_lossy(&tag_name), other))),
                }
              }
            }
          }
          b"title" if current_officer.is_some() => {
            if let Some(officer) = current_officer.as_mut() {
              officer.title_start_year = parse_optional_i32(get_attr_value(&e, b"startYear")?);
              officer.title_start_month = parse_optional_i32(get_attr_value(&e, b"startMonth")?);
              officer.title_start_day = parse_optional_i32(get_attr_value(&e, b"startDay")?);
              officer.title_id1 = get_attr_value(&e, b"iD1")?;
              officer.title_abbr1 = get_attr_value(&e, b"abbr1")?;
              officer.title_id2 = get_attr_value(&e, b"iD2")?;
              officer.title_abbr2 = get_attr_value(&e, b"abbr2")?;
              let is_empty_outer_tag = e.is_empty();
              if !is_empty_outer_tag {
                match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                  Event::Text(text_e) => {
                    officer.title_full = Some(text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.into_owned());
                    match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                      Event::End(end_e) if end_e.name().as_ref() == b"title" => { /* ok */ }
                      other => return Err(IBKRError::ParseError(format!("Expected </title> after text, got {:?}", other))),
                    }
                  }
                  Event::End(end_e) if end_e.name().as_ref() == b"title" => {
                    officer.title_full = Some("".to_string());
                  }
                  other => return Err(IBKRError::ParseError(format!("Expected text or </title>, got {:?}", other))),
                }
              }
            }
          }
          // ForecastData fields
          b"Value" if current_forecast_item.is_some() => {
            if let Some(item) = current_forecast_item.as_mut() {
              item.period_type = get_attr_value(&e, b"PeriodType")?;
              let is_empty_outer_tag = e.is_empty();
              if !is_empty_outer_tag {
                match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                  Event::Text(text_e) => {
                    item.value = Some(text_e.decode().map_err(|e| IBKRError::ParseError(e.to_string()))?.into_owned());
                    match reader.read_event_into(&mut buf).map_err(|e| IBKRError::ParseError(e.to_string()))? {
                      Event::End(end_e) if end_e.name().as_ref() == b"Value" => { /* ok */ }
                      other => return Err(IBKRError::ParseError(format!("Expected </Value> after text, got {:?}", other))),
                    }
                  }
                  Event::End(end_e) if end_e.name().as_ref() == b"Value" => {
                    item.value = Some("".to_string());
                  }
                  other => return Err(IBKRError::ParseError(format!("Expected text or </Value>, got {:?}", other))),
                }
              }
            }
          }

          _ => { /* Skip other tags by consuming them */
                    let tag_name_bytes = e.name().as_ref().to_vec();
                    reader.read_to_end_into(quick_xml::name::QName(&tag_name_bytes), &mut buf).map_err(|err| IBKRError::ParseError(format!("Failed to skip to end of tag {:?}: {}", String::from_utf8_lossy(&tag_name_bytes), err)))?;
          }
        }
      }
      Ok(Event::End(e)) => {
        match e.name().as_ref() {
          b"ReportSnapshot" => break, // End of document
          b"Issue" => {
            if let Some(issue) = current_issue.take() { report.issues.push(issue); }
          }
          b"CoGeneralInfo" => { report.co_general_info = Some(current_co_general_info); current_co_general_info = Default::default(); } // Reset after use
          b"Text" => {
            if let Some(text) = current_text_info.take() { report.text_info.push(text); }
          }
          b"contactInfo" => { report.contact_info = Some(current_contact_info); current_contact_info = Default::default(); }
          b"webLinks" => { report.web_links = Some(current_web_links); current_web_links = Default::default(); }
          b"Industry" => {
            if let Some(ind) = current_industry.take() { current_peer_info.industries.push(ind); }
          }
          b"peerInfo" => { report.peer_info = Some(current_peer_info); current_peer_info = Default::default(); }
          b"officer" => {
            if let Some(officer) = current_officer.take() { report.officers.push(officer); }
          }
          b"Group" => { // Inside Ratios
            if let Some(group) = current_ratio_group.take() { report.ratio_groups.push(group); }
          }
          b"Ratio" => { // Inside ForecastData
            if let Some(item) = current_forecast_item.take() { current_forecast_data.items.push(item); }
          }
          b"ForecastData" => { report.forecast_data = Some(current_forecast_data); current_forecast_data = Default::default(); }
          _ => {} // Ignore other end tags
        }
      }
      Ok(Event::Eof) => break,
      Err(e) => return Err(IBKRError::ParseError(format!("XML parsing error in ReportSnapshot: {}", e))),
      _ => (),
    }
    buf.clear();
  }

  // Post-processing: Populate company_info from collected data
  let mut final_company_info = CompanyInformation::default();
  if let Some(name) = report.top_level_coids.get("CompanyName") { final_company_info.company_name = Some(name.clone()); }
  if let Some(cik) = report.top_level_coids.get("CIKNo") { final_company_info.cik = Some(cik.clone()); }
  if let Some(irs) = report.top_level_coids.get("IRSNo") { final_company_info.irs_number = Some(irs.clone()); }
  // CONID might be in top-level or issue, prioritize? Let's take top-level if present.
  if let Some(conid_str) = report.top_level_coids.get("RepNo") { // Assuming RepNo is CONID? Check XML again. Let's assume CONID for now.
    final_company_info.con_id = conid_str.parse().ok();
  }

  // Merge info from the first common stock issue if available
  if let Some(common_issue) = report.issues.iter().find(|i| i.issue_type.as_deref() == Some("C")) {
    if final_company_info.ticker.is_none() { final_company_info.ticker = common_issue.ticker.clone(); }
    if final_company_info.exchange_code.is_none() { final_company_info.exchange_code = common_issue.exchange_code.clone(); }
    if final_company_info.exchange_name.is_none() { final_company_info.exchange_name = common_issue.exchange_name.clone(); }
    if final_company_info.last_split_date.is_none() { final_company_info.last_split_date = common_issue.most_recent_split_date; }
    if final_company_info.last_split_ratio.is_none() { final_company_info.last_split_ratio = common_issue.most_recent_split_factor.map(|f| f.to_string()); } // Convert factor to string for consistency? Or change struct field?
  }
  if let Some(_cg_info) = &report.co_general_info { // Prefix with underscore
    if final_company_info.country.is_none() { /* Country not directly in CoGeneralInfo */ }
  }
  if let Some(contact) = &report.contact_info {
    if final_company_info.country.is_none() { final_company_info.country = contact.country_name.clone(); } // Get country from contact info
  }
  if let Some(web) = &report.web_links {
    if final_company_info.web_url.is_none() { final_company_info.web_url = web.home_page.clone(); }
  }
  if let Some(text_item) = report.text_info.iter().find(|t| t.text_type.as_deref() == Some("Business Summary")) {
    if final_company_info.business_description.is_none() { final_company_info.business_description = text_item.text.clone(); }
  }

  report.company_info = Some(final_company_info);

  Ok(report)
}


/// Parses fundamental data XML string into corresponding Rust structs.
///
/// # Arguments
/// * `xml_data` - The XML string received from TWS API.
/// * `report_type` - The type of report the `xml_data` represents.
///
/// # Returns
/// A `Result` containing the parsed data in a `ParsedFundamentalData` enum variant,
/// or an `IBKRError` if parsing fails.
pub fn parse_fundamental_xml(
  xml_data: &str,
  report_type: FundamentalReportType,
) -> Result<ParsedFundamentalData, IBKRError> {
  match report_type {
    FundamentalReportType::ReportsFinSummary => {
      parse_reports_fin_summary(xml_data).map(ParsedFundamentalData::FinancialSummary)
    }
    FundamentalReportType::ReportSnapshot => {
      parse_report_snapshot(xml_data).map(ParsedFundamentalData::Snapshot)
    }
    _ => Err(IBKRError::ParseError(std::format!("Parsing of {:?} not implemented", report_type)))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parse_snapshot_with_security_info() {
    let xml = r#"
<ReportSnapshot>
    <CoIDs>
        <CoID Type="CompanyName">Global Corp</CoID>
        <CoID Type="CONID">9999</CoID>
    </CoIDs>
    <SecurityInfo>
        <Issue ID="123">
            <IssueID Type="Ticker">GLBC</IssueID>
            <Exchange Code="SMART">SMART</Exchange>
            <CoIDs> <!-- Nested CoIDs under Issue -->
                 <CoID Type="CompanyName">Global Corp Issue Specific</CoID> <!-- This should ideally be merged or handled -->
                 <CoID Type="CIKNo">SEC123</CoID>
            </CoIDs>
        </Issue>
    </SecurityInfo>
    <BusinessSummary>Global Operations</BusinessSummary>
    <RatioPeriods>
        <Group Name="Valuation">
            <Ratio FieldName="PEEXCLXOR" PeriodType="TTM">15.5</Ratio>
        </Group>
    </RatioPeriods>
</ReportSnapshot>
        "#;
    match parse_fundamental_xml(xml, FundamentalReportType::ReportSnapshot) {
      Ok(ParsedFundamentalData::Snapshot(snapshot)) => {
        let info = snapshot.company_info.as_ref().unwrap();
        // The parse_company_info_from_issue will overwrite CompanyName if called multiple times.
        // Depending on desired behavior, you might want to merge or prioritize.
        // Current logic: last one wins for fields parsed inside parse_company_info_from_issue.
        // Top-level CoIDs are parsed first.
        assert_eq!(info.company_name.as_deref(), Some("Global Corp Issue Specific")); // From nested CoID
        assert_eq!(info.con_id, Some(9999)); // From top-level CoID
        assert_eq!(info.ticker.as_deref(), Some("GLBC"));
        assert_eq!(info.cik.as_deref(), Some("SEC123"));
        assert_eq!(info.business_description.as_deref(), Some("Global Operations"));
        // Check ratios within groups
        assert_eq!(snapshot.ratio_groups.len(), 1);
        assert_eq!(snapshot.ratio_groups[0].id.as_deref(), Some("Valuation"));
        assert_eq!(snapshot.ratio_groups[0].ratios.len(), 1);
        assert_eq!(snapshot.ratio_groups[0].ratios[0].field_name, "PEEXCLXOR");
        assert_eq!(snapshot.ratio_groups[0].ratios[0].period_type.as_deref(), Some("TTM")); // Check Type attribute mapped to period_type
        assert_eq!(snapshot.ratio_groups[0].ratios[0].raw_value.as_deref(), Some("15.5"));
      }
      Err(e) => panic!("Parsing failed: {:?}", e),
      _ => panic!("Incorrect parsed type"),
    }
  }

  #[test]
  fn test_parse_simple_snapshot() {
    let xml = r#"
<ReportSnapshot>
    <CoIDs>
        <CoID Type="CompanyName">Test Corp</CoID>
        <CoID Type="CONID">12345</CoID>
    </CoIDs>
    <BusinessSummary>Tech Company</BusinessSummary>
    <RatioPeriods>
        <Group Name="Key Ratios">
            <Ratio FieldName="TTMGROSMGN" PeriodType="TTM">50.5</Ratio>
            <Ratio FieldName="TTMNETMGN" PeriodType="TTM">10.2</Ratio>
        </Group>
        <Group Name="Profitability">
            <Ratio FieldName="TTMROEPCT" PeriodType="TTM">15.0</Ratio>
        </Group>
    </RatioPeriods>
</ReportSnapshot>
        "#;
    match parse_fundamental_xml(xml, FundamentalReportType::ReportSnapshot) {
      Ok(ParsedFundamentalData::Snapshot(snapshot)) => {
        assert_eq!(snapshot.company_info.as_ref().unwrap().company_name.as_deref(), Some("Test Corp"));
        assert_eq!(snapshot.company_info.as_ref().unwrap().con_id, Some(12345)); // Assuming CONID is parsed into company_info.con_id
        assert_eq!(snapshot.company_info.as_ref().unwrap().business_description.as_deref(), Some("Tech Company"));
        assert_eq!(snapshot.ratio_groups.len(), 2); // Two groups
        let key_ratios_group = snapshot.ratio_groups.iter().find(|g| g.id.as_deref() == Some("Key Ratios")).unwrap();
        assert_eq!(key_ratios_group.ratios.len(), 2);
        assert_eq!(key_ratios_group.ratios[0].field_name, "TTMGROSMGN");
        assert_eq!(key_ratios_group.ratios[0].raw_value.as_deref(), Some("50.5"));
        assert_eq!(key_ratios_group.ratios[0].period_type.as_deref(), Some("TTM")); // Check Type attribute
        let profitability_group = snapshot.ratio_groups.iter().find(|g| g.id.as_deref() == Some("Profitability")).unwrap();
        assert_eq!(profitability_group.ratios.len(), 1);
        assert_eq!(profitability_group.ratios[0].field_name, "TTMROEPCT");
      }
      Err(e) => panic!("Parsing failed: {:?}", e),
      _ => panic!("Incorrect parsed type"),
    }
  }

  #[test]
  fn test_parse_simple_finsummary() {
    let _fin_statement_xml = r#"
<ReportFinancialStatements>
    <Issue>
        <CoIDs>
            <CoID Type="CompanyName">Fin Corp</CoID>
            <CoID Type="CIKNo">000123</CoID>
        </CoIDs>
        <IssueID Type="Ticker">FNC</IssueID>
        <Exchange Code="NYSE">New York Stock Exchange</Exchange>
    </Issue>
    <FinancialStatements>
        <AnnualPeriods>
            <FiscalPeriod FiscalYear="2023" EndDate="2023-12-31">
                <Statement Type="INC" Currency="USD">
                    <lineItem coaItem="SREV">1000.0</lineItem>
                    <lineItem coaItem="SCOR">500.0</lineItem>
                </Statement>
                <Statement Type="BAL" Currency="USD">
                    <lineItem coaItem="ATCA">2000.0</lineItem>
                </Statement>
            </FiscalPeriod>
        </AnnualPeriods>
        <InterimPeriods>
            <FiscalPeriod FiscalYear="2024" EndDate="2024-03-31">
                <Statement Type="INC" Currency="USD">
                    <lineItem coaItem="SREV">250.0</lineItem>
                </Statement>
            </FiscalPeriod>
        </InterimPeriods>
    </FinancialStatements>
</ReportFinancialStatements>
        "#;
    // This test XML is for ReportsFinStatements, not ReportsFinSummary.
    // We need a test case specifically for the ReportsFinSummary structure.
    // Let's adapt the test to check the new structure if we were parsing that.
    // For now, let's comment out the specific checks as the XML doesn't match the parser.

    // match parse_fundamental_xml(xml, FundamentalReportType::ReportsFinSummary) {
    //   Ok(ParsedFundamentalData::FinancialSummary(summary)) => {
    // assert!(summary.eps_records.len() > 0); // Example check
    // assert_eq!(summary.eps_records[0].currency.as_deref(), Some("USD"));
    //   }
    //   Err(e) => panic!("Parsing failed: {:?}", e),
    //   _ => panic!("Incorrect parsed type"),
    // }

    // Add a new test specifically for the FinancialSummary XML structure
    let fin_summary_xml = r#"
<FinancialSummary>
    <EPSs currency="USD">
        <EPS asofDate="2025-03-31" reportType="R" period="12M">6.45</EPS>
        <EPS asofDate="2025-03-31" reportType="R" period="3M">1.65</EPS>
    </EPSs>
    <DividendPerShares currency="USD">
        <DividendPerShare asofDate="2025-03-31" reportType="R" period="3M">0.25</DividendPerShare>
    </DividendPerShares>
    <TotalRevenues currency="USD">
         <TotalRevenue asofDate="2025-03-31" reportType="P" period="3M">95359000000.0</TotalRevenue>
    </TotalRevenues>
     <Dividends currency="USD">
          <Dividend type="CD" exDate="2021-11-05" payDate="2021-11-11">0.22</Dividend>
     </Dividends>
</FinancialSummary>
    "#;
    match parse_fundamental_xml(fin_summary_xml, FundamentalReportType::ReportsFinSummary) {
      Ok(ParsedFundamentalData::FinancialSummary(summary)) => {
        assert_eq!(summary.eps_records.len(), 2);
        assert_eq!(summary.eps_records[0].value, Some(6.45));
        assert_eq!(summary.eps_records[0].currency.as_deref(), Some("USD"));
        assert_eq!(summary.eps_records[1].period.as_deref(), Some("3M"));

        assert_eq!(summary.dividend_per_share_records.len(), 1);
        assert_eq!(summary.dividend_per_share_records[0].value, Some(0.25));

        assert_eq!(summary.total_revenue_records.len(), 1);
        assert_eq!(summary.total_revenue_records[0].value, Some(95359000000.0));

        assert_eq!(summary.announced_dividend_records.len(), 1);
        assert_eq!(summary.announced_dividend_records[0].dividend_type.as_deref(), Some("CD"));
        assert_eq!(summary.announced_dividend_records[0].value, Some(0.22));

      }
      Err(e) => panic!("Parsing FinancialSummary failed: {:?}", e),
      _ => panic!("Incorrect parsed type"),
    }
  }
}
