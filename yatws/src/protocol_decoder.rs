// yatws/src/protocol_decoder.rs
// Decoder for the TWS API protocol messages

use crate::base::IBKRError;
use crate::contract::{Contract, ContractDetails, OptionRight, SecType, ComboLeg, Bar, BarSize, WhatToShow};
use crate::order::{Order, OrderRequest, OrderState, OrderStatus, OrderType, OrderSide, TimeInForce, Execution};
use crate::account::{AccountInfo, AccountValue, Position, MarketDataSubscription, MarketDataType, NewsArticle};
use log::{debug, warn, error};
use std::io::{self, Read, BufRead, BufReader};
use std::collections::HashMap;
use chrono::{DateTime, Utc, NaiveDateTime, Duration};
use std::str::FromStr;

/// Message tags for incoming messages
pub enum IncomingMessageType {
  TickPrice = 1,
  TickSize = 2,
  OrderStatus = 3,
  ErrorMessage = 4,
  OpenOrder = 5,
  AccountValue = 6,
  PortfolioValue = 7,
  AccountUpdateTime = 8,
  NextValidId = 9,
  ContractData = 10,
  ExecutionData = 11,
  MarketDepth = 12,
  MarketDepthL2 = 13,
  NewsBulletins = 14,
  ManagedAccounts = 15,
  ReceiveFA = 16,
  HistoricalData = 17,
  BondContractData = 18,
  ScannerParameters = 19,
  ScannerData = 20,
  TickOptionComputation = 21,
  TickGeneric = 45,
  TickString = 46,
  TickEFP = 47,
  CurrentTime = 49,
  RealTimeBars = 50,
  FundamentalData = 51,
  ContractDataEnd = 52,
  OpenOrderEnd = 53,
  AccountDownloadEnd = 54,
  ExecutionDataEnd = 55,
  DeltaNeutralValidation = 56,
  TickSnapshotEnd = 57,
  MarketDataType = 58,
  CommissionReport = 59,
  Position = 61,
  PositionEnd = 62,
  AccountSummary = 63,
  AccountSummaryEnd = 64,
  VerifyMessageAPI = 65,
  VerifyCompleted = 66,
  DisplayGroupList = 67,
  DisplayGroupUpdated = 68,
  VerifyAndAuthMessageAPI = 69,
  VerifyAndAuthCompleted = 70,
  PositionMulti = 71,
  PositionMultiEnd = 72,
  AccountUpdateMulti = 73,
  AccountUpdateMultiEnd = 74,
  SecurityDefinitionOptionParameter = 75,
  SecurityDefinitionOptionParameterEnd = 76,
  SoftDollarTiers = 77,
  FamilyCodes = 78,
  SymbolSamples = 79,
  MktDepthExchanges = 80,
  TickReqParams = 81,
  SmartComponents = 82,
  NewsArticle = 83,
  TickNews = 84,
  NewsProviders = 85,
  HistoricalNews = 86,
  HistoricalNewsEnd = 87,
  HeadTimestamp = 88,
  HistogramData = 89,
  HistoricalDataUpdate = 90,
  RerouteMktDataReq = 91,
  RerouteMktDepthReq = 92,
  MarketRule = 93,
  PnL = 94,
  PnLSingle = 95,
  HistoricalTicks = 96,
  HistoricalTicksBidAsk = 97,
  HistoricalTicksLast = 98,
  TickByTick = 99,
  OrderBound = 100,
  CompletedOrder = 101,
  CompletedOrdersEnd = 102,
}

/// Message decoder for the TWS API protocol
pub struct Decoder<R: Read> {
  reader: BufReader<R>,
  server_version: i32,
}

impl<R: Read> Decoder<R> {
  /// Create a new message decoder
  pub fn new(reader: R, server_version: i32) -> Self {
    Self {
      reader: BufReader::new(reader),
      server_version,
    }
  }

  /// Set the server version
  pub fn set_server_version(&mut self, version: i32) {
    self.server_version = version;
  }

