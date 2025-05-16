// yatws/test_fin.rs
use anyhow::{anyhow, Result};
use log::{error, info, warn};
use yatws::{
  IBKRClient,
  contract::Contract,
  data::{FundamentalReportType, ParsedFundamentalData},
  parse_fundamental_xml
};

pub(super) fn financial_reports_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
  info!("--- Testing Get Financial Reports (Summary & Snapshot) ---");
  let fin_data_mgr = client.data_financials();
  let contract = Contract::stock("AAPL"); // Test with AAPL stock
  let fundamental_data_options = &[]; // No specific options for now

  let mut overall_success = true;

  // 1. Request Financial Summary
  info!("Requesting 'ReportsFinSummary' for {}...", contract.symbol);
  match fin_data_mgr.get_fundamental_data(&contract, FundamentalReportType::ReportsFinSummary, fundamental_data_options) {
    Ok(xml_data) => {
      info!("Successfully received 'ReportsFinSummary' XML for {}: {}", contract.symbol, xml_data);
      if xml_data.is_empty() {
        warn!("Received empty 'ReportsFinSummary' XML data for {}.", contract.symbol);
      } else {
        // Parse the XML
        match parse_fundamental_xml(&xml_data, FundamentalReportType::ReportsFinSummary) {
          Ok(ParsedFundamentalData::FinancialSummary(summary)) => {
            info!("Successfully parsed 'ReportsFinSummary' for {}:", contract.symbol);
            info!("  Number of EPS Records: {}", summary.eps_records.len());
            if let Some(eps) = summary.eps_records.first() {
              info!("    Sample EPS: Date: {:?}, Type: {:?}, Period: {:?}, Value: {:?}, Currency: {:?}",
                    eps.as_of_date, eps.report_type, eps.period, eps.value, eps.currency);
            }
            info!("  Number of Dividend Per Share Records: {}", summary.dividend_per_share_records.len());
            if let Some(dps) = summary.dividend_per_share_records.first() {
              info!("    Sample DPS: Date: {:?}, Type: {:?}, Period: {:?}, Value: {:?}, Currency: {:?}",
                    dps.as_of_date, dps.report_type, dps.period, dps.value, dps.currency);
            }
            info!("  Number of Total Revenue Records: {}", summary.total_revenue_records.len());
            if let Some(rev) = summary.total_revenue_records.first() {
              info!("    Sample Revenue: Date: {:?}, Type: {:?}, Period: {:?}, Value: {:?}, Currency: {:?}",
                    rev.as_of_date, rev.report_type, rev.period, rev.value, rev.currency);
            }
            info!("  Number of Announced Dividend Records: {}", summary.announced_dividend_records.len());
            if let Some(div) = summary.announced_dividend_records.first() {
              info!("    Sample Announced Dividend: ExDate: {:?}, Type: {:?}, Value: {:?}, Currency: {:?}",
                    div.ex_date, div.dividend_type, div.value, div.currency);
            }
          }
          Ok(_) => {
            error!("Parsed 'ReportsFinSummary' but got unexpected data type for {}.", contract.symbol);
            overall_success = false;
          }
          Err(parse_err) => {
            error!("Failed to parse 'ReportsFinSummary' XML for {}: {:?}", contract.symbol, parse_err);
            overall_success = false;
          }
        }
      }
    }
    Err(e) => {
      error!("Failed to get 'ReportsFinSummary' for {}: {:?}", contract.symbol, e);
      overall_success = false;
    }
  }

  // 2. Request Report Snapshot
  info!("Requesting 'ReportSnapshot' for {}...", contract.symbol);
  match fin_data_mgr.get_fundamental_data(&contract, FundamentalReportType::ReportSnapshot, fundamental_data_options) {
    Ok(xml_data) => {
      info!("Successfully received 'ReportSnapshot' XML for {}. Length: {}", contract.symbol, xml_data.len());
      info!("Successfully received 'ReportSnapshot' XML for {}: {}", contract.symbol, xml_data);
      if xml_data.is_empty() {
        warn!("Received empty 'ReportSnapshot' XML data for {}.", contract.symbol);
      } else {
        // Parse the XML
        match parse_fundamental_xml(&xml_data, FundamentalReportType::ReportSnapshot) {
          Ok(ParsedFundamentalData::Snapshot(snapshot)) => {
            info!("Successfully parsed 'ReportSnapshot' for {}:", contract.symbol);
            if let Some(info) = &snapshot.company_info {
              info!("  Parsed Company Info:");
              info!("    Name: {}", info.company_name.as_deref().unwrap_or("N/A"));
              info!("    Ticker: {}", info.ticker.as_deref().unwrap_or("N/A"));
              info!("    ConID: {:?}", info.con_id);
              info!("    CIK: {}", info.cik.as_deref().unwrap_or("N/A"));
              info!("    Business Desc (start): {}...", info.business_description.as_deref().unwrap_or("N/A").chars().take(70).collect::<String>());
            } else {
              warn!("  No consolidated company information parsed from snapshot.");
            }
            info!("  Number of Issues: {}", snapshot.issues.len());
            if let Some(issue) = snapshot.issues.first() {
              info!("    First Issue Ticker: {}", issue.ticker.as_deref().unwrap_or("N/A"));
            }
            info!("  Number of Ratio Groups: {}", snapshot.ratio_groups.len());
            if let Some(group) = snapshot.ratio_groups.first() {
              info!("    First Ratio Group: '{}', Num Ratios: {}", group.id.as_deref().unwrap_or("N/A"), group.ratios.len());
              if let Some(ratio) = group.ratios.first() {
                info!("      Sample Ratio: Field='{}', Value='{}', Type='{}'",
                      ratio.field_name,
                      ratio.raw_value.as_deref().unwrap_or("N/A"),
                      ratio.period_type.as_deref().unwrap_or("N/A")); // Note: XML uses 'Type' attr, mapped to period_type field
              }
            }
            info!("  Number of Officers: {}", snapshot.officers.len());
            if let Some(officer) = snapshot.officers.first() {
              info!("    First Officer: {} {}", officer.first_name.as_deref().unwrap_or("?"), officer.last_name.as_deref().unwrap_or("?"));
            }
            if let Some(fc) = &snapshot.forecast_data {
              info!("  Forecast Data: {} items", fc.items.len());
              if let Some(item) = fc.items.first() {
                info!("    Sample Forecast: Field='{}', Value='{}'", item.field_name, item.value.as_deref().unwrap_or("N/A"));
              }
            } else {
              info!("  No Forecast Data found.");
            }

          }
          Ok(_) => {
            error!("Parsed 'ReportSnapshot' but got unexpected data type for {}.", contract.symbol);
            overall_success = false;
          }
          Err(parse_err) => {
            error!("Failed to parse 'ReportSnapshot' XML for {}: {:?}", contract.symbol, parse_err);
            overall_success = false;
          }
        }
      }
    }
    Err(e) => {
      error!("Failed to get 'ReportSnapshot' for {}: {:?}", contract.symbol, e);
      overall_success = false;
    }
  }

  // 3. Request financial statements.
  // We need to specify reportType to one of these:
  // AnnualReports - Annual financial statements
  // QuarterlyReports - Quarterly financial statements
  // Interim - Interim reports
  // ThreeMonth - 3-month statements
  // SixMonth - 6-month statements
  // TwelveMonth - 12-month statements
  // But the tag-value encoder is broken.

  if overall_success {
    Ok(())
  } else {
    Err(anyhow!("One or more financial report requests failed for {}", contract.symbol))
  }
}
