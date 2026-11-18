use crate::base::IBKRError;
use crate::data::{
  AnnouncedDividendRecord, CoGeneralInfo, CompanyInformation, ContactInfo, DividendPerShareRecord,
  EPSRecord, ForecastData, ForecastDataItem, FundamentalReportType, Industry, Issue, Officer,
  ParsedFundamentalData, PeerInfo, Ratio, RatioGroup, ReportSnapshot, ReportsFinSummary,
  TextInfoItem, TotalRevenueRecord, WebLink,
};
use chrono::{NaiveDate, TimeZone};
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

// Helper to extract normalized/unescaped attribute value
fn get_attr_value(e: &BytesStart<'_>, key: &str) -> Result<Option<String>, IBKRError> {
  for attr_result in e.attributes() {
    let attr = attr_result
      .map_err(|err| IBKRError::ParseError(format!("XML attribute parsing error: {}", err)))?;
    if attr.key.as_ref() == key {
      let unescaped_value_cow = attr
        .normalized_value(XmlVersion::Implicit1_0)
        .map_err(|err| IBKRError::ParseError(format!("Attribute value unescape error: {}", err)))?;
      return Ok(Some(unescaped_value_cow.into_owned()));
    }
  }
  Ok(None)
}

/// Reads and unescapes text content up to (and consuming) the matching closing tag of `start`.
/// Requires `reader.config_mut().expand_empty_elements = true` so self-closing `<Tag />` also works.
fn read_tag_text(reader: &mut Reader<&[u8]>, start: &BytesStart<'_>) -> Result<String, IBKRError> {
  let raw = reader
    .read_text(start.name())
    .map_err(|e| IBKRError::ParseError(e.to_string()))?;
  let unescaped = quick_xml::escape::unescape(&raw)
    .map_err(|e| IBKRError::ParseError(e.to_string()))?;
  Ok(unescaped.trim().to_string())
}

fn parse_optional_f64(opt_str: Option<String>) -> Option<f64> {
  opt_str.as_deref().and_then(|s| s.parse::<f64>().ok())
}

fn parse_optional_i32(opt_str: Option<String>) -> Option<i32> {
  opt_str.as_deref().and_then(|s| s.parse::<i32>().ok())
}

fn parse_optional_date(opt_str: Option<String>) -> Option<NaiveDate> {
  opt_str
    .as_deref()
    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
}

fn parse_optional_datetime(opt_str: Option<String>) -> Option<chrono::DateTime<chrono::Utc>> {
  opt_str.as_deref().and_then(|s| {
    chrono::DateTime::parse_from_rfc3339(s)
      .map(|dt| dt.with_timezone(&chrono::Utc))
      .or_else(|_| {
        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
          .map(|ndt| chrono::Utc.from_utc_datetime(&ndt))
      })
      .ok()
  })
}

fn parse_optional_bool_int(opt_str: Option<String>) -> Option<bool> {
  opt_str.as_deref().and_then(|s| match s {
    "1" => Some(true),
    "0" => Some(false),
    _ => None,
  })
}

