// yatws/src/parser_client.rs

use std::sync::Arc;
use std::convert::TryFrom;
use crate::handler::{ClientHandler, MessageHandler}; // Import MessageHandler
use crate::protocol_decoder::ClientErrorCode; // Import the error enum
use crate::base::IBKRError;
use crate::protocol_dec_parser::FieldParser;
use log::{warn, error, info}; // Import logging levels

/// Process an error message (Type 4)
pub fn process_error_message(handler: &MessageHandler, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let version = parser.read_int(false)?;

  let mut id: i32 = -1; // Default for version < 2 or errors not associated with a request
  let error_code_int: i32;
  let error_msg_str: &str; // Temporarily read as &str

  if version < 2 {
    // Very old format, just a single message string
    error_msg_str = parser.read_str()?;
    error_code_int = 0; // No code provided in this format, treat as unknown or info?
  } else {
    id = parser.read_int(false)?;
    error_code_int = parser.read_int(false)?;
    error_msg_str = parser.read_str()?;
    // Version >= 3 adds an optional 'advanced order reject' JSON string
    // TODO: Parse advanced_order_reject if server_version supports it
    // if handler.get_server_version() >= crate::versions::ADVANCED_ORDER_REJECT {
    //     let _advanced_order_reject = parser.read_str()?;
    // }
  }

  // Convert integer code to ClientErrorCode enum
  let error_code = match ClientErrorCode::try_from(error_code_int) {
    Ok(code) => code,
    Err(_) => {
      if error_code_int >= 1000 && error_code_int < 2000 { // Example range for TWS info/warnings
        info!("Received TWS Info/Warning (ID: {}, Code: {}): {}", id, error_code_int, error_msg_str);
        return Ok(());
      } else {
        error!("Received unknown TWS Error Code (ID: {}, Code: {}): {}", id, error_code_int, error_msg_str);
        return Ok(());
      }
    }
  };

  // Log based on severity
  if error_code_int >= 2000 && error_code_int < 3000 {
    info!("TWS Info/Warning (ID: {}, Code: {:?}={}): {}", id, error_code, error_code_int, error_msg_str);
  } else {
    error!("TWS Error (ID: {}, Code: {:?}={}): {}", id, error_code, error_code_int, error_msg_str);
  }

  // Convert to owned String for the handler if it expects String
  let error_msg_owned = error_msg_str.to_string();

  // --- Route to the appropriate handler based on error code ---
  match error_code {
    // Order Handling Errors
    ClientErrorCode::FailSendOrder | ClientErrorCode::FailSendCOrder | ClientErrorCode::FailSendOOrder |
    ClientErrorCode::FailSendReqCompletedOrders | ClientErrorCode::DuplicateOrderId | ClientErrorCode::CannotModifyFilledOrder |
    ClientErrorCode::OrderModificationMismatch | ClientErrorCode::CannotTransmitOrderId | ClientErrorCode::CannotTransmitIncompleteOrder |
    ClientErrorCode::PriceOutOfPercentageRange | ClientErrorCode::PriceIncrementViolation | ClientErrorCode::TifOrderTypeIncompatible |
    ClientErrorCode::TifRequiresDayForMocLoc | ClientErrorCode::RelativeOrdersStocksOnly | ClientErrorCode::RelativeOrdersUsStocksRouting |
    ClientErrorCode::CannotTransmitToDeadExchange | ClientErrorCode::BlockOrderSizeTooSmall | ClientErrorCode::VwapOrdersRequireVwapExchange |
    ClientErrorCode::OnlyVwapOrdersOnVwapExchange | ClientErrorCode::TooLateForVwapOrder | ClientErrorCode::InvalidBdFlag |
    ClientErrorCode::NoRequestTagForOrder | ClientErrorCode::BuyPriceMustMatchBestAsk | ClientErrorCode::SellPriceMustMatchBestBid |
    ClientErrorCode::VwapOrderSubmitTimeViolation | ClientErrorCode::SweepToFillDisplaySizeIgnored | ClientErrorCode::MissingClearingAccount |
    ClientErrorCode::SubmitNewOrderFailed | ClientErrorCode::ModifyOrderFailed | ClientErrorCode::CannotFindOrderToCancel |
    ClientErrorCode::OrderCannotBeCancelled | ClientErrorCode::VwapOrderCancelTimeViolation | ClientErrorCode::SizeValueShouldBeInteger |
    ClientErrorCode::PriceValueShouldBeDouble | ClientErrorCode::OrderSizeAllocationMismatch | ClientErrorCode::ValidationErrorInEntryFields |
    ClientErrorCode::InvalidTriggerMethod | ClientErrorCode::ConditionalContractInfoIncomplete | ClientErrorCode::ConditionalOrderRequiresLimitMarket |
    ClientErrorCode::MissingUserNameDDE | ClientErrorCode::HiddenAttributeNotAllowed | ClientErrorCode::EfpRequiresLimitOrder |
    ClientErrorCode::CannotTransmitOrderForHaltedSecurity | ClientErrorCode::SizeOpOrderRequiresUserAccount | ClientErrorCode::SizeOpOrderRequiresIBSX |
    ClientErrorCode::IcebergDiscretionaryConflict | ClientErrorCode::MissingTrailOffset | ClientErrorCode::PercentOffsetOutOfRange |
    ClientErrorCode::SizeValueCannotBeZero | ClientErrorCode::CancelAttemptNotInCancellableState | ClientErrorCode::PriceViolatesPercentageConstraint |
    ClientErrorCode::NoMarketDataForPricePercentCheck | ClientErrorCode::VwapOrderTimeNotInFuture | ClientErrorCode::DiscretionaryAmountIncrementViolation |
    ClientErrorCode::OrderRejected | ClientErrorCode::OrderCancelled | ClientErrorCode::InvalidAction | ClientErrorCode::InvalidOrigin |
    ClientErrorCode::InvalidComboDetails | ClientErrorCode::InvalidComboLegDetails | ClientErrorCode::BagSecurityTypeRequiresComboLegs |
    ClientErrorCode::StockComboLegsRequireSmartRouting | ClientErrorCode::DiscretionaryOrdersNotSupported | ClientErrorCode::TrailStopAttachViolation |
    ClientErrorCode::OrderModifyFailedCannotChangeType | ClientErrorCode::InvalidShareAllocationSyntax | ClientErrorCode::InvalidGoodTillDateOrder |
    ClientErrorCode::InvalidDeltaRange | ClientErrorCode::InvalidTimeOrTimeZoneFormat | ClientErrorCode::InvalidDateTimeOrTimeZoneFormat |
    ClientErrorCode::GoodAfterTimeDisabled | ClientErrorCode::FuturesSpreadNotSupported | ClientErrorCode::InvalidImprovementAmountBoxAuction |
    ClientErrorCode::InvalidDeltaValue1To100 | ClientErrorCode::PeggedOrderNotSupported | ClientErrorCode::InvalidDateTimeOrTimeZoneFormatYmdHms |
    ClientErrorCode::GenericComboNotSupportedForFA | ClientErrorCode::InvalidShortSaleSlotValue | ClientErrorCode::ShortSaleSlotRequiresSshortAction |
    ClientErrorCode::GenericComboDoesNotSupportGoodAfter | ClientErrorCode::MinQuantityNotSupportedForBestCombo | ClientErrorCode::RthOnlyFlagNotValid |
    ClientErrorCode::ShortSaleSlot2RequiresLocation | ClientErrorCode::ShortSaleSlot1RequiresNoLocation | ClientErrorCode::OrderSizeMarketRuleViolation |
    ClientErrorCode::SmartComboDoesNotSupportOca | ClientErrorCode::SmartComboChildOrderNotSupported | ClientErrorCode::ComboOrderReduceOnFillOcaViolation |
    ClientErrorCode::NoWhatifSupportForSmartCombo | ClientErrorCode::InvalidTriggerPrice | ClientErrorCode::InvalidAdjustedStopPrice |
    ClientErrorCode::InvalidAdjustedStopLimitPrice | ClientErrorCode::InvalidAdjustedTrailingAmount | ClientErrorCode::InvalidVolatilityTypeForVolOrder |
    ClientErrorCode::InvalidReferencePriceTypeForVolOrder | ClientErrorCode::VolatilityOrdersOnlyForUsOptions | ClientErrorCode::DynamicVolatilityRoutingViolation |
    ClientErrorCode::VolOrderRequiresPositiveVolatility | ClientErrorCode::CannotSetDynamicVolOnNonVolOrder | ClientErrorCode::StockRangeAttributeRequiresVolOrRel |
    ClientErrorCode::InvalidStockRangeAttributesOrder | ClientErrorCode::StockRangeAttributesCannotBeNegative | ClientErrorCode::NotEligibleForContinuousUpdate |
    ClientErrorCode::MustSpecifyValidDeltaHedgeAuxPrice | ClientErrorCode::DeltaHedgeOrderRequiresAuxPrice | ClientErrorCode::DeltaHedgeOrderRequiresNoAuxPrice |
    ClientErrorCode::OrderTypeNotAllowedForDeltaHedge | ClientErrorCode::PriceViolatesTicksConstraint | ClientErrorCode::SizeViolatesSizeConstraint |
    ClientErrorCode::UnsupportedOrderTypeForExchangeSecType | ClientErrorCode::OrderSizeSmallerThanMinimum | ClientErrorCode::RoutedOrderIdNotUnique |
    ClientErrorCode::RoutedOrderIdInvalid | ClientErrorCode::InvalidTimeOrTimeZoneFormatHms | ClientErrorCode::InvalidOrderContractExpired |
    ClientErrorCode::ShortSaleSlotOnlyForDeltaHedge | ClientErrorCode::InvalidProcessTime | ClientErrorCode::OcaOrdersNotAcceptedSystemProblem |
    ClientErrorCode::OnlyMarketLimitOrdersAcceptedSystemProblem | ClientErrorCode::InvalidConditionTrigger | ClientErrorCode::OrderMessageError |
    ClientErrorCode::AlgoOrderError | ClientErrorCode::LengthRestriction | ClientErrorCode::ConditionsNotAllowedForContract |
    ClientErrorCode::InvalidStopPrice | ClientErrorCode::ShortSaleSharesNotAvailable | ClientErrorCode::ChildQuantityShouldMatchParentSize |
    ClientErrorCode::CurrencyNotAllowed | ClientErrorCode::SymbolRequiresNonUnicode | ClientErrorCode::InvalidScaleOrderIncrement |
    ClientErrorCode::InvalidScaleOrderMissingComponentSize | ClientErrorCode::InvalidScaleOrderSubsequentComponentSize | ClientErrorCode::OutsideRthFlagNotValidForOrder |
    ClientErrorCode::WhatIfOrderRequiresTransmitTrue | ClientErrorCode::WaitPreviousRfqFinish | ClientErrorCode::RfqNotApplicableForContract |
    ClientErrorCode::InvalidScaleOrderInitialComponentSize | ClientErrorCode::InvalidScaleOrderProfitOffset | ClientErrorCode::MissingScaleOrderInitialComponentSize |
    ClientErrorCode::CannotChangeAccountClearingAttributes | ClientErrorCode::CrossOrderRfqExpired | ClientErrorCode::MutualFundOrderRequiresMonetaryValue |
    ClientErrorCode::MutualFundSellOrderRequiresShares | ClientErrorCode::DeltaNeutralOnlyForCombos | ClientErrorCode::CommissionMustNotBeNegative |
    ClientErrorCode::InvalidRestoreSizeAfterProfit | ClientErrorCode::OrderSizeCannotBeZero | ClientErrorCode::MustSpecifyAllocation |
    ClientErrorCode::OnlyOneOutsideRthOrAllowPreOpen | ClientErrorCode::AlgoDefinitionNotFound | ClientErrorCode::AlgoCannotBeModified |
    ClientErrorCode::AlgoAttributesValidationFailed | ClientErrorCode::AlgoNotAllowedForOrder | ClientErrorCode::UnknownAlgoAttribute |
    ClientErrorCode::VolComboOrderNotAcknowledged | ClientErrorCode::RfqNoLongerValid | ClientErrorCode::MissingScaleOrderProfitOffset |
    ClientErrorCode::MissingScalePriceAdjustment | ClientErrorCode::InvalidScalePriceAdjustmentInterval | ClientErrorCode::UnexpectedScalePriceAdjustment |
    ClientErrorCode::NoTradingPermissions | ClientErrorCode::MissingParentOrder | ClientErrorCode::InvalidDeltaHedgeOrder |
    ClientErrorCode::OrdersUseEvWarning | ClientErrorCode::TradesUseEvWarning | ClientErrorCode::DisplaySizeShouldBeSmaller |
    ClientErrorCode::InvalidLeg2ToMktOffsetApi | ClientErrorCode::InvalidLegPrioApi | ClientErrorCode::InvalidComboDisplaySizeApi |
    ClientErrorCode::InvalidDontStartNextLeginApi | ClientErrorCode::InvalidLeg2ToMktTime1Api | ClientErrorCode::InvalidLeg2ToMktTime2Api |
    ClientErrorCode::InvalidComboRoutingTagApi | ClientErrorCode::CannotCancelNotFoundOrder | ClientErrorCode::CannotCancelFilledOrder |
    ClientErrorCode::DefaultsInheritedFromCashPreset | ClientErrorCode::DecisionMakerRequiredNonDesktop | ClientErrorCode::DecisionMakerRequiredIbbot |
    ClientErrorCode::ChildMustBeAonIfParentAon | ClientErrorCode::AonTicketCanRouteEntireUnfilledOnly | ClientErrorCode::OrderAffectsFlaggedAccountsRiskScore |
    ClientErrorCode::MustEnterValidPriceCap | ClientErrorCode::MonetaryQuantityModificationNotSupported | ClientErrorCode::FractionalOrderModificationNotSupported |
    ClientErrorCode::FractionalOrderPlacementNotSupported | ClientErrorCode::CashQuantityNotAllowedForOrder | ClientErrorCode::OrderDoesNotSupportFractional |
    ClientErrorCode::OrderTypeDoesNotSupportFractional | ClientErrorCode::SizeDoesNotConformToMinVariation | ClientErrorCode::FractionalNotSupportedForAllocationOrders |
    ClientErrorCode::NonClosePositionOrderDoesNotSupportFractional | ClientErrorCode::ClearAwayNotSupportedForMultiLegHedge | ClientErrorCode::InvalidOrderBondExpired |
    ClientErrorCode::EtradeOnlyAttributeNotSupported | ClientErrorCode::FirmQuoteOnlyAttributeNotSupported | ClientErrorCode::NbboPriceCapAttributeNotSupported => {
      handler.order.handle_error(id, error_code, &error_msg_owned);
    }

    // Market Data Errors
    ClientErrorCode::FailSendReqMkt | ClientErrorCode::FailSendCanMkt | ClientErrorCode::FailSendReqMktDepth |
    ClientErrorCode::FailSendCanMktDepth | ClientErrorCode::FailSendReqScanner | ClientErrorCode::FailSendCanScanner |
    ClientErrorCode::FailSendReqScannerParameters | ClientErrorCode::FailSendReqHistData | ClientErrorCode::FailSendCanHistData |
    ClientErrorCode::FailSendReqRtBars | ClientErrorCode::FailSendCanRtBars | ClientErrorCode::FailSendReqCalcImpliedVolat |
    ClientErrorCode::FailSendReqCalcOptionPrice | ClientErrorCode::FailSendCanCalcImpliedVolat | ClientErrorCode::FailSendCanCalcOptionPrice |
    ClientErrorCode::FailSendReqMarketDataType | ClientErrorCode::FailSendReqHistogramData | ClientErrorCode::FailSendCancelHistogramData |
    ClientErrorCode::FailSendReqHistoricalTicks | ClientErrorCode::FailSendReqTickByTickData | ClientErrorCode::FailSendCancelTickByTickData |
    ClientErrorCode::FailSendReqHeadTimestamp | ClientErrorCode::FailSendCancelHeadTimestamp | ClientErrorCode::MaxTickersReached |
    ClientErrorCode::DuplicateTickerId | ClientErrorCode::HistoricalDataServiceError | ClientErrorCode::HistoricalDataServiceQueryMessage |
    ClientErrorCode::HistoricalDataExpiredContractViolation | ClientErrorCode::CouldNotParseTickerRequest | ClientErrorCode::CannotFindEidWithTickerId |
    ClientErrorCode::InvalidTickerAction | ClientErrorCode::ErrorParsingStopTickerString | ClientErrorCode::MaxMarketDepthRequestsReached |
    ClientErrorCode::CannotFindSubscribedMarketDepth | ClientErrorCode::MarketDepthDataHalted | ClientErrorCode::MarketDepthDataReset |
    ClientErrorCode::NotSubscribedToMarketData | ClientErrorCode::NoScannerSubscriptionFound | ClientErrorCode::NoHistoricalDataQueryFound |
    ClientErrorCode::DuplicateTickerIdApiScanner | ClientErrorCode::DuplicateTickerIdApiHistorical | ClientErrorCode::SnapshotNotApplicableToGenericTicks |
    ClientErrorCode::InvalidRealTimeQuery | ClientErrorCode::NotSubscribedMarketData | ClientErrorCode::PartiallySubscribedMarketData |
    ClientErrorCode::MarketDataNotSubscribedDisplayDelayed | ClientErrorCode::MarketDataNotSubscribedDelayedDisabled | ClientErrorCode::NoMarketDataDuringCompetingSession |
    ClientErrorCode::BustEventDeactivatedSubscription => {
      handler.data_market.handle_error(id, error_code, &error_msg_owned);
    }

    // Account/Position Errors
    ClientErrorCode::FailSendAcct | ClientErrorCode::FailSendExec | ClientErrorCode::FailSendReqPositions |
    ClientErrorCode::FailSendCanPositions | ClientErrorCode::FailSendReqAccountData | ClientErrorCode::FailSendCanAccountData |
    ClientErrorCode::FailSendReqPositionsMulti | ClientErrorCode::FailSendCanPositionsMulti | ClientErrorCode::FailSendReqAccountUpdatesMulti |
    ClientErrorCode::FailSendCanAccountUpdatesMulti | ClientErrorCode::FailSendReqPnl | ClientErrorCode::FailSendCancelPnl |
    ClientErrorCode::FailSendReqPnlSingle | ClientErrorCode::FailSendCancelPnlSingle | ClientErrorCode::PreLiquidationWarning |
    ClientErrorCode::InstitutionalAccountMissingInfo | ClientErrorCode::SecurityNotAvailableOrAllowed | ClientErrorCode::InvalidAccountValueAction |
    ClientErrorCode::NotFinancialAdvisorAccount | ClientErrorCode::NotInstitutionalOrAwayClearingAccount | ClientErrorCode::NoAccountHasEnoughShares |
    ClientErrorCode::MustSpecifyAccount | ClientErrorCode::AccountDoesNotHaveFractionalPermission | ClientErrorCode::TradingNotAllowedInApi => {
      handler.account.handle_error(id, error_code, &error_msg_owned);
    }

    // Reference Data Errors
    ClientErrorCode::UnknownContract | ClientErrorCode::FailSendReqContract | ClientErrorCode::FailSendReqSecDefOptParams |
    ClientErrorCode::FailSendReqSoftDollarTiers | ClientErrorCode::FailSendReqFamilyCodes | ClientErrorCode::FailSendReqMatchingSymbols |
    ClientErrorCode::FailSendReqMktDepthExchanges | ClientErrorCode::FailSendReqSmartComponents | ClientErrorCode::FailSendReqMarketRule |
    ClientErrorCode::NoRecordForConid | ClientErrorCode::NoMarketRuleForConid | ClientErrorCode::AmbiguousContract |
    ClientErrorCode::InvalidRoute | ClientErrorCode::ContractNotAvailableForTrading | ClientErrorCode::WhatToShowMissingOrIncorrect |
    ClientErrorCode::CrossCurrencyComboError | ClientErrorCode::CrossCurrencyVolError | ClientErrorCode::InvalidNonGuaranteedLegs |
    ClientErrorCode::IbsxNotAllowed | ClientErrorCode::ReadOnlyModels | ClientErrorCode::InvalidHedgeType |
    ClientErrorCode::InvalidBetaValue | ClientErrorCode::InvalidHedgeRatio | ClientErrorCode::CurrencyNotSupportedForSmartCombo |
    ClientErrorCode::SmartRoutingApiErrorOptOutRequired | ClientErrorCode::PctChangeLimits | ClientErrorCode::ContractNotVisible |
    ClientErrorCode::ContractsNotVisible | ClientErrorCode::InstrumentDoesNotSupportFractional | ClientErrorCode::OnlySmartRoutingSupportsFractional => {
      handler.data_ref.handle_error(id, error_code, &error_msg_owned);
    }

    // Financial Data Errors
    ClientErrorCode::FailSendReqFundData | ClientErrorCode::FailSendCanFundData | ClientErrorCode::FailSendReqWshMetaData |
    ClientErrorCode::FailSendCanWshMetaData | ClientErrorCode::FailSendReqWshEventData | ClientErrorCode::FailSendCanWshEventData |
    ClientErrorCode::FundamentalsDataNotAvailable | ClientErrorCode::DuplicateWshMetadataRequest | ClientErrorCode::FailedRequestWshMetadata |
    ClientErrorCode::FailedCancelWshMetadata | ClientErrorCode::DuplicateWshEventDataRequest | ClientErrorCode::WshMetadataNotRequested |
    ClientErrorCode::FailRequestWshEventData | ClientErrorCode::FailCancelWshEventData => {
      handler.data_fin.handle_error(id, error_code, &error_msg_owned);
    }

    // News Data Errors
    ClientErrorCode::FailSendReqNewsProviders | ClientErrorCode::FailSendReqNewsArticle | ClientErrorCode::FailSendReqHistoricalNews |
    ClientErrorCode::NewsFeedNotAllowed | ClientErrorCode::NewsFeedPermissionsRequired => {
      handler.data_news.handle_error(id, error_code, &error_msg_owned);
    }

    // Financial Advisor Errors
    ClientErrorCode::FailSendFaRequest | ClientErrorCode::FailSendFaReplace | ClientErrorCode::FaProfileNotSupported |
    ClientErrorCode::ManagedAccountsListRequiresFaStl | ClientErrorCode::FaStlHasNoManagedAccounts | ClientErrorCode::InvalidAccountCodesForOrderProfile |
    ClientErrorCode::FaOrderRequiresAllocation | ClientErrorCode::FaOrderRequiresManualAllocation | ClientErrorCode::InvalidAllocationPercentage |
    ClientErrorCode::UnsavedFaChanges | ClientErrorCode::FaGroupsProfilesContainInvalidAccounts | ClientErrorCode::AdvisorSetupWebAppCommunicationError => {
      handler.fin_adv.handle_error(id, error_code, &error_msg_owned);
    }

    // General Client/Connection/API Errors & Warnings
    ClientErrorCode::NoValidId | ClientErrorCode::AlreadyConnected | ClientErrorCode::ConnectFail |
    ClientErrorCode::UpdateTws | ClientErrorCode::NotConnected | ClientErrorCode::UnknownId |
    ClientErrorCode::UnsupportedVersion | ClientErrorCode::BadLength | ClientErrorCode::BadMessage |
    ClientErrorCode::FailSend | ClientErrorCode::FailSendServerLogLevel | ClientErrorCode::FailSendReqCurrTime |
    ClientErrorCode::FailSendReqGlobalCancel | ClientErrorCode::FailSendVerifyRequest | ClientErrorCode::FailSendVerifyMessage |
    ClientErrorCode::FailSendQueryDisplayGroups | ClientErrorCode::FailSendSubscribeToGroupEvents | ClientErrorCode::FailSendUpdateDisplayGroup |
    ClientErrorCode::FailSendUnsubscribeFromGroupEvents | ClientErrorCode::FailSendStartApi | ClientErrorCode::FailSendVerifyAndAuthRequest |
    ClientErrorCode::FailSendVerifyAndAuthMessage | ClientErrorCode::InvalidSymbol | ClientErrorCode::FailSendReqUserInfo |
    ClientErrorCode::ConnectivityLost | ClientErrorCode::ConnectivityRestoredDataLost | ClientErrorCode::ConnectivityRestoredDataMaintained |
    ClientErrorCode::SocketPortReset | ClientErrorCode::AccountUpdateSubscriptionOverridden | ClientErrorCode::AccountUpdateSubscriptionRejected |
    ClientErrorCode::OrderModificationRejectedProcessing | ClientErrorCode::MarketDataFarmDisconnected | ClientErrorCode::MarketDataFarmConnecting |
    ClientErrorCode::MarketDataFarmConnected | ClientErrorCode::HistoricalDataFarmDisconnected | ClientErrorCode::HistoricalDataFarmConnected |
    ClientErrorCode::HistoricalDataFarmInactive | ClientErrorCode::MarketDataFarmInactive | ClientErrorCode::OutsideRthAttributeIgnored |
    ClientErrorCode::TwsToServerConnectionBroken | ClientErrorCode::CrossSideWarning | ClientErrorCode::SecurityDefinitionDataFarmConnected |
    ClientErrorCode::EtradeOnlyNotSupportedWarning | ClientErrorCode::FirmQuoteOnlyNotSupportedWarning | ClientErrorCode::NbboPriceCapAttributeNotSupported |
    ClientErrorCode::MaxMessagesPerSecondExceeded | ClientErrorCode::RequestIdNotInteger | ClientErrorCode::ParsingError |
    ClientErrorCode::RequestParsingErrorIgnored | ClientErrorCode::ErrorProcessingDdeRequest | ClientErrorCode::InvalidRequestTopic |
    ClientErrorCode::MaxApiPagesReached | ClientErrorCode::InvalidLogLevel | ClientErrorCode::ServerErrorCause |
    ClientErrorCode::ServerErrorReadingDdeClientRequest | ClientErrorCode::ClientIdInUse | ClientErrorCode::AutoBindRequiresClientIdZero |
    ClientErrorCode::ClientVersionOutOfDate | ClientErrorCode::DdeDllNeedsUpgrade | ClientErrorCode::InvalidDdeArrayRequest |
    ClientErrorCode::ApplicationLocked => {
      handler.client.handle_error(id, error_code, &error_msg_owned);
    }

    ClientErrorCode::ServerErrorReadingApiClientRequest |
    ClientErrorCode::ServerErrorValidatingApiClientRequest |
    ClientErrorCode::ServerErrorProcessingApiClientRequest => {
      // Manual routing:
      match error_msg_str {
        msg if msg.contains("account summary requests") => {
          handler.account.handle_error(id, error_code, msg);
        },
        _ => { handler.client.handle_error(id, error_code, error_msg_str); }
      }
    }
    ClientErrorCode::NoSecurityDefinitionFound => {
      // Broadcast to managers. Note that the client ID is in a separate space, so it might
      // trigger a cancel accidentally.
      handler.order.handle_error(id, error_code, &error_msg_owned);
      handler.data_market.handle_error(id, error_code, &error_msg_owned);
      handler.data_ref.handle_error(id, error_code, &error_msg_owned);
      handler.data_news.handle_error(id, error_code, &error_msg_owned);
    }
  }

  Ok(())
}

