use crate::base::IBKRError;
use crate::contract::{DateOrMonth, YearMonth};

use chrono::{DateTime, Utc, TimeZone, NaiveDateTime, NaiveDate, NaiveTime};
use chrono_tz::Tz;
use std::str::FromStr;

/// Parses various TWS date/time string formats into UTC DateTime.
///
/// Supported Input Formats:
/// - `YYYYMMDD{ }HH:MM:SS` (Login TZ assumed - **interpreted as UTC here due to complexity**)
/// - `YYYYMMDD-HH:MM:SS` (UTC assumed)
/// - `YYYYMMDD{ }HH:MM:SS TZID` (e.g., `America/New_York`)
/// - `YYYYMMDD-HH:MM:SS TZID` (e.g., `US/Eastern`)
///
/// `{ }` denotes one or two spaces. `TZID` is an IANA timezone name (e.g., "UTC", "US/Eastern").
///
/// **Timezone Handling Notes:**
/// *   Formats with an explicit `TZID` are parsed using that timezone and converted to UTC.
/// *   The `YYYYMMDD-HH:MM:SS` format is explicitly treated as UTC.
/// *   The `YYYYMMDD{ }HH:MM:SS` format (without TZID) *should* represent the TWS Login Timezone.
///     However, determining the login timezone dynamically is not feasible within this function.
///     **Therefore, this format is currently interpreted as UTC for simplicity.**
///     For accurate handling of this format, the application may need user configuration
///     or context about the TWS login settings.
/// *   Epoch seconds are **not** currently handled by this function but could be added.
///
/// # Arguments
/// * `time_str` - The date/time string from TWS.
///
/// # Returns
/// A `Result` containing the `DateTime<Utc>` on success, or an `IBKRError::ParseError` on failure.
pub fn parse_tws_date_time(time_str: &str) -> Result<DateTime<Utc>, IBKRError> {
  let trimmed = time_str.trim();
  if trimmed.is_empty() {
    return Err(IBKRError::ParseError("Cannot parse empty date/time string".to_string()));
  }

  // Try parsing formats with explicit timezones first
  // These look like "DATETIME TZID"
  if let Some(space_pos) = trimmed.rfind(|c: char| c.is_whitespace()) {
    let potential_dt_part = &trimmed[..space_pos];
    let potential_tz_part = &trimmed[space_pos..].trim(); // Trim whitespace around TZ

    // Check if the part after the last space looks like a timezone
    if potential_tz_part.contains('/') || potential_tz_part.to_uppercase() == "UTC" {
      match Tz::from_str(potential_tz_part) {
        Ok(tz) => {
          // Try parsing the datetime part with space or hyphen separator
          let dt_format_space = "%Y%m%d %H:%M:%S";
          let dt_format_hyphen = "%Y%m%d-%H:%M:%S";

          let naive_dt = NaiveDateTime::parse_from_str(potential_dt_part.replace("  ", " ").as_str(), dt_format_space)
            .or_else(|_| NaiveDateTime::parse_from_str(potential_dt_part, dt_format_hyphen));

          match naive_dt {
            Ok(ndt) => {
              // Successfully parsed NaiveDateTime and Tz
              return tz.from_local_datetime(&ndt)
                .single() // Handle ambiguity/non-existence
                .map(|dt_with_tz| dt_with_tz.with_timezone(&Utc)) // Convert to UTC
                .ok_or_else(|| IBKRError::ParseError(format!("Ambiguous or invalid time '{}' for timezone '{}'", ndt, tz)));
            },
            Err(_) => {
              // Timezone looked valid, but datetime part failed. Fall through to other formats.
              log::trace!("Potential TZ '{}' found, but DT part '{}' failed parsing. Trying other formats.", potential_tz_part, potential_dt_part);
            }
          }
        },
        Err(_) => {
          // String after space didn't parse as a valid Tz. Fall through.
          log::trace!("Part after space '{}' is not a valid chrono-tz timezone. Trying other formats.", potential_tz_part);
        }
      }
    }
  }

  // Try parsing "YYYYMMDD-HH:MM:SS" (Assumed UTC)
  if let Ok(naive_dt) = NaiveDateTime::parse_from_str(trimmed, "%Y%m%d-%H:%M:%S") {
    log::trace!("Parsed '{}' as YYYYMMDD-HH:MM:SS (UTC)", trimmed);
    return Utc.from_local_datetime(&naive_dt)
      .single()
      .ok_or_else(|| IBKRError::ParseError(format!("Ambiguous or invalid time '{}' for UTC conversion", naive_dt)));
  }

  // Try parsing "YYYYMMDD HH:MM:SS" (potentially double space)
  // **ASSUMING UTC** (Ideally Login TZ, but that's hard)
  let cleaned_for_space = trimmed.replace("  ", " ");
  if let Ok(naive_dt) = NaiveDateTime::parse_from_str(&cleaned_for_space, "%Y%m%d %H:%M:%S") {
    log::warn!("Parsed '{}' as YYYYMMDD HH:MM:SS, assuming UTC (ideally Login TZ)", trimmed);
    return Utc.from_local_datetime(&naive_dt)
      .single()
      .ok_or_else(|| IBKRError::ParseError(format!("Ambiguous or invalid time '{}' for UTC conversion", naive_dt)));
  }

  // Add other formats here if needed (e.g., epoch seconds)

  // If all known formats fail
  Err(IBKRError::ParseError(format!("Failed to parse TWS date/time string '{}' using known formats", time_str)))
}

