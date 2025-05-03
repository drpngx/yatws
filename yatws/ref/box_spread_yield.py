#!/usr/bin/env python3
# -*- coding: utf-8 -*-

# --- Derived from historical_data.py ---

import argparse
import datetime
import logging
import sys
import time
import threading
import math
from collections import defaultdict

from ibapi import wrapper
from ibapi.client import EClient, ScannerSubscription
from ibapi.contract import Contract, ComboLeg, ContractDetails
from ibapi.utils import iswrapper
from ibapi.comm import * # For TagValueList

# --- Constants ---
DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 4002
DEFAULT_CLIENT_ID = 103 # Use a different ID than other tests

# --- Logging Setup ---
log = logging.getLogger(__name__)
log.setLevel(logging.INFO) # Keep INFO level for cleaner output by default
handler = logging.StreamHandler(sys.stdout)
formatter = logging.Formatter('%(asctime)s - %(name)s - %(levelname)s - %(message)s')
handler.setFormatter(formatter)
log.addHandler(handler)

# --- Test Application Class ---
class TestApp(wrapper.EWrapper, EClient):
    def __init__(self):
        wrapper.EWrapper.__init__(self)
        EClient.__init__(self, wrapper=self)
        self.started = False
        self.nextValidOrderId = -1 # Not used for orders, but needed for reqIds
        self.nextReqId = 0 # Track request IDs

        # State management
        self._contract_details = {} # reqId -> list[ContractDetails]
        self._contract_details_finished = {} # reqId -> threading.Event
        self._market_data = {} # reqId -> {'bid': None, 'ask': None, 'contract': None, 'strike_diff': 0.0, 'expiry': None}
        self._market_data_finished = {} # reqId -> threading.Event
        self._option_params = {} # reqId -> {'exchange': '', 'expirations': set(), 'strikes': set()}
        self._option_params_finished = {} # reqId -> threading.Event
        self._errors = defaultdict(list) # reqId -> list[(errorCode, errorString)]
        self._reqid_to_description = {} # reqId -> str (for logging)

    def next_req_id(self):
        """Generates the next request ID."""
        req_id = self.nextReqId
        self.nextReqId += 1
        return req_id

    @iswrapper
    def connectAck(self):
        log.info("Connection acknowledged.")

    @iswrapper
    def nextValidId(self, orderId: int):
        super().nextValidId(orderId)
        self.nextReqId = orderId # Start reqIds from the first valid orderId
        log.info("nextValidId: %d. Starting requests...", orderId)
        self.start()

    def start(self):
        if self.started:
            return
        self.started = True
        log.info("Executing requests")
        self.test_box_spread_yield()
        log.info("Requests finished scheduling. Waiting for responses...")
        # Waiting logic is handled in main()

    def keyboardInterrupt(self):
        log.info("Keyboard interrupt detected. Disconnecting...")
        self.disconnect()
        sys.exit(0) # Exit cleanly

    @iswrapper
    def error(self, reqId: int, errorCode: int, errorString: str, advancedOrderReject=""):
        super().error(reqId, errorCode, errorString, advancedOrderReject)
        msg = f"Error. Id: {reqId}, Code: {errorCode}, Msg: {errorString}"
        if advancedOrderReject:
            msg += f", AdvancedOrderReject: {advancedOrderReject}"
        log.error(msg)
        self._errors[reqId].append((errorCode, errorString))

        # If error occurs for a specific request, signal its completion
        if reqId in self._contract_details_finished:
            self._contract_details_finished[reqId].set()
        if reqId in self._market_data_finished:
            self._market_data_finished[reqId].set()
        if reqId in self._option_params_finished: # Also signal option param errors
            self._option_params_finished[reqId].set()

    # --- Contract Details Callbacks ---
    @iswrapper
    def contractDetails(self, reqId: int, contractDetails):
        super().contractDetails(reqId, contractDetails)
        log.debug(f"ContractDetails. ReqId: {reqId}, Symbol: {contractDetails.contract.symbol}, ConId: {contractDetails.contract.conId}")
        if reqId not in self._contract_details:
            self._contract_details[reqId] = []
        self._contract_details[reqId].append(contractDetails)

    @iswrapper
    def contractDetailsEnd(self, reqId: int):
        super().contractDetailsEnd(reqId)
        log.debug(f"ContractDetailsEnd. ReqId: {reqId}")
        if reqId in self._contract_details_finished:
            self._contract_details_finished[reqId].set()
        elif reqId in self._option_params_finished:
            # This might be the end signal for option params if no params were found
            log.warning(f"Received contractDetailsEnd for option param reqId {reqId}, but expected securityDefinitionOptionParameterEnd.")
            self._option_params_finished[reqId].set() # Signal anyway
        else:
            log.warning(f"Received contractDetailsEnd for unknown reqId: {reqId}")

    # --- Market Data Callbacks (for Combo) ---
    @iswrapper
    def tickPrice(self, reqId: int, tickType: int, price: float, attrib):
        super().tickPrice(reqId, tickType, price, attrib)
        log.debug(f"TickPrice. ReqId: {reqId}, TickType: {tickType}, Price: {price}")
        if reqId in self._market_data:
            if tickType == 1: # Bid
                self._market_data[reqId]['bid'] = price
            elif tickType == 2: # Ask
                self._market_data[reqId]['ask'] = price
            # Check if we have both bid and ask to potentially finish early
            if self._market_data[reqId]['bid'] is not None and self._market_data[reqId]['ask'] is not None:
                 if reqId in self._market_data_finished:
                     log.debug(f"Bid and Ask received for ReqId {reqId}, signaling completion.")
                     self._market_data_finished[reqId].set()

    @iswrapper
    def tickSize(self, reqId: int, tickType: int, size: float):
        # We don't strictly need size for yield, but log for debug
        super().tickSize(reqId, tickType, size)
        log.debug(f"TickSize. ReqId: {reqId}, TickType: {tickType}, Size: {size}")

    @iswrapper
    def tickSnapshotEnd(self, reqId: int):
        super().tickSnapshotEnd(reqId)
        log.debug(f"TickSnapshotEnd. ReqId: {reqId}")
        if reqId in self._market_data_finished:
            self._market_data_finished[reqId].set() # Signal completion

    # --- Option Chain Parameter Callbacks ---
    @iswrapper
    def securityDefinitionOptionParameter(self, reqId: int, exchange: str, underlyingConId: int, tradingClass: str, multiplier: str, expirations: set, strikes: set):
        super().securityDefinitionOptionParameter(reqId, exchange, underlyingConId, tradingClass, multiplier, expirations, strikes)
        log.debug(f"SecurityDefinitionOptionParameter. ReqId: {reqId}, Exchange: {exchange}, UnderlyingConId: {underlyingConId}, TradingClass: {tradingClass}, Multiplier: {multiplier}, Expirations: {len(expirations)}, Strikes: {len(strikes)}")
        if reqId in self._option_params:
            # Aggregate results (multiple exchanges might respond)
            self._option_params[reqId]['expirations'].update(expirations)
            self._option_params[reqId]['strikes'].update(strikes)
            # Store multiplier per trading class if needed, for now just store the first one?
            if 'multiplier' not in self._option_params[reqId] or not self._option_params[reqId]['multiplier']:
                 self._option_params[reqId]['multiplier'] = multiplier
            if 'tradingClass' not in self._option_params[reqId] or not self._option_params[reqId]['tradingClass']:
                 self._option_params[reqId]['tradingClass'] = tradingClass
            # Store the primary exchange if it matches the request
            if exchange == self._option_params[reqId]['requested_exchange']:
                 self._option_params[reqId]['primary_exchange_found'] = True


    @iswrapper
    def securityDefinitionOptionParameterEnd(self, reqId: int):
        super().securityDefinitionOptionParameterEnd(reqId)
        log.debug(f"SecurityDefinitionOptionParameterEnd. ReqId: {reqId}")
        if reqId in self._option_params_finished:
            self._option_params_finished[reqId].set()
        else:
            log.warning(f"Received securityDefinitionOptionParameterEnd for unknown reqId: {reqId}")

    # --- Test Logic ---
    def get_contract_details(self, contract: Contract) -> list:
        """Synchronously fetches contract details."""
        req_id = self.next_req_id()
        desc = f"ContractDetails for {contract.symbol} {contract.secType}"
        log.info(f"Requesting {desc} (ReqId: {req_id})")
        self._reqid_to_description[req_id] = desc
        self._contract_details[req_id] = [] # Initialize empty list
        self._contract_details_finished[req_id] = threading.Event()
        self.reqContractDetails(req_id, contract)
        # Wait for completion or timeout
        timeout = 15 # seconds
        finished = self._contract_details_finished[req_id].wait(timeout=timeout)
        if not finished:
            log.error(f"Timeout waiting for {desc} (ReqId: {req_id})")
            # Clean up
            del self._contract_details_finished[req_id]
            if req_id in self._contract_details: del self._contract_details[req_id]
            return [] # Return empty list on timeout/error
        # Check for errors during the request
        if req_id in self._errors and self._errors[req_id]:
             log.error(f"Errors occurred fetching {desc} (ReqId: {req_id}): {self._errors[req_id]}")
             # Clean up
             del self._contract_details_finished[req_id]
             if req_id in self._contract_details: del self._contract_details[req_id]
             return []
        result = self._contract_details.pop(req_id, [])
        del self._contract_details_finished[req_id]
        log.info(f"Finished {desc} (ReqId: {req_id}), found {len(result)} details.")
        return result

    def get_option_chain_params(self, underlying_contract: Contract) -> dict:
        """Synchronously fetches option chain parameters (expirations, strikes)."""
        req_id = self.next_req_id()
        desc = f"OptionParams for {underlying_contract.symbol} ({underlying_contract.secType})"
        log.info(f"Requesting {desc} (ReqId: {req_id})")
        self._reqid_to_description[req_id] = desc
        # Initialize state, store requested exchange for filtering results
        self._option_params[req_id] = {'requested_exchange': underlying_contract.exchange, 'expirations': set(), 'strikes': set(), 'multiplier': None, 'tradingClass': None, 'primary_exchange_found': False}
        self._option_params_finished[req_id] = threading.Event()

        # Determine parameters for reqSecDefOptParams based on underlying type
        underlyingSymbol = underlying_contract.symbol
        futFopExchange = "" # Default for STK
        underlyingSecType = underlying_contract.secType
        underlyingConId = underlying_contract.conId

        if underlyingSecType == "FUT":
            futFopExchange = underlying_contract.exchange # Use future's exchange for futFopExchange

        self.reqSecDefOptParams(req_id, underlyingSymbol, futFopExchange, underlyingSecType, underlyingConId)

        # Wait for completion or timeout
        timeout = 20 # seconds
        finished = self._option_params_finished[req_id].wait(timeout=timeout)

        if not finished:
            log.error(f"Timeout waiting for {desc} (ReqId: {req_id})")
            del self._option_params_finished[req_id]
            if req_id in self._option_params: del self._option_params[req_id]
            return {}

        # Check for errors
        if req_id in self._errors and self._errors[req_id]:
             log.error(f"Errors occurred fetching {desc} (ReqId: {req_id}): {self._errors[req_id]}")
             del self._option_params_finished[req_id]
             if req_id in self._option_params: del self._option_params[req_id]
             return {}

        result = self._option_params.pop(req_id, {})
        del self._option_params_finished[req_id]

        # Convert sets to sorted lists for easier use
        result['expirations'] = sorted(list(result.get('expirations', set())))
        result['strikes'] = sorted(list(result.get('strikes', set())))

        log.info(f"Finished {desc} (ReqId: {req_id}), found {len(result.get('expirations',[]))} expirations, {len(result.get('strikes',[]))} strikes.")
        return result

    def get_combo_quote(self, combo_contract: Contract, strike_diff: float, expiry: datetime.date) -> dict:
        """Synchronously fetches a snapshot quote for a combo contract."""
        req_id = self.next_req_id()
        desc = f"Quote for {combo_contract.symbol} BOX {expiry.strftime('%Y%m%d')} (StrikeDiff: {strike_diff})"
        log.info(f"Requesting {desc} (ReqId: {req_id})")
        self._reqid_to_description[req_id] = desc
        self._market_data[req_id] = {'bid': None, 'ask': None, 'contract': combo_contract, 'strike_diff': strike_diff, 'expiry': expiry}
        self._market_data_finished[req_id] = threading.Event()
        # Request snapshot market data
        self.reqMktData(req_id, combo_contract, "", True, False, [])
        # Wait for completion (tickSnapshotEnd or both bid/ask received) or timeout
        timeout = 20 # seconds
        finished = self._market_data_finished[req_id].wait(timeout=timeout)
        if not finished:
            log.error(f"Timeout waiting for {desc} (ReqId: {req_id})")
            # Clean up
            del self._market_data_finished[req_id]
            result = self._market_data.pop(req_id, None) # Get data even on timeout
            return result or {} # Return potentially partial data or empty dict
        # Check for errors
        if req_id in self._errors and self._errors[req_id]:
             log.error(f"Errors occurred fetching {desc} (ReqId: {req_id}): {self._errors[req_id]}")
             # Clean up
             del self._market_data_finished[req_id]
             result = self._market_data.pop(req_id, None)
             return result or {}
        result = self._market_data.pop(req_id, {})
        del self._market_data_finished[req_id]
        log.info(f"Finished {desc} (ReqId: {req_id}), Bid: {result.get('bid')}, Ask: {result.get('ask')}")
        return result
    def calculate_yield(self, mid_price: float, strike_diff: float, expiry_date: datetime.date) -> float | None:
        """Calculates the annualized yield from the box spread price."""
        if mid_price is None or mid_price <= 0 or strike_diff <= 0:
            return None
        today = datetime.date.today()
        if expiry_date <= today:
            return None # Expired or today
        days_to_expiry = (expiry_date - today).days
        time_to_expiry_years = days_to_expiry / 365.0
        # Formula: r = -ln(Mid / StrikeDiff) / T
        try:
            # Mid price should be close to strike_diff but slightly less due to discount
            if mid_price >= strike_diff:
                 log.warning(f"Mid price ({mid_price}) >= strike difference ({strike_diff}). Cannot calculate yield.")
                 return None
            ratio = mid_price / strike_diff
            if ratio <= 0: # Avoid log(0) or log(negative)
                 log.warning(f"Price/StrikeDiff ratio ({ratio}) is non-positive. Cannot calculate yield.")
                 return None
            annual_yield = -math.log(ratio) / time_to_expiry_years
            return annual_yield * 100 # Return as percentage
        except ValueError as e:
            log.error(f"Math error calculating yield (Mid={mid_price}, Diff={strike_diff}, T={time_to_expiry_years}): {e}")
            return None
        except ZeroDivisionError:
             log.error(f"Zero division error calculating yield (T={time_to_expiry_years})")
             return None

    def test_box_spread_yield(self):
        """Main test function."""
        # Define underlyings and parameters
        underlyings = [
            {"symbol": "ES", "exchange": "CME", "currency": "USD", "secType": "FUT", "expiry": "202509"}, # Example E-mini S&P (Adjust expiry as needed)
            # {"symbol": "RTY", "exchange": "CME", "currency": "USD", "secType": "FUT", "expiry": "202509"}, # Example E-mini Russell
        ]
        # Define strike differences and expiry offsets to test
        strike_diffs = [50, 100] # Adjust for futures scale (e.g., 5000/5050 box)
        expiry_offsets_days = [30, 60, 90] # Approx days from today

        today = datetime.date.today()
        target_expiries = [today + datetime.timedelta(days=d) for d in expiry_offsets_days]

        results = []

        for und_info in underlyings:
            symbol = und_info['symbol']
            log.info(f"--- Testing Underlying: {symbol} ---")

            # 1. Define Underlying Contract (Future)
            underlying_contract = Contract()
            underlying_contract.symbol = und_info["symbol"]
            underlying_contract.secType = und_info["secType"] # FUT
            underlying_contract.exchange = und_info["exchange"]
            underlying_contract.currency = und_info["currency"]
            underlying_contract.lastTradeDateOrContractMonth = und_info["expiry"] # Specify future expiry YYYYMM

            # Get underlying details to find its conId (needed for option params request)
            underlying_details_list = self.get_contract_details(underlying_contract)
            if not underlying_details_list:
                log.error(f"Could not get contract details for underlying {symbol} {und_info['expiry']}. Skipping.")
                continue
            underlying_contract.conId = underlying_details_list[0].contract.conId # Store conId
            log.info(f"Underlying {symbol} ConId: {underlying_contract.conId}")

            # 2. Get Option Chain Parameters using reqSecDefOptParams
            # Pass the contract with the conId now set
            option_params = self.get_option_chain_params(underlying_contract)
            if not option_params or not option_params.get('expirations') or not option_params.get('strikes'):
                 log.error(f"Could not get option chain parameters for {symbol}. Skipping.")
                 continue

            # Extract available expirations and strikes from the result
            available_expiries = option_params['expirations'] # YYYYMMDD strings
            available_strikes = option_params['strikes'] # float values
            option_multiplier = option_params.get('multiplier') # Get fetched multiplier
            option_trading_class = option_params.get('tradingClass') # Get fetched trading class

            if not option_multiplier:
                log.warning(f"Multiplier not found for {symbol}, defaulting to 50.")
                option_multiplier = '50'
            if not option_trading_class:
                 log.warning(f"TradingClass not found for {symbol}, using symbol as fallback.")
                 option_trading_class = symbol

            log.info(f"Found {len(available_expiries)} expirations, {len(available_strikes)} strikes. Multiplier: {option_multiplier}, TradingClass: {option_trading_class}")

            # Find a reasonable ATM strike based on a guess (replace with actual quote later if needed)
            # Use a placeholder price for now, ideally fetch underlying quote
            placeholder_underlying_price = 5000.0 if symbol == "ES" else 2000.0
            if not available_strikes:
                log.error(f"No strikes found for {symbol}. Skipping.")
                continue
            atm_strike_guess = min(available_strikes, key=lambda x:abs(x-placeholder_underlying_price))

            # 3. Iterate through target expiries and strike differences
            for target_expiry in target_expiries:
                # Find nearest available expiry >= target
                target_expiry_str = target_expiry.strftime("%Y%m%d")
                valid_expiry_str = min((e for e in available_expiries if e >= target_expiry_str), default=None)
                if not valid_expiry_str:
                    log.warning(f"No expiry found >= {target_expiry_str} for {symbol}. Skipping.")
                    continue
                expiry_date = datetime.datetime.strptime(valid_expiry_str, "%Y%m%d").date()

                for strike_diff in strike_diffs:
                    # Define target strikes around the ATM guess
                    target_strike1 = atm_strike_guess - strike_diff / 2.0
                    target_strike2 = atm_strike_guess + strike_diff / 2.0

                    # Find nearest available strikes
                    strike1 = min(available_strikes, key=lambda x:abs(x-target_strike1))
                    strike2 = min(available_strikes, key=lambda x:abs(x-target_strike2))

                    # Ensure strikes are distinct and ordered after snapping
                    if abs(strike1 - strike2) < 0.01: # Check if they snapped to the same strike
                        log.warning(f"Strikes {target_strike1:.1f}/{target_strike2:.1f} snapped to the same value {strike1:.1f} for {symbol}. Skipping.")
                        continue
                    if strike1 > strike2: # Ensure order
                        strike1, strike2 = strike2, strike1 # Swap if needed

                    actual_strike_diff = strike2 - strike1
                    log.info(f"Testing {symbol} Box: Expiry={valid_expiry_str}, Strikes={strike1:.1f}/{strike2:.1f} (Diff={actual_strike_diff:.2f})")

                    # 4. Define the 4 option legs contracts
                    legs_contracts = []
                    leg_params = [
                        (strike1, "CALL", "BUY", 1), (strike2, "CALL", "SELL", 1), # Bull Call Spread
                        (strike2, "PUT", "BUY", 1), (strike1, "PUT", "SELL", 1)   # Bear Put Spread
                    ]
                    all_legs_found = True
                    for strike, right, action, ratio in leg_params:
                        opt_contract = Contract()
                        opt_contract.symbol = symbol
                        opt_contract.secType = "FOP" # Futures Option
                        opt_contract.currency = und_info["currency"]
                        opt_contract.exchange = und_info["exchange"] # Use future's exchange for options
                        opt_contract.lastTradeDateOrContractMonth = valid_expiry_str
                        opt_contract.strike = strike
                        opt_contract.right = right
                        opt_contract.multiplier = "100" # Assume standard multiplier

                        # Get details to find conId (and confirm multiplier/class)
                        opt_details_list = self.get_contract_details(opt_contract)
                        if not opt_details_list:
                            log.error(f"Failed to get details for leg: {symbol} {valid_expiry_str} {strike:.1f} {right}. Skipping box.")
                            all_legs_found = False
                            break
                        # Use details from the first result
                        opt_details = opt_details_list[0]
                        legs_contracts.append({
                            "conId": opt_details[0].contract.conId,
                            "ratio": ratio,
                            "action": action,
                            "exchange": opt_details[0].contract.exchange # Use resolved exchange
                        })

                    if not all_legs_found:
                        continue

                    # 5. Define the Combo (BAG) contract
                    combo_contract = Contract() # BAG contract for the strategy
                    combo_contract.symbol = symbol # Use underlying symbol for BAG targeting FOPs? Or currency? Let's try symbol.
                    combo_contract.secType = "BAG"
                    combo_contract.currency = und_info["currency"] # USD
                    combo_contract.exchange = und_info["exchange"] # Route BAG via underlying's exchange
                    combo_contract.comboLegs = []

                    for leg_info in legs_contracts:
                        leg = ComboLeg()
                        leg.conId = leg_info["conId"]
                        leg.ratio = leg_info["ratio"]
                        leg.action = leg_info["action"]
                        leg.exchange = leg_info["exchange"]
                        # leg.openClose = 0 # Same
                        # leg.shortSaleSlot = 0
                        # leg.designatedLocation = ""
                        # leg.exemptCode = -1
                        combo_contract.comboLegs.append(leg)

                    # 6. Get the quote for the combo contract
                    quote_data = self.get_combo_quote(combo_contract, actual_strike_diff, expiry_date)

                    # 7. Calculate yield
                    mid_price = None
                    if quote_data.get('bid') is not None and quote_data.get('ask') is not None:
                        mid_price = (quote_data['bid'] + quote_data['ask']) / 2.0
                        log.info(f"  Mid Price: {mid_price:.4f} (Bid: {quote_data['bid']:.4f}, Ask: {quote_data['ask']:.4f})")
                    else:
                        log.warning(f"  Could not get valid Bid/Ask for {symbol} box {valid_expiry_str} {strike1:.1f}/{strike2:.1f}.")

                    yield_pct = self.calculate_yield(mid_price, actual_strike_diff, expiry_date)

                    if yield_pct is not None:
                        log.info(f"  => Calculated Annual Yield: {yield_pct:.4f}%")
                        results.append({
                            "symbol": symbol,
                            "expiry": valid_expiry_str,
                            "strike1": strike1,
                            "strike2": strike2,
                            "strike_diff": actual_strike_diff,
                            "mid_price": mid_price,
                            "yield_pct": yield_pct
                        })
                    else:
                         log.warning(f"  => Failed to calculate yield.")

                    time.sleep(1) # Pace requests slightly

        log.info("--- Box Spread Yield Test Summary ---")
        if not results:
            log.warning("No successful yield calculations.")
        else:
            # Sort results for consistent output
            results.sort(key=lambda r: (r["symbol"], r["expiry"], r["strike1"]))
            log.info(f"{'Symbol':<6} {'Expiry':<10} {'Strikes':<12} {'Mid Price':<10} {'Yield (%)':<10}")
            log.info("-" * 50)
            for r in results:
                log.info(f"{r['symbol']:<6} {r['expiry']:<10} {f'{r['strike1']:.0f}/{r['strike2']:.0f}':<12} {r['mid_price']:.4f}{'':<10-len(f'{r['mid_price']:.4f}')} {r['yield_pct']:.4f}")
        log.info("-" * 50)