/// Process current time message (Type 49)
pub fn process_current_time(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Version is unused for this message type according to docs
  let _version = parser.read_int(false)?;
  let time_unix = parser.read_int(false)? as i64; // TWS sends Unix timestamp

  // --- Call the handler ---
  handler.current_time(time_unix);
  // ---

  Ok(())
}

/// Process verify message API message (Type 65)
pub fn process_verify_message_api(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int(false)?;
  let api_data = parser.read_str()?;

  // --- Call the handler ---
  handler.verify_message_api(api_data);
  // ---

  warn!("Processing logic for VerifyMessageAPI not fully implemented.");
  Ok(())
}

/// Process verify completed message (Type 66)
pub fn process_verify_completed(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int(false)?;
  let is_successful_str = parser.read_str()?; // "true" or "false"
  let error_text = parser.read_str()?;

  let is_successful = is_successful_str.eq_ignore_ascii_case("true");

  // --- Call the handler ---
  handler.verify_completed(is_successful, error_text);
  // ---

  Ok(())
}

/// Process verify and auth message API (Type 69)
pub fn process_verify_and_auth_message_api(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int(false)?;
  let api_data = parser.read_str()?;
  let xyz_challenge = parser.read_str()?;

  // --- Call the handler ---
  handler.verify_and_auth_message_api(api_data, xyz_challenge);
  // ---

  warn!("Processing logic for VerifyAndAuthMessageAPI not fully implemented (challenge response needed).");
  Ok(())
}