fn parse_reports_fin_summary(xml_data: &str) -> Result<ReportsFinSummary, IBKRError> {
  let mut reader = Reader::from_str(xml_data);
  reader.config_mut().trim_text(true);
  reader.config_mut().expand_empty_elements = true;
  let mut report = ReportsFinSummary::default();
  let mut current_section_currency: Option<String> = None;

  loop {
    match reader.read_event() {
      Ok(Event::Start(e)) => match e.name().as_ref() {
        "FinancialSummary" => {}
        "EPSs" | "DividendPerShares" | "TotalRevenues" | "Dividends" => {
          current_section_currency = get_attr_value(&e, "currency")?;
        }
        "EPS" => {
          let mut record = EPSRecord {
            currency: current_section_currency.clone(),
            ..Default::default()
          };
          record.as_of_date = parse_optional_date(get_attr_value(&e, "asofDate")?);
          record.report_type = get_attr_value(&e, "reportType")?;
          record.period = get_attr_value(&e, "period")?;
          let text = read_tag_text(&mut reader, &e)?;
          if !text.is_empty() {
            record.value = text.parse().ok();
          }
          report.eps_records.push(record);
        }
        "DividendPerShare" => {
          let mut record = DividendPerShareRecord {
            currency: current_section_currency.clone(),
            ..Default::default()
          };
          record.as_of_date = parse_optional_date(get_attr_value(&e, "asofDate")?);
          record.report_type = get_attr_value(&e, "reportType")?;
          record.period = get_attr_value(&e, "period")?;
          let text = read_tag_text(&mut reader, &e)?;
          if !text.is_empty() {
            record.value = text.parse().ok();
          }
          report.dividend_per_share_records.push(record);
        }
        "TotalRevenue" => {
          let mut record = TotalRevenueRecord {
            currency: current_section_currency.clone(),
            ..Default::default()
          };
          record.as_of_date = parse_optional_date(get_attr_value(&e, "asofDate")?);
          record.report_type = get_attr_value(&e, "reportType")?;
          record.period = get_attr_value(&e, "period")?;
          let text = read_tag_text(&mut reader, &e)?;
          if !text.is_empty() {
            record.value = text.parse().ok();
          }
          report.total_revenue_records.push(record);
        }
        "Dividend" => {
          let mut record = AnnouncedDividendRecord {
            currency: current_section_currency.clone(),
            ..Default::default()
          };
          record.dividend_type = get_attr_value(&e, "type")?;
          record.ex_date = parse_optional_date(get_attr_value(&e, "exDate")?);
          record.record_date = parse_optional_date(get_attr_value(&e, "recordDate")?);
          record.pay_date = parse_optional_date(get_attr_value(&e, "payDate")?);
          record.declaration_date = parse_optional_date(get_attr_value(&e, "declarationDate")?);
          let text = read_tag_text(&mut reader, &e)?;
          if !text.is_empty() {
            record.value = text.parse().ok();
          }
          report.announced_dividend_records.push(record);
        }
        _ => {
          let tag_name = e.name();
          reader.read_to_end(tag_name).map_err(|err| {
            IBKRError::ParseError(format!(
              "Failed to skip to end of tag {:?}: {}",
              tag_name.as_ref(),
              err
            ))
          })?;
        }
      },
      Ok(Event::End(e)) => match e.name().as_ref() {
        "FinancialSummary" => break,
        "EPSs" | "DividendPerShares" | "TotalRevenues" | "Dividends" => {
          current_section_currency = None;
        }
        _ => {}
      },
      Ok(Event::Eof) => break,
      Err(err) => {
        return Err(IBKRError::ParseError(format!(
          "XML parsing error in ReportsFinSummary: {}",
          err
        )))
      }
      _ => (),
    }
  }
  Ok(report)
}

