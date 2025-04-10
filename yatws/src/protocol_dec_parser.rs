use crate::base::IBKRError;

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