pub fn parse_opt_tws_date_time(time_opt_str: Option<String>) -> Result<Option<DateTime<Utc>>, IBKRError> {
  time_opt_str.map(|x| parse_tws_date_time(&x)).transpose()
}

pub struct FieldParser<'a> {
  data: &'a [u8],
  fields: Vec<(usize, usize)>, // (start, end) indices for each field
  current_field: usize,
}

impl<'a> FieldParser<'a> {
  /// Create a new field parser
  pub fn new(data: &'a [u8]) -> Self {
    let mut parser = Self {
      data,
      fields: Vec::new(),
      current_field: 0,
    };

    // Pre-parse all fields
    parser.parse_fields();

    parser
  }

  /// Parse all fields in the message
  fn parse_fields(&mut self) {
    let mut start = 0;

    for i in 0..self.data.len() {
      if self.data[i] == 0 {
        self.fields.push((start, i));
        start = i + 1;
      }
    }
  }

  #[allow(dead_code)]
  pub fn curpos(&self) -> usize { self.current_field }

  #[allow(dead_code)]
  pub fn num(&self) -> usize { self.fields.len() }

  /// Read a string field
  pub fn read_string(&mut self) -> Result<String, IBKRError> {
    if self.current_field >= self.fields.len() {
      return Err(IBKRError::ParseError("Unexpected end of message".to_string()));
    }

    let (start, end) = self.fields[self.current_field];
    self.current_field += 1;

    if start >= end {
      return Ok(String::new());
    }

    std::str::from_utf8(&self.data[start..end])
      .map(|s| s.to_string())
      .map_err(|e| IBKRError::ParseError(format!("Failed to parse string: {}", e)))
  }

  pub fn read_string_opt(&mut self) -> Result<Option<String>, IBKRError> {
    if self.current_field >= self.fields.len() {
      return Err(IBKRError::ParseError("Unexpected end of message".to_string()));
    }

    let (start, end) = self.fields[self.current_field];
    self.current_field += 1;

    if start >= end {
      return Ok(None);
    }

    Ok(Some(std::str::from_utf8(&self.data[start..end])
       .map(|s| s.to_string())
       .map_err(|e| IBKRError::ParseError(format!("Failed to parse string: {}", e)))?))
  }

  /// Read an integer field
  pub fn read_int(&mut self) -> Result<i32, IBKRError> {
    let s = self.read_string()?;

    if s.is_empty() {
      return Ok(0);
    }

    s.parse::<i32>()
      .map_err(|e| IBKRError::ParseError(format!("Failed to parse integer `{}`: {}", s, e)))
  }

  /// Read an integer field
  pub fn read_i64(&mut self) -> Result<i64, IBKRError> {
    let s = self.read_string()?;

    if s.is_empty() {
      return Ok(0);
    }

    s.parse::<i64>()
      .map_err(|e| IBKRError::ParseError(format!("Failed to parse integer `{}`: {}", s, e)))
  }

