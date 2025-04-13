use crate::base::IBKRError;

use chrono::{DateTime, Utc, TimeZone, NaiveDateTime};
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

pub struct FieldParser<'a> {
  data: &'a [u8],
  pos: usize,
  fields: Vec<(usize, usize)>, // (start, end) indices for each field
  current_field: usize,
}

impl<'a> FieldParser<'a> {
  /// Create a new field parser
  pub fn new(data: &'a [u8]) -> Self {
    let mut parser = Self {
      data,
      pos: 0,
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

  /// Read an integer field
  pub fn read_int(&mut self) -> Result<i32, IBKRError> {
    let s = self.read_string()?;

    if s.is_empty() {
      return Ok(0);
    }

    s.parse::<i32>()
      .map_err(|e| IBKRError::ParseError(format!("Failed to parse integer: {}", e)))
  }

  /// Read an integer field
  pub fn read_i64(&mut self) -> Result<i64, IBKRError> {
    let s = self.read_string()?;

    if s.is_empty() {
      return Ok(0);
    }

    s.parse::<i64>()
      .map_err(|e| IBKRError::ParseError(format!("Failed to parse integer: {}", e)))
  }

  /// Read a double field
  pub fn read_double(&mut self) -> Result<f64, IBKRError> {
    let s = self.read_string()?;

    if s.is_empty() {
      return Ok(0.0);
    }

    s.parse::<f64>()
      .map_err(|e| IBKRError::ParseError(format!("Failed to parse double: {}", e)))
  }

  /// Read a boolean field (as 0 or 1)
  pub fn read_bool(&mut self) -> Result<bool, IBKRError> {
    let val = self.read_int()?;
    Ok(val != 0)
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
