// yatws/src/protocol_decoder.rs
// Decoder for the TWS API protocol messages - Part 1: Definitions and basics

use std::fmt;
use std::convert::TryFrom; // Add the import here

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


/// Error codes sent from TWS or the client library itself.
/// Based on the Java client's EClientErrors class.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientErrorCode {
  /// No Valid ID.
  NoValidId = -1,
  /// Already connected.
  AlreadyConnected = 501,
  /// Couldn't connect to TWS. Confirm that "Enable ActiveX and Socket Clients" is enabled and connection port is the same as "Socket Port" on the TWS "Edit->Global Configuration...->API->Settings" menu. Live Trading ports: TWS: 7496; IB Gateway: 4001. Simulated Trading ports for new installations of version 954.1 or newer: TWS: 7497; IB Gateway: 4002
  ConnectFail = 502,
  /// The TWS is out of date and must be upgraded.
  UpdateTws = 503,
  /// Not connected
  NotConnected = 504,
  /// Fatal Error: Unknown message id.
  UnknownId = 505,
  /// Unsupported Version
  UnsupportedVersion = 506,
  /// Bad Message Length
  BadLength = 507,
  /// Bad Message
  BadMessage = 508,
  /// Failed to send message
  FailSend = 509,
  /// Request Market Data Sending Error
  FailSendReqMkt = 510,
  /// Cancel Market Data Sending Error
  FailSendCanMkt = 511,
  /// Order Sending Error
  FailSendOrder = 512,
  /// Account Update Request Sending Error
  FailSendAcct = 513,
  /// Request For Executions Sending Error
  FailSendExec = 514,
  /// Cancel Order Sending Error
  FailSendCOrder = 515,
  /// Request Open Order Sending Error
  FailSendOOrder = 516,
  /// Unknown contract. Verify the contract details supplied.
  UnknownContract = 517,
  /// Request Contract Data Sending Error
  FailSendReqContract = 518,
  /// Request Market Depth Sending Error
  FailSendReqMktDepth = 519,
  /// Cancel Market Depth Sending Error
  FailSendCanMktDepth = 520,
  /// Set Server Log Level Sending Error
  FailSendServerLogLevel = 521,
  /// FA Information Request Sending Error
  FailSendFaRequest = 522,
  /// FA Information Replace Sending Error
  FailSendFaReplace = 523,
  /// Request Scanner Subscription Sending Error
  FailSendReqScanner = 524,
  /// Cancel Scanner Subscription Sending Error
  FailSendCanScanner = 525,
  /// Request Scanner Parameter Sending Error
  FailSendReqScannerParameters = 526,
  /// Request Historical Data Sending Error
  FailSendReqHistData = 527,
  /// Request Historical Data Sending Error (Duplicate of 527?)
  FailSendCanHistData = 528, // Note: Original Java name implies cancel, but message is same as 527
  /// Request Real-time Bar Data Sending Error
  FailSendReqRtBars = 529,
  /// Cancel Real-time Bar Data Sending Error
  FailSendCanRtBars = 530,
  /// Request Current Time Sending Error
  FailSendReqCurrTime = 531,
  /// Request Fundamental Data Sending Error
  FailSendReqFundData = 532,
  /// Cancel Fundamental Data Sending Error
  FailSendCanFundData = 533,
  /// Request Calculate Implied Volatility Sending Error
  FailSendReqCalcImpliedVolat = 534,
  /// Request Calculate Option Price Sending Error
  FailSendReqCalcOptionPrice = 535,
  /// Cancel Calculate Implied Volatility Sending Error
  FailSendCanCalcImpliedVolat = 536,
  /// Cancel Calculate Option Price Sending Error
  FailSendCanCalcOptionPrice = 537,
  /// Request Global Cancel Sending Error
  FailSendReqGlobalCancel = 538,
  /// Request Market Data Type Sending Error
  FailSendReqMarketDataType = 539,
  /// Request Positions Sending Error
  FailSendReqPositions = 540,
  /// Cancel Positions Sending Error
  FailSendCanPositions = 541,
  /// Request Account Data Sending Error
  FailSendReqAccountData = 542,
  /// Cancel Account Data Sending Error
  FailSendCanAccountData = 543,
  /// Verify Request Sending Error
  FailSendVerifyRequest = 544,
  /// Verify Message Sending Error
  FailSendVerifyMessage = 545,
  /// Query Display Groups Sending Error
  FailSendQueryDisplayGroups = 546,
  /// Subscribe To Group Events Sending Error
  FailSendSubscribeToGroupEvents = 547,
  /// Update Display Group Sending Error
  FailSendUpdateDisplayGroup = 548,
  /// Unsubscribe From Group Events Sending Error
  FailSendUnsubscribeFromGroupEvents = 549,
  /// Start API Sending Error
  FailSendStartApi = 550,
  /// Verify And Auth Request Sending Error
  FailSendVerifyAndAuthRequest = 551,
  /// Verify And Auth Message Sending Error
  FailSendVerifyAndAuthMessage = 552,
  /// Request Positions Multi Sending Error
  FailSendReqPositionsMulti = 553,
  /// Cancel Positions Multi Sending Error
  FailSendCanPositionsMulti = 554,
  /// Request Account Updates Multi Sending Error
  FailSendReqAccountUpdatesMulti = 555,
  /// Cancel Account Updates Multi Sending Error
  FailSendCanAccountUpdatesMulti = 556,
  /// Request Security Definition Option Params Sending Error
  FailSendReqSecDefOptParams = 557,
  /// Request Soft Dollar Tiers Sending Error
  FailSendReqSoftDollarTiers = 558,
  /// Request Family Codes Sending Error
  FailSendReqFamilyCodes = 559,
  /// Request Matching Symbols Sending Error
  FailSendReqMatchingSymbols = 560,
  /// Request Market Depth Exchanges Sending Error
  FailSendReqMktDepthExchanges = 561,
  /// Request Smart Components Sending Error
  FailSendReqSmartComponents = 562,
  /// Request News Providers Sending Error
  FailSendReqNewsProviders = 563,
  /// Request News Article Sending Error
  FailSendReqNewsArticle = 564,
  /// Request Historical News Sending Error
  FailSendReqHistoricalNews = 565,
  /// Request Head Time Stamp Sending Error
  FailSendReqHeadTimestamp = 566,
  /// Request Histogram Data Sending Error
  FailSendReqHistogramData = 567,
  /// Cancel Request Histogram Data Sending Error
  FailSendCancelHistogramData = 568,
  /// Cancel Head Time Stamp Sending Error
  FailSendCancelHeadTimestamp = 569,
  /// Request Market Rule Sending Error
  FailSendReqMarketRule = 570,
  /// Request PnL Sending Error
  FailSendReqPnl = 571,
  /// Cancel PnL Sending Error
  FailSendCancelPnl = 572,
  /// Request PnL Single Error
  FailSendReqPnlSingle = 573,
  /// Cancel PnL Single Sending Error
  FailSendCancelPnlSingle = 574,
  /// Request Historical Ticks Error
  FailSendReqHistoricalTicks = 575,
  /// Request Tick-By-Tick Data Sending Error
  FailSendReqTickByTickData = 576,
  /// Cancel Tick-By-Tick Data Sending Error
  FailSendCancelTickByTickData = 577,
  /// Request Completed Orders Sending Error
  FailSendReqCompletedOrders = 578,
  /// Invalid symbol in string
  InvalidSymbol = 579,
  /// Request WSH Meta Data Sending Error
  FailSendReqWshMetaData = 580,
  /// Cancel WSH Meta Data Sending Error
  FailSendCanWshMetaData = 581,
  /// Request WSH Event Data Sending Error
  FailSendReqWshEventData = 582,
  /// Cancel WSH Event Data Sending Error
  FailSendCanWshEventData = 583,
  /// Request User Info Sending Error
  FailSendReqUserInfo = 584,
  /// FA Profile is not supported anymore, use FA Group instead
  FaProfileNotSupported = 585,
}