/// Process verify and auth completed message (Type 70)
pub fn process_verify_and_auth_completed(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int(false)?;
  let is_successful_str = parser.read_str()?; // "true" or "false"
  let error_text = parser.read_str()?;

  let is_successful = is_successful_str.eq_ignore_ascii_case("true");

  // --- Call the handler ---
  handler.verify_and_auth_completed(is_successful, error_text);
  // ---

  Ok(())
}

/// Process head timestamp message (Type 88)
pub fn process_head_timestamp(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Version is unused according to docs
  // let _version = parser.read_int(false)?;
  let req_id = parser.read_int(false)?;
  let timestamp_str = parser.read_str()?; // Can be Unix timestamp or "yyyyMMdd HH:mm:ss"

  // --- Call the handler ---
  handler.head_timestamp(req_id, timestamp_str);
  // ---

  // Parsing the timestamp string could happen here or in the handler
  Ok(())
}

/// Process user info message (Type 107)
pub fn process_user_info(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Version is unused according to docs
  // let _version = parser.read_int(false)?;
  let req_id = parser.read_int(false)?; // Will always be 0 from TWS
  let white_branding_id = parser.read_str()?;

  // --- Call the handler ---
  handler.user_info(req_id, white_branding_id);
  // ---

  Ok(())
}

/// Process display group list message (Type 67)
pub fn process_display_group_list(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int(false)?;
  let req_id = parser.read_int(false)?;
  let groups = parser.read_str()?; // Comma-separated list of group IDs

  // --- Call the handler ---
  handler.display_group_list(req_id, groups);
  // ---

  Ok(())
}

/// Process display group updated message (Type 68)
pub fn process_display_group_updated(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int(false)?;
  let req_id = parser.read_int(false)?;
  let contract_info = parser.read_str()?; // Encoded contract string (needs parsing if used)

  // --- Call the handler ---
  handler.display_group_updated(req_id, contract_info);
  // ---

  warn!("Parsing of contract_info in DisplayGroupUpdated not implemented.");
  Ok(())
}