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
  let version = parser.read_int()?;

  let mut id: i32 = -1; // Default for version < 2 or errors not associated with a request
  let error_code_int: i32;
  let error_msg: String;

  if version < 2 {
    // Very old format, just a single message string
    error_msg = parser.read_string()?;
    error_code_int = 0; // No code provided in this format, treat as unknown or info?
    // Let's try to map 0 to a specific code if appropriate, or handle it
  } else {
    id = parser.read_int()?;
    error_code_int = parser.read_int()?;
    error_msg = parser.read_string()?;
    // Version >= 3 adds an optional 'advanced order reject' JSON string
    // TODO: Parse advanced_order_reject if server_version supports it
    // if handler.get_server_version() >= crate::versions::ADVANCED_ORDER_REJECT {
    //     let _advanced_order_reject = parser.read_string()?;
    // }
  }

  // Convert integer code to ClientErrorCode enum
  let error_code = match ClientErrorCode::try_from(error_code_int) {
    Ok(code) => code,
    Err(_) => {
      // Handle unknown error codes - log and potentially use a generic error variant
      // TWS often uses codes >= 1000 for warnings/info related to specific situations
      if error_code_int >= 1000 && error_code_int < 2000 { // Example range for TWS info/warnings
        info!("Received TWS Info/Warning (ID: {}, Code: {}): {}", id, error_code_int, error_msg);
        // Optionally map to a generic Info/Warning enum variant if you add one
        // For now, we might skip calling the handler or call with a special code
        return Ok(()); // Or decide how to proceed
      } else {
        error!("Received unknown TWS Error Code (ID: {}, Code: {}): {}", id, error_code_int, error_msg);
        // TODO: Consider adding an Unknown variant to ClientErrorCode
        // For now, skip calling handler for truly unknown codes? Or call with NoValidId?
        // Calling with NoValidId might be misleading. Let's skip for now.
        return Ok(());
      }
    }
  };

  // Log based on severity (using the enum might simplify this later)
  // Codes >= 2000 are usually info/warnings in TWS docs, but our enum doesn't reflect that split yet.
  // We'll rely on the handler implementation to interpret the code.
  // Basic logging here:
  if error_code_int >= 2000 && error_code_int < 3000 {
    info!("TWS Info/Warning (ID: {}, Code: {:?}={}): {}", id, error_code, error_code_int, error_msg);
  } else {
    error!("TWS Error (ID: {}, Code: {:?}={}): {}", id, error_code, error_code_int, error_msg);
  }


  // --- Route to the appropriate handler based on error code ---
  // Note: This routing is based on the error code's likely origin.
  // A more robust system would track request IDs and their associated handler types.
  match error_code {
    // Order Handling Errors (Client-side send failures + TWS order-related errors)
    ClientErrorCode::FailSendOrder |
    ClientErrorCode::FailSendCOrder |
    ClientErrorCode::FailSendOOrder |
    ClientErrorCode::FailSendReqCompletedOrders |
    // TWS Order Errors
    ClientErrorCode::DuplicateOrderId |
    ClientErrorCode::CannotModifyFilledOrder |
    ClientErrorCode::OrderModificationMismatch |
    ClientErrorCode::CannotTransmitOrderId | // Placeholder
    ClientErrorCode::CannotTransmitIncompleteOrder |
    ClientErrorCode::PriceOutOfPercentageRange |
    ClientErrorCode::PriceIncrementViolation |
    ClientErrorCode::TifOrderTypeIncompatible |
    ClientErrorCode::TifRequiresDayForMocLoc |
    ClientErrorCode::RelativeOrdersStocksOnly | // Deprecated
    ClientErrorCode::RelativeOrdersUsStocksRouting | // Deprecated
    ClientErrorCode::CannotTransmitToDeadExchange |
    ClientErrorCode::BlockOrderSizeTooSmall |
    ClientErrorCode::VwapOrdersRequireVwapExchange |
    ClientErrorCode::OnlyVwapOrdersOnVwapExchange |
    ClientErrorCode::TooLateForVwapOrder |
    ClientErrorCode::InvalidBdFlag | // Deprecated
    ClientErrorCode::NoRequestTagForOrder | // Placeholder
    ClientErrorCode::BuyPriceMustMatchBestAsk |
    ClientErrorCode::SellPriceMustMatchBestBid |
    ClientErrorCode::VwapOrderSubmitTimeViolation |
    ClientErrorCode::SweepToFillDisplaySizeIgnored |
    ClientErrorCode::MissingClearingAccount | // Could be Account, but often order-related
    ClientErrorCode::SubmitNewOrderFailed |
    ClientErrorCode::ModifyOrderFailed |
    ClientErrorCode::CannotFindOrderToCancel | // Placeholder
    ClientErrorCode::OrderCannotBeCancelled |
    ClientErrorCode::VwapOrderCancelTimeViolation |
    ClientErrorCode::SizeValueShouldBeInteger | // Placeholder
    ClientErrorCode::PriceValueShouldBeDouble | // Placeholder
    ClientErrorCode::OrderSizeAllocationMismatch |
    ClientErrorCode::ValidationErrorInEntryFields | // Placeholder, could be general
    ClientErrorCode::InvalidTriggerMethod |
    ClientErrorCode::ConditionalContractInfoIncomplete | // Could be Contract
    ClientErrorCode::ConditionalOrderRequiresLimitMarket | // Deprecated
    ClientErrorCode::MissingUserNameDDE | // DDE specific
    ClientErrorCode::HiddenAttributeNotAllowed |
    ClientErrorCode::EfpRequiresLimitOrder | // Deprecated
    ClientErrorCode::CannotTransmitOrderForHaltedSecurity |
    ClientErrorCode::SizeOpOrderRequiresUserAccount | // Deprecated
    ClientErrorCode::SizeOpOrderRequiresIBSX | // Deprecated
    ClientErrorCode::IcebergDiscretionaryConflict |
    ClientErrorCode::MissingTrailOffset |
    ClientErrorCode::PercentOffsetOutOfRange |
    ClientErrorCode::SizeValueCannotBeZero | // Also 434
    ClientErrorCode::CancelAttemptNotInCancellableState | // Placeholder
    ClientErrorCode::PriceViolatesPercentageConstraint |
    ClientErrorCode::NoMarketDataForPricePercentCheck | // Could be Market Data
    ClientErrorCode::VwapOrderTimeNotInFuture |
    ClientErrorCode::DiscretionaryAmountIncrementViolation |
    ClientErrorCode::OrderRejected | // Placeholder
    ClientErrorCode::OrderCancelled | // Placeholder
    ClientErrorCode::InvalidAction | // Placeholder, could be general
    ClientErrorCode::InvalidOrigin |
    ClientErrorCode::InvalidComboDetails | // Could be Contract
    ClientErrorCode::InvalidComboLegDetails | // Placeholder, Could be Contract
    ClientErrorCode::BagSecurityTypeRequiresComboLegs | // Could be Contract
    ClientErrorCode::StockComboLegsRequireSmartRouting | // Could be Contract
    ClientErrorCode::DiscretionaryOrdersNotSupported |
    ClientErrorCode::TrailStopAttachViolation |
    ClientErrorCode::OrderModifyFailedCannotChangeType |
    ClientErrorCode::InvalidShareAllocationSyntax | // Could be FA
    ClientErrorCode::InvalidGoodTillDateOrder |
    ClientErrorCode::InvalidDeltaRange |
    ClientErrorCode::InvalidTimeOrTimeZoneFormat | // Could be general
    ClientErrorCode::InvalidDateTimeOrTimeZoneFormat | // Could be general
    ClientErrorCode::GoodAfterTimeDisabled |
    ClientErrorCode::FuturesSpreadNotSupported | // Use Combos (Contract)
    ClientErrorCode::InvalidImprovementAmountBoxAuction |
    ClientErrorCode::InvalidDeltaValue1To100 |
    ClientErrorCode::PeggedOrderNotSupported |
    ClientErrorCode::InvalidDateTimeOrTimeZoneFormatYmdHms | // Could be general
    ClientErrorCode::GenericComboNotSupportedForFA | // Could be FA/Contract
    ClientErrorCode::InvalidShortSaleSlotValue |
    ClientErrorCode::ShortSaleSlotRequiresSshortAction |
    ClientErrorCode::GenericComboDoesNotSupportGoodAfter | // Could be Contract
    ClientErrorCode::MinQuantityNotSupportedForBestCombo | // Could be Contract
    ClientErrorCode::RthOnlyFlagNotValid |
    ClientErrorCode::ShortSaleSlot2RequiresLocation |
    ClientErrorCode::ShortSaleSlot1RequiresNoLocation |
    ClientErrorCode::OrderSizeMarketRuleViolation | // Could be Contract/Market Data
    ClientErrorCode::SmartComboDoesNotSupportOca | // Could be Contract
    ClientErrorCode::SmartComboChildOrderNotSupported | // Could be Contract
    ClientErrorCode::ComboOrderReduceOnFillOcaViolation | // Could be Contract
    ClientErrorCode::NoWhatifSupportForSmartCombo | // Could be Contract
    ClientErrorCode::InvalidTriggerPrice |
    ClientErrorCode::InvalidAdjustedStopPrice |
    ClientErrorCode::InvalidAdjustedStopLimitPrice |
    ClientErrorCode::InvalidAdjustedTrailingAmount |
    ClientErrorCode::InvalidVolatilityTypeForVolOrder |
    ClientErrorCode::InvalidReferencePriceTypeForVolOrder |
    ClientErrorCode::VolatilityOrdersOnlyForUsOptions | // Could be Contract
    ClientErrorCode::DynamicVolatilityRoutingViolation |
    ClientErrorCode::VolOrderRequiresPositiveVolatility |
    ClientErrorCode::CannotSetDynamicVolOnNonVolOrder |
    ClientErrorCode::StockRangeAttributeRequiresVolOrRel |
    ClientErrorCode::InvalidStockRangeAttributesOrder |
    ClientErrorCode::StockRangeAttributesCannotBeNegative |
    ClientErrorCode::NotEligibleForContinuousUpdate | // Could be Contract/Market Data
    ClientErrorCode::MustSpecifyValidDeltaHedgeAuxPrice |
    ClientErrorCode::DeltaHedgeOrderRequiresAuxPrice |
    ClientErrorCode::DeltaHedgeOrderRequiresNoAuxPrice |
    ClientErrorCode::OrderTypeNotAllowedForDeltaHedge |
    ClientErrorCode::PriceViolatesTicksConstraint | // Could be Contract/Market Data
    ClientErrorCode::SizeViolatesSizeConstraint | // Could be Contract/Market Data
    ClientErrorCode::UnsupportedOrderTypeForExchangeSecType | // Could be Contract
    ClientErrorCode::OrderSizeSmallerThanMinimum | // Could be Contract
    ClientErrorCode::RoutedOrderIdNotUnique |
    ClientErrorCode::RoutedOrderIdInvalid |
    ClientErrorCode::InvalidTimeOrTimeZoneFormatHms | // Could be general
    ClientErrorCode::InvalidOrderContractExpired | // Could be Contract
    ClientErrorCode::ShortSaleSlotOnlyForDeltaHedge |
    ClientErrorCode::InvalidProcessTime | // Placeholder
    ClientErrorCode::OcaOrdersNotAcceptedSystemProblem | // System/Client
    ClientErrorCode::OnlyMarketLimitOrdersAcceptedSystemProblem | // System/Client
    ClientErrorCode::InvalidConditionTrigger | // Placeholder
    ClientErrorCode::OrderMessageError | // Placeholder
    ClientErrorCode::AlgoOrderError | // Placeholder
    ClientErrorCode::LengthRestriction | // Placeholder, could be general
    ClientErrorCode::ConditionsNotAllowedForContract | // Could be Contract
    ClientErrorCode::InvalidStopPrice |
    ClientErrorCode::ShortSaleSharesNotAvailable | // Could be Account
    ClientErrorCode::ChildQuantityShouldMatchParentSize | // Deprecated
    ClientErrorCode::CurrencyNotAllowed | // Placeholder, Could be Contract
    ClientErrorCode::SymbolRequiresNonUnicode | // Could be Contract
    ClientErrorCode::InvalidScaleOrderIncrement |
    ClientErrorCode::InvalidScaleOrderMissingComponentSize |
    ClientErrorCode::InvalidScaleOrderSubsequentComponentSize |
    ClientErrorCode::OutsideRthFlagNotValidForOrder | // Also 2109 (Warning)
    ClientErrorCode::WhatIfOrderRequiresTransmitTrue |
    ClientErrorCode::WaitPreviousRfqFinish |
    ClientErrorCode::RfqNotApplicableForContract | // Placeholder, Could be Contract
    ClientErrorCode::InvalidScaleOrderInitialComponentSize |
    ClientErrorCode::InvalidScaleOrderProfitOffset |
    ClientErrorCode::MissingScaleOrderInitialComponentSize |
    ClientErrorCode::CannotChangeAccountClearingAttributes | // Could be Account
    ClientErrorCode::CrossOrderRfqExpired |
    ClientErrorCode::MutualFundOrderRequiresMonetaryValue | // Deprecated
    ClientErrorCode::MutualFundSellOrderRequiresShares | // Deprecated
    ClientErrorCode::DeltaNeutralOnlyForCombos | // Could be Contract
    ClientErrorCode::CommissionMustNotBeNegative | // Deprecated
    ClientErrorCode::InvalidRestoreSizeAfterProfit | // Could be FA/Account
    ClientErrorCode::OrderSizeCannotBeZero | // Also 160
    ClientErrorCode::MustSpecifyAllocation | // Could be FA
    ClientErrorCode::OnlyOneOutsideRthOrAllowPreOpen | // Deprecated
    ClientErrorCode::AlgoDefinitionNotFound |
    ClientErrorCode::AlgoCannotBeModified |
    ClientErrorCode::AlgoAttributesValidationFailed | // Placeholder
    ClientErrorCode::AlgoNotAllowedForOrder |
    ClientErrorCode::UnknownAlgoAttribute |
    ClientErrorCode::VolComboOrderNotAcknowledged | // Could be Contract
    ClientErrorCode::RfqNoLongerValid |
    ClientErrorCode::MissingScaleOrderProfitOffset | // Also 418
    ClientErrorCode::MissingScalePriceAdjustment |
    ClientErrorCode::InvalidScalePriceAdjustmentInterval |
    ClientErrorCode::UnexpectedScalePriceAdjustment |
    // TWS Order Errors (10000+)
    ClientErrorCode::MissingParentOrder |
    ClientErrorCode::InvalidDeltaHedgeOrder |
    ClientErrorCode::OrdersUseEvWarning | // Warning
    ClientErrorCode::TradesUseEvWarning | // Warning
    ClientErrorCode::DisplaySizeShouldBeSmaller |
    ClientErrorCode::InvalidLeg2ToMktOffsetApi | // Deprecated
    ClientErrorCode::InvalidLegPrioApi | // Deprecated
    ClientErrorCode::InvalidComboDisplaySizeApi | // Deprecated
    ClientErrorCode::InvalidDontStartNextLeginApi | // Deprecated
    ClientErrorCode::InvalidLeg2ToMktTime1Api | // Deprecated
    ClientErrorCode::InvalidLeg2ToMktTime2Api | // Deprecated
    ClientErrorCode::InvalidComboRoutingTagApi | // Deprecated
    ClientErrorCode::CannotCancelFilledOrder | // Placeholder
    ClientErrorCode::DefaultsInheritedFromCashPreset | // Info/Warning
    ClientErrorCode::DecisionMakerRequiredNonDesktop |
    ClientErrorCode::DecisionMakerRequiredIbbot |
    ClientErrorCode::ChildMustBeAonIfParentAon |
    ClientErrorCode::AonTicketCanRouteEntireUnfilledOnly |
    ClientErrorCode::OrderAffectsFlaggedAccountsRiskScore | // Warning/FA
    ClientErrorCode::MustEnterValidPriceCap |
    ClientErrorCode::MonetaryQuantityModificationNotSupported |
    ClientErrorCode::FractionalOrderModificationNotSupported |
    ClientErrorCode::FractionalOrderPlacementNotSupported |
    ClientErrorCode::CashQuantityNotAllowedForOrder |
    ClientErrorCode::OrderDoesNotSupportFractional |
    ClientErrorCode::OrderTypeDoesNotSupportFractional | // Placeholder
    ClientErrorCode::SizeDoesNotConformToMinVariation | // Could be Contract
    ClientErrorCode::FractionalNotSupportedForAllocationOrders | // Could be FA
    ClientErrorCode::NonClosePositionOrderDoesNotSupportFractional |
    ClientErrorCode::ClearAwayNotSupportedForMultiLegHedge | // Could be Contract
    ClientErrorCode::InvalidOrderBondExpired | // Could be Contract
    ClientErrorCode::EtradeOnlyAttributeNotSupported |
    ClientErrorCode::FirmQuoteOnlyAttributeNotSupported |
    ClientErrorCode::NbboPriceCapAttributeNotSupported => {
      handler.order.handle_error(id, error_code, &error_msg);
    }

    // Market Data Errors (Client-side send failures + TWS market data errors)
    ClientErrorCode::FailSendReqMkt |
    ClientErrorCode::FailSendCanMkt |
    ClientErrorCode::FailSendReqMktDepth |
    ClientErrorCode::FailSendCanMktDepth |
    ClientErrorCode::FailSendReqScanner |
    ClientErrorCode::FailSendCanScanner |
    ClientErrorCode::FailSendReqScannerParameters |
    ClientErrorCode::FailSendReqHistData |
    ClientErrorCode::FailSendCanHistData | // Note: Same message as ReqHistData
    ClientErrorCode::FailSendReqRtBars |
    ClientErrorCode::FailSendCanRtBars |
    ClientErrorCode::FailSendReqCalcImpliedVolat |
    ClientErrorCode::FailSendReqCalcOptionPrice |
    ClientErrorCode::FailSendCanCalcImpliedVolat |
    ClientErrorCode::FailSendCanCalcOptionPrice |
    ClientErrorCode::FailSendReqMarketDataType |
    ClientErrorCode::FailSendReqHistogramData |
    ClientErrorCode::FailSendCancelHistogramData |
    ClientErrorCode::FailSendReqHistoricalTicks |
    ClientErrorCode::FailSendReqTickByTickData |
    ClientErrorCode::FailSendCancelTickByTickData |
    ClientErrorCode::FailSendReqHeadTimestamp |
    ClientErrorCode::FailSendCancelHeadTimestamp |
    // TWS Market Data Errors
    ClientErrorCode::MaxTickersReached |
    ClientErrorCode::DuplicateTickerId |
    ClientErrorCode::HistoricalDataServiceError | // Placeholder
    ClientErrorCode::HistoricalDataServiceQueryMessage | // Placeholder
    ClientErrorCode::HistoricalDataExpiredContractViolation | // Could be Contract
    ClientErrorCode::CouldNotParseTickerRequest | // Placeholder
    ClientErrorCode::CannotFindEidWithTickerId | // Placeholder, DDE specific
    ClientErrorCode::InvalidTickerAction | // Placeholder, DDE specific
    ClientErrorCode::ErrorParsingStopTickerString | // Placeholder, DDE specific
    ClientErrorCode::MaxMarketDepthRequestsReached |
    ClientErrorCode::CannotFindSubscribedMarketDepth | // Placeholder
    ClientErrorCode::MarketDepthDataHalted |
    ClientErrorCode::MarketDepthDataReset |
    ClientErrorCode::NotSubscribedToMarketData |
    ClientErrorCode::NoScannerSubscriptionFound | // Placeholder
    ClientErrorCode::NoHistoricalDataQueryFound | // Placeholder
    ClientErrorCode::DuplicateTickerIdApiScanner |
    ClientErrorCode::DuplicateTickerIdApiHistorical |
    ClientErrorCode::SnapshotNotApplicableToGenericTicks |
    ClientErrorCode::InvalidRealTimeQuery |
    // TWS Market Data Errors (10000+)
    ClientErrorCode::PartiallySubscribedMarketData | // Warning
    ClientErrorCode::MarketDataNotSubscribedDelayedDisabled | // Warning/Info
    ClientErrorCode::NoMarketDataDuringCompetingSession | // Warning/Info
    ClientErrorCode::BustEventDeactivatedSubscription => { // System/Warning
      handler.data_market.handle_error(id, error_code, &error_msg);
    }

    // Account/Position Errors (Client-side send failures + TWS account/position errors)
    ClientErrorCode::FailSendAcct |
    ClientErrorCode::FailSendExec |
    ClientErrorCode::FailSendReqPositions |
    ClientErrorCode::FailSendCanPositions |
    ClientErrorCode::FailSendReqAccountData |
    ClientErrorCode::FailSendCanAccountData |
    ClientErrorCode::FailSendReqPositionsMulti |
    ClientErrorCode::FailSendCanPositionsMulti |
    ClientErrorCode::FailSendReqAccountUpdatesMulti |
    ClientErrorCode::FailSendCanAccountUpdatesMulti |
    ClientErrorCode::FailSendReqPnl |
    ClientErrorCode::FailSendCancelPnl |
    ClientErrorCode::FailSendReqPnlSingle |
    ClientErrorCode::FailSendCancelPnlSingle |
    // TWS Account/Position Errors
    ClientErrorCode::InstitutionalAccountMissingInfo |
    ClientErrorCode::SecurityNotAvailableOrAllowed | // Could be Contract
    ClientErrorCode::InvalidAccountValueAction | // Placeholder, DDE specific
    ClientErrorCode::NotFinancialAdvisorAccount | // Could be FA
    ClientErrorCode::NotInstitutionalOrAwayClearingAccount |
    ClientErrorCode::NoAccountHasEnoughShares |
    ClientErrorCode::MustSpecifyAccount | // Could be general
    ClientErrorCode::AccountDoesNotHaveFractionalPermission | // Placeholder
    ClientErrorCode::TradingNotAllowedInApi => { // Could be Client/System
      handler.account.handle_error(id, error_code, &error_msg);
    }

    // Reference Data Errors (Client-side send failures + TWS contract/ref data errors)
    ClientErrorCode::UnknownContract |
    ClientErrorCode::FailSendReqContract |
    ClientErrorCode::FailSendReqSecDefOptParams |
    ClientErrorCode::FailSendReqSoftDollarTiers |
    ClientErrorCode::FailSendReqFamilyCodes |
    ClientErrorCode::FailSendReqMatchingSymbols |
    ClientErrorCode::FailSendReqMktDepthExchanges |
    ClientErrorCode::FailSendReqSmartComponents |
    ClientErrorCode::FailSendReqMarketRule |
    // TWS Reference Data Errors
    ClientErrorCode::NoRecordForConid | // Placeholder, Deprecated
    ClientErrorCode::NoMarketRuleForConid | // Placeholder
    ClientErrorCode::NoSecurityDefinitionFound |
    ClientErrorCode::AmbiguousContract | // Special code 2001
    ClientErrorCode::InvalidRoute | // Deprecated
    ClientErrorCode::ContractNotAvailableForTrading |
    ClientErrorCode::WhatToShowMissingOrIncorrect | // Deprecated
    // TWS Reference Data Errors (10000+)
    ClientErrorCode::CrossCurrencyComboError |
    ClientErrorCode::CrossCurrencyVolError |
    ClientErrorCode::InvalidNonGuaranteedLegs |
    ClientErrorCode::IbsxNotAllowed |
    ClientErrorCode::ReadOnlyModels |
    ClientErrorCode::InvalidHedgeType |
    ClientErrorCode::InvalidBetaValue |
    ClientErrorCode::InvalidHedgeRatio |
    ClientErrorCode::CurrencyNotSupportedForSmartCombo |
    ClientErrorCode::SmartRoutingApiErrorOptOutRequired |
    ClientErrorCode::PctChangeLimits | // Deprecated
    ClientErrorCode::ContractNotVisible | // Deprecated
    ClientErrorCode::ContractsNotVisible | // Deprecated
    ClientErrorCode::InstrumentDoesNotSupportFractional |
    ClientErrorCode::OnlySmartRoutingSupportsFractional => {
      handler.data_ref.handle_error(id, error_code, &error_msg);
    }

    // Financial Data Errors (Fundamental/WSH) (Client-side send failures + TWS errors)
    ClientErrorCode::FailSendReqFundData |
    ClientErrorCode::FailSendCanFundData |
    ClientErrorCode::FailSendReqWshMetaData |
    ClientErrorCode::FailSendCanWshMetaData |
    ClientErrorCode::FailSendReqWshEventData |
    ClientErrorCode::FailSendCanWshEventData |
    // TWS Financial Data Errors
    ClientErrorCode::FundamentalsDataNotAvailable |
    ClientErrorCode::DuplicateWshMetadataRequest |
    ClientErrorCode::FailedRequestWshMetadata |
    ClientErrorCode::FailedCancelWshMetadata |
    ClientErrorCode::DuplicateWshEventDataRequest |
    ClientErrorCode::WshMetadataNotRequested |
    ClientErrorCode::FailRequestWshEventData |
    ClientErrorCode::FailCancelWshEventData => {
      handler.data_fin.handle_error(id, error_code, &error_msg);
    }

    // News Data Errors (Client-side send failures + TWS errors)
    ClientErrorCode::FailSendReqNewsProviders |
    ClientErrorCode::FailSendReqNewsArticle |
    ClientErrorCode::FailSendReqHistoricalNews |
    // TWS News Errors
    ClientErrorCode::NewsFeedNotAllowed |
    ClientErrorCode::NewsFeedPermissionsRequired => {
      handler.data_news.handle_error(id, error_code, &error_msg);
    }

    // Financial Advisor Errors (Client-side send failures + TWS FA errors)
    ClientErrorCode::FailSendFaRequest |
    ClientErrorCode::FailSendFaReplace |
    ClientErrorCode::FaProfileNotSupported |
    // TWS FA Errors
    ClientErrorCode::ManagedAccountsListRequiresFaStl |
    ClientErrorCode::FaStlHasNoManagedAccounts |
    ClientErrorCode::InvalidAccountCodesForOrderProfile |
    ClientErrorCode::FaOrderRequiresAllocation | // Deprecated
    ClientErrorCode::FaOrderRequiresManualAllocation | // Deprecated
    ClientErrorCode::InvalidAllocationPercentage |
    ClientErrorCode::UnsavedFaChanges | // Warning
    ClientErrorCode::FaGroupsProfilesContainInvalidAccounts | // Placeholder
    ClientErrorCode::AdvisorSetupWebAppCommunicationError => { // Warning/Info
      handler.fin_adv.handle_error(id, error_code, &error_msg);
    }

    // General Client/Connection/API Errors & Warnings (Route to ClientHandler)
    // Client Library Errors
    ClientErrorCode::NoValidId |
    ClientErrorCode::AlreadyConnected |
    ClientErrorCode::ConnectFail |
    ClientErrorCode::UpdateTws |
    ClientErrorCode::NotConnected |
    ClientErrorCode::UnknownId |
    ClientErrorCode::UnsupportedVersion |
    ClientErrorCode::BadLength |
    ClientErrorCode::BadMessage |
    ClientErrorCode::FailSend | // Generic send failure
    ClientErrorCode::FailSendServerLogLevel |
    ClientErrorCode::FailSendReqCurrTime |
    ClientErrorCode::FailSendReqGlobalCancel |
    ClientErrorCode::FailSendVerifyRequest |
    ClientErrorCode::FailSendVerifyMessage |
    ClientErrorCode::FailSendQueryDisplayGroups |
    ClientErrorCode::FailSendSubscribeToGroupEvents |
    ClientErrorCode::FailSendUpdateDisplayGroup |
    ClientErrorCode::FailSendUnsubscribeFromGroupEvents |
    ClientErrorCode::FailSendStartApi |
    ClientErrorCode::FailSendVerifyAndAuthRequest |
    ClientErrorCode::FailSendVerifyAndAuthMessage |
    ClientErrorCode::InvalidSymbol | // Could be argued for others, but often general
    ClientErrorCode::FailSendReqUserInfo |
    // System Messages
    ClientErrorCode::ConnectivityLost |
    ClientErrorCode::ConnectivityRestoredDataLost |
    ClientErrorCode::ConnectivityRestoredDataMaintained |
    ClientErrorCode::SocketPortReset |
    // Warnings
    ClientErrorCode::AccountUpdateSubscriptionOverridden | // Could be Account
    ClientErrorCode::AccountUpdateSubscriptionRejected | // Could be Account
    ClientErrorCode::OrderModificationRejectedProcessing | // Could be Order
    ClientErrorCode::MarketDataFarmDisconnected | // Could be Market Data
    ClientErrorCode::MarketDataFarmConnected | // Could be Market Data
    ClientErrorCode::HistoricalDataFarmDisconnected | // Could be Market Data
    ClientErrorCode::HistoricalDataFarmConnected | // Could be Market Data
    ClientErrorCode::HistoricalDataFarmInactive | // Could be Market Data
    ClientErrorCode::MarketDataFarmInactive | // Could be Market Data
    ClientErrorCode::OutsideRthAttributeIgnored | // Could be Order
    ClientErrorCode::TwsToServerConnectionBroken |
    ClientErrorCode::CrossSideWarning | // Could be Order
    ClientErrorCode::SecurityDefinitionDataFarmConnected | // Could be Ref Data
    ClientErrorCode::EtradeOnlyNotSupportedWarning | // Could be Order
    ClientErrorCode::FirmQuoteOnlyNotSupportedWarning | // Could be Order
    // General TWS Errors
    ClientErrorCode::MaxMessagesPerSecondExceeded |
    ClientErrorCode::RequestIdNotInteger |
    ClientErrorCode::ParsingError | // Placeholder, DDE specific
    ClientErrorCode::RequestParsingErrorIgnored | // DDE specific
    ClientErrorCode::ErrorProcessingDdeRequest | // Placeholder, DDE specific
    ClientErrorCode::InvalidRequestTopic | // Placeholder, DDE specific
    ClientErrorCode::MaxApiPagesReached |
    ClientErrorCode::InvalidLogLevel | // Placeholder
    ClientErrorCode::ServerErrorReadingApiClientRequest |
    ClientErrorCode::ServerErrorValidatingApiClientRequest |
    ClientErrorCode::ServerErrorProcessingApiClientRequest |
    ClientErrorCode::ServerErrorCause | // Placeholder
    ClientErrorCode::ServerErrorReadingDdeClientRequest | // DDE specific
    ClientErrorCode::ClientIdInUse |
    ClientErrorCode::AutoBindRequiresClientIdZero |
    ClientErrorCode::ClientVersionOutOfDate |
    ClientErrorCode::DdeDllNeedsUpgrade | // DDE specific
    ClientErrorCode::InvalidDdeArrayRequest | // DDE specific
    ClientErrorCode::ApplicationLocked | // Deprecated
    ClientErrorCode::UnknownCode => { // Catch-all for explicitly unknown or default
      handler.client.handle_error(id, error_code, &error_msg);
    }
    // Catch-all for any codes missed in the above explicit routing.
    // This prevents compile errors if new codes are added to the enum
    // but not yet routed here. It defaults to the client handler.
    // _ => {
    //     warn!("Error code {:?} (ID: {}) not explicitly routed, defaulting to ClientHandler: {}", error_code, id, error_msg);
    //     handler.client.handle_error(id, error_code, &error_msg);
    // }
  }

  // Example of potential future routing (requires ID mapping):
  // if id > 0 {
  //     match id_registry.get_handler_type(id) { // id_registry needs to be accessible
  //         Some(HandlerType::Order) => handler.order.handle_error(id, error_code, &error_msg),
  //         Some(HandlerType::MarketData) => handler.data_market.handle_error(id, error_code, &error_msg),
  //         // ... other handlers ...
  //         None => {
  //             warn!("Error received for unknown or inactive ID: {}. Routing to client handler.", id);
  //             handler.client.handle_error(id, error_code, &error_msg);
  //         }
  //     }
  // } else {
  //     // General error, route to client handler
  //     handler.client.handle_error(id, error_code, &error_msg);
  // }


  Ok(())
}

