# Test
- options exercise and lapse
- finish happy paths
- failure paths
- same client_id failure path
- lib.rs doc about tick types, perhaps api.md and README.

# Missing features from ibapi:
1.  `account_updates_multi`: Request account updates for multiple accounts/models.
2.  `auto_open_orders`: Request status updates about future orders placed from TWS, with optional auto-binding.
3.  `completed_orders`: Request completed orders.
4.  `calculate_implied_volatility`: Calculate implied volatility for an option.
5.  `calculate_option_price`: Calculate option price based on volatility and underlying price.
6.  ~~`exercise_options`: Exercise an options contract.~~
7.  ~~`global_cancel`: Cancel all open orders.~~
8.  `head_timestamp`: Request the timestamp of the earliest available historical data for a contract and data type.
9.  `histogram_data`: Request data histogram for a contract. (Handlers exist, but no public request method).
10. `historical_schedules`: Request historical schedules for a contract and time interval. (Handlers exist, but no public request
method).
11. `historical_schedules_ending_now`: Request historical schedules ending now. (Handlers exist, but no public request method).
12. `historical_ticks_bid_ask`: Request historical Bid/Ask ticks. (Handlers exist, but no public request method).
13. `historical_ticks_mid_point`: Request historical Midpoint ticks. (Handlers exist, but no public request method).
14. `historical_ticks_trade`: Request historical Trade ticks. (Handlers exist, but no public request method).
15. `managed_accounts`: Request the list of accounts managed by the logged-in user. (Handler exists, but no public request method).
16. ~~`next_order_id`: Public method to get and increment the client's internal order ID sequence. (Internal logic exists, but no
public method with this specific behavior).~~
17. ~~`next_valid_order_id`: Public method to request the next valid order ID from the TWS server. (Initial fetch is handled
internally, but no public method to re-request).~~
18. ~~`open_orders`: Request all open orders placed by this specific API client ID.~~
19. ~~`pnl`: Explicit subscription request for real-time daily PnL and unrealized PnL updates. (Data is received via account
subscription, but no dedicated request method returning a subscription).~~
20. `pnl_single`: Explicit subscription request for real-time daily PnL of individual positions. (Data is received via account
subscription, but no dedicated request method returning a subscription, and handler implementation is noted as unreliable).
21. ~~`positions`: Explicit subscription request for position updates for all accessible accounts. (Data is received via account
subscription, but no dedicated request method returning a subscription).~~
22. `positions_multi`: Request position updates for multiple accounts/models.
23. ~~`scanner_parameters`: Request an XML list of scanner parameters. (Handler exists, but no public request method).~~
24. ~~`scanner_subscription`: Start a subscription to market scan results. (Handlers exist, but no public request method).~~
25. `server_time`: Request the current server time from TWS.
26. ~~`switch_market_data_type`: Public method to explicitly switch the market data type (Live, Frozen, Delayed, FrozenDelayed).
(Internal helper exists, but no public method).~~
27. `wsh_metadata`: Public blocking method to request and return WSH metadata. (Non-blocking request and internal caching exist, but
no public blocking getter).
28. `contract_news`: Request real-time contract-specific news. (Data is received via market data tick, but no dedicated request
method returning a subscription).

# Missing features from ib insync
1.  `reqCurrentTime`: Requests the current time from the TWS server.
2.  `reqAccountUpdatesMulti`: Subscribes to account updates for multiple accounts/models.
3.  ~~`reqCompletedOrders`: Requests a list of completed orders.~~
4.  ~~`reqPnL`: Subscribes to general PnL updates (across accounts/models).~~
5.  ~~`cancelPnL`: Cancels the general PnL subscription.~~
6.  ~~`reqPnLSingle`: Subscribes to PnL updates for a single position.~~
7.  ~~`cancelPnLSingle`: Cancels the single position PnL subscription.~~
8.  ~~`reqHistoricalSchedule`: Requests historical trading session schedules for a contract.~~
9.  ~~`reqHistoricalTicks`: Requests historical tick data for a contract.~~
10. `reqHeadTimeStamp`: Requests the earliest available historical data timestamp for a contract.
11. ~~`reqHistogramData`: Requests histogram data for a contract.~~
12. `calculateImpliedVolatility`: Requests TWS to calculate implied volatility for an option.
13. `calculateOptionPrice`: Requests TWS to calculate the option price based on volatility.
14. ~~`exerciseOptions`: Requests TWS to exercise or lapse an option.~~
15. ~~`reqGlobalCancel`: Requests TWS to cancel all active orders across all clients.~~
16. `requestFA`: Requests Financial Advisor configuration (Groups, Profiles, Aliases).
17. `replaceFA`: Replaces Financial Advisor configuration.
18. `reqUserInfo`: Requests the user's white branding ID.
19. ~~`reqScannerData`: Performs a blocking market scanner request.~~
20. ~~`reqScannerSubscription`: Subscribes to streaming market scanner data.~~
21. ~~`cancelScannerSubscription`: Cancels a market scanner subscription.~~
22. ~~`whatIfOrder`: Requests TWS to calculate margin and commission impact for a potential order without placing it.~~