impl fmt::Display for ClientErrorCode {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let msg = match self {
      ClientErrorCode::NoValidId => "No Valid ID.",
      ClientErrorCode::AlreadyConnected => "Already connected.",
      ClientErrorCode::ConnectFail => "Couldn't connect to TWS. Confirm that \"Enable ActiveX and Socket Clients\" is enabled and connection port is the same as \"Socket Port\" on the TWS \"Edit->Global Configuration...->API->Settings\" menu. Live Trading ports: TWS: 7496; IB Gateway: 4001. Simulated Trading ports for new installations of version 954.1 or newer: TWS: 7497; IB Gateway: 4002",
      ClientErrorCode::UpdateTws => "The TWS is out of date and must be upgraded.",
      ClientErrorCode::NotConnected => "Not connected",
      ClientErrorCode::UnknownId => "Fatal Error: Unknown message id.",
      ClientErrorCode::UnsupportedVersion => "Unsupported Version",
      ClientErrorCode::BadLength => "Bad Message Length",
      ClientErrorCode::BadMessage => "Bad Message",
      ClientErrorCode::FailSend => "Failed to send message",
      ClientErrorCode::FailSendReqMkt => "Request Market Data Sending Error",
      ClientErrorCode::FailSendCanMkt => "Cancel Market Data Sending Error",
      ClientErrorCode::FailSendOrder => "Order Sending Error",
      ClientErrorCode::FailSendAcct => "Account Update Request Sending Error",
      ClientErrorCode::FailSendExec => "Request For Executions Sending Error",
      ClientErrorCode::FailSendCOrder => "Cancel Order Sending Error",
      ClientErrorCode::FailSendOOrder => "Request Open Order Sending Error",
      ClientErrorCode::UnknownContract => "Unknown contract. Verify the contract details supplied.",
      ClientErrorCode::FailSendReqContract => "Request Contract Data Sending Error",
      ClientErrorCode::FailSendReqMktDepth => "Request Market Depth Sending Error",
      ClientErrorCode::FailSendCanMktDepth => "Cancel Market Depth Sending Error",
      ClientErrorCode::FailSendServerLogLevel => "Set Server Log Level Sending Error",
      ClientErrorCode::FailSendFaRequest => "FA Information Request Sending Error",
      ClientErrorCode::FailSendFaReplace => "FA Information Replace Sending Error",
      ClientErrorCode::FailSendReqScanner => "Request Scanner Subscription Sending Error",
      ClientErrorCode::FailSendCanScanner => "Cancel Scanner Subscription Sending Error",
      ClientErrorCode::FailSendReqScannerParameters => "Request Scanner Parameter Sending Error",
      ClientErrorCode::FailSendReqHistData => "Request Historical Data Sending Error",
      ClientErrorCode::FailSendCanHistData => "Request Historical Data Sending Error", // Note: Same message as ReqHistData
      ClientErrorCode::FailSendReqRtBars => "Request Real-time Bar Data Sending Error",
      ClientErrorCode::FailSendCanRtBars => "Cancel Real-time Bar Data Sending Error",
      ClientErrorCode::FailSendReqCurrTime => "Request Current Time Sending Error",
      ClientErrorCode::FailSendReqFundData => "Request Fundamental Data Sending Error",
      ClientErrorCode::FailSendCanFundData => "Cancel Fundamental Data Sending Error",
      ClientErrorCode::FailSendReqCalcImpliedVolat => "Request Calculate Implied Volatility Sending Error",
      ClientErrorCode::FailSendReqCalcOptionPrice => "Request Calculate Option Price Sending Error",
      ClientErrorCode::FailSendCanCalcImpliedVolat => "Cancel Calculate Implied Volatility Sending Error",
      ClientErrorCode::FailSendCanCalcOptionPrice => "Cancel Calculate Option Price Sending Error",
      ClientErrorCode::FailSendReqGlobalCancel => "Request Global Cancel Sending Error",
      ClientErrorCode::FailSendReqMarketDataType => "Request Market Data Type Sending Error",
      ClientErrorCode::FailSendReqPositions => "Request Positions Sending Error",
      ClientErrorCode::FailSendCanPositions => "Cancel Positions Sending Error",
      ClientErrorCode::FailSendReqAccountData => "Request Account Data Sending Error",
      ClientErrorCode::FailSendCanAccountData => "Cancel Account Data Sending Error",
      ClientErrorCode::FailSendVerifyRequest => "Verify Request Sending Error",
      ClientErrorCode::FailSendVerifyMessage => "Verify Message Sending Error",
      ClientErrorCode::FailSendQueryDisplayGroups => "Query Display Groups Sending Error",
      ClientErrorCode::FailSendSubscribeToGroupEvents => "Subscribe To Group Events Sending Error",
      ClientErrorCode::FailSendUpdateDisplayGroup => "Update Display Group Sending Error",
      ClientErrorCode::FailSendUnsubscribeFromGroupEvents => "Unsubscribe From Group Events Sending Error",
      ClientErrorCode::FailSendStartApi => "Start API Sending Error",
      ClientErrorCode::FailSendVerifyAndAuthRequest => "Verify And Auth Request Sending Error",
      ClientErrorCode::FailSendVerifyAndAuthMessage => "Verify And Auth Message Sending Error",
      ClientErrorCode::FailSendReqPositionsMulti => "Request Positions Multi Sending Error",
      ClientErrorCode::FailSendCanPositionsMulti => "Cancel Positions Multi Sending Error",
      ClientErrorCode::FailSendReqAccountUpdatesMulti => "Request Account Updates Multi Sending Error",
      ClientErrorCode::FailSendCanAccountUpdatesMulti => "Cancel Account Updates Multi Sending Error",
      ClientErrorCode::FailSendReqSecDefOptParams => "Request Security Definition Option Params Sending Error",
      ClientErrorCode::FailSendReqSoftDollarTiers => "Request Soft Dollar Tiers Sending Error",
      ClientErrorCode::FailSendReqFamilyCodes => "Request Family Codes Sending Error",
      ClientErrorCode::FailSendReqMatchingSymbols => "Request Matching Symbols Sending Error",
      ClientErrorCode::FailSendReqMktDepthExchanges => "Request Market Depth Exchanges Sending Error",
      ClientErrorCode::FailSendReqSmartComponents => "Request Smart Components Sending Error",
      ClientErrorCode::FailSendReqNewsProviders => "Request News Providers Sending Error",
      ClientErrorCode::FailSendReqNewsArticle => "Request News Article Sending Error",
      ClientErrorCode::FailSendReqHistoricalNews => "Request Historical News Sending Error",
      ClientErrorCode::FailSendReqHeadTimestamp => "Request Head Time Stamp Sending Error",
      ClientErrorCode::FailSendReqHistogramData => "Request Histogram Data Sending Error",
      ClientErrorCode::FailSendCancelHistogramData => "Cancel Request Histogram Data Sending Error",
      ClientErrorCode::FailSendCancelHeadTimestamp => "Cancel Head Time Stamp Sending Error",
      ClientErrorCode::FailSendReqMarketRule => "Request Market Rule Sending Error",
      ClientErrorCode::FailSendReqPnl => "Request PnL Sending Error",
      ClientErrorCode::FailSendCancelPnl => "Cancel PnL Sending Error",
      ClientErrorCode::FailSendReqPnlSingle => "Request PnL Single Error",
      ClientErrorCode::FailSendCancelPnlSingle => "Cancel PnL Single Sending Error",
      ClientErrorCode::FailSendReqHistoricalTicks => "Request Historical Ticks Error",
      ClientErrorCode::FailSendReqTickByTickData => "Request Tick-By-Tick Data Sending Error",
      ClientErrorCode::FailSendCancelTickByTickData => "Cancel Tick-By-Tick Data Sending Error",
      ClientErrorCode::FailSendReqCompletedOrders => "Request Completed Orders Sending Error",
      ClientErrorCode::InvalidSymbol => "Invalid symbol in string",
      ClientErrorCode::FailSendReqWshMetaData => "Request WSH Meta Data Sending Error",
      ClientErrorCode::FailSendCanWshMetaData => "Cancel WSH Meta Data Sending Error",
      ClientErrorCode::FailSendReqWshEventData => "Request WSH Event Data Sending Error",
      ClientErrorCode::FailSendCanWshEventData => "Cancel WSH Event Data Sending Error",
      ClientErrorCode::FailSendReqUserInfo => "Request User Info Sending Error",
      ClientErrorCode::FaProfileNotSupported => "FA Profile is not supported anymore, use FA Group instead",
    };
    write!(f, "{}", msg)
  }
}