  /// Read a message from the stream and process it
  pub fn decode_message(&mut self) -> Result<(i32, Vec<u8>), IBKRError> {
    // Read message length
    let mut length_buf = [0u8; 4];
    if let Err(e) = self.reader.read_exact(&mut length_buf) {
      return Err(IBKRError::SocketError(format!("Failed to read message length: {}", e)));
    }
    let length = i32::from_be_bytes(length_buf) as usize;

    if length <= 0 || length > 1024 * 1024 {
      return Err(IBKRError::ParseError(format!("Invalid message length: {}", length)));
    }

    // Read the message data
    let mut data = vec![0u8; length];
    if let Err(e) = self.reader.read_exact(&mut data) {
      return Err(IBKRError::SocketError(format!("Failed to read message data: {}", e)));
    }

    // Get message type by reading the first field
    let mut fields = Vec::new();
    let mut start = 0;
    for i in 0..data.len() {
      if data[i] == 0 {
        if i > start {
          fields.push(&data[start..i]);
        } else {
          fields.push(&data[0..0]); // Empty field
        }
        start = i + 1;
      }
    }

    if fields.is_empty() {
      return Err(IBKRError::ParseError("Empty message".to_string()));
    }

    // Parse the message type
    let msg_type_str = std::str::from_utf8(fields[0])
      .map_err(|e| IBKRError::ParseError(format!("Failed to parse message type: {}", e)))?;

    let msg_type = msg_type_str.parse::<i32>()
      .map_err(|e| IBKRError::ParseError(format!("Failed to parse message type as integer: {}", e)))?;

    debug!("Received message type: {}", msg_type);

    Ok((msg_type, data))
  }

  /// Process a message based on its type
  pub fn process_message(&mut self, msg_type: i32, data: &[u8]) -> Result<(), IBKRError> {
    let mut parser = FieldParser::new(data);

    match msg_type {
      4 => self.process_error_message(&mut parser)?,
      9 => self.process_next_valid_id(&mut parser)?,
      _ => {
        // For brevity, we'll just acknowledge other message types
        debug!("Received message type {} - not processed in this example", msg_type);
      }
    }

    Ok(())
  }

  /// Process an error message
  fn process_error_message(&mut self, parser: &mut FieldParser) -> Result<(), IBKRError> {
    let _msg_type = parser.read_int()?; // Skip message type
    let version = parser.read_int()?;

    if version < 2 {
      let message = parser.read_string()?;
      error!("TWS Error: {}", message);
    } else {
      let id = parser.read_int()?;
      let error_code = parser.read_int()?;
      let error_msg = parser.read_string()?;

      error!("TWS Error {}: {} ({})", id, error_msg, error_code);
    }

    Ok(())
  }

  /// Process next valid ID message
  fn process_next_valid_id(&mut self, parser: &mut FieldParser) -> Result<(), IBKRError> {
    let _msg_type = parser.read_int()?; // Skip message type
    let _version = parser.read_int()?;
    let id = parser.read_int()?;

    debug!("Next valid ID: {}", id);

    Ok(())
  }

  /// Process tick price message
  fn process_tick_price(&mut self, parser: &mut FieldParser) -> Result<(), IBKRError> {
    let _msg_type = parser.read_int()?; // Skip message type
    let _version = parser.read_int()?;
    let ticker_id = parser.read_int()?;
    let tick_type = parser.read_int()?;
    let price = parser.read_double()?;

    debug!("Tick Price: ID={}, Type={}, Price={}", ticker_id, tick_type, price);

    Ok(())
  }

  /// Process tick size message
  fn process_tick_size(&mut self, parser: &mut FieldParser) -> Result<(), IBKRError> {
    let _msg_type = parser.read_int()?; // Skip message type
    let _version = parser.read_int()?;
    let ticker_id = parser.read_int()?;
    let tick_type = parser.read_int()?;
    let size = parser.read_int()?;

    debug!("Tick Size: ID={}, Type={}, Size={}", ticker_id, tick_type, size);

    Ok(())
  }

