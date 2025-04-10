// yatws/src/protocol_decoder.rs
// Decoder for the TWS API protocol messages - Part 1: Definitions and basics

use crate::base::IBKRError;
use crate::contract::{Contract, ContractDetails, OptionRight, SecType, SecIdType, ComboLeg, Bar, BarSize, WhatToShow};
use crate::order::{Order, OrderRequest, OrderState, OrderStatus, OrderType, OrderSide, TimeInForce};
use crate::account::{AccountInfo, AccountValue, Execution, Position};
use crate::data::MarketDataType;
use crate::news::NewsArticle;
use crate::min_server_ver::min_server_ver;

use log::{debug, warn, error, trace};
use std::io::{self, Read, BufRead, BufReader};
use std::collections::HashMap;
use chrono::{DateTime, Utc, NaiveDateTime, Duration};
use std::str::FromStr;

/// Message tags for incoming messages
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
  ReplaceFAEnd = 103,
  WshMetaData = 104,
  WshEventData = 105,
  HistoricalSchedule = 106,
  UserInfo = 107,
}

/// Message decoder for the TWS API protocol
pub struct Decoder {
  reader: BufReader<Box<dyn Read + Send>>,
  server_version: i32,
}

impl Decoder {
  /// Create a new message decoder
  pub fn new<R: Read + Send + 'static>(reader: R, server_version: i32) -> Self {
    Self {
      reader: BufReader::new(Box::new(reader)),
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
}