/// Process current time message (Type 49)
pub fn process_current_time(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Version is unused for this message type according to docs
  let _version = parser.read_int()?;
  let time_unix = parser.read_int()? as i64; // TWS sends Unix timestamp

  // --- Call the handler ---
  handler.current_time(time_unix);
  // ---

  Ok(())
}

/// Process verify message API message (Type 65)
pub fn process_verify_message_api(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let api_data = parser.read_string()?;

  // --- Call the handler ---
  handler.verify_message_api(&api_data);
  // ---

  warn!("Processing logic for VerifyMessageAPI not fully implemented.");
  Ok(())
}

/// Process verify completed message (Type 66)
pub fn process_verify_completed(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let is_successful_str = parser.read_string()?; // "true" or "false"
  let error_text = parser.read_string()?;

  let is_successful = is_successful_str.eq_ignore_ascii_case("true");

  // --- Call the handler ---
  handler.verify_completed(is_successful, &error_text);
  // ---

  Ok(())
}

/// Process verify and auth message API message (Type 69)
pub fn process_verify_and_auth_message_api(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let api_data = parser.read_string()?;
  let xyz_challenge = parser.read_string()?;

  // --- Call the handler ---
  handler.verify_and_auth_message_api(&api_data, &xyz_challenge);
  // ---

  warn!("Processing logic for VerifyAndAuthMessageAPI not fully implemented (challenge response needed).");
  Ok(())
}