  /// Process order status message
  fn process_order_status(&mut self, parser: &mut FieldParser) -> Result<(), IBKRError> {
    let _msg_type = parser.read_int()?; // Skip message type
    let _version = parser.read_int()?;
    let id = parser.read_int()?;
    let status = parser.read_string()?;
    let filled = parser.read_double()?;
    let remaining = parser.read_double()?;
    let avg_fill_price = parser.read_double()?;

    debug!("Order Status: ID={}, Status={}, Filled={}, Remaining={}, AvgPrice={}",
           id, status, filled, remaining, avg_fill_price);

    Ok(())
  }

  /// Process portfolio value message
  fn process_portfolio_value(&mut self, parser: &mut FieldParser) -> Result<(), IBKRError> {
    let _msg_type = parser.read_int()?; // Skip message type
    let version = parser.read_int()?;

    let mut contract = Contract::new();

    if version >= 6 {
      contract.con_id = parser.read_int()?;
    }

    contract.symbol = parser.read_string()?;
    let sec_type_str = parser.read_string()?;
    contract.sec_type = SecType::from_str(&sec_type_str).unwrap_or(SecType::Stock);

    let expiry = parser.read_string()?;
    if !expiry.is_empty() {
      contract.last_trade_date_or_contract_month = Some(expiry);
    }

    let strike = parser.read_double()?;
    if strike > 0.0 {
      contract.strike = Some(strike);
    }

    let right_str = parser.read_string()?;
    if !right_str.is_empty() {
      contract.right = match right_str.as_str() {
        "C" => Some(OptionRight::Call),
        "P" => Some(OptionRight::Put),
        _ => None,
      };
    }

    if version >= 7 {
      let multiplier = parser.read_string()?;
      if !multiplier.is_empty() {
        contract.multiplier = Some(multiplier);
      }

      contract.primary_exchange = Some(parser.read_string()?);
    }

    contract.currency = parser.read_string()?;

    if version >= 2 {
      let local_symbol = parser.read_string()?;
      if !local_symbol.is_empty() {
        contract.local_symbol = Some(local_symbol);
      }
    }

    if version >= 8 {
      let trading_class = parser.read_string()?;
      if !trading_class.is_empty() {
        contract.trading_class = Some(trading_class);
      }
    }

    let position = parser.read_double()?;
    let market_price = parser.read_double()?;
    let market_value = parser.read_double()?;

    let mut average_cost = 0.0;
    let mut unrealized_pnl = 0.0;
    let mut realized_pnl = 0.0;

    if version >= 3 {
      average_cost = parser.read_double()?;
      unrealized_pnl = parser.read_double()?;
      realized_pnl = parser.read_double()?;
    }

    let mut account_name = String::new();
    if version >= 4 {
      account_name = parser.read_string()?;
    }

    debug!("Portfolio Value: Symbol={}, Position={}, MarketPrice={}, MarketValue={}, UnrealizedPnL={}",
           contract.symbol, position, market_price, market_value, unrealized_pnl);

    Ok(())
  }

  /// Process account value message
  fn process_account_value(&mut self, parser: &mut FieldParser) -> Result<(), IBKRError> {
    let _msg_type = parser.read_int()?; // Skip message type
    let version = parser.read_int()?;

    let key = parser.read_string()?;
    let value = parser.read_string()?;
    let currency = parser.read_string()?;

    let mut account_name = String::new();
    if version >= 2 {
      account_name = parser.read_string()?;
    }

    debug!("Account Value: Key={}, Value={}, Currency={}, Account={}",
           key, value, currency, account_name);

    Ok(())
  }

  /// Process managed accounts message
  fn process_managed_accounts(&mut self, parser: &mut FieldParser) -> Result<(), IBKRError> {
    let _msg_type = parser.read_int()?; // Skip message type
    let _version = parser.read_int()?;

    let accounts_list = parser.read_string()?;
    let accounts: Vec<&str> = accounts_list.split(',').collect();

    debug!("Managed Accounts: {}", accounts_list);

    Ok(())
  }