fn parse_report_snapshot(xml_data: &str) -> Result<ReportSnapshot, IBKRError> {
  let mut reader = Reader::from_str(xml_data);
  reader.config_mut().trim_text(true);
  reader.config_mut().expand_empty_elements = true;
  let mut report = ReportSnapshot::default();
  let mut current_issue: Option<Issue> = None;
  let mut current_officer: Option<Officer> = None;
  let mut current_ratio_group: Option<RatioGroup> = None;
  let mut current_forecast_item: Option<ForecastDataItem> = None;
  let mut current_contact_info = ContactInfo::default();
  let mut current_web_links = WebLink::default();
  let mut current_peer_info = PeerInfo::default();
  let mut current_co_general_info = CoGeneralInfo::default();
  let mut current_forecast_data = ForecastData::default();

  loop {
    match reader.read_event() {
      Ok(Event::Start(e)) => match e.name().as_ref() {
        "ReportSnapshot" | "Issues" | "SecurityInfo" | "TextInfo" | "officers" | "IndustryInfo" | "RatioPeriods" => {}
        "CoIDs" => loop {
          match reader.read_event() {
            Ok(Event::Start(coid_e)) if coid_e.name().as_ref() == "CoID" => {
              let coid_type_opt = get_attr_value(&coid_e, "Type")?;
              let val = read_tag_text(&mut reader, &coid_e)?;
              if let Some(coid_type) = coid_type_opt {
                report.top_level_coids.insert(coid_type, val);
              }
            }
            Ok(Event::End(end_e)) if end_e.name().as_ref() == "CoIDs" => break,
            Ok(Event::Eof) => return Err(IBKRError::ParseError("EOF in CoIDs".into())),
            Err(err) => return Err(IBKRError::ParseError(format!("XML error in CoIDs: {}", err))),
            _ => {}
          }
        },
        "Issue" => {
          current_issue = Some(Issue {
            id: get_attr_value(&e, "ID")?,
            issue_type: get_attr_value(&e, "Type")?,
            description: get_attr_value(&e, "Desc")?,
            order: parse_optional_i32(get_attr_value(&e, "Order")?),
            ..Default::default()
          });
        }
        "CoGeneralInfo" => {
          current_co_general_info.last_modified = parse_optional_date(get_attr_value(&e, "LastModified")?);
        }
        "Text" => {
          let text_type = get_attr_value(&e, "Type")?;
          let last_modified = parse_optional_datetime(get_attr_value(&e, "lastModified")?);
          let text = read_tag_text(&mut reader, &e)?;
          report.text_info.push(TextInfoItem {
            text_type,
            last_modified,
            text: if text.is_empty() { None } else { Some(text) },
          });
        }
        "BusinessSummary" => {
          let text = read_tag_text(&mut reader, &e)?;
          if !text.is_empty() {
            report.text_info.push(TextInfoItem {
              text_type: Some("Business Summary".to_string()),
              last_modified: None,
              text: Some(text),
            });
          }
        }
        "contactInfo" => {
          current_contact_info.last_updated = parse_optional_datetime(get_attr_value(&e, "lastUpdated")?);
        }
        "webLinks" => {
          current_web_links.last_updated = parse_optional_datetime(get_attr_value(&e, "lastUpdated")?);
        }
        "peerInfo" => {
          current_peer_info.last_updated = parse_optional_datetime(get_attr_value(&e, "lastUpdated")?);
        }
        "officer" => {
          current_officer = Some(Officer {
            rank: parse_optional_i32(get_attr_value(&e, "rank")?),
            since: get_attr_value(&e, "since")?,
            ..Default::default()
          });
        }
        "Ratios" => {
          report.ratios_price_currency = get_attr_value(&e, "PriceCurrency")?;
          report.ratios_reporting_currency = get_attr_value(&e, "ReportingCurrency")?;
          report.ratios_exchange_rate = parse_optional_f64(get_attr_value(&e, "ExchangeRate")?);
          report.ratios_latest_available_date = parse_optional_date(get_attr_value(&e, "LatestAvailableDate")?);
        }
        "Group" => {
          let group_id = match get_attr_value(&e, "ID")? {
            Some(id) => Some(id),
            None => get_attr_value(&e, "Name")?,
          };
          current_ratio_group = Some(RatioGroup {
            id: group_id,
            ..Default::default()
          });
        }
        "Ratio" => {
          if current_ratio_group.is_some() {
            let period_type = match get_attr_value(&e, "Type")? {
              Some(t) => Some(t),
              None => get_attr_value(&e, "PeriodType")?,
            };
            let mut ratio = Ratio {
              group_name: current_ratio_group
                .as_ref()
                .and_then(|g| g.id.clone())
                .unwrap_or_default(),
              field_name: get_attr_value(&e, "FieldName")?.unwrap_or_default(),
              period_type,
              ..Default::default()
            };
            let val = read_tag_text(&mut reader, &e)?;
            if !val.is_empty() {
              ratio.raw_value = Some(val);
            }
            if let Some(group) = current_ratio_group.as_mut() {
              group.ratios.push(ratio);
            }
          } else if current_forecast_data.consensus_type.is_some() {
            current_forecast_item = Some(ForecastDataItem {
              field_name: get_attr_value(&e, "FieldName")?.unwrap_or_default(),
              value_type: get_attr_value(&e, "Type")?,
              ..Default::default()
            });
          } else {
            reader
              .read_to_end(e.name())
              .map_err(|err| IBKRError::ParseError(format!("Failed to skip <Ratio>: {}", err)))?;
          }
        }
        "ForecastData" => {
          current_forecast_data.consensus_type = get_attr_value(&e, "ConsensusType")?;
          current_forecast_data.cur_fiscal_year = parse_optional_i32(get_attr_value(&e, "CurFiscalYear")?);
          current_forecast_data.cur_fiscal_year_end_month = parse_optional_i32(get_attr_value(&e, "CurFiscalYearEndMonth")?);
          current_forecast_data.cur_interim_end_cal_year = parse_optional_i32(get_attr_value(&e, "CurInterimEndCalYear")?);
          current_forecast_data.cur_interim_end_month = parse_optional_i32(get_attr_value(&e, "CurInterimEndMonth")?);
          current_forecast_data.earnings_basis = get_attr_value(&e, "EarningsBasis")?;
        }
        "IssueID" if current_issue.is_some() => {
          let id_type_opt = get_attr_value(&e, "Type")?;
          let value = read_tag_text(&mut reader, &e)?;
          if !value.is_empty() {
            if let (Some(issue), Some(id_type)) = (current_issue.as_mut(), id_type_opt) {
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
            }
          }
        }
        "Exchange" if current_issue.is_some() => {
          let code = get_attr_value(&e, "Code")?;
          let country = get_attr_value(&e, "Country")?;
          let val = read_tag_text(&mut reader, &e)?;
          if let Some(issue) = current_issue.as_mut() {
            issue.exchange_code = code;
            issue.exchange_country = country;
            if !val.is_empty() {
              issue.exchange_name = Some(val);
            }
          }
        }
        "GlobalListingType" if current_issue.is_some() => {
          let val = read_tag_text(&mut reader, &e)?;
          if !val.is_empty() {
            if let Some(issue) = current_issue.as_mut() {
              issue.global_listing_type = Some(val);
            }
          }
        }
        "MostRecentSplit" if current_issue.is_some() => {
          let split_date = parse_optional_date(get_attr_value(&e, "Date")?);
          let val = read_tag_text(&mut reader, &e)?;
          if let Some(issue) = current_issue.as_mut() {
            issue.most_recent_split_date = split_date;
            if !val.is_empty() {
              issue.most_recent_split_factor = val.parse().ok();
            }
          }
        }
        "CoStatus" => {
          current_co_general_info.co_status_code = parse_optional_i32(get_attr_value(&e, "Code")?);
          let val = read_tag_text(&mut reader, &e)?;
          if !val.is_empty() {
            current_co_general_info.co_status_desc = Some(val);
          }
        }
        "CoType" => {
          current_co_general_info.co_type_code = get_attr_value(&e, "Code")?;
          let val = read_tag_text(&mut reader, &e)?;
          if !val.is_empty() {
            current_co_general_info.co_type_desc = Some(val);
          }
        }
        "LatestAvailableAnnual" | "LatestAvailableInterim" => {
          let is_annual = e.name().as_ref() == "LatestAvailableAnnual";
          let val = read_tag_text(&mut reader, &e)?;
          if !val.is_empty() {
            let date_val = parse_optional_date(Some(val));
            if is_annual {
              current_co_general_info.latest_available_annual = date_val;
            } else {
              current_co_general_info.latest_available_interim = date_val;
            }
          }
        }
        "Employees" => {
          current_co_general_info.employees_last_updated = parse_optional_date(get_attr_value(&e, "LastUpdated")?);
          let val = read_tag_text(&mut reader, &e)?;
          if !val.is_empty() {
            current_co_general_info.employees = val.parse().ok();
          }
        }
        "SharesOut" => {
          current_co_general_info.shares_out_date = parse_optional_date(get_attr_value(&e, "Date")?);
          current_co_general_info.total_float = parse_optional_f64(get_attr_value(&e, "TotalFloat")?);
          let val = read_tag_text(&mut reader, &e)?;
          if !val.is_empty() {
            current_co_general_info.shares_out = val.parse().ok();
          }
        }
        "ReportingCurrency" => {
          current_co_general_info.reporting_currency_code = get_attr_value(&e, "Code")?;
          let val = read_tag_text(&mut reader, &e)?;
          if !val.is_empty() {
            current_co_general_info.reporting_currency_name = Some(val);
          }
        }
        "MostRecentExchange" => {
          current_co_general_info.most_recent_exchange_rate_date = parse_optional_date(get_attr_value(&e, "Date")?);
          let val = read_tag_text(&mut reader, &e)?;
          if !val.is_empty() {
            current_co_general_info.most_recent_exchange_rate = val.parse().ok();
          }
        }
        "streetAddress" => {
          let line_opt = get_attr_value(&e, "line")?;
          let value = read_tag_text(&mut reader, &e)?;
          if let Some(line) = line_opt {
            match line.as_str() {
              "1" => current_contact_info.address_line1 = Some(value),
              "2" => current_contact_info.address_line2 = Some(value),
              "3" => current_contact_info.address_line3 = Some(value),
              _ => {}
            }
          }
        }
        "city" | "state-region" | "postalCode" | "contactName" | "contactTitle" => {
          let tag_name = e.name().as_ref().to_string();
          let value_str = read_tag_text(&mut reader, &e)?;
          match tag_name.as_str() {
            "city" => current_contact_info.city = Some(value_str),
            "state-region" => current_contact_info.state_region = Some(value_str),
            "postalCode" => current_contact_info.postal_code = Some(value_str),
            "contactName" => current_contact_info.contact_name = Some(value_str),
            "contactTitle" => current_contact_info.contact_title = Some(value_str),
            _ => {}
          }
        }
        "country" if current_contact_info.last_updated.is_some() => {
          current_contact_info.country_code = get_attr_value(&e, "code")?;
          current_contact_info.country_name = Some(read_tag_text(&mut reader, &e)?);
        }
        "phone" if current_contact_info.last_updated.is_some() => {
          let is_main_phone = get_attr_value(&e, "type")?.as_deref() == Some("mainphone");
          if is_main_phone {
            loop {
              match reader.read_event() {
                Ok(Event::Start(ph_e)) => {
                  let ph_tag_name = ph_e.name().as_ref().to_string();
                  let val = read_tag_text(&mut reader, &ph_e)?;
                  if !val.is_empty() {
                    match ph_tag_name.as_str() {
                      "countryPhoneCode" => current_contact_info.main_phone_country_code = Some(val),
                      "city-areacode" => current_contact_info.main_phone_area_code = Some(val),
                      "number" => current_contact_info.main_phone_number = Some(val),
                      _ => {}
                    }
                  }
                }
                Ok(Event::End(end_e)) if end_e.name().as_ref() == "phone" => break,
                Ok(Event::Eof) => return Err(IBKRError::ParseError("EOF in contactInfo/phone".into())),
                Err(err) => return Err(IBKRError::ParseError(format!("XML error in contactInfo/phone: {}", err))),
                _ => {}
              }
            }
          } else {
            let tag_name = e.name();
            reader.read_to_end(tag_name).map_err(|err| {
              IBKRError::ParseError(format!(
                "Failed to skip to end of non-main phone tag {:?}: {}",
                tag_name.as_ref(),
                err
              ))
            })?;
          }
        }
        "webSite" => {
          current_web_links.home_page = Some(read_tag_text(&mut reader, &e)?);
        }
        "eMail" => {
          current_web_links.email = Some(read_tag_text(&mut reader, &e)?);
        }
        "Industry" => {
          let mut ind = Industry {
            industry_type: get_attr_value(&e, "type")?,
            order: parse_optional_i32(get_attr_value(&e, "order")?),
            reported: parse_optional_bool_int(get_attr_value(&e, "reported")?),
            code: get_attr_value(&e, "code")?,
            mnemonic: get_attr_value(&e, "mnem")?,
            ..Default::default()
          };
          ind.description = Some(read_tag_text(&mut reader, &e)?);
          current_peer_info.industries.push(ind);
        }
        "Indexconstituet" => {
          let val = read_tag_text(&mut reader, &e)?;
          if !val.is_empty() {
            current_peer_info.index_constituents.push(val);
          }
        }
        "firstName" | "mI" | "lastName" | "age" if current_officer.is_some() => {
          let tag_name = e.name().as_ref().to_string();
          let value_str = read_tag_text(&mut reader, &e)?;
          if let Some(officer) = current_officer.as_mut() {
            match tag_name.as_str() {
              "firstName" => officer.first_name = Some(value_str),
              "mI" => officer.middle_initial = Some(value_str),
              "lastName" => officer.last_name = Some(value_str),
              "age" => officer.age = Some(value_str),
              _ => {}
            }
          }
        }
        "title" if current_officer.is_some() => {
          let start_year = parse_optional_i32(get_attr_value(&e, "startYear")?);
          let start_month = parse_optional_i32(get_attr_value(&e, "startMonth")?);
          let start_day = parse_optional_i32(get_attr_value(&e, "startDay")?);
          let id1 = get_attr_value(&e, "iD1")?;
          let abbr1 = get_attr_value(&e, "abbr1")?;
          let id2 = get_attr_value(&e, "iD2")?;
          let abbr2 = get_attr_value(&e, "abbr2")?;
          let title_full = read_tag_text(&mut reader, &e)?;
          if let Some(officer) = current_officer.as_mut() {
            officer.title_start_year = start_year;
            officer.title_start_month = start_month;
            officer.title_start_day = start_day;
            officer.title_id1 = id1;
            officer.title_abbr1 = abbr1;
            officer.title_id2 = id2;
            officer.title_abbr2 = abbr2;
            officer.title_full = Some(title_full);
          }
        }
        "Value" if current_forecast_item.is_some() => {
          let period_type = get_attr_value(&e, "PeriodType")?;
          let val = read_tag_text(&mut reader, &e)?;
          if let Some(item) = current_forecast_item.as_mut() {
            item.period_type = period_type;
            item.value = Some(val);
          }
        }
        _ => {
          let tag_name = e.name();
          reader.read_to_end(tag_name).map_err(|err| {
            IBKRError::ParseError(format!(
              "Failed to skip to end of tag {:?}: {}",
              tag_name.as_ref(),
              err
            ))
          })?;
        }
      },
      Ok(Event::End(e)) => match e.name().as_ref() {
        "ReportSnapshot" => break,
        "Issue" => {
          if let Some(issue) = current_issue.take() {
            report.issues.push(issue);
          }
        }
        "CoGeneralInfo" => {
          report.co_general_info = Some(current_co_general_info);
          current_co_general_info = Default::default();
        }
        "contactInfo" => {
          report.contact_info = Some(current_contact_info);
          current_contact_info = Default::default();
        }
        "webLinks" => {
          report.web_links = Some(current_web_links);
          current_web_links = Default::default();
        }
        "peerInfo" => {
          report.peer_info = Some(current_peer_info);
          current_peer_info = Default::default();
        }
        "officer" => {
          if let Some(officer) = current_officer.take() {
            report.officers.push(officer);
          }
        }
        "Group" => {
          if let Some(group) = current_ratio_group.take() {
            report.ratio_groups.push(group);
          }
        }
        "Ratio" => {
          if let Some(item) = current_forecast_item.take() {
            current_forecast_data.items.push(item);
          }
        }
        "ForecastData" => {
          report.forecast_data = Some(current_forecast_data);
          current_forecast_data = Default::default();
        }
        _ => {}
      },
      Ok(Event::Eof) => break,
      Err(e) => {
        return Err(IBKRError::ParseError(format!(
          "XML parsing error in ReportSnapshot: {}",
          e
        )))
      }
      _ => (),
    }
  }

  let mut final_company_info = CompanyInformation::default();
  if let Some(name) = report.top_level_coids.get("CompanyName") {
    final_company_info.company_name = Some(name.clone());
  }
  if let Some(cik) = report.top_level_coids.get("CIKNo") {
    final_company_info.cik = Some(cik.clone());
  }
  if let Some(irs) = report.top_level_coids.get("IRSNo") {
    final_company_info.irs_number = Some(irs.clone());
  }
  if let Some(conid_str) = report
    .top_level_coids
    .get("CONID")
    .or_else(|| report.top_level_coids.get("RepNo"))
  {
    final_company_info.con_id = conid_str.parse().ok();
  }

  if let Some(common_issue) = report
    .issues
    .iter()
    .find(|i| i.issue_type.as_deref() == Some("C"))
    .or_else(|| report.issues.first())
  {
    if final_company_info.ticker.is_none() {
      final_company_info.ticker = common_issue.ticker.clone();
    }
    if final_company_info.exchange_code.is_none() {
      final_company_info.exchange_code = common_issue.exchange_code.clone();
    }
    if final_company_info.exchange_name.is_none() {
      final_company_info.exchange_name = common_issue.exchange_name.clone();
    }
    if final_company_info.last_split_date.is_none() {
      final_company_info.last_split_date = common_issue.most_recent_split_date;
    }
    if final_company_info.last_split_ratio.is_none() {
      final_company_info.last_split_ratio = common_issue.most_recent_split_factor.map(|f| f.to_string());
    }
  }
  if let Some(contact) = &report.contact_info {
    if final_company_info.country.is_none() {
      final_company_info.country = contact.country_name.clone();
    }
  }
  if let Some(web) = &report.web_links {
    if final_company_info.web_url.is_none() {
      final_company_info.web_url = web.home_page.clone();
    }
  }
  if let Some(text_item) = report
    .text_info
    .iter()
    .find(|t| t.text_type.as_deref() == Some("Business Summary"))
  {
    if final_company_info.business_description.is_none() {
      final_company_info.business_description = text_item.text.clone();
    }
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
    _ => Err(IBKRError::ParseError(std::format!(
      "Parsing of {:?} not implemented",
      report_type
    ))),
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
                 <CoID Type="CompanyName">Global Corp Issue Specific</CoID>
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
        assert_eq!(info.company_name.as_deref(), Some("Global Corp Issue Specific"));
        assert_eq!(info.con_id, Some(9999));
        assert_eq!(info.ticker.as_deref(), Some("GLBC"));
        assert_eq!(info.cik.as_deref(), Some("SEC123"));
        assert_eq!(info.business_description.as_deref(), Some("Global Operations"));
        assert_eq!(snapshot.ratio_groups.len(), 1);
        assert_eq!(snapshot.ratio_groups[0].id.as_deref(), Some("Valuation"));
        assert_eq!(snapshot.ratio_groups[0].ratios.len(), 1);
        assert_eq!(snapshot.ratio_groups[0].ratios[0].field_name, "PEEXCLXOR");
        assert_eq!(snapshot.ratio_groups[0].ratios[0].period_type.as_deref(), Some("TTM"));
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
        assert_eq!(snapshot.company_info.as_ref().unwrap().con_id, Some(12345));
        assert_eq!(snapshot.company_info.as_ref().unwrap().business_description.as_deref(), Some("Tech Company"));
        assert_eq!(snapshot.ratio_groups.len(), 2);
        let key_ratios_group = snapshot.ratio_groups.iter().find(|g| g.id.as_deref() == Some("Key Ratios")).unwrap();
        assert_eq!(key_ratios_group.ratios.len(), 2);
        assert_eq!(key_ratios_group.ratios[0].field_name, "TTMGROSMGN");
        assert_eq!(key_ratios_group.ratios[0].raw_value.as_deref(), Some("50.5"));
        assert_eq!(key_ratios_group.ratios[0].period_type.as_deref(), Some("TTM"));
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