/// Process verify and auth completed message (Type 70)
pub fn process_verify_and_auth_completed(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let is_successful_str = parser.read_string()?; // "true" or "false"
  let error_text = parser.read_string()?;

  let is_successful = is_successful_str.eq_ignore_ascii_case("true");

  // --- Call the handler ---
  handler.verify_and_auth_completed(is_successful, &error_text);
  // ---

  Ok(())
}

/// Process head timestamp message (Type 88)
pub fn process_head_timestamp(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Version is unused according to docs
  // let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let timestamp_str = parser.read_string()?; // Can be Unix timestamp or "yyyyMMdd HH:mm:ss"

  // --- Call the handler ---
  handler.head_timestamp(req_id, &timestamp_str);
  // ---

  // Parsing the timestamp string could happen here or in the handler
  Ok(())
}

/// Process user info message (Type 107)
pub fn process_user_info(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  // Version is unused according to docs
  // let _version = parser.read_int()?;
  let req_id = parser.read_int()?; // Will always be 0 from TWS
  let white_branding_id = parser.read_string()?;

  // --- Call the handler ---
  handler.user_info(req_id, &white_branding_id);
  // ---

  Ok(())
}

/// Process display group list message (Type 67)
pub fn process_display_group_list(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let groups = parser.read_string()?; // Comma-separated list of group IDs

  // --- Call the handler ---
  handler.display_group_list(req_id, &groups);
  // ---

  Ok(())
}

/// Process display group updated message (Type 68)
pub fn process_display_group_updated(handler: &Arc<dyn ClientHandler>, parser: &mut FieldParser) -> Result<(), IBKRError> {
  let _version = parser.read_int()?;
  let req_id = parser.read_int()?;
  let contract_info = parser.read_string()?; // Encoded contract string (needs parsing if used)

  // --- Call the handler ---
  handler.display_group_updated(req_id, &contract_info);
  // ---

  warn!("Parsing of contract_info in DisplayGroupUpdated not implemented.");
  Ok(())
}