  /// Process contract data message
  fn process_contract_data(&mut self, parser: &mut FieldParser) -> Result<(), IBKRError> {
    let _msg_type = parser.read_int()?; // Skip message type
    let version = parser.read_int()?;

    let mut req_id = -1;
    if version >= 3 {
      req_id = parser.read_int()?;
    }

    let mut contract_details = ContractDetails::default();

    contract_details.contract.symbol = parser.read_string()?;

    let sec_type_str = parser.read_string()?;
    contract_details.contract.sec_type = SecType::from_str(&sec_type_str).unwrap_or(SecType::Stock);

    // Parse expiry date
    let expiry = parser.read_string()?;
    if !expiry.is_empty() {
      contract_details.contract.last_trade_date_or_contract_month = Some(expiry);
    }

    contract_details.contract.strike = Some(parser.read_double()?);

    let right_str = parser.read_string()?;
    if !right_str.is_empty() {
      contract_details.contract.right = match right_str.as_str() {
        "C" => Some(OptionRight::Call),
        "P" => Some(OptionRight::Put),
        _ => None,
      };
    }

    contract_details.contract.exchange = parser.read_string()?;
    contract_details.contract.currency = parser.read_string()?;

    let local_symbol = parser.read_string()?;
    if !local_symbol.is_empty() {
      contract_details.contract.local_symbol = Some(local_symbol);
    }

    contract_details.market_name = parser.read_string()?;

    let trading_class = parser.read_string()?;
    if !trading_class.is_empty() {
      contract_details.contract.trading_class = Some(trading_class);
    }

    contract_details.contract.con_id = parser.read_int()?;
    contract_details.min_tick = parser.read_double()?;

    // More fields can be parsed here as needed

    debug!("Contract Data: ReqID={}, Symbol={}, SecType={}, Exchange={}",
           req_id, contract_details.contract.symbol, sec_type_str, contract_details.contract.exchange);

    Ok(())
  }

  /// Process historical data message
  fn process_historical_data(&mut self, parser: &mut FieldParser) -> Result<(), IBKRError> {
    let _msg_type = parser.read_int()?; // Skip message type
    let _version = parser.read_int()?;

    let req_id = parser.read_int()?;

    // Parse start and end dates if version >= 2
    let start_date_str = parser.read_string()?;
    let end_date_str = parser.read_string()?;

    let item_count = parser.read_int()?;
    let mut bars = Vec::with_capacity(item_count as usize);

    for _ in 0..item_count {
      let date = parser.read_string()?;
      let open = parser.read_double()?;
      let high = parser.read_double()?;
      let low = parser.read_double()?;
      let close = parser.read_double()?;
      let volume = parser.read_int()? as i64;
      let wap = parser.read_double()?;

      // Skip has_gaps field
      parser.read_string()?;

      let bar_count = parser.read_int()?;

      let time = if date.contains(':') {
        // Time format
        match NaiveDateTime::parse_from_str(&date, "%Y%m%d %H:%M:%S") {
          Ok(ndt) => DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc),
          Err(_) => Utc::now(), // Fallback
        }
      } else {
        // Date format
        match NaiveDateTime::parse_from_str(&format!("{} 00:00:00", date), "%Y%m%d %H:%M:%S") {
          Ok(ndt) => DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc),
          Err(_) => Utc::now(), // Fallback
        }
      };

      let bar = Bar {
        time,
        open,
        high,
        low,
        close,
        volume,
        wap,
        count: bar_count,
      };

