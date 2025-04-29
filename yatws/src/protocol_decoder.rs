// yatws/src/protocol_decoder.rs
// Decoder for the TWS API protocol messages - Part 1: Definitions and basics
// Need IBKRError definition available or replace Self::Error with a suitable type
use crate::base::IBKRError;

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


/// Error codes sent from TWS/IBG or the client library itself.
/// Includes system messages, warnings, and errors.
/// Based on the Java client's EClientErrors class and TWS documentation.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientErrorCode {
  // --- Client Library Specific Errors (typically < 1000, often 5xx) ---
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

  // --- TWS/IBG System Message Codes (11xx, 13xx) ---
  /// Connectivity between IB and the TWS has been lost.
  ConnectivityLost = 1100,
  /// Connectivity between IB and TWS has been restored- data lost.
  ConnectivityRestoredDataLost = 1101,
  /// Connectivity between IB and TWS has been restored- data maintained.
  ConnectivityRestoredDataMaintained = 1102,
  /// TWS socket port has been reset and this connection is being dropped. Please reconnect on the new port - <port_num>
  SocketPortReset = 1300,

  // --- TWS/IBG Warning Message Codes (21xx) ---
  /// New account data requested from TWS. API client has been unsubscribed from account data.
  AccountUpdateSubscriptionOverridden = 2100,
  /// Unable to subscribe to account as the following clients are subscribed to a different account.
  AccountUpdateSubscriptionRejected = 2101,
  /// Unable to modify this order as it is still being processed.
  OrderModificationRejectedProcessing = 2102,
  /// A market data farm is disconnected.
  MarketDataFarmDisconnected = 2103,
  /// Market data farm connection is OK
  MarketDataFarmConnected = 2104,
  /// A historical data farm is disconnected.
  HistoricalDataFarmDisconnected = 2105,
  /// A historical data farm is connected.
  HistoricalDataFarmConnected = 2106,
  /// A historical data farm connection has become inactive but should be available upon demand.
  HistoricalDataFarmInactive = 2107,
  /// A market data farm connection has become inactive but should be available upon demand.
  MarketDataFarmInactive = 2108,
  /// Order Event Warning: Attribute "Outside Regular Trading Hours" is ignored based on the order type and destination. PlaceOrder is now processed.
  OutsideRthAttributeIgnored = 2109,
  /// Connectivity between TWS and server is broken. It will be restored automatically.
  TwsToServerConnectionBroken = 2110,
  /// Cross Side Warning
  CrossSideWarning = 2137,
  /// Sec-def data farm connection is OK
  SecurityDefinitionDataFarmConnected = 2158,
  /// Etrade Only Not Supported Warning
  EtradeOnlyNotSupportedWarning = 2168,
  /// Firm Quote Only Not Supported Warning
  FirmQuoteOnlyNotSupportedWarning = 2169,

  // --- TWS/IBG Error Codes (1xx-4xx, 10xxx) ---
  /// Max rate of messages per second has been exceeded.
  MaxMessagesPerSecondExceeded = 100,
  /// Max number of tickers has been reached.
  MaxTickersReached = 101,
  /// Duplicate ticker ID.
  DuplicateTickerId = 102,
  /// Duplicate order ID.
  DuplicateOrderId = 103,
  /// Can't modify a filled order.
  CannotModifyFilledOrder = 104,
  /// Order being modified does not match original order.
  OrderModificationMismatch = 105,
  /// Can't transmit order ID: <OrderId>
  CannotTransmitOrderId = 106, // Placeholder, specific ID not part of enum
  /// Cannot transmit incomplete order.
  CannotTransmitIncompleteOrder = 107,
  /// Price is out of the range defined by the Percentage setting at order defaults frame. The order will not be transmitted.
  PriceOutOfPercentageRange = 109,
  /// The price does not conform to the minimum price variation for this contract.
  PriceIncrementViolation = 110,
  /// The TIF (Tif type) and the order type are incompatible.
  TifOrderTypeIncompatible = 111,
  /// The Tif option should be set to DAY for MOC and LOC orders.
  TifRequiresDayForMocLoc = 113,
  /// Relative orders are valid for stocks only. (Deprecated)
  RelativeOrdersStocksOnly = 114,
  /// Relative orders for US stocks can only be submitted to SMART, SMART_ECN, INSTINET, or PRIMEX. (Deprecated)
  RelativeOrdersUsStocksRouting = 115,
  /// The order cannot be transmitted to a dead exchange.
  CannotTransmitToDeadExchange = 116,
  /// The block order size must be at least 50.
  BlockOrderSizeTooSmall = 117,
  /// VWAP orders must be routed through the VWAP exchange.
  VwapOrdersRequireVwapExchange = 118,
  /// Only VWAP orders may be placed on the VWAP exchange.
  OnlyVwapOrdersOnVwapExchange = 119,
  /// It is too late to place a VWAP order for today.
  TooLateForVwapOrder = 120,
  /// Invalid BD flag for the order. Check "Destination" and "BD" flag. (Deprecated)
  InvalidBdFlag = 121,
  /// No request tag has been found for order: <OrderId>
  NoRequestTagForOrder = 122, // Placeholder
  /// No record is available for conid: <ConId> (Deprecated)
  NoRecordForConid = 123, // Placeholder
  /// No market rule is available for conid: <ConId>
  NoMarketRuleForConid = 124, // Placeholder
  /// Buy price must be the same as the best asking price.
  BuyPriceMustMatchBestAsk = 125,
  /// Sell price must be the same as the best bidding price.
  SellPriceMustMatchBestBid = 126,
  /// VWAP orders must be submitted at least three minutes before the start time.
  VwapOrderSubmitTimeViolation = 129,
  /// The sweep-to-fill flag and display size are only valid for US stocks routed through SMART, and will be ignored.
  SweepToFillDisplaySizeIgnored = 131,
  /// This order cannot be transmitted without a clearing account.
  MissingClearingAccount = 132,
  /// Submit new order failed.
  SubmitNewOrderFailed = 133,
  /// Modify order failed.
  ModifyOrderFailed = 134,
  /// Can't find order with ID = <OrderId>
  CannotFindOrderToCancel = 135, // Placeholder
  /// This order cannot be cancelled.
  OrderCannotBeCancelled = 136,
  /// VWAP orders can only be cancelled up to three minutes before the start time.
  VwapOrderCancelTimeViolation = 137,
  /// Could not parse ticker request: <Request>
  CouldNotParseTickerRequest = 138, // Placeholder
  /// Parsing error: <Error>
  ParsingError = 139, // Placeholder
  /// The size value should be an integer: <Value>
  SizeValueShouldBeInteger = 140, // Placeholder
  /// The price value should be a double: <Value>
  PriceValueShouldBeDouble = 141, // Placeholder
  /// Institutional customer account does not have account info
  InstitutionalAccountMissingInfo = 142,
  /// Requested ID is not an integer number.
  RequestIdNotInteger = 143,
  /// Order size does not match total share allocation. To adjust the share allocation, right-click on the order and select Modify > Share Allocation.
  OrderSizeAllocationMismatch = 144,
  /// Error in validating entry fields - <Field>
  ValidationErrorInEntryFields = 145, // Placeholder
  /// Invalid trigger method.
  InvalidTriggerMethod = 146,
  /// The conditional contract info is incomplete.
  ConditionalContractInfoIncomplete = 147,
  /// A conditional order can only be submitted when the order type is set to limit or market. (Deprecated)
  ConditionalOrderRequiresLimitMarket = 148,
  /// This order cannot be transmitted without a user name. (DDE specific)
  MissingUserNameDDE = 151,
  /// The "hidden" order attribute may not be specified for this order.
  HiddenAttributeNotAllowed = 152,
  /// EFPs can only be limit orders. (Deprecated)
  EfpRequiresLimitOrder = 153,
  /// Orders cannot be transmitted for a halted security.
  CannotTransmitOrderForHaltedSecurity = 154,
  /// A sizeOp order must have a user name and account. (Deprecated)
  SizeOpOrderRequiresUserAccount = 155,
  /// A SizeOp order must go to IBSX (Deprecated)
  SizeOpOrderRequiresIBSX = 156,
  /// An order can be EITHER Iceberg or Discretionary. Please remove either the Discretionary amount or the Display size.
  IcebergDiscretionaryConflict = 157,
  /// You must specify an offset amount or a percent offset value.
  MissingTrailOffset = 158,
  /// The percent offset value must be between 0% and 100%.
  PercentOffsetOutOfRange = 159,
  /// The size value cannot be zero.
  SizeValueCannotBeZero = 160,
  /// Cancel attempted when order is not in a cancellable state. Order permId = <PermId>
  CancelAttemptNotInCancellableState = 161, // Placeholder
  /// Historical market data Service error message. <Message>
  HistoricalDataServiceError = 162, // Placeholder
  /// The price specified would violate the percentage constraint specified in the default order settings.
  PriceViolatesPercentageConstraint = 163,
  /// There is no market data to check price percent violations.
  NoMarketDataForPricePercentCheck = 164,
  /// Historical market Data Service query message. <Message>
  HistoricalDataServiceQueryMessage = 165, // Placeholder
  /// HMDS Expired Contract Violation.
  HistoricalDataExpiredContractViolation = 166,
  /// VWAP order time must be in the future.
  VwapOrderTimeNotInFuture = 167,
  /// Discretionary amount does not conform to the minimum price variation for this contract.
  DiscretionaryAmountIncrementViolation = 168,
  /// No security definition has been found for the request.
  NoSecurityDefinitionFound = 200,
  /// The contract description specified for <Symbol> is ambiguous
  AmbiguousContract = 2001, // Using a distinct code as 200 is taken
  /// Order rejected - Reason: <Reason>
  OrderRejected = 201, // Placeholder
  /// Order cancelled - Reason: <Reason>
  OrderCancelled = 202, // Placeholder
  /// The security <security> is not available or allowed for this account.
  SecurityNotAvailableOrAllowed = 203,
  /// Can't find EId with ticker Id: <TickerId> (DDE specific)
  CannotFindEidWithTickerId = 300, // Placeholder
  /// Invalid ticker action: <Action> (DDE specific)
  InvalidTickerAction = 301, // Placeholder
  /// Error parsing stop ticker string: <String> (DDE specific)
  ErrorParsingStopTickerString = 302, // Placeholder
  /// Invalid action: <Action>
  InvalidAction = 303, // Placeholder
  /// Invalid account value action: <Action> (DDE specific)
  InvalidAccountValueAction = 304, // Placeholder
  /// Request parsing error, the request has been ignored. (DDE specific)
  RequestParsingErrorIgnored = 305,
  /// Error processing DDE request: <Request> (DDE specific)
  ErrorProcessingDdeRequest = 306, // Placeholder
  /// Invalid request topic: <Topic> (DDE specific)
  InvalidRequestTopic = 307, // Placeholder
  /// Unable to create the 'API' page in TWS as the maximum number of pages already exists.
  MaxApiPagesReached = 308,
  /// Max number (3) of market depth requests has been reached.
  MaxMarketDepthRequestsReached = 309,
  /// Can't find the subscribed market depth with tickerId: <TickerId>
  CannotFindSubscribedMarketDepth = 310, // Placeholder
  /// The origin is invalid.
  InvalidOrigin = 311,
  /// The combo details are invalid.
  InvalidComboDetails = 312,
  /// The combo details for leg '<leg number>' are invalid.
  InvalidComboLegDetails = 313, // Placeholder
  /// Security type 'BAG' requires combo leg details.
  BagSecurityTypeRequiresComboLegs = 314,
  /// Stock combo legs are restricted to SMART order routing.
  StockComboLegsRequireSmartRouting = 315,
  /// Market depth data has been HALTED. Please re-subscribe.
  MarketDepthDataHalted = 316,
  /// Market depth data has been RESET. Please empty deep book contents before applying any new entries.
  MarketDepthDataReset = 317,
  /// Invalid log level <log level>
  InvalidLogLevel = 319, // Placeholder
  /// Server error when reading an API client request.
  ServerErrorReadingApiClientRequest = 320,
  /// Server error when validating an API client request.
  ServerErrorValidatingApiClientRequest = 321,
  /// Server error when processing an API client request.
  ServerErrorProcessingApiClientRequest = 322,
  /// Server error: cause - s
  ServerErrorCause = 323, // Placeholder, 's' likely represents a specific cause string
  /// Server error when reading a DDE client request (missing information).
  ServerErrorReadingDdeClientRequest = 324,
  /// Discretionary orders are not supported for this combination of exchange and order type.
  DiscretionaryOrdersNotSupported = 325,
  /// Unable to connect as the client id is already in use. Retry with a unique client id.
  ClientIdInUse = 326,
  /// Only API connections with clientId set to 0 can set the auto bind TWS orders property.
  AutoBindRequiresClientIdZero = 327,
  /// Trailing stop orders can be attached to limit or stop-limit orders only.
  TrailStopAttachViolation = 328,
  /// Order modify failed. Cannot change to the new order type.
  OrderModifyFailedCannotChangeType = 329,
  /// Only FA or STL customers can request managed accounts list.
  ManagedAccountsListRequiresFaStl = 330,
  /// Internal error. FA or STL does not have any managed accounts.
  FaStlHasNoManagedAccounts = 331,
  /// The account codes for the order profile are invalid.
  InvalidAccountCodesForOrderProfile = 332,
  /// Invalid share allocation syntax.
  InvalidShareAllocationSyntax = 333,
  /// Invalid Good Till Date order
  InvalidGoodTillDateOrder = 334,
  /// Invalid delta: The delta must be between 0 and 100.
  InvalidDeltaRange = 335,
  /// The time or time zone is invalid. The correct format is hh:mm:ss xxx where xxx is an optionally specified time-zone. E.g.: 15:59:00 EST Note that there is a space between the time and the time zone. If no time zone is specified, local time is assumed.
  InvalidTimeOrTimeZoneFormat = 336,
  /// The date, time, or time-zone entered is invalid. The correct format is yyyymmdd hh:mm:ss xxx where yyyymmdd and xxx are optional. E.g.: 20031126 15:59:00 ESTNote that there is a space between the date and time, and between the time and time-zone.
  InvalidDateTimeOrTimeZoneFormat = 337,
  /// Good After Time orders are currently disabled on this exchange.
  GoodAfterTimeDisabled = 338,
  /// Futures spread are no longer supported. Please use combos instead.
  FuturesSpreadNotSupported = 339,
  /// Invalid improvement amount for box auction strategy.
  InvalidImprovementAmountBoxAuction = 340,
  /// Invalid delta. Valid values are from 1 to 100. You can set the delta from the "Pegged to Stock" section of the Order Ticket Panel, or by selecting Page/Layout from the main menu and adding the Delta column.
  InvalidDeltaValue1To100 = 341,
  /// Pegged order is not supported on this exchange.
  PeggedOrderNotSupported = 342,
  /// The date, time, or time-zone entered is invalid. The correct format is yyyymmdd hh:mm:ss xxx
  InvalidDateTimeOrTimeZoneFormatYmdHms = 343, // Slightly different format description than 337
  /// The account logged into is not a financial advisor account.
  NotFinancialAdvisorAccount = 344,
  /// Generic combo is not supported for FA advisor account.
  GenericComboNotSupportedForFA = 345,
  /// Not an institutional account or an away clearing account.
  NotInstitutionalOrAwayClearingAccount = 346,
  /// Short sale slot value must be 1 (broker holds shares) or 2 (delivered from elsewhere).
  InvalidShortSaleSlotValue = 347,
  /// Order not a short sale -- type must be SSHORT to specify short sale slot.
  ShortSaleSlotRequiresSshortAction = 348,
  /// Generic combo does not support "Good After" attribute.
  GenericComboDoesNotSupportGoodAfter = 349,
  /// Minimum quantity is not supported for best combo order.
  MinQuantityNotSupportedForBestCombo = 350,
  /// The "Regular Trading Hours only" flag is not valid for this order.
  RthOnlyFlagNotValid = 351,
  /// Short sale slot value of 2 (delivered from elsewhere) requires location.
  ShortSaleSlot2RequiresLocation = 352,
  /// Short sale slot value of 1 requires no location be specified.
  ShortSaleSlot1RequiresNoLocation = 353,
  /// Not subscribed to requested market data.
  NotSubscribedToMarketData = 354,
  /// Order size does not conform to market rule.
  OrderSizeMarketRuleViolation = 355,
  /// Smart-combo order does not support OCA group.
  SmartComboDoesNotSupportOca = 356,
  /// Your client version is out of date.
  ClientVersionOutOfDate = 357,
  /// Smart combo child order not supported.
  SmartComboChildOrderNotSupported = 358,
  /// Combo order only supports reduce on fill without block(OCA).
  ComboOrderReduceOnFillOcaViolation = 359,
  /// No whatif check support for smart combo order.
  NoWhatifSupportForSmartCombo = 360,
  /// Invalid trigger price.
  InvalidTriggerPrice = 361,
  /// Invalid adjusted stop price.
  InvalidAdjustedStopPrice = 362,
  /// Invalid adjusted stop limit price.
  InvalidAdjustedStopLimitPrice = 363,
  /// Invalid adjusted trailing amount.
  InvalidAdjustedTrailingAmount = 364,
  /// No scanner subscription found for ticker id: <TickerId>
  NoScannerSubscriptionFound = 365, // Placeholder
  /// No historical data query found for ticker id: <TickerId>
  NoHistoricalDataQueryFound = 366, // Placeholder
  /// Volatility type if set must be 1 or 2 for VOL orders. Do not set it for other order types.
  InvalidVolatilityTypeForVolOrder = 367,
  /// Reference Price Type must be 1 or 2 for dynamic volatility management. Do not set it for non-VOL orders.
  InvalidReferencePriceTypeForVolOrder = 368,
  /// Volatility orders are only valid for US options.
  VolatilityOrdersOnlyForUsOptions = 369,
  /// Dynamic Volatility orders must be SMART routed, or trade on a Price Improvement Exchange.
  DynamicVolatilityRoutingViolation = 370,
  /// VOL order requires positive floating point value for volatility. Do not set it for other order types.
  VolOrderRequiresPositiveVolatility = 371,
  /// Cannot set dynamic VOL attribute on non-VOL order.
  CannotSetDynamicVolOnNonVolOrder = 372,
  /// Can only set stock range attribute on VOL or RELATIVE TO STOCK order.
  StockRangeAttributeRequiresVolOrRel = 373,
  /// If both are set, the lower stock range attribute must be less than the upper stock range attribute.
  InvalidStockRangeAttributesOrder = 374,
  /// Stock range attributes cannot be negative.
  StockRangeAttributesCannotBeNegative = 375,
  /// The order is not eligible for continuous update. The option must trade on a cheap-to-reroute exchange.
  NotEligibleForContinuousUpdate = 376,
  /// Must specify valid delta hedge order aux. price.
  MustSpecifyValidDeltaHedgeAuxPrice = 377,
  /// Delta hedge order type requires delta hedge aux. price to be specified.
  DeltaHedgeOrderRequiresAuxPrice = 378,
  /// Delta hedge order type requires that no delta hedge aux. price be specified.
  DeltaHedgeOrderRequiresNoAuxPrice = 379,
  /// This order type is not allowed for delta hedge orders.
  OrderTypeNotAllowedForDeltaHedge = 380,
  /// Your DDE.dll needs to be upgraded. (DDE specific)
  DdeDllNeedsUpgrade = 381,
  /// The price specified violates the number of ticks constraint specified in the default order settings.
  PriceViolatesTicksConstraint = 382,
  /// The size specified violates the size constraint specified in the default order settings.
  SizeViolatesSizeConstraint = 383,
  /// Invalid DDE array request. (DDE specific)
  InvalidDdeArrayRequest = 384,
  /// Duplicate ticker ID for API scanner subscription.
  DuplicateTickerIdApiScanner = 385,
  /// Duplicate ticker ID for API historical data query.
  DuplicateTickerIdApiHistorical = 386,
  /// Unsupported order type for this exchange and security type.
  UnsupportedOrderTypeForExchangeSecType = 387,
  /// Order size is smaller than the minimum requirement.
  OrderSizeSmallerThanMinimum = 388,
  /// Supplied routed order ID is not unique.
  RoutedOrderIdNotUnique = 389,
  /// Supplied routed order ID is invalid.
  RoutedOrderIdInvalid = 390,
  /// The time or time-zone entered is invalid. The correct format is hh:mm:ss xxx
  InvalidTimeOrTimeZoneFormatHms = 391, // Slightly different format description than 336
  /// Invalid order: contract expired.
  InvalidOrderContractExpired = 392,
  /// Short sale slot may be specified for delta hedge orders only.
  ShortSaleSlotOnlyForDeltaHedge = 393,
  /// Invalid Process Time: must be integer number of milliseconds between 100 and 2000. Found: <Value>
  InvalidProcessTime = 394, // Placeholder
  /// Due to system problems, orders with OCA groups are currently not being accepted.
  OcaOrdersNotAcceptedSystemProblem = 395,
  /// Due to system problems, application is currently accepting only Market and Limit orders for this contract.
  OnlyMarketLimitOrdersAcceptedSystemProblem = 396, // Note: 397 has same message
  /// <Condition> cannot be used as a condition trigger.
  InvalidConditionTrigger = 398, // Placeholder
  /// Order message error <ErrorCode>
  OrderMessageError = 399, // Placeholder
  /// Algo order error. <Message>
  AlgoOrderError = 400, // Placeholder
  /// Length restriction. <Field> <MaxLen>
  LengthRestriction = 401, // Placeholder
  /// Conditions are not allowed for this contract.
  ConditionsNotAllowedForContract = 402,
  /// Invalid stop price.
  InvalidStopPrice = 403,
  /// Shares for this order are not immediately available for short sale. The order will be held while we attempt to locate the shares.
  ShortSaleSharesNotAvailable = 404,
  /// The child order quantity should be equivalent to the parent order size. (Deprecated)
  ChildQuantityShouldMatchParentSize = 405,
  /// The currency <Currency> is not allowed.
  CurrencyNotAllowed = 406, // Placeholder
  /// The symbol should contain valid non-unicode characters only.
  SymbolRequiresNonUnicode = 407,
  /// Invalid scale order increment.
  InvalidScaleOrderIncrement = 408,
  /// Invalid scale order. You must specify order component size.
  InvalidScaleOrderMissingComponentSize = 409,
  /// Invalid subsequent component size for scale order.
  InvalidScaleOrderSubsequentComponentSize = 410,
  /// The "Outside Regular Trading Hours" flag is not valid for this order.
  OutsideRthFlagNotValidForOrder = 411, // Note: Similar to 351 but different wording
  /// The contract is not available for trading.
  ContractNotAvailableForTrading = 412,
  /// What-if order should have the transmit flag set to true.
  WhatIfOrderRequiresTransmitTrue = 413,
  /// Snapshot market data subscription is not applicable to generic ticks.
  SnapshotNotApplicableToGenericTicks = 414,
  /// Wait until previous RFQ finishes and try again.
  WaitPreviousRfqFinish = 415,
  /// RFQ is not applicable for the contract. Order ID: <OrderId>
  RfqNotApplicableForContract = 416, // Placeholder
  /// Invalid initial component size for scale order.
  InvalidScaleOrderInitialComponentSize = 417, // Note: Similar to 409 but different wording
  /// Invalid scale order profit offset.
  InvalidScaleOrderProfitOffset = 418,
  /// Missing initial component size for scale order.
  MissingScaleOrderInitialComponentSize = 419,
  /// Invalid real-time query.
  InvalidRealTimeQuery = 420,
  /// Invalid route. (Deprecated)
  InvalidRoute = 421,
  /// The account and clearing attributes on this order may not be changed.
  CannotChangeAccountClearingAttributes = 422,
  /// Cross order RFQ has been expired. THI committed size is no longer available. Please open order dialog and verify liquidity allocation.
  CrossOrderRfqExpired = 423,
  /// FA Order requires allocation to be specified. (Deprecated)
  FaOrderRequiresAllocation = 424,
  /// FA Order requires per-account manual allocations because there is no common clearing instruction. Please use order dialog Adviser tab to enter the allocation. (Deprecated)
  FaOrderRequiresManualAllocation = 425,
  /// None of the accounts have enough shares.
  NoAccountHasEnoughShares = 426,
  /// Mutual Fund order requires monetary value to be specified. (Deprecated)
  MutualFundOrderRequiresMonetaryValue = 427,
  /// Mutual Fund Sell order requires shares to be specified. (Deprecated)
  MutualFundSellOrderRequiresShares = 428,
  /// Delta neutral orders are only supported for combos (BAG security type).
  DeltaNeutralOnlyForCombos = 429,
  /// We are sorry, but fundamentals data for the security specified is not available.
  FundamentalsDataNotAvailable = 430,
  /// What to show field is missing or incorrect. (Deprecated)
  WhatToShowMissingOrIncorrect = 431,
  /// Commission must not be negative. (Deprecated)
  CommissionMustNotBeNegative = 432,
  /// Invalid "Restore size after taking profit" for multiple account allocation scale order.
  InvalidRestoreSizeAfterProfit = 433,
  /// The order size cannot be zero.
  OrderSizeCannotBeZero = 434, // Note: Same message as 160
  /// You must specify an account.
  MustSpecifyAccount = 435,
  /// You must specify an allocation (either a single account, group, or profile).
  MustSpecifyAllocation = 436,
  /// Order can have only one flag Outside RTH or Allow PreOpen. (Deprecated)
  OnlyOneOutsideRthOrAllowPreOpen = 437,
  /// The application is now locked. (Deprecated)
  ApplicationLocked = 438,
  /// Order processing failed. Algorithm definition not found.
  AlgoDefinitionNotFound = 439,
  /// Order modify failed. Algorithm cannot be modified.
  AlgoCannotBeModified = 440,
  /// Algo attributes validation failed: <Reason>
  AlgoAttributesValidationFailed = 441, // Placeholder
  /// Specified algorithm is not allowed for this order.
  AlgoNotAllowedForOrder = 442,
  /// Order processing failed. Unknown algo attribute.
  UnknownAlgoAttribute = 443,
  /// Volatility Combo order is not yet acknowledged. Cannot submit changes at this time.
  VolComboOrderNotAcknowledged = 444,
  /// The RFQ for this order is no longer valid.
  RfqNoLongerValid = 445,
  /// Missing scale order profit offset.
  MissingScaleOrderProfitOffset = 446, // Note: Same message as 418? Check context.
  /// Missing scale price adjustment amount or interval.
  MissingScalePriceAdjustment = 447,
  /// Invalid scale price adjustment interval.
  InvalidScalePriceAdjustmentInterval = 448,
  /// Unexpected scale price adjustment amount or interval.
  UnexpectedScalePriceAdjustment = 449,
  // Note: 507 Bad Message Length is already defined in the client section

  // --- TWS Error Codes (10000+) ---
  /// Cross currency combo error.
  CrossCurrencyComboError = 10000,
  /// Cross currency vol error.
  CrossCurrencyVolError = 10001,
  /// Invalid non-guaranteed legs.
  InvalidNonGuaranteedLegs = 10002,
  /// IBSX not allowed.
  IbsxNotAllowed = 10003,
  /// Read-only models.
  ReadOnlyModels = 10005,
  /// Missing parent order.
  MissingParentOrder = 10006,
  /// Invalid hedge type.
  InvalidHedgeType = 10007,
  /// Invalid beta value.
  InvalidBetaValue = 10008,
  /// Invalid hedge ratio.
  InvalidHedgeRatio = 10009,
  /// Invalid delta hedge order.
  InvalidDeltaHedgeOrder = 10010,
  /// Currency is not supported for Smart combo.
  CurrencyNotSupportedForSmartCombo = 10011,
  /// Invalid allocation percentage
  InvalidAllocationPercentage = 10012,
  /// Smart routing API error (Smart routing opt-out required).
  SmartRoutingApiErrorOptOutRequired = 10013,
  /// PctChange limits. (Deprecated)
  PctChangeLimits = 10014,
  /// Trading is not allowed in the API.
  TradingNotAllowedInApi = 10015,
  /// Contract is not visible. (Deprecated)
  ContractNotVisible = 10016,
  /// Contracts are not visible. (Deprecated)
  ContractsNotVisible = 10017,
  /// Orders use EV warning.
  OrdersUseEvWarning = 10018,
  /// Trades use EV warning.
  TradesUseEvWarning = 10019,
  /// Display size should be smaller than order size.
  DisplaySizeShouldBeSmaller = 10020,
  /// Invalid leg2 to Mkt Offset API. (Deprecated)
  InvalidLeg2ToMktOffsetApi = 10021,
  /// Invalid Leg Prio API. (Deprecated)
  InvalidLegPrioApi = 10022,
  /// Invalid combo display size API. (Deprecated)
  InvalidComboDisplaySizeApi = 10023,
  /// Invalid don't start next legin API. (Deprecated)
  InvalidDontStartNextLeginApi = 10024,
  /// Invalid leg2 to Mkt time1 API. (Deprecated)
  InvalidLeg2ToMktTime1Api = 10025,
  /// Invalid leg2 to Mkt time2 API. (Deprecated)
  InvalidLeg2ToMktTime2Api = 10026,
  /// Invalid combo routing tag API. (Deprecated)
  InvalidComboRoutingTagApi = 10027,
  /// Part of requested market data is not subscribed.
  NotSubscribedMarketData = 10089,
  /// Part of requested market data is not subscribed.
  PartiallySubscribedMarketData = 10090,
  /// OrderId <OrderId> that needs to be cancelled can not be cancelled, state: <State>
  CannotCancelFilledOrder = 10148, // Placeholder
  /// Requested market data is not subscribed. Delayed market data is not enabled
  MarketDataNotSubscribedDelayedDisabled = 10186,
  /// No market data during competing session
  NoMarketDataDuringCompetingSession = 10197,
  /// Bust event occurred, current subscription is deactivated. Please resubscribe real-time bars immediately
  BustEventDeactivatedSubscription = 10225,
  /// You have unsaved FA changes. Please retry 'request FA' operation later, when 'replace FA' operation is complete
  UnsavedFaChanges = 10230,
  /// The following Groups and/or Profiles contain invalid accounts: <list of groups/profiles>
  FaGroupsProfilesContainInvalidAccounts = 10231, // Placeholder
  /// Defaults were inherited from CASH preset during the creation of this order.
  DefaultsInheritedFromCashPreset = 10233,
  /// The Decision Maker field is required and not set for this order (non-desktop).
  DecisionMakerRequiredNonDesktop = 10234,
  /// The Decision Maker field is required and not set for this order (ibbot).
  DecisionMakerRequiredIbbot = 10235,
  /// Child has to be AON if parent order is AON
  ChildMustBeAonIfParentAon = 10236,
  /// All or None ticket can route entire unfilled size only
  AonTicketCanRouteEntireUnfilledOnly = 10237,
  /// Some error occured during communication with Advisor Setup web-app
  AdvisorSetupWebAppCommunicationError = 10238,
  /// This order will affect one or more accounts that are flagged because they do not fit the required risk score criteria prescribed by the group/profile/model allocation.
  OrderAffectsFlaggedAccountsRiskScore = 10239,
  /// You must enter a valid Price Cap.
  MustEnterValidPriceCap = 10240,
  /// Order Quantity is expressed in monetary terms. Modification is not supported via API. Please use desktop version to revise this order.
  MonetaryQuantityModificationNotSupported = 10241,
  /// Fractional-sized order cannot be modified via API. Please use desktop version to revise this order.
  FractionalOrderModificationNotSupported = 10242,
  /// Fractional-sized order cannot be placed via API. Please use desktop version to place this order.
  FractionalOrderPlacementNotSupported = 10243,
  /// Cash Quantity cannot be used for this order
  CashQuantityNotAllowedForOrder = 10244,
  /// This financial instrument does not support fractional shares trading
  InstrumentDoesNotSupportFractional = 10245,
  /// This order doesn't support fractional shares trading
  OrderDoesNotSupportFractional = 10246,
  /// Only IB SmartRouting supports fractional shares
  OnlySmartRoutingSupportsFractional = 10247,
  /// <Account> doesn't have permission to trade fractional shares
  AccountDoesNotHaveFractionalPermission = 10248, // Placeholder
  /// <Order type>="" order doesn't support fractional shares
  OrderTypeDoesNotSupportFractional = 10249, // Placeholder
  /// The size does not conform to the minimum variation of for this contract
  SizeDoesNotConformToMinVariation = 10250,
  /// Fractional shares are not supported for allocation orders
  FractionalNotSupportedForAllocationOrders = 10251,
  /// This non-close-position order doesn't support fractional shares trading
  NonClosePositionOrderDoesNotSupportFractional = 10252,
  /// Clear Away orders are not supported for multi-leg combo with attached hedge.
  ClearAwayNotSupportedForMultiLegHedge = 10253,
  /// Invalid Order: bond expired
  InvalidOrderBondExpired = 10254,
  /// The 'EtradeOnly' order attribute is not supported
  EtradeOnlyAttributeNotSupported = 10268,
  /// The 'firmQuoteOnly' order attribute is not supported
  FirmQuoteOnlyAttributeNotSupported = 10269,
  /// The 'nbboPriceCap' order attribute is not supported
  NbboPriceCapAttributeNotSupported = 10270,
  /// News feed is not allowed
  NewsFeedNotAllowed = 10276,
  /// News feed permissions required
  NewsFeedPermissionsRequired = 10277,
  /// Duplicate WSH metadata request
  DuplicateWshMetadataRequest = 10278,
  /// Failed request WSH metadata
  FailedRequestWshMetadata = 10279,
  /// Failed cancel WSH metadata
  FailedCancelWshMetadata = 10280,
  /// Duplicate WSH event data request
  DuplicateWshEventDataRequest = 10281,
  /// WSH metadata not requested
  WshMetadataNotRequested = 10282,
  /// Fail request WSH event data
  FailRequestWshEventData = 10283,
  /// Fail cancel WSH event data
  FailCancelWshEventData = 10284,

  // --- Placeholder for unknown codes ---
  /// Unknown error code received from TWS/IBG or client library.
  UnknownCode = 0, // Or another suitable default/sentinel value
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
      // System Messages
      ClientErrorCode::ConnectivityLost => "Connectivity between IB and the TWS has been lost.",
      ClientErrorCode::ConnectivityRestoredDataLost => "Connectivity between IB and TWS has been restored- data lost.",
      ClientErrorCode::ConnectivityRestoredDataMaintained => "Connectivity between IB and TWS has been restored- data maintained.",
      ClientErrorCode::SocketPortReset => "TWS socket port has been reset and this connection is being dropped. Please reconnect on the new port - <port_num>",
      // Warnings
      ClientErrorCode::AccountUpdateSubscriptionOverridden => "New account data requested from TWS. API client has been unsubscribed from account data.",
      ClientErrorCode::AccountUpdateSubscriptionRejected => "Unable to subscribe to account as the following clients are subscribed to a different account.",
      ClientErrorCode::OrderModificationRejectedProcessing => "Unable to modify this order as it is still being processed.",
      ClientErrorCode::MarketDataFarmDisconnected => "A market data farm is disconnected.",
      ClientErrorCode::MarketDataFarmConnected => "Market data farm connection is OK",
      ClientErrorCode::HistoricalDataFarmDisconnected => "A historical data farm is disconnected.",
      ClientErrorCode::HistoricalDataFarmConnected => "A historical data farm is connected.",
      ClientErrorCode::HistoricalDataFarmInactive => "A historical data farm connection has become inactive but should be available upon demand.",
      ClientErrorCode::MarketDataFarmInactive => "A market data farm connection has become inactive but should be available upon demand.",
      ClientErrorCode::OutsideRthAttributeIgnored => "Order Event Warning: Attribute \"Outside Regular Trading Hours\" is ignored based on the order type and destination. PlaceOrder is now processed.",
      ClientErrorCode::TwsToServerConnectionBroken => "Connectivity between TWS and server is broken. It will be restored automatically.",
      ClientErrorCode::CrossSideWarning => "Cross Side Warning",
      ClientErrorCode::SecurityDefinitionDataFarmConnected => "Sec-def data farm connection is OK",
      ClientErrorCode::EtradeOnlyNotSupportedWarning => "Etrade Only Not Supported Warning",
      ClientErrorCode::FirmQuoteOnlyNotSupportedWarning => "Firm Quote Only Not Supported Warning",
      // TWS Errors (1xx-4xx)
      ClientErrorCode::MaxMessagesPerSecondExceeded => "Max rate of messages per second has been exceeded.",
      ClientErrorCode::MaxTickersReached => "Max number of tickers has been reached.",
      ClientErrorCode::DuplicateTickerId => "Duplicate ticker ID.",
      ClientErrorCode::DuplicateOrderId => "Duplicate order ID.",
      ClientErrorCode::CannotModifyFilledOrder => "Can't modify a filled order.",
      ClientErrorCode::OrderModificationMismatch => "Order being modified does not match original order.",
      ClientErrorCode::CannotTransmitOrderId => "Can't transmit order ID: <OrderId>",
      ClientErrorCode::CannotTransmitIncompleteOrder => "Cannot transmit incomplete order.",
      ClientErrorCode::PriceOutOfPercentageRange => "Price is out of the range defined by the Percentage setting at order defaults frame. The order will not be transmitted.",
      ClientErrorCode::PriceIncrementViolation => "The price does not conform to the minimum price variation for this contract.",
      ClientErrorCode::TifOrderTypeIncompatible => "The TIF (Tif type) and the order type are incompatible.",
      ClientErrorCode::TifRequiresDayForMocLoc => "The Tif option should be set to DAY for MOC and LOC orders.",
      ClientErrorCode::RelativeOrdersStocksOnly => "Relative orders are valid for stocks only.",
      ClientErrorCode::RelativeOrdersUsStocksRouting => "Relative orders for US stocks can only be submitted to SMART, SMART_ECN, INSTINET, or PRIMEX.",
      ClientErrorCode::CannotTransmitToDeadExchange => "The order cannot be transmitted to a dead exchange.",
      ClientErrorCode::BlockOrderSizeTooSmall => "The block order size must be at least 50.",
      ClientErrorCode::VwapOrdersRequireVwapExchange => "VWAP orders must be routed through the VWAP exchange.",
      ClientErrorCode::OnlyVwapOrdersOnVwapExchange => "Only VWAP orders may be placed on the VWAP exchange.",
      ClientErrorCode::TooLateForVwapOrder => "It is too late to place a VWAP order for today.",
      ClientErrorCode::InvalidBdFlag => "Invalid BD flag for the order. Check \"Destination\" and \"BD\" flag.",
      ClientErrorCode::NoRequestTagForOrder => "No request tag has been found for order: <OrderId>",
      ClientErrorCode::NoRecordForConid => "No record is available for conid: <ConId>",
      ClientErrorCode::NoMarketRuleForConid => "No market rule is available for conid: <ConId>",
      ClientErrorCode::BuyPriceMustMatchBestAsk => "Buy price must be the same as the best asking price.",
      ClientErrorCode::SellPriceMustMatchBestBid => "Sell price must be the same as the best bidding price.",
      ClientErrorCode::VwapOrderSubmitTimeViolation => "VWAP orders must be submitted at least three minutes before the start time.",
      ClientErrorCode::SweepToFillDisplaySizeIgnored => "The sweep-to-fill flag and display size are only valid for US stocks routed through SMART, and will be ignored.",
      ClientErrorCode::MissingClearingAccount => "This order cannot be transmitted without a clearing account.",
      ClientErrorCode::SubmitNewOrderFailed => "Submit new order failed.",
      ClientErrorCode::ModifyOrderFailed => "Modify order failed.",
      ClientErrorCode::CannotFindOrderToCancel => "Can't find order with ID = <OrderId>",
      ClientErrorCode::OrderCannotBeCancelled => "This order cannot be cancelled.",
      ClientErrorCode::VwapOrderCancelTimeViolation => "VWAP orders can only be cancelled up to three minutes before the start time.",
      ClientErrorCode::CouldNotParseTickerRequest => "Could not parse ticker request: <Request>",
      ClientErrorCode::ParsingError => "Parsing error: <Error>",
      ClientErrorCode::SizeValueShouldBeInteger => "The size value should be an integer: <Value>",
      ClientErrorCode::PriceValueShouldBeDouble => "The price value should be a double: <Value>",
      ClientErrorCode::InstitutionalAccountMissingInfo => "Institutional customer account does not have account info",
      ClientErrorCode::RequestIdNotInteger => "Requested ID is not an integer number.",
      ClientErrorCode::OrderSizeAllocationMismatch => "Order size does not match total share allocation. To adjust the share allocation, right-click on the order and select Modify > Share Allocation.",
      ClientErrorCode::ValidationErrorInEntryFields => "Error in validating entry fields - <Field>",
      ClientErrorCode::InvalidTriggerMethod => "Invalid trigger method.",
      ClientErrorCode::ConditionalContractInfoIncomplete => "The conditional contract info is incomplete.",
      ClientErrorCode::ConditionalOrderRequiresLimitMarket => "A conditional order can only be submitted when the order type is set to limit or market.",
      ClientErrorCode::MissingUserNameDDE => "This order cannot be transmitted without a user name.",
      ClientErrorCode::HiddenAttributeNotAllowed => "The \"hidden\" order attribute may not be specified for this order.",
      ClientErrorCode::EfpRequiresLimitOrder => "EFPs can only be limit orders.",
      ClientErrorCode::CannotTransmitOrderForHaltedSecurity => "Orders cannot be transmitted for a halted security.",
      ClientErrorCode::SizeOpOrderRequiresUserAccount => "A sizeOp order must have a user name and account.",
      ClientErrorCode::SizeOpOrderRequiresIBSX => "A SizeOp order must go to IBSX",
      ClientErrorCode::IcebergDiscretionaryConflict => "An order can be EITHER Iceberg or Discretionary. Please remove either the Discretionary amount or the Display size.",
      ClientErrorCode::MissingTrailOffset => "You must specify an offset amount or a percent offset value.",
      ClientErrorCode::PercentOffsetOutOfRange => "The percent offset value must be between 0% and 100%.",
      ClientErrorCode::SizeValueCannotBeZero => "The size value cannot be zero.",
      ClientErrorCode::CancelAttemptNotInCancellableState => "Cancel attempted when order is not in a cancellable state. Order permId = <PermId>",
      ClientErrorCode::HistoricalDataServiceError => "Historical market data Service error message. <Message>",
      ClientErrorCode::PriceViolatesPercentageConstraint => "The price specified would violate the percentage constraint specified in the default order settings.",
      ClientErrorCode::NoMarketDataForPricePercentCheck => "There is no market data to check price percent violations.",
      ClientErrorCode::HistoricalDataServiceQueryMessage => "Historical market Data Service query message. <Message>",
      ClientErrorCode::HistoricalDataExpiredContractViolation => "HMDS Expired Contract Violation.",
      ClientErrorCode::VwapOrderTimeNotInFuture => "VWAP order time must be in the future.",
      ClientErrorCode::DiscretionaryAmountIncrementViolation => "Discretionary amount does not conform to the minimum price variation for this contract.",
      ClientErrorCode::NoSecurityDefinitionFound => "No security definition has been found for the request.",
      ClientErrorCode::AmbiguousContract => "The contract description specified for <Symbol> is ambiguous",
      ClientErrorCode::OrderRejected => "Order rejected - Reason: <Reason>",
      ClientErrorCode::OrderCancelled => "Order cancelled - Reason: <Reason>",
      ClientErrorCode::SecurityNotAvailableOrAllowed => "The security <security> is not available or allowed for this account.",
      ClientErrorCode::CannotFindEidWithTickerId => "Can't find EId with ticker Id: <TickerId>",
      ClientErrorCode::InvalidTickerAction => "Invalid ticker action: <Action>",
      ClientErrorCode::ErrorParsingStopTickerString => "Error parsing stop ticker string: <String>",
      ClientErrorCode::InvalidAction => "Invalid action: <Action>",
      ClientErrorCode::InvalidAccountValueAction => "Invalid account value action: <Action>",
      ClientErrorCode::RequestParsingErrorIgnored => "Request parsing error, the request has been ignored.",
      ClientErrorCode::ErrorProcessingDdeRequest => "Error processing DDE request: <Request>",
      ClientErrorCode::InvalidRequestTopic => "Invalid request topic: <Topic>",
      ClientErrorCode::MaxApiPagesReached => "Unable to create the 'API' page in TWS as the maximum number of pages already exists.",
      ClientErrorCode::MaxMarketDepthRequestsReached => "Max number (3) of market depth requests has been reached.",
      ClientErrorCode::CannotFindSubscribedMarketDepth => "Can't find the subscribed market depth with tickerId: <TickerId>",
      ClientErrorCode::InvalidOrigin => "The origin is invalid.",
      ClientErrorCode::InvalidComboDetails => "The combo details are invalid.",
      ClientErrorCode::InvalidComboLegDetails => "The combo details for leg '<leg number>' are invalid.",
      ClientErrorCode::BagSecurityTypeRequiresComboLegs => "Security type 'BAG' requires combo leg details.",
      ClientErrorCode::StockComboLegsRequireSmartRouting => "Stock combo legs are restricted to SMART order routing.",
      ClientErrorCode::MarketDepthDataHalted => "Market depth data has been HALTED. Please re-subscribe.",
      ClientErrorCode::MarketDepthDataReset => "Market depth data has been RESET. Please empty deep book contents before applying any new entries.",
      ClientErrorCode::InvalidLogLevel => "Invalid log level <log level>",
      ClientErrorCode::ServerErrorReadingApiClientRequest => "Server error when reading an API client request.",
      ClientErrorCode::ServerErrorValidatingApiClientRequest => "Server error when validating an API client request.",
      ClientErrorCode::ServerErrorProcessingApiClientRequest => "Server error when processing an API client request.",
      ClientErrorCode::ServerErrorCause => "Server error: cause - s",
      ClientErrorCode::ServerErrorReadingDdeClientRequest => "Server error when reading a DDE client request (missing information).",
      ClientErrorCode::DiscretionaryOrdersNotSupported => "Discretionary orders are not supported for this combination of exchange and order type.",
      ClientErrorCode::ClientIdInUse => "Unable to connect as the client id is already in use. Retry with a unique client id.",
      ClientErrorCode::AutoBindRequiresClientIdZero => "Only API connections with clientId set to 0 can set the auto bind TWS orders property.",
      ClientErrorCode::TrailStopAttachViolation => "Trailing stop orders can be attached to limit or stop-limit orders only.",
      ClientErrorCode::OrderModifyFailedCannotChangeType => "Order modify failed. Cannot change to the new order type.",
      ClientErrorCode::ManagedAccountsListRequiresFaStl => "Only FA or STL customers can request managed accounts list.",
      ClientErrorCode::FaStlHasNoManagedAccounts => "Internal error. FA or STL does not have any managed accounts.",
      ClientErrorCode::InvalidAccountCodesForOrderProfile => "The account codes for the order profile are invalid.",
      ClientErrorCode::InvalidShareAllocationSyntax => "Invalid share allocation syntax.",
      ClientErrorCode::InvalidGoodTillDateOrder => "Invalid Good Till Date order",
      ClientErrorCode::InvalidDeltaRange => "Invalid delta: The delta must be between 0 and 100.",
      ClientErrorCode::InvalidTimeOrTimeZoneFormat => "The time or time zone is invalid. The correct format is hh:mm:ss xxx where xxx is an optionally specified time-zone. E.g.: 15:59:00 EST Note that there is a space between the time and the time zone. If no time zone is specified, local time is assumed.",
      ClientErrorCode::InvalidDateTimeOrTimeZoneFormat => "The date, time, or time-zone entered is invalid. The correct format is yyyymmdd hh:mm:ss xxx where yyyymmdd and xxx are optional. E.g.: 20031126 15:59:00 ESTNote that there is a space between the date and time, and between the time and time-zone.",
      ClientErrorCode::GoodAfterTimeDisabled => "Good After Time orders are currently disabled on this exchange.",
      ClientErrorCode::FuturesSpreadNotSupported => "Futures spread are no longer supported. Please use combos instead.",
      ClientErrorCode::InvalidImprovementAmountBoxAuction => "Invalid improvement amount for box auction strategy.",
      ClientErrorCode::InvalidDeltaValue1To100 => "Invalid delta. Valid values are from 1 to 100. You can set the delta from the \"Pegged to Stock\" section of the Order Ticket Panel, or by selecting Page/Layout from the main menu and adding the Delta column.",
      ClientErrorCode::PeggedOrderNotSupported => "Pegged order is not supported on this exchange.",
      ClientErrorCode::InvalidDateTimeOrTimeZoneFormatYmdHms => "The date, time, or time-zone entered is invalid. The correct format is yyyymmdd hh:mm:ss xxx",
      ClientErrorCode::NotFinancialAdvisorAccount => "The account logged into is not a financial advisor account.",
      ClientErrorCode::GenericComboNotSupportedForFA => "Generic combo is not supported for FA advisor account.",
      ClientErrorCode::NotInstitutionalOrAwayClearingAccount => "Not an institutional account or an away clearing account.",
      ClientErrorCode::InvalidShortSaleSlotValue => "Short sale slot value must be 1 (broker holds shares) or 2 (delivered from elsewhere).",
      ClientErrorCode::ShortSaleSlotRequiresSshortAction => "Order not a short sale -- type must be SSHORT to specify short sale slot.",
      ClientErrorCode::GenericComboDoesNotSupportGoodAfter => "Generic combo does not support \"Good After\" attribute.",
      ClientErrorCode::MinQuantityNotSupportedForBestCombo => "Minimum quantity is not supported for best combo order.",
      ClientErrorCode::RthOnlyFlagNotValid => "The \"Regular Trading Hours only\" flag is not valid for this order.",
      ClientErrorCode::ShortSaleSlot2RequiresLocation => "Short sale slot value of 2 (delivered from elsewhere) requires location.",
      ClientErrorCode::ShortSaleSlot1RequiresNoLocation => "Short sale slot value of 1 requires no location be specified.",
      ClientErrorCode::NotSubscribedToMarketData => "Not subscribed to requested market data.",
      ClientErrorCode::OrderSizeMarketRuleViolation => "Order size does not conform to market rule.",
      ClientErrorCode::SmartComboDoesNotSupportOca => "Smart-combo order does not support OCA group.",
      ClientErrorCode::ClientVersionOutOfDate => "Your client version is out of date.",
      ClientErrorCode::SmartComboChildOrderNotSupported => "Smart combo child order not supported.",
      ClientErrorCode::ComboOrderReduceOnFillOcaViolation => "Combo order only supports reduce on fill without block(OCA).",
      ClientErrorCode::NoWhatifSupportForSmartCombo => "No whatif check support for smart combo order.",
      ClientErrorCode::InvalidTriggerPrice => "Invalid trigger price.",
      ClientErrorCode::InvalidAdjustedStopPrice => "Invalid adjusted stop price.",
      ClientErrorCode::InvalidAdjustedStopLimitPrice => "Invalid adjusted stop limit price.",
      ClientErrorCode::InvalidAdjustedTrailingAmount => "Invalid adjusted trailing amount.",
      ClientErrorCode::NoScannerSubscriptionFound => "No scanner subscription found for ticker id: <TickerId>",
      ClientErrorCode::NoHistoricalDataQueryFound => "No historical data query found for ticker id: <TickerId>",
      ClientErrorCode::InvalidVolatilityTypeForVolOrder => "Volatility type if set must be 1 or 2 for VOL orders. Do not set it for other order types.",
      ClientErrorCode::InvalidReferencePriceTypeForVolOrder => "Reference Price Type must be 1 or 2 for dynamic volatility management. Do not set it for non-VOL orders.",
      ClientErrorCode::VolatilityOrdersOnlyForUsOptions => "Volatility orders are only valid for US options.",
      ClientErrorCode::DynamicVolatilityRoutingViolation => "Dynamic Volatility orders must be SMART routed, or trade on a Price Improvement Exchange.",
      ClientErrorCode::VolOrderRequiresPositiveVolatility => "VOL order requires positive floating point value for volatility. Do not set it for other order types.",
      ClientErrorCode::CannotSetDynamicVolOnNonVolOrder => "Cannot set dynamic VOL attribute on non-VOL order.",
      ClientErrorCode::StockRangeAttributeRequiresVolOrRel => "Can only set stock range attribute on VOL or RELATIVE TO STOCK order.",
      ClientErrorCode::InvalidStockRangeAttributesOrder => "If both are set, the lower stock range attribute must be less than the upper stock range attribute.",
      ClientErrorCode::StockRangeAttributesCannotBeNegative => "Stock range attributes cannot be negative.",
      ClientErrorCode::NotEligibleForContinuousUpdate => "The order is not eligible for continuous update. The option must trade on a cheap-to-reroute exchange.",
      ClientErrorCode::MustSpecifyValidDeltaHedgeAuxPrice => "Must specify valid delta hedge order aux. price.",
      ClientErrorCode::DeltaHedgeOrderRequiresAuxPrice => "Delta hedge order type requires delta hedge aux. price to be specified.",
      ClientErrorCode::DeltaHedgeOrderRequiresNoAuxPrice => "Delta hedge order type requires that no delta hedge aux. price be specified.",
      ClientErrorCode::OrderTypeNotAllowedForDeltaHedge => "This order type is not allowed for delta hedge orders.",
      ClientErrorCode::DdeDllNeedsUpgrade => "Your DDE.dll needs to be upgraded.",
      ClientErrorCode::PriceViolatesTicksConstraint => "The price specified violates the number of ticks constraint specified in the default order settings.",
      ClientErrorCode::SizeViolatesSizeConstraint => "The size specified violates the size constraint specified in the default order settings.",
      ClientErrorCode::InvalidDdeArrayRequest => "Invalid DDE array request.",
      ClientErrorCode::DuplicateTickerIdApiScanner => "Duplicate ticker ID for API scanner subscription.",
      ClientErrorCode::DuplicateTickerIdApiHistorical => "Duplicate ticker ID for API historical data query.",
      ClientErrorCode::UnsupportedOrderTypeForExchangeSecType => "Unsupported order type for this exchange and security type.",
      ClientErrorCode::OrderSizeSmallerThanMinimum => "Order size is smaller than the minimum requirement.",
      ClientErrorCode::RoutedOrderIdNotUnique => "Supplied routed order ID is not unique.",
      ClientErrorCode::RoutedOrderIdInvalid => "Supplied routed order ID is invalid.",
      ClientErrorCode::InvalidTimeOrTimeZoneFormatHms => "The time or time-zone entered is invalid. The correct format is hh:mm:ss xxx",
      ClientErrorCode::InvalidOrderContractExpired => "Invalid order: contract expired.",
      ClientErrorCode::ShortSaleSlotOnlyForDeltaHedge => "Short sale slot may be specified for delta hedge orders only.",
      ClientErrorCode::InvalidProcessTime => "Invalid Process Time: must be integer number of milliseconds between 100 and 2000. Found: <Value>",
      ClientErrorCode::OcaOrdersNotAcceptedSystemProblem => "Due to system problems, orders with OCA groups are currently not being accepted.",
      ClientErrorCode::OnlyMarketLimitOrdersAcceptedSystemProblem => "Due to system problems, application is currently accepting only Market and Limit orders for this contract.", // Covers 396 & 397
      ClientErrorCode::InvalidConditionTrigger => "<Condition> cannot be used as a condition trigger.",
      ClientErrorCode::OrderMessageError => "Order message error <ErrorCode>",
      ClientErrorCode::AlgoOrderError => "Algo order error. <Message>",
      ClientErrorCode::LengthRestriction => "Length restriction. <Field> <MaxLen>",
      ClientErrorCode::ConditionsNotAllowedForContract => "Conditions are not allowed for this contract.",
      ClientErrorCode::InvalidStopPrice => "Invalid stop price.",
      ClientErrorCode::ShortSaleSharesNotAvailable => "Shares for this order are not immediately available for short sale. The order will be held while we attempt to locate the shares.",
      ClientErrorCode::ChildQuantityShouldMatchParentSize => "The child order quantity should be equivalent to the parent order size.",
      ClientErrorCode::CurrencyNotAllowed => "The currency <Currency> is not allowed.",
      ClientErrorCode::SymbolRequiresNonUnicode => "The symbol should contain valid non-unicode characters only.",
      ClientErrorCode::InvalidScaleOrderIncrement => "Invalid scale order increment.",
      ClientErrorCode::InvalidScaleOrderMissingComponentSize => "Invalid scale order. You must specify order component size.",
      ClientErrorCode::InvalidScaleOrderSubsequentComponentSize => "Invalid subsequent component size for scale order.",
      ClientErrorCode::OutsideRthFlagNotValidForOrder => "The \"Outside Regular Trading Hours\" flag is not valid for this order.",
      ClientErrorCode::ContractNotAvailableForTrading => "The contract is not available for trading.",
      ClientErrorCode::WhatIfOrderRequiresTransmitTrue => "What-if order should have the transmit flag set to true.",
      ClientErrorCode::SnapshotNotApplicableToGenericTicks => "Snapshot market data subscription is not applicable to generic ticks.",
      ClientErrorCode::WaitPreviousRfqFinish => "Wait until previous RFQ finishes and try again.",
      ClientErrorCode::RfqNotApplicableForContract => "RFQ is not applicable for the contract. Order ID: <OrderId>",
      ClientErrorCode::InvalidScaleOrderInitialComponentSize => "Invalid initial component size for scale order.",
      ClientErrorCode::InvalidScaleOrderProfitOffset => "Invalid scale order profit offset.",
      ClientErrorCode::MissingScaleOrderInitialComponentSize => "Missing initial component size for scale order.",
      ClientErrorCode::InvalidRealTimeQuery => "Invalid real-time query.",
      ClientErrorCode::InvalidRoute => "Invalid route.",
      ClientErrorCode::CannotChangeAccountClearingAttributes => "The account and clearing attributes on this order may not be changed.",
      ClientErrorCode::CrossOrderRfqExpired => "Cross order RFQ has been expired. THI committed size is no longer available. Please open order dialog and verify liquidity allocation.",
      ClientErrorCode::FaOrderRequiresAllocation => "FA Order requires allocation to be specified.",
      ClientErrorCode::FaOrderRequiresManualAllocation => "FA Order requires per-account manual allocations because there is no common clearing instruction. Please use order dialog Adviser tab to enter the allocation.",
      ClientErrorCode::NoAccountHasEnoughShares => "None of the accounts have enough shares.",
      ClientErrorCode::MutualFundOrderRequiresMonetaryValue => "Mutual Fund order requires monetary value to be specified.",
      ClientErrorCode::MutualFundSellOrderRequiresShares => "Mutual Fund Sell order requires shares to be specified.",
      ClientErrorCode::DeltaNeutralOnlyForCombos => "Delta neutral orders are only supported for combos (BAG security type).",
      ClientErrorCode::FundamentalsDataNotAvailable => "We are sorry, but fundamentals data for the security specified is not available.",
      ClientErrorCode::WhatToShowMissingOrIncorrect => "What to show field is missing or incorrect.",
      ClientErrorCode::CommissionMustNotBeNegative => "Commission must not be negative.",
      ClientErrorCode::InvalidRestoreSizeAfterProfit => "Invalid \"Restore size after taking profit\" for multiple account allocation scale order.",
      ClientErrorCode::OrderSizeCannotBeZero => "The order size cannot be zero.",
      ClientErrorCode::MustSpecifyAccount => "You must specify an account.",
      ClientErrorCode::MustSpecifyAllocation => "You must specify an allocation (either a single account, group, or profile).",
      ClientErrorCode::OnlyOneOutsideRthOrAllowPreOpen => "Order can have only one flag Outside RTH or Allow PreOpen.",
      ClientErrorCode::ApplicationLocked => "The application is now locked.",
      ClientErrorCode::AlgoDefinitionNotFound => "Order processing failed. Algorithm definition not found.",
      ClientErrorCode::AlgoCannotBeModified => "Order modify failed. Algorithm cannot be modified.",
      ClientErrorCode::AlgoAttributesValidationFailed => "Algo attributes validation failed: <Reason>",
      ClientErrorCode::AlgoNotAllowedForOrder => "Specified algorithm is not allowed for this order.",
      ClientErrorCode::UnknownAlgoAttribute => "Order processing failed. Unknown algo attribute.",
      ClientErrorCode::VolComboOrderNotAcknowledged => "Volatility Combo order is not yet acknowledged. Cannot submit changes at this time.",
      ClientErrorCode::RfqNoLongerValid => "The RFQ for this order is no longer valid.",
      ClientErrorCode::MissingScaleOrderProfitOffset => "Missing scale order profit offset.",
      ClientErrorCode::MissingScalePriceAdjustment => "Missing scale price adjustment amount or interval.",
      ClientErrorCode::InvalidScalePriceAdjustmentInterval => "Invalid scale price adjustment interval.",
      ClientErrorCode::UnexpectedScalePriceAdjustment => "Unexpected scale price adjustment amount or interval.",
      // TWS Errors (10000+)
      ClientErrorCode::CrossCurrencyComboError => "Cross currency combo error.",
      ClientErrorCode::CrossCurrencyVolError => "Cross currency vol error.",
      ClientErrorCode::InvalidNonGuaranteedLegs => "Invalid non-guaranteed legs.",
      ClientErrorCode::IbsxNotAllowed => "IBSX not allowed.",
      ClientErrorCode::ReadOnlyModels => "Read-only models.",
      ClientErrorCode::MissingParentOrder => "Missing parent order.",
      ClientErrorCode::InvalidHedgeType => "Invalid hedge type.",
      ClientErrorCode::InvalidBetaValue => "Invalid beta value.",
      ClientErrorCode::InvalidHedgeRatio => "Invalid hedge ratio.",
      ClientErrorCode::InvalidDeltaHedgeOrder => "Invalid delta hedge order.",
      ClientErrorCode::CurrencyNotSupportedForSmartCombo => "Currency is not supported for Smart combo.",
      ClientErrorCode::InvalidAllocationPercentage => "Invalid allocation percentage",
      ClientErrorCode::SmartRoutingApiErrorOptOutRequired => "Smart routing API error (Smart routing opt-out required).",
      ClientErrorCode::PctChangeLimits => "PctChange limits.",
      ClientErrorCode::TradingNotAllowedInApi => "Trading is not allowed in the API.",
      ClientErrorCode::ContractNotVisible => "Contract is not visible.",
      ClientErrorCode::ContractsNotVisible => "Contracts are not visible.",
      ClientErrorCode::OrdersUseEvWarning => "Orders use EV warning.",
      ClientErrorCode::TradesUseEvWarning => "Trades use EV warning.",
      ClientErrorCode::DisplaySizeShouldBeSmaller => "Display size should be smaller than order size.",
      ClientErrorCode::InvalidLeg2ToMktOffsetApi => "Invalid leg2 to Mkt Offset API.",
      ClientErrorCode::InvalidLegPrioApi => "Invalid Leg Prio API.",
      ClientErrorCode::InvalidComboDisplaySizeApi => "Invalid combo display size API.",
      ClientErrorCode::InvalidDontStartNextLeginApi => "Invalid don't start next legin API.",
      ClientErrorCode::InvalidLeg2ToMktTime1Api => "Invalid leg2 to Mkt time1 API.",
      ClientErrorCode::InvalidLeg2ToMktTime2Api => "Invalid leg2 to Mkt time2 API.",
      ClientErrorCode::InvalidComboRoutingTagApi => "Invalid combo routing tag API.",
      ClientErrorCode::NotSubscribedMarketData => "Not subscribed for requested market data.",
      ClientErrorCode::PartiallySubscribedMarketData => "Part of requested market data is not subscribed.",
      ClientErrorCode::CannotCancelFilledOrder => "OrderId <OrderId> that needs to be cancelled can not be cancelled, state: <State>",
      ClientErrorCode::MarketDataNotSubscribedDelayedDisabled => "Requested market data is not subscribed. Delayed market data is not enabled",
      ClientErrorCode::NoMarketDataDuringCompetingSession => "No market data during competing session",
      ClientErrorCode::BustEventDeactivatedSubscription => "Bust event occurred, current subscription is deactivated. Please resubscribe real-time bars immediately",
      ClientErrorCode::UnsavedFaChanges => "You have unsaved FA changes. Please retry 'request FA' operation later, when 'replace FA' operation is complete",
      ClientErrorCode::FaGroupsProfilesContainInvalidAccounts => "The following Groups and/or Profiles contain invalid accounts: <list of groups/profiles>",
      ClientErrorCode::DefaultsInheritedFromCashPreset => "Defaults were inherited from CASH preset during the creation of this order.",
      ClientErrorCode::DecisionMakerRequiredNonDesktop => "The Decision Maker field is required and not set for this order (non-desktop).",
      ClientErrorCode::DecisionMakerRequiredIbbot => "The Decision Maker field is required and not set for this order (ibbot).",
      ClientErrorCode::ChildMustBeAonIfParentAon => "Child has to be AON if parent order is AON",
      ClientErrorCode::AonTicketCanRouteEntireUnfilledOnly => "All or None ticket can route entire unfilled size only",
      ClientErrorCode::AdvisorSetupWebAppCommunicationError => "Some error occured during communication with Advisor Setup web-app",
      ClientErrorCode::OrderAffectsFlaggedAccountsRiskScore => "This order will affect one or more accounts that are flagged because they do not fit the required risk score criteria prescribed by the group/profile/model allocation.",
      ClientErrorCode::MustEnterValidPriceCap => "You must enter a valid Price Cap.",
      ClientErrorCode::MonetaryQuantityModificationNotSupported => "Order Quantity is expressed in monetary terms. Modification is not supported via API. Please use desktop version to revise this order.",
      ClientErrorCode::FractionalOrderModificationNotSupported => "Fractional-sized order cannot be modified via API. Please use desktop version to revise this order.",
      ClientErrorCode::FractionalOrderPlacementNotSupported => "Fractional-sized order cannot be placed via API. Please use desktop version to place this order.",
      ClientErrorCode::CashQuantityNotAllowedForOrder => "Cash Quantity cannot be used for this order",
      ClientErrorCode::InstrumentDoesNotSupportFractional => "This financial instrument does not support fractional shares trading",
      ClientErrorCode::OrderDoesNotSupportFractional => "This order doesn't support fractional shares trading",
      ClientErrorCode::OnlySmartRoutingSupportsFractional => "Only IB SmartRouting supports fractional shares",
      ClientErrorCode::AccountDoesNotHaveFractionalPermission => "<Account> doesn't have permission to trade fractional shares",
      ClientErrorCode::OrderTypeDoesNotSupportFractional => "<Order type>=\"\" order doesn't support fractional shares",
      ClientErrorCode::SizeDoesNotConformToMinVariation => "The size does not conform to the minimum variation of for this contract",
      ClientErrorCode::FractionalNotSupportedForAllocationOrders => "Fractional shares are not supported for allocation orders",
      ClientErrorCode::NonClosePositionOrderDoesNotSupportFractional => "This non-close-position order doesn't support fractional shares trading",
      ClientErrorCode::ClearAwayNotSupportedForMultiLegHedge => "Clear Away orders are not supported for multi-leg combo with attached hedge.",
      ClientErrorCode::InvalidOrderBondExpired => "Invalid Order: bond expired",
      ClientErrorCode::EtradeOnlyAttributeNotSupported => "The 'EtradeOnly' order attribute is not supported",
      ClientErrorCode::FirmQuoteOnlyAttributeNotSupported => "The 'firmQuoteOnly' order attribute is not supported",
      ClientErrorCode::NbboPriceCapAttributeNotSupported => "The 'nbboPriceCap' order attribute is not supported",
      ClientErrorCode::NewsFeedNotAllowed => "News feed is not allowed",
      ClientErrorCode::NewsFeedPermissionsRequired => "News feed permissions required",
      ClientErrorCode::DuplicateWshMetadataRequest => "Duplicate WSH metadata request",
      ClientErrorCode::FailedRequestWshMetadata => "Failed request WSH metadata",
      ClientErrorCode::FailedCancelWshMetadata => "Failed cancel WSH metadata",
      ClientErrorCode::DuplicateWshEventDataRequest => "Duplicate WSH event data request",
      ClientErrorCode::WshMetadataNotRequested => "WSH metadata not requested",
      ClientErrorCode::FailRequestWshEventData => "Fail request WSH event data",
      ClientErrorCode::FailCancelWshEventData => "Fail cancel WSH event data",
      // Unknown/Default
      ClientErrorCode::UnknownCode => "Unknown error code received",
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

      // System Messages
      1100 => Ok(ClientErrorCode::ConnectivityLost),
      1101 => Ok(ClientErrorCode::ConnectivityRestoredDataLost),
      1102 => Ok(ClientErrorCode::ConnectivityRestoredDataMaintained),
      1300 => Ok(ClientErrorCode::SocketPortReset),

      // Warnings
      2100 => Ok(ClientErrorCode::AccountUpdateSubscriptionOverridden),
      2101 => Ok(ClientErrorCode::AccountUpdateSubscriptionRejected),
      2102 => Ok(ClientErrorCode::OrderModificationRejectedProcessing),
      2103 => Ok(ClientErrorCode::MarketDataFarmDisconnected),
      2104 => Ok(ClientErrorCode::MarketDataFarmConnected),
      2105 => Ok(ClientErrorCode::HistoricalDataFarmDisconnected),
      2106 => Ok(ClientErrorCode::HistoricalDataFarmConnected),
      2107 => Ok(ClientErrorCode::HistoricalDataFarmInactive),
      2108 => Ok(ClientErrorCode::MarketDataFarmInactive),
      2109 => Ok(ClientErrorCode::OutsideRthAttributeIgnored),
      2110 => Ok(ClientErrorCode::TwsToServerConnectionBroken),
      2137 => Ok(ClientErrorCode::CrossSideWarning),
      2158 => Ok(ClientErrorCode::SecurityDefinitionDataFarmConnected),
      2168 => Ok(ClientErrorCode::EtradeOnlyNotSupportedWarning),
      2169 => Ok(ClientErrorCode::FirmQuoteOnlyNotSupportedWarning),

      // TWS Errors (1xx-4xx)
      100 => Ok(ClientErrorCode::MaxMessagesPerSecondExceeded),
      101 => Ok(ClientErrorCode::MaxTickersReached),
      102 => Ok(ClientErrorCode::DuplicateTickerId),
      103 => Ok(ClientErrorCode::DuplicateOrderId),
      104 => Ok(ClientErrorCode::CannotModifyFilledOrder),
      105 => Ok(ClientErrorCode::OrderModificationMismatch),
      106 => Ok(ClientErrorCode::CannotTransmitOrderId),
      107 => Ok(ClientErrorCode::CannotTransmitIncompleteOrder),
      109 => Ok(ClientErrorCode::PriceOutOfPercentageRange),
      110 => Ok(ClientErrorCode::PriceIncrementViolation),
      111 => Ok(ClientErrorCode::TifOrderTypeIncompatible),
      113 => Ok(ClientErrorCode::TifRequiresDayForMocLoc),
      114 => Ok(ClientErrorCode::RelativeOrdersStocksOnly),
      115 => Ok(ClientErrorCode::RelativeOrdersUsStocksRouting),
      116 => Ok(ClientErrorCode::CannotTransmitToDeadExchange),
      117 => Ok(ClientErrorCode::BlockOrderSizeTooSmall),
      118 => Ok(ClientErrorCode::VwapOrdersRequireVwapExchange),
      119 => Ok(ClientErrorCode::OnlyVwapOrdersOnVwapExchange),
      120 => Ok(ClientErrorCode::TooLateForVwapOrder),
      121 => Ok(ClientErrorCode::InvalidBdFlag),
      122 => Ok(ClientErrorCode::NoRequestTagForOrder),
      123 => Ok(ClientErrorCode::NoRecordForConid),
      124 => Ok(ClientErrorCode::NoMarketRuleForConid),
      125 => Ok(ClientErrorCode::BuyPriceMustMatchBestAsk),
      126 => Ok(ClientErrorCode::SellPriceMustMatchBestBid),
      129 => Ok(ClientErrorCode::VwapOrderSubmitTimeViolation),
      131 => Ok(ClientErrorCode::SweepToFillDisplaySizeIgnored),
      132 => Ok(ClientErrorCode::MissingClearingAccount),
      133 => Ok(ClientErrorCode::SubmitNewOrderFailed),
      134 => Ok(ClientErrorCode::ModifyOrderFailed),
      135 => Ok(ClientErrorCode::CannotFindOrderToCancel),
      136 => Ok(ClientErrorCode::OrderCannotBeCancelled),
      137 => Ok(ClientErrorCode::VwapOrderCancelTimeViolation),
      138 => Ok(ClientErrorCode::CouldNotParseTickerRequest),
      139 => Ok(ClientErrorCode::ParsingError),
      140 => Ok(ClientErrorCode::SizeValueShouldBeInteger),
      141 => Ok(ClientErrorCode::PriceValueShouldBeDouble),
      142 => Ok(ClientErrorCode::InstitutionalAccountMissingInfo),
      143 => Ok(ClientErrorCode::RequestIdNotInteger),
      144 => Ok(ClientErrorCode::OrderSizeAllocationMismatch),
      145 => Ok(ClientErrorCode::ValidationErrorInEntryFields),
      146 => Ok(ClientErrorCode::InvalidTriggerMethod),
      147 => Ok(ClientErrorCode::ConditionalContractInfoIncomplete),
      148 => Ok(ClientErrorCode::ConditionalOrderRequiresLimitMarket),
      151 => Ok(ClientErrorCode::MissingUserNameDDE),
      152 => Ok(ClientErrorCode::HiddenAttributeNotAllowed),
      153 => Ok(ClientErrorCode::EfpRequiresLimitOrder),
      154 => Ok(ClientErrorCode::CannotTransmitOrderForHaltedSecurity),
      155 => Ok(ClientErrorCode::SizeOpOrderRequiresUserAccount),
      156 => Ok(ClientErrorCode::SizeOpOrderRequiresIBSX),
      157 => Ok(ClientErrorCode::IcebergDiscretionaryConflict),
      158 => Ok(ClientErrorCode::MissingTrailOffset),
      159 => Ok(ClientErrorCode::PercentOffsetOutOfRange),
      160 => Ok(ClientErrorCode::SizeValueCannotBeZero),
      161 => Ok(ClientErrorCode::CancelAttemptNotInCancellableState),
      162 => Ok(ClientErrorCode::HistoricalDataServiceError),
      163 => Ok(ClientErrorCode::PriceViolatesPercentageConstraint),
      164 => Ok(ClientErrorCode::NoMarketDataForPricePercentCheck),
      165 => Ok(ClientErrorCode::HistoricalDataServiceQueryMessage),
      166 => Ok(ClientErrorCode::HistoricalDataExpiredContractViolation),
      167 => Ok(ClientErrorCode::VwapOrderTimeNotInFuture),
      168 => Ok(ClientErrorCode::DiscretionaryAmountIncrementViolation),
      200 => Ok(ClientErrorCode::NoSecurityDefinitionFound),
      2001 => Ok(ClientErrorCode::AmbiguousContract), // Using distinct code
      201 => Ok(ClientErrorCode::OrderRejected),
      202 => Ok(ClientErrorCode::OrderCancelled),
      203 => Ok(ClientErrorCode::SecurityNotAvailableOrAllowed),
      300 => Ok(ClientErrorCode::CannotFindEidWithTickerId),
      301 => Ok(ClientErrorCode::InvalidTickerAction),
      302 => Ok(ClientErrorCode::ErrorParsingStopTickerString),
      303 => Ok(ClientErrorCode::InvalidAction),
      304 => Ok(ClientErrorCode::InvalidAccountValueAction),
      305 => Ok(ClientErrorCode::RequestParsingErrorIgnored),
      306 => Ok(ClientErrorCode::ErrorProcessingDdeRequest),
      307 => Ok(ClientErrorCode::InvalidRequestTopic),
      308 => Ok(ClientErrorCode::MaxApiPagesReached),
      309 => Ok(ClientErrorCode::MaxMarketDepthRequestsReached),
      310 => Ok(ClientErrorCode::CannotFindSubscribedMarketDepth),
      311 => Ok(ClientErrorCode::InvalidOrigin),
      312 => Ok(ClientErrorCode::InvalidComboDetails),
      313 => Ok(ClientErrorCode::InvalidComboLegDetails),
      314 => Ok(ClientErrorCode::BagSecurityTypeRequiresComboLegs),
      315 => Ok(ClientErrorCode::StockComboLegsRequireSmartRouting),
      316 => Ok(ClientErrorCode::MarketDepthDataHalted),
      317 => Ok(ClientErrorCode::MarketDepthDataReset),
      319 => Ok(ClientErrorCode::InvalidLogLevel),
      320 => Ok(ClientErrorCode::ServerErrorReadingApiClientRequest),
      321 => Ok(ClientErrorCode::ServerErrorValidatingApiClientRequest),
      322 => Ok(ClientErrorCode::ServerErrorProcessingApiClientRequest),
      323 => Ok(ClientErrorCode::ServerErrorCause),
      324 => Ok(ClientErrorCode::ServerErrorReadingDdeClientRequest),
      325 => Ok(ClientErrorCode::DiscretionaryOrdersNotSupported),
      326 => Ok(ClientErrorCode::ClientIdInUse),
      327 => Ok(ClientErrorCode::AutoBindRequiresClientIdZero),
      328 => Ok(ClientErrorCode::TrailStopAttachViolation),
      329 => Ok(ClientErrorCode::OrderModifyFailedCannotChangeType),
      330 => Ok(ClientErrorCode::ManagedAccountsListRequiresFaStl),
      331 => Ok(ClientErrorCode::FaStlHasNoManagedAccounts),
      332 => Ok(ClientErrorCode::InvalidAccountCodesForOrderProfile),
      333 => Ok(ClientErrorCode::InvalidShareAllocationSyntax),
      334 => Ok(ClientErrorCode::InvalidGoodTillDateOrder),
      335 => Ok(ClientErrorCode::InvalidDeltaRange),
      336 => Ok(ClientErrorCode::InvalidTimeOrTimeZoneFormat),
      337 => Ok(ClientErrorCode::InvalidDateTimeOrTimeZoneFormat),
      338 => Ok(ClientErrorCode::GoodAfterTimeDisabled),
      339 => Ok(ClientErrorCode::FuturesSpreadNotSupported),
      340 => Ok(ClientErrorCode::InvalidImprovementAmountBoxAuction),
      341 => Ok(ClientErrorCode::InvalidDeltaValue1To100),
      342 => Ok(ClientErrorCode::PeggedOrderNotSupported),
      343 => Ok(ClientErrorCode::InvalidDateTimeOrTimeZoneFormatYmdHms),
      344 => Ok(ClientErrorCode::NotFinancialAdvisorAccount),
      345 => Ok(ClientErrorCode::GenericComboNotSupportedForFA),
      346 => Ok(ClientErrorCode::NotInstitutionalOrAwayClearingAccount),
      347 => Ok(ClientErrorCode::InvalidShortSaleSlotValue),
      348 => Ok(ClientErrorCode::ShortSaleSlotRequiresSshortAction),
      349 => Ok(ClientErrorCode::GenericComboDoesNotSupportGoodAfter),
      350 => Ok(ClientErrorCode::MinQuantityNotSupportedForBestCombo),
      351 => Ok(ClientErrorCode::RthOnlyFlagNotValid),
      352 => Ok(ClientErrorCode::ShortSaleSlot2RequiresLocation),
      353 => Ok(ClientErrorCode::ShortSaleSlot1RequiresNoLocation),
      354 => Ok(ClientErrorCode::NotSubscribedToMarketData),
      355 => Ok(ClientErrorCode::OrderSizeMarketRuleViolation),
      356 => Ok(ClientErrorCode::SmartComboDoesNotSupportOca),
      357 => Ok(ClientErrorCode::ClientVersionOutOfDate),
      358 => Ok(ClientErrorCode::SmartComboChildOrderNotSupported),
      359 => Ok(ClientErrorCode::ComboOrderReduceOnFillOcaViolation),
      360 => Ok(ClientErrorCode::NoWhatifSupportForSmartCombo),
      361 => Ok(ClientErrorCode::InvalidTriggerPrice),
      362 => Ok(ClientErrorCode::InvalidAdjustedStopPrice),
      363 => Ok(ClientErrorCode::InvalidAdjustedStopLimitPrice),
      364 => Ok(ClientErrorCode::InvalidAdjustedTrailingAmount),
      365 => Ok(ClientErrorCode::NoScannerSubscriptionFound),
      366 => Ok(ClientErrorCode::NoHistoricalDataQueryFound),
      367 => Ok(ClientErrorCode::InvalidVolatilityTypeForVolOrder),
      368 => Ok(ClientErrorCode::InvalidReferencePriceTypeForVolOrder),
      369 => Ok(ClientErrorCode::VolatilityOrdersOnlyForUsOptions),
      370 => Ok(ClientErrorCode::DynamicVolatilityRoutingViolation),
      371 => Ok(ClientErrorCode::VolOrderRequiresPositiveVolatility),
      372 => Ok(ClientErrorCode::CannotSetDynamicVolOnNonVolOrder),
      373 => Ok(ClientErrorCode::StockRangeAttributeRequiresVolOrRel),
      374 => Ok(ClientErrorCode::InvalidStockRangeAttributesOrder),
      375 => Ok(ClientErrorCode::StockRangeAttributesCannotBeNegative),
      376 => Ok(ClientErrorCode::NotEligibleForContinuousUpdate),
      377 => Ok(ClientErrorCode::MustSpecifyValidDeltaHedgeAuxPrice),
      378 => Ok(ClientErrorCode::DeltaHedgeOrderRequiresAuxPrice),
      379 => Ok(ClientErrorCode::DeltaHedgeOrderRequiresNoAuxPrice),
      380 => Ok(ClientErrorCode::OrderTypeNotAllowedForDeltaHedge),
      381 => Ok(ClientErrorCode::DdeDllNeedsUpgrade),
      382 => Ok(ClientErrorCode::PriceViolatesTicksConstraint),
      383 => Ok(ClientErrorCode::SizeViolatesSizeConstraint),
      384 => Ok(ClientErrorCode::InvalidDdeArrayRequest),
      385 => Ok(ClientErrorCode::DuplicateTickerIdApiScanner),
      386 => Ok(ClientErrorCode::DuplicateTickerIdApiHistorical),
      387 => Ok(ClientErrorCode::UnsupportedOrderTypeForExchangeSecType),
      388 => Ok(ClientErrorCode::OrderSizeSmallerThanMinimum),
      389 => Ok(ClientErrorCode::RoutedOrderIdNotUnique),
      390 => Ok(ClientErrorCode::RoutedOrderIdInvalid),
      391 => Ok(ClientErrorCode::InvalidTimeOrTimeZoneFormatHms),
      392 => Ok(ClientErrorCode::InvalidOrderContractExpired),
      393 => Ok(ClientErrorCode::ShortSaleSlotOnlyForDeltaHedge),
      394 => Ok(ClientErrorCode::InvalidProcessTime),
      395 => Ok(ClientErrorCode::OcaOrdersNotAcceptedSystemProblem),
      396 | 397 => Ok(ClientErrorCode::OnlyMarketLimitOrdersAcceptedSystemProblem), // Handle both 396 and 397
      398 => Ok(ClientErrorCode::InvalidConditionTrigger),
      399 => Ok(ClientErrorCode::OrderMessageError),
      400 => Ok(ClientErrorCode::AlgoOrderError),
      401 => Ok(ClientErrorCode::LengthRestriction),
      402 => Ok(ClientErrorCode::ConditionsNotAllowedForContract),
      403 => Ok(ClientErrorCode::InvalidStopPrice),
      404 => Ok(ClientErrorCode::ShortSaleSharesNotAvailable),
      405 => Ok(ClientErrorCode::ChildQuantityShouldMatchParentSize),
      406 => Ok(ClientErrorCode::CurrencyNotAllowed),
      407 => Ok(ClientErrorCode::SymbolRequiresNonUnicode),
      408 => Ok(ClientErrorCode::InvalidScaleOrderIncrement),
      409 => Ok(ClientErrorCode::InvalidScaleOrderMissingComponentSize),
      410 => Ok(ClientErrorCode::InvalidScaleOrderSubsequentComponentSize),
      411 => Ok(ClientErrorCode::OutsideRthFlagNotValidForOrder),
      412 => Ok(ClientErrorCode::ContractNotAvailableForTrading),
      413 => Ok(ClientErrorCode::WhatIfOrderRequiresTransmitTrue),
      414 => Ok(ClientErrorCode::SnapshotNotApplicableToGenericTicks),
      415 => Ok(ClientErrorCode::WaitPreviousRfqFinish),
      416 => Ok(ClientErrorCode::RfqNotApplicableForContract),
      417 => Ok(ClientErrorCode::InvalidScaleOrderInitialComponentSize),
      418 => Ok(ClientErrorCode::InvalidScaleOrderProfitOffset),
      419 => Ok(ClientErrorCode::MissingScaleOrderInitialComponentSize),
      420 => Ok(ClientErrorCode::InvalidRealTimeQuery),
      421 => Ok(ClientErrorCode::InvalidRoute),
      422 => Ok(ClientErrorCode::CannotChangeAccountClearingAttributes),
      423 => Ok(ClientErrorCode::CrossOrderRfqExpired),
      424 => Ok(ClientErrorCode::FaOrderRequiresAllocation),
      425 => Ok(ClientErrorCode::FaOrderRequiresManualAllocation),
      426 => Ok(ClientErrorCode::NoAccountHasEnoughShares),
      427 => Ok(ClientErrorCode::MutualFundOrderRequiresMonetaryValue),
      428 => Ok(ClientErrorCode::MutualFundSellOrderRequiresShares),
      429 => Ok(ClientErrorCode::DeltaNeutralOnlyForCombos),
      430 => Ok(ClientErrorCode::FundamentalsDataNotAvailable),
      431 => Ok(ClientErrorCode::WhatToShowMissingOrIncorrect),
      432 => Ok(ClientErrorCode::CommissionMustNotBeNegative),
      433 => Ok(ClientErrorCode::InvalidRestoreSizeAfterProfit),
      434 => Ok(ClientErrorCode::OrderSizeCannotBeZero),
      435 => Ok(ClientErrorCode::MustSpecifyAccount),
      436 => Ok(ClientErrorCode::MustSpecifyAllocation),
      437 => Ok(ClientErrorCode::OnlyOneOutsideRthOrAllowPreOpen),
      438 => Ok(ClientErrorCode::ApplicationLocked),
      439 => Ok(ClientErrorCode::AlgoDefinitionNotFound),
      440 => Ok(ClientErrorCode::AlgoCannotBeModified),
      441 => Ok(ClientErrorCode::AlgoAttributesValidationFailed),
      442 => Ok(ClientErrorCode::AlgoNotAllowedForOrder),
      443 => Ok(ClientErrorCode::UnknownAlgoAttribute),
      444 => Ok(ClientErrorCode::VolComboOrderNotAcknowledged),
      445 => Ok(ClientErrorCode::RfqNoLongerValid),
      446 => Ok(ClientErrorCode::MissingScaleOrderProfitOffset),
      447 => Ok(ClientErrorCode::MissingScalePriceAdjustment),
      448 => Ok(ClientErrorCode::InvalidScalePriceAdjustmentInterval),
      449 => Ok(ClientErrorCode::UnexpectedScalePriceAdjustment),

      // TWS Errors (10000+)
      10000 => Ok(ClientErrorCode::CrossCurrencyComboError),
      10001 => Ok(ClientErrorCode::CrossCurrencyVolError),
      10002 => Ok(ClientErrorCode::InvalidNonGuaranteedLegs),
      10003 => Ok(ClientErrorCode::IbsxNotAllowed),
      10005 => Ok(ClientErrorCode::ReadOnlyModels),
      10006 => Ok(ClientErrorCode::MissingParentOrder),
      10007 => Ok(ClientErrorCode::InvalidHedgeType),
      10008 => Ok(ClientErrorCode::InvalidBetaValue),
      10009 => Ok(ClientErrorCode::InvalidHedgeRatio),
      10010 => Ok(ClientErrorCode::InvalidDeltaHedgeOrder),
      10011 => Ok(ClientErrorCode::CurrencyNotSupportedForSmartCombo),
      10012 => Ok(ClientErrorCode::InvalidAllocationPercentage),
      10013 => Ok(ClientErrorCode::SmartRoutingApiErrorOptOutRequired),
      10014 => Ok(ClientErrorCode::PctChangeLimits),
      10015 => Ok(ClientErrorCode::TradingNotAllowedInApi),
      10016 => Ok(ClientErrorCode::ContractNotVisible),
      10017 => Ok(ClientErrorCode::ContractsNotVisible),
      10018 => Ok(ClientErrorCode::OrdersUseEvWarning),
      10019 => Ok(ClientErrorCode::TradesUseEvWarning),
      10020 => Ok(ClientErrorCode::DisplaySizeShouldBeSmaller),
      10021 => Ok(ClientErrorCode::InvalidLeg2ToMktOffsetApi),
      10022 => Ok(ClientErrorCode::InvalidLegPrioApi),
      10023 => Ok(ClientErrorCode::InvalidComboDisplaySizeApi),
      10024 => Ok(ClientErrorCode::InvalidDontStartNextLeginApi),
      10025 => Ok(ClientErrorCode::InvalidLeg2ToMktTime1Api),
      10026 => Ok(ClientErrorCode::InvalidLeg2ToMktTime2Api),
      10027 => Ok(ClientErrorCode::InvalidComboRoutingTagApi),
      10089 => Ok(ClientErrorCode::NotSubscribedToMarketData),
      10090 => Ok(ClientErrorCode::PartiallySubscribedMarketData),
      10148 => Ok(ClientErrorCode::CannotCancelFilledOrder),
      10186 => Ok(ClientErrorCode::MarketDataNotSubscribedDelayedDisabled),
      10197 => Ok(ClientErrorCode::NoMarketDataDuringCompetingSession),
      10225 => Ok(ClientErrorCode::BustEventDeactivatedSubscription),
      10230 => Ok(ClientErrorCode::UnsavedFaChanges),
      10231 => Ok(ClientErrorCode::FaGroupsProfilesContainInvalidAccounts),
      10233 => Ok(ClientErrorCode::DefaultsInheritedFromCashPreset),
      10234 => Ok(ClientErrorCode::DecisionMakerRequiredNonDesktop),
      10235 => Ok(ClientErrorCode::DecisionMakerRequiredIbbot),
      10236 => Ok(ClientErrorCode::ChildMustBeAonIfParentAon),
      10237 => Ok(ClientErrorCode::AonTicketCanRouteEntireUnfilledOnly),
      10238 => Ok(ClientErrorCode::AdvisorSetupWebAppCommunicationError),
      10239 => Ok(ClientErrorCode::OrderAffectsFlaggedAccountsRiskScore),
      10240 => Ok(ClientErrorCode::MustEnterValidPriceCap),
      10241 => Ok(ClientErrorCode::MonetaryQuantityModificationNotSupported),
      10242 => Ok(ClientErrorCode::FractionalOrderModificationNotSupported),
      10243 => Ok(ClientErrorCode::FractionalOrderPlacementNotSupported),
      10244 => Ok(ClientErrorCode::CashQuantityNotAllowedForOrder),
      10245 => Ok(ClientErrorCode::InstrumentDoesNotSupportFractional),
      10246 => Ok(ClientErrorCode::OrderDoesNotSupportFractional),
      10247 => Ok(ClientErrorCode::OnlySmartRoutingSupportsFractional),
      10248 => Ok(ClientErrorCode::AccountDoesNotHaveFractionalPermission),
      10249 => Ok(ClientErrorCode::OrderTypeDoesNotSupportFractional),
      10250 => Ok(ClientErrorCode::SizeDoesNotConformToMinVariation),
      10251 => Ok(ClientErrorCode::FractionalNotSupportedForAllocationOrders),
      10252 => Ok(ClientErrorCode::NonClosePositionOrderDoesNotSupportFractional),
      10253 => Ok(ClientErrorCode::ClearAwayNotSupportedForMultiLegHedge),
      10254 => Ok(ClientErrorCode::InvalidOrderBondExpired),
      10268 => Ok(ClientErrorCode::EtradeOnlyAttributeNotSupported),
      10269 => Ok(ClientErrorCode::FirmQuoteOnlyAttributeNotSupported),
      10270 => Ok(ClientErrorCode::NbboPriceCapAttributeNotSupported),
      10276 => Ok(ClientErrorCode::NewsFeedNotAllowed),
      10277 => Ok(ClientErrorCode::NewsFeedPermissionsRequired),
      10278 => Ok(ClientErrorCode::DuplicateWshMetadataRequest),
      10279 => Ok(ClientErrorCode::FailedRequestWshMetadata),
      10280 => Ok(ClientErrorCode::FailedCancelWshMetadata),
      10281 => Ok(ClientErrorCode::DuplicateWshEventDataRequest),
      10282 => Ok(ClientErrorCode::WshMetadataNotRequested),
      10283 => Ok(ClientErrorCode::FailRequestWshEventData),
      10284 => Ok(ClientErrorCode::FailCancelWshEventData),

      // Handle unknown codes gracefully
      _ => {
          log::warn!("Received unknown ClientErrorCode value: {}", value);
          // Decide whether to return an error or a default UnknownCode variant
          // Returning an error might be safer if strict handling is needed.
          Err(IBKRError::ParseError(format!("Unknown ClientErrorCode value: {}", value)))
          // Or return a default variant: Ok(ClientErrorCode::UnknownCode)
      }
    }
  }
}