  /// Read a double field
  pub fn read_double(&mut self) -> Result<f64, IBKRError> {
    let s = self.read_string()?;

    if s.is_empty() {
      return Ok(0.0);
    }

    s.parse::<f64>()
      .map_err(|e| IBKRError::ParseError(format!("Failed to parse double `{}`: {}", s, e)))
  }

  pub fn read_double_max(&mut self) -> Result<Option<f64>, IBKRError> {
    let val = self.read_double()?;
    if val == f64::MAX {
      Ok(None)
    } else {
      Ok(Some(val))
    }
  }

  pub fn read_int_max(&mut self) -> Result<Option<i32>, IBKRError> {
    let val = self.read_int()?;
    // Assuming i32::MAX represents "no value" for ints in this context
    // Adjust if a different sentinel value is used (e.g., 0 or -1 sometimes)
    if val == i32::MAX {
      Ok(None)
    } else {
      Ok(Some(val))
    }
  }

  // Example using f64 for Decimal. Adjust if using rust_decimal::Decimal
  pub fn read_decimal_max(&mut self) -> Result<Option<f64>, IBKRError> {
    let s = self.read_string()?;
    if s.is_empty() { return Ok(None); } // Or handle as error depending on context
    match s.parse::<f64>() {
      Ok(val) => {
        // Need to know the *exact* string representation TWS uses for MAX decimal
        // Assuming it might just be f64::MAX converted to string, or a specific string literal
        // This check might need refinement based on actual TWS behavior.
        // For now, let's assume f64::MAX check after parsing is sufficient.
        if val == f64::MAX { Ok(None) } else { Ok(Some(val)) }
      },
      Err(e) => Err(IBKRError::ParseError(format!("Failed to parse decimal string '{}': {}", s, e))),
    }
    // --- OR ---
    // If TWS sends MAX as a specific numeric value for decimals:
    // let val = self.read_double()?; // Assuming it sends as double
    // if val == f64::MAX { Ok(None) } else { Ok(Some(val)) }
  }

  #[allow(dead_code)]
  pub fn read_i64_max(&mut self) -> Result<Option<i64>, IBKRError> {
    let val = self.read_i64()?;
    if val == i64::MAX {
      Ok(None)
    } else {
      Ok(Some(val))
    }
  }

  /// Read a boolean field (as 0 or 1)
  pub fn read_bool(&mut self) -> Result<bool, IBKRError> {
    let val = self.read_int()?;
    Ok(val != 0)
  }

  pub fn read_bool_opt(&mut self) -> Result<Option<bool>, IBKRError> {
    if self.peek_string()?.is_empty() {
      Ok(None)
    } else {
      let val = self.read_int()?;
      Ok(Some(val != 0))
    }
  }

  /// Peek at the next string without advancing the cursor
  pub fn peek_string(&self) -> Result<String, IBKRError> {
    if self.current_field >= self.fields.len() {
      return Err(IBKRError::ParseError("Unexpected end of message".to_string()));
    }

    let (start, end) = self.fields[self.current_field];

    if start >= end {
      return Ok(String::new());
    }

    std::str::from_utf8(&self.data[start..end])
      .map(|s| s.to_string())
      .map_err(|e| IBKRError::ParseError(format!("Failed to parse string: {}", e)))
  }

  /// Skip a field
  pub fn skip_field(&mut self) -> Result<(), IBKRError> {
    if self.current_field >= self.fields.len() {
      return Err(IBKRError::ParseError("Unexpected end of message".to_string()));
    }

    self.current_field += 1;
    Ok(())
  }

  /// Get the number of remaining fields
  pub fn remaining_fields(&self) -> usize {
    if self.current_field >= self.fields.len() {
      0
    } else {
      self.fields.len() - self.current_field
    }
  }
}