impl TryFrom<i32> for ClientErrorCode {
  type Error = IBKRError; // Or a more specific error type

  fn try_from(value: i32) -> Result<Self, Self::Error> {
    match value {
      -1 => Ok(ClientErrorCode::NoValidId),
      501 => Ok(ClientErrorCode::AlreadyConnected),
      502 => Ok(ClientErrorCode::ConnectFail),
      503 => Ok(ClientErrorCode::UpdateTws),
      504 => Ok(ClientErrorCode::NotConnected),
      505 => Ok(ClientErrorCode::UnknownId),
      506 => Ok(ClientErrorCode::UnsupportedVersion),
      507 => Ok(ClientErrorCode::BadLength),
      508 => Ok(ClientErrorCode::BadMessage),
      509 => Ok(ClientErrorCode::FailSend),
      510 => Ok(ClientErrorCode::FailSendReqMkt),
      511 => Ok(ClientErrorCode::FailSendCanMkt),
      512 => Ok(ClientErrorCode::FailSendOrder),
      513 => Ok(ClientErrorCode::FailSendAcct),
      514 => Ok(ClientErrorCode::FailSendExec),
      515 => Ok(ClientErrorCode::FailSendCOrder),
      516 => Ok(ClientErrorCode::FailSendOOrder),
      517 => Ok(ClientErrorCode::UnknownContract),
      518 => Ok(ClientErrorCode::FailSendReqContract),
      519 => Ok(ClientErrorCode::FailSendReqMktDepth),
      520 => Ok(ClientErrorCode::FailSendCanMktDepth),
      521 => Ok(ClientErrorCode::FailSendServerLogLevel),
      522 => Ok(ClientErrorCode::FailSendFaRequest),
      523 => Ok(ClientErrorCode::FailSendFaReplace),
      524 => Ok(ClientErrorCode::FailSendReqScanner),
      525 => Ok(ClientErrorCode::FailSendCanScanner),
      526 => Ok(ClientErrorCode::FailSendReqScannerParameters),
      527 => Ok(ClientErrorCode::FailSendReqHistData),
      528 => Ok(ClientErrorCode::FailSendCanHistData),
      529 => Ok(ClientErrorCode::FailSendReqRtBars),
      530 => Ok(ClientErrorCode::FailSendCanRtBars),
      531 => Ok(ClientErrorCode::FailSendReqCurrTime),
      532 => Ok(ClientErrorCode::FailSendReqFundData),
      533 => Ok(ClientErrorCode::FailSendCanFundData),
      534 => Ok(ClientErrorCode::FailSendReqCalcImpliedVolat),
      535 => Ok(ClientErrorCode::FailSendReqCalcOptionPrice),
      536 => Ok(ClientErrorCode::FailSendCanCalcImpliedVolat),
      537 => Ok(ClientErrorCode::FailSendCanCalcOptionPrice),
      538 => Ok(ClientErrorCode::FailSendReqGlobalCancel),
      539 => Ok(ClientErrorCode::FailSendReqMarketDataType),
      540 => Ok(ClientErrorCode::FailSendReqPositions),
      541 => Ok(ClientErrorCode::FailSendCanPositions),
      542 => Ok(ClientErrorCode::FailSendReqAccountData),
      543 => Ok(ClientErrorCode::FailSendCanAccountData),
      544 => Ok(ClientErrorCode::FailSendVerifyRequest),
      545 => Ok(ClientErrorCode::FailSendVerifyMessage),
      546 => Ok(ClientErrorCode::FailSendQueryDisplayGroups),
      547 => Ok(ClientErrorCode::FailSendSubscribeToGroupEvents),
      548 => Ok(ClientErrorCode::FailSendUpdateDisplayGroup),
      549 => Ok(ClientErrorCode::FailSendUnsubscribeFromGroupEvents),
      550 => Ok(ClientErrorCode::FailSendStartApi),
      551 => Ok(ClientErrorCode::FailSendVerifyAndAuthRequest),
      552 => Ok(ClientErrorCode::FailSendVerifyAndAuthMessage),
      553 => Ok(ClientErrorCode::FailSendReqPositionsMulti),
      554 => Ok(ClientErrorCode::FailSendCanPositionsMulti),
      555 => Ok(ClientErrorCode::FailSendReqAccountUpdatesMulti),
      556 => Ok(ClientErrorCode::FailSendCanAccountUpdatesMulti),
      557 => Ok(ClientErrorCode::FailSendReqSecDefOptParams),
      558 => Ok(ClientErrorCode::FailSendReqSoftDollarTiers),
      559 => Ok(ClientErrorCode::FailSendReqFamilyCodes),
      560 => Ok(ClientErrorCode::FailSendReqMatchingSymbols),
      561 => Ok(ClientErrorCode::FailSendReqMktDepthExchanges),
      562 => Ok(ClientErrorCode::FailSendReqSmartComponents),
      563 => Ok(ClientErrorCode::FailSendReqNewsProviders),
      564 => Ok(ClientErrorCode::FailSendReqNewsArticle),
      565 => Ok(ClientErrorCode::FailSendReqHistoricalNews),
      566 => Ok(ClientErrorCode::FailSendReqHeadTimestamp),
      567 => Ok(ClientErrorCode::FailSendReqHistogramData),
      568 => Ok(ClientErrorCode::FailSendCancelHistogramData),
      569 => Ok(ClientErrorCode::FailSendCancelHeadTimestamp),
      570 => Ok(ClientErrorCode::FailSendReqMarketRule),
      571 => Ok(ClientErrorCode::FailSendReqPnl),
      572 => Ok(ClientErrorCode::FailSendCancelPnl),
      573 => Ok(ClientErrorCode::FailSendReqPnlSingle),
      574 => Ok(ClientErrorCode::FailSendCancelPnlSingle),
      575 => Ok(ClientErrorCode::FailSendReqHistoricalTicks),
      576 => Ok(ClientErrorCode::FailSendReqTickByTickData),
      577 => Ok(ClientErrorCode::FailSendCancelTickByTickData),
      578 => Ok(ClientErrorCode::FailSendReqCompletedOrders),
      579 => Ok(ClientErrorCode::InvalidSymbol),
      580 => Ok(ClientErrorCode::FailSendReqWshMetaData),
      581 => Ok(ClientErrorCode::FailSendCanWshMetaData),
      582 => Ok(ClientErrorCode::FailSendReqWshEventData),
      583 => Ok(ClientErrorCode::FailSendCanWshEventData),
      584 => Ok(ClientErrorCode::FailSendReqUserInfo),
      585 => Ok(ClientErrorCode::FaProfileNotSupported),
      // Add TWS specific codes (>= 1000) if needed, or handle them generically
      _ => Err(IBKRError::ParseError(format!("Unknown ClientErrorCode value: {}", value))),
    }
  }
}

// Need IBKRError definition available or replace Self::Error with a suitable type
use crate::base::IBKRError;