# --- Main Execution ---
def main():
    parser = argparse.ArgumentParser(description="IB API Box Spread Yield Test")
    parser.add_argument("--host", default=DEFAULT_HOST, help="Host address")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help="Port number")
    parser.add_argument("--clientId", type=int, default=DEFAULT_CLIENT_ID, help="Client ID")
    parser.add_argument("--verbose", "-v", action="store_true", help="Enable DEBUG logging")
    args = parser.parse_args()

    if args.verbose:
        log.setLevel(logging.DEBUG)

    log.info("Starting Box Spread Yield Test")
    log.info(f"Connecting to {args.host}:{args.port} with clientId {args.clientId}")
    log.info(f"Logger level set to: {logging.getLevelName(log.level)}")

    try:
        app = TestApp()
        app.connect(args.host, args.port, args.clientId)
        log.info("Connection initiated. Server version: %s", app.serverVersion())

        log.info("Starting EClient.run() message loop in background thread...")
        thread = threading.Thread(target=app.run, daemon=True)
        thread.start()
        log.info("EClient.run() thread started.")

        # Wait for the test logic (which runs synchronously within start()) to indicate completion
        # Since the test logic now blocks internally, we just need to wait for the thread to finish naturally after disconnect
        # Add a maximum overall timeout
        max_wait_time = 180 # seconds for the whole test
        thread.join(timeout=max_wait_time)

        if thread.is_alive():
             log.warning(f"EClient thread still alive after {max_wait_time}s. Forcing disconnect.")
             app.disconnect()
             thread.join(timeout=10) # Short extra wait after forced disconnect

        log.info("Box Spread Yield Test finished.")

    except Exception as e:
        log.exception("Unhandled exception in main:")
    finally:
        # Ensure disconnect happens even on error
        if 'app' in locals() and app.isConnected():
            log.info("Ensuring disconnection...")
            app.disconnect()
        log.info("Exiting.")


if __name__ == "__main__":
    main()