/// Parse a TWS date or month. It must be either YYYYMM (month) or date (YYYYMMDD).
pub fn parse_tws_date_or_month(time_str: &str) -> Result<DateOrMonth, IBKRError> {
  if time_str.len() == 8 {
    // YYYYMMDD
    NaiveDate::parse_from_str(time_str, "%Y%m%d")
      .map(DateOrMonth::Date)
      .map_err(|e| IBKRError::ParseError(format!("Date parse error: {}", e)))
  } else if time_str.len() == 6 {
    // YYYYMM
    let year = time_str[0..4].parse::<i32>().map_err(|e| IBKRError::ParseError(format!("Year parse error: {}", e)))?;
    let month = time_str[4..6].parse::<u32>().map_err(|e| IBKRError::ParseError(format!("Month parse error: {}", e)))?;
    if !(1..=12).contains(&month) {
      return Err(IBKRError::ParseError(format!("Month must be 1..12, got {}", month)));
    }
    Ok(DateOrMonth::Month(YearMonth { year, month }))
  } else {
    Err(IBKRError::ParseError(format!("Input must be YYYYMM or YYYYMMDD, got {}", time_str)))
  }
}

/// Parse from an optional non-empty string.
///
/// If the string is defined, it must be a valid `DateOrMonth`.
pub fn parse_opt_tws_date_or_month(time_opt_str: Option<String>) -> Result<Option<DateOrMonth>, IBKRError> {
  time_opt_str.map(|x| parse_tws_date_or_month(&x)).transpose()
}

/// Parses a TWS date string in "YYYYMMDD" format into a NaiveDate.
/// Returns an error if the input does not match the expected format or is invalid.
pub fn parse_tws_date(date_str: &str) -> Result<NaiveDate, IBKRError> {
  if date_str.len() != 8 {
    return Err(IBKRError::ParseError(format!(
      "Expected format YYYYMMDD, got '{}'", date_str
    )));
  }
  NaiveDate::parse_from_str(date_str, "%Y%m%d")
    .map_err(|e| IBKRError::ParseError(format!("Date parse error: {} in `{}`", e, date_str)))
}

pub fn parse_opt_tws_date(date_opt_str: Option<String>) -> Result<Option<NaiveDate>, IBKRError> {
  date_opt_str.map(|x| parse_tws_date(&x)).transpose()
}

/// Parses a TWS year-month string in "YYYYMM" format into a YearMonth.
/// Returns an error if the input does not match the expected format or is invalid.
pub fn parse_tws_month(ym_str: &str) -> Result<YearMonth, IBKRError> {
  if ym_str.len() != 6 {
    return Err(IBKRError::ParseError(format!(
      "Expected format YYYYMM, got '{}'", ym_str
    )));
  }
  let year = ym_str[0..4].parse::<i32>()
    .map_err(|e| IBKRError::ParseError(format!("Year parse error: {}: `{}`", e, &ym_str[0..4])))?;
  let month = ym_str[4..6].parse::<u32>()
    .map_err(|e| IBKRError::ParseError(format!("Month parse error: {}: `{}`", e, &ym_str[4..6])))?;
  if !(1..=12).contains(&month) {
    return Err(IBKRError::ParseError(format!(
      "Month must be 1..12, got '{}'", month
    )));
  }
  Ok(YearMonth { year, month })
}

/// Parses an Option<String> TWS year-month string in "YYYYMM" format into Option<YearMonth>.
pub fn parse_opt_tws_month(ym_opt_str: Option<String>) -> Result<Option<YearMonth>, IBKRError> {
  ym_opt_str.map(|x| parse_tws_month(&x)).transpose()
}

/// Parses a TWS year-month string in "YYYYMM" format into a YearMonth.
/// Returns an error if the input does not match the expected format or is invalid.
pub fn parse_tws_time(time_str: &str) -> Result<NaiveTime, IBKRError> {
  if time_str.len() != 8 {
    return Err(IBKRError::ParseError(format!(
      "Expected format HH:MM:SS, got '{}'", time_str
    )));
  }
  NaiveTime::parse_from_str(time_str, "%H:%M:%S")
    .map_err(|e| IBKRError::ParseError(format!("Failed to parse time: {} in `{}`", e, time_str)))
}

#[allow(dead_code)]
pub fn parse_opt_tws_time(time_opt_str: Option<String>) -> Result<Option<NaiveTime>, IBKRError> {
  time_opt_str.map(|x| parse_tws_time(&x)).transpose()
}