      bars.push(bar);
    }

    debug!("Historical Data: ReqID={}, Bars={}", req_id, bars.len());

    Ok(())
  }

  /// Process position message
  fn process_position(&mut self, parser: &mut FieldParser) -> Result<(), IBKRError> {
    let _msg_type = parser.read_int()?; // Skip message type
    let version = parser.read_int()?;

    let account = parser.read_string()?;

    let mut contract = Contract::new();
    contract.con_id = parser.read_int()?;
    contract.symbol = parser.read_string()?;

    let sec_type_str = parser.read_string()?;
    contract.sec_type = SecType::from_str(&sec_type_str).unwrap_or(SecType::Stock);

    let expiry = parser.read_string()?;
    if !expiry.is_empty() {
      contract.last_trade_date_or_contract_month = Some(expiry);
    }

    let strike = parser.read_double()?;
    if strike > 0.0 {
      contract.strike = Some(strike);
    }

    let right_str = parser.read_string()?;
    if !right_str.is_empty() {
      contract.right = match right_str.as_str() {
        "C" => Some(OptionRight::Call),
        "P" => Some(OptionRight::Put),
        _ => None,
      };
    }

    let multiplier = parser.read_string()?;
    if !multiplier.is_empty() {
      contract.multiplier = Some(multiplier);
    }

    contract.exchange = parser.read_string()?;
    contract.currency = parser.read_string()?;

    let local_symbol = parser.read_string()?;
    if !local_symbol.is_empty() {
      contract.local_symbol = Some(local_symbol);
    }

    if version >= 2 {
      let trading_class = parser.read_string()?;
      if !trading_class.is_empty() {
        contract.trading_class = Some(trading_class);
      }
    }

    let position = parser.read_double()?;

    let mut avg_cost = 0.0;
    if version >= 3 {
      avg_cost = parser.read_double()?;
    }

    debug!("Position: Account={}, Symbol={}, Position={}, AvgCost={}",
           account, contract.symbol, position, avg_cost);

    Ok(())
  }

  /// Process execution data message
  fn process_execution_data(&mut self, parser: &mut FieldParser) -> Result<(), IBKRError> {
    let _msg_type = parser.read_int()?; // Skip message type
    let version = parser.read_int()?;

    let mut req_id = -1;
    if version >= 7 {
      req_id = parser.read_int()?;
    }

    let order_id = parser.read_int()?;

    // Read contract fields
    let mut contract = Contract::new();

    if version >= 5 {
      contract.con_id = parser.read_int()?;
    }

    contract.symbol = parser.read_string()?;

    let sec_type_str = parser.read_string()?;
    contract.sec_type = SecType::from_str(&sec_type_str).unwrap_or(SecType::Stock);

    let expiry = parser.read_string()?;
    if !expiry.is_empty() {
      contract.last_trade_date_or_contract_month = Some(expiry);
    }

    let strike = parser.read_double()?;
    if strike > 0.0 {
      contract.strike = Some(strike);
    }

    let right_str = parser.read_string()?;
    if !right_str.is_empty() {
      contract.right = match right_str.as_str() {
        "C" => Some(OptionRight::Call),
        "P" => Some(OptionRight::Put),
        _ => None,
      };
    }

    if version >= 9 {
      let multiplier = parser.read_string()?;
      if !multiplier.is_empty() {
        contract.multiplier = Some(multiplier);
      }
    }

    contract.exchange = parser.read_string()?;
    contract.currency = parser.read_string()?;

    let local_symbol = parser.read_string()?;
    if !local_symbol.is_empty() {
      contract.local_symbol = Some(local_symbol);
    }

    if version >= 10 {
      let trading_class = parser.read_string()?;
      if !trading_class.is_empty() {
        contract.trading_class = Some(trading_class);
      }
    }

    // Read execution fields
    let mut execution = Execution {
      execution_id: parser.read_string()?,
      order_id: order_id.to_string(),
      symbol: contract.symbol.clone(),
      side: match parser.read_string()?.as_str() {
        "BOT" => OrderSide::Buy,
        "SLD" => OrderSide::Sell,
        "SSHORT" => OrderSide::SellShort,
        _ => OrderSide::Buy,
      },
      quantity: parser.read_double()?,
      price: parser.read_double()?,
      time: Utc::now(), // Default time
      commission: 0.0,
      exchange: parser.read_string()?,
      account: parser.read_string()?,
    };

    if version >= 6 {
      // More execution fields can be parsed here
    }

    debug!("Execution Data: ReqID={}, OrderID={}, Symbol={}, Side={}, Quantity={}, Price={}",
           req_id, order_id, contract.symbol, execution.side, execution.quantity, execution.price);

    Ok(())
  }

  /// Process open order message
  fn process_open_order(&mut self, parser: &mut FieldParser) -> Result<(), IBKRError> {
    let _msg_type = parser.read_int()?; // Skip message type
    let version = parser.read_int()?;

    let order_id = parser.read_int()?;

    // Read contract fields
    let mut contract = Contract::new();

    if version >= 17 {
      contract.con_id = parser.read_int()?;
    }

    contract.symbol = parser.read_string()?;

    let sec_type_str = parser.read_string()?;
    contract.sec_type = SecType::from_str(&sec_type_str).unwrap_or(SecType::Stock);

    let expiry = parser.read_string()?;
    if !expiry.is_empty() {
      contract.last_trade_date_or_contract_month = Some(expiry);
    }

    let strike = parser.read_double()?;
    if strike > 0.0 {
      contract.strike = Some(strike);
    }

    let right_str = parser.read_string()?;
    if !right_str.is_empty() {
      contract.right = match right_str.as_str() {
        "C" => Some(OptionRight::Call),
        "P" => Some(OptionRight::Put),
        _ => None,
      };
    }

    if version >= 32 {
      let multiplier = parser.read_string()?;
      if !multiplier.is_empty() {
        contract.multiplier = Some(multiplier);
      }
    }

    contract.exchange = parser.read_string()?;
    contract.currency = parser.read_string()?;

    if version >= 2 {
      let local_symbol = parser.read_string()?;
      if !local_symbol.is_empty() {
        contract.local_symbol = Some(local_symbol);
      }
    }

    if version >= 32 {
      let trading_class = parser.read_string()?;
      if !trading_class.is_empty() {
        contract.trading_class = Some(trading_class);
      }
    }

    // Read order fields
    let side_str = parser.read_string()?;
    let quantity = parser.read_double()?;
    let order_type_str = parser.read_string()?;
    let limit_price = parser.read_double()?;
    let aux_price = parser.read_double()?;
    let tif_str = parser.read_string()?;

    // Create OrderRequest with basic fields
    let order_request = OrderRequest {
      order_type: match order_type_str.as_str() {
        "MKT" => OrderType::Market,
        "LMT" => OrderType::Limit,
        "STP" => OrderType::Stop,
        "STP LMT" => OrderType::StopLimit,
        _ => OrderType::None,
      },
      side: match side_str.as_str() {
        "BUY" => OrderSide::Buy,
        "SELL" => OrderSide::Sell,
        "SSHORT" => OrderSide::SellShort,
        _ => OrderSide::Buy,
      },
      quantity,
      limit_price: if limit_price > 0.0 { Some(limit_price) } else { None },
      aux_price: if aux_price > 0.0 { Some(aux_price) } else { None },
      time_in_force: match tif_str.as_str() {
        "DAY" => TimeInForce::Day,
        "GTC" => TimeInForce::GoodTillCancelled,
        "FOK" => TimeInForce::FillOrKill,
        "IOC" => TimeInForce::ImmediateOrCancel,
        "GTD" => TimeInForce::GoodTillDate,
        _ => TimeInForce::Day,
      },
      good_till_date: None,
      all_or_none: false,
      min_quantity: None,
      outside_rth: false,
      hidden: false,
      transmit: true,
      parent_id: None,
      order_ref: None,
      account: None,
      fa_group: None,
      fa_method: None,
      fa_percentage: None,
      algo_strategy: None,
      algo_params: Vec::new(),
    };

    // Create order and state
    let order = Order {
      id: order_id.to_string(),
      contract: contract.clone(),
      request: order_request,
      state: OrderState::default(),
      created_at: Utc::now(),
      updated_at: Utc::now(),
    };

    let order_state = OrderState {
      status: OrderStatus::PreSubmitted,
      filled_quantity: 0.0,
      remaining_quantity: quantity,
      average_fill_price: 0.0,
      last_fill_price: None,
      why_held: None,
      error: None,
    };

    debug!("Open Order: ID={}, Symbol={}, Side={}, Type={}, Quantity={}",
           order_id, contract.symbol, side_str, order_type_str, quantity);

    Ok(())
  }
}

/// Helper for parsing fields from a message buffer
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
}
