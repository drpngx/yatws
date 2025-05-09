#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import argparse
import logging
import sys
import threading
import time

from ibapi import wrapper
from ibapi.client import EClient
from ibapi.contract import Contract
from ibapi.utils import iswrapper
#from ibapi.common import TagValue # For option calculation options
from ibapi.ticktype import TickTypeEnum

# --- Constants ---
DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 4002 # Default TWS port, use 7497 for IB Gateway paper, 4001 for TWS paper
DEFAULT_CLIENT_ID = 103 # Choose a unique client ID

# --- Logging Setup ---
log = logging.getLogger(__name__)
log.setLevel(logging.INFO) # DEBUG for more verbose output
handler = logging.StreamHandler(sys.stdout)
formatter = logging.Formatter('%(asctime)s - %(name)s - %(levelname)s - %(message)s')
handler.setFormatter(formatter)
log.addHandler(handler)


# --- Test Application Class ---
class OptionPriceCalcApp(wrapper.EWrapper, EClient):
    def __init__(self):
        wrapper.EWrapper.__init__(self)
        EClient.__init__(self, wrapper=self)
        self.nKeybInt = 0
        self.started = False
        self.nextValidOrderId = -1 # Will be used for reqIds
        self.done = False # Flag to signal completion or error

        # For option calculation data
        self.impl_vol_req_id = -1
        self.impl_vol_data = None
        self.impl_vol_error = None
        self.impl_vol_finished = threading.Event()

        self.opt_price_req_id = -1
        self.opt_price_data = None
        self.opt_price_error = None
        self.opt_price_finished = threading.Event()

        self._my_errors = {}


    @iswrapper
    def connectAck(self):
        log.info("Connection acknowledged.")

    @iswrapper
    def nextValidId(self, orderId: int):
        super().nextValidId(orderId)
        self.nextValidOrderId = orderId
        log.info("nextValidId: %d", orderId)
        # Start requests after receiving nextValidId
        self.start()


    def start(self):
        if self.started:
            return
        self.started = True
        log.info("Executing requests for option calculations")
        self.request_calculate_implied_volatility()
        # Small delay to ensure reqIds are distinct if nextValidOrderId isn't incremented fast enough by server
        time.sleep(0.1)
        self.request_calculate_option_price()
        log.info("Option calculation requests finished sending.")

    def keyboardInterrupt(self):
        self.nKeybInt += 1
        if self.nKeybInt == 1:
            log.info("Keyboard interrupt detected. Disconnecting...")
            self.done = True
            self.impl_vol_finished.set() # Unblock main thread
            self.opt_price_finished.set()  # Unblock main thread
            self.disconnect()
        else:
            log.info("Forcing exit...")
            sys.exit(0)

    @iswrapper
    def error(self, reqId: int, errorCode: int, errorString: str, advancedOrderReject=""):
        super().error(reqId, errorCode, errorString, advancedOrderReject)
        if advancedOrderReject:
            log.error("Error. Id: %d, Code: %d, Msg: %s, AdvancedOrderReject: %s", reqId, errorCode, errorString, advancedOrderReject)
        else:
            log.error("Error. Id: %d, Code: %d, Msg: %s", reqId, errorCode, errorString)

        # Store errors keyed by reqId
        self._my_errors[reqId] = (errorCode, errorString)

        # Check if the error pertains to one of our requests
        if reqId == self.impl_vol_req_id:
            log.error(f"Implied Volatility request {reqId} failed with error {errorCode}: {errorString}")
            self.impl_vol_error = (errorCode, errorString)
            self.impl_vol_finished.set()
        elif reqId == self.opt_price_req_id:
            log.error(f"Option Price request {reqId} failed with error {errorCode}: {errorString}")
            self.opt_price_error = (errorCode, errorString)
            self.opt_price_finished.set()


    # --- Option Calculation Callback ---
    @iswrapper
    def tickOptionComputation(self, reqId: int, tickType: TickTypeEnum, tickAttrib: int,
                              impliedVol: float, delta: float, optPrice: float, pvDividend: float,
                              gamma: float, vega: float, theta: float, undPrice: float):
        super().tickOptionComputation(reqId, tickType, tickAttrib, impliedVol, delta, optPrice, pvDividend,
                                     gamma, vega, theta, undPrice)
        log.info(f"CALLBACK tickOptionComputation: ReqId: {reqId}, TickType: {TickTypeEnum.idx2name[tickType]}({tickType}), "
                 f"ImpliedVol: {impliedVol if impliedVol != -1 else 'N/A'}, Delta: {delta if delta != -2 else 'N/A'}, "
                 f"OptPrice: {optPrice if optPrice != -1 else 'N/A'}, PvDividend: {pvDividend if pvDividend != -1 else 'N/A'}, "
                 f"Gamma: {gamma if gamma != -2 else 'N/A'}, Vega: {vega if vega != -2 else 'N/A'}, "
                 f"Theta: {theta if theta != -2 else 'N/A'}, UndPrice: {undPrice if undPrice != -1 else 'N/A'}")

        data = {
            'tickType': TickTypeEnum.idx2name[tickType],
            'tickAttrib': tickAttrib,
            'impliedVol': impliedVol,
            'delta': delta,
            'optPrice': optPrice,
            'pvDividend': pvDividend,
            'gamma': gamma,
            'vega': vega,
            'theta': theta,
            'undPrice': undPrice
        }

        if reqId == self.impl_vol_req_id:
            self.impl_vol_data = data
            self.impl_vol_finished.set()
            log.info(f"Implied Volatility data received for reqId {reqId}.")
        elif reqId == self.opt_price_req_id:
            self.opt_price_data = data
            self.opt_price_finished.set()
            log.info(f"Option Price data received for reqId {reqId}.")
        else:
            log.warning(f"tickOptionComputation received for unknown reqId: {reqId}")

    # --- Option Calculation Requests ---
    # 54·3·1·0·AAPL·OPT·20250620·200.0·C··SMART··USD···2.5·170.0·0··
    def request_calculate_implied_volatility(self):
        if self.nextValidOrderId == -1:
            log.error("nextValidId has not been received yet. Cannot send request.")
            self.impl_vol_error = (-1, "nextValidId not received")
            self.impl_vol_finished.set()
            return

        self.impl_vol_req_id = self.nextValidOrderId
        self.nextValidOrderId += 1 # Increment for the next request
        log.info(f"Requesting Implied Volatility with reqId: {self.impl_vol_req_id}")

        # Define a sample contract (e.g., AAPL Call Option)
        contract = Contract()
        contract.symbol = "AAPL"
        contract.secType = "OPT"
        contract.exchange = "SMART" # Or a specific exchange like "CBOE"
        contract.currency = "USD"
        contract.lastTradeDateOrContractMonth = "20250620" # YYYYMMDD format
        contract.strike = 200.0
        contract.right = "C" # Call
        # contract.multiplier = "100" # Optional, usually defaults to 100 for US options

        # Placeholder values for calculation
        option_price = 2.50  # Market price of the option
        under_price = 170.00 # Current price of the underlying stock

        log.info(f"Calculating Implied Volatility for: {contract.symbol} {contract.lastTradeDateOrContractMonth} "
                 f"{contract.strike}{contract.right} at OptionPrice={option_price}, UnderPrice={under_price}")

        # API call: calculateImpliedVolatility(reqId, contract, optionPrice, underPrice, impliedVolatilityOptions)
        # impliedVolatilityOptions is a list of TagValue, pass empty list for defaults
        self.calculateImpliedVolatility(self.impl_vol_req_id, contract, option_price, under_price, [])


    # 55·3·2·0·AAPL·OPT·20250620·200.0·C··SMART··USD···0.3·170.0·0··
    def request_calculate_option_price(self):
        if self.nextValidOrderId == -1:
            log.error("nextValidId has not been received yet. Cannot send request.")
            self.opt_price_error = (-1, "nextValidId not received")
            self.opt_price_finished.set()
            return

        self.opt_price_req_id = self.nextValidOrderId
        self.nextValidOrderId += 1
        log.info(f"Requesting Option Price with reqId: {self.opt_price_req_id}")

        # Define the same sample contract
        contract = Contract()
        contract.symbol = "AAPL"
        contract.secType = "OPT"
        contract.exchange = "SMART"
        contract.currency = "USD"
        contract.lastTradeDateOrContractMonth = "20250620"
        contract.strike = 200.0
        contract.right = "C"

        # Placeholder values for calculation
        volatility = 0.30  # Implied volatility of the option (e.g., 30%)
        under_price = 170.00 # Current price of the underlying stock

        log.info(f"Calculating Option Price for: {contract.symbol} {contract.lastTradeDateOrContractMonth} "
                 f"{contract.strike}{contract.right} at Volatility={volatility}, UnderPrice={under_price}")

        # API call: calculateOptionPrice(reqId, contract, volatility, underPrice, optionPriceOptions)
        # optionPriceOptions is a list of TagValue, pass empty list for defaults
        self.calculateOptionPrice(self.opt_price_req_id, contract, volatility, under_price, [])


# --- Main Execution ---
def main():
    parser = argparse.ArgumentParser(description="IB API Option Price Calculation Test")
    parser.add_argument("--host", default=DEFAULT_HOST, help="Host address")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help="Port number")
    parser.add_argument("--clientId", type=int, default=DEFAULT_CLIENT_ID, help="Client ID")
    args = parser.parse_args()

    log.info("Starting Option Price Calculation Test")
    log.info(f"Connecting to {args.host}:{args.port} with clientId {args.clientId}")

    app = OptionPriceCalcApp()
    try:
        log.info(f"Logger level set to: {logging.getLevelName(log.level)}")

        app.connect(args.host, args.port, args.clientId)
        log.info("Connection initiated. Server version: %s", app.serverVersion())

        # Start the EClient message loop in a separate thread
        log.info("Starting EClient.run() message loop in background thread...")
        thread = threading.Thread(target=app.run, daemon=True)
        thread.start()
        log.info("EClient.run() thread started.")

        # Wait for the calculation results
        wait_timeout_secs = 30  # Timeout for each calculation

        log.info(f"Main thread waiting up to {wait_timeout_secs}s for Implied Volatility result (reqId: {app.impl_vol_req_id})...")
        impl_vol_wait_successful = app.impl_vol_finished.wait(timeout=wait_timeout_secs)

        if impl_vol_wait_successful:
            if app.impl_vol_data:
                log.info(f"Main thread: Implied Volatility request {app.impl_vol_req_id} finished.")
                log.info("Implied Volatility Result:")
                for key, value in app.impl_vol_data.items():
                    log.info(f"  {key}: {value}")
            elif app.impl_vol_error:
                log.error(f"Main thread: Implied Volatility request {app.impl_vol_req_id} failed: {app.impl_vol_error}")
            else:
                log.warning(f"Main thread: Implied Volatility request {app.impl_vol_req_id} finished but no data or error captured.")
        else:
            log.warning(f"Main thread: Implied Volatility request {app.impl_vol_req_id} timed out!")
            if app.isConnected():
                log.info(f"Attempting to cancel Implied Volatility request {app.impl_vol_req_id} due to timeout.")
                app.cancelCalculateImpliedVolatility(app.impl_vol_req_id)


        log.info(f"Main thread waiting up to {wait_timeout_secs}s for Option Price result (reqId: {app.opt_price_req_id})...")
        opt_price_wait_successful = app.opt_price_finished.wait(timeout=wait_timeout_secs)

        if opt_price_wait_successful:
            if app.opt_price_data:
                log.info(f"Main thread: Option Price request {app.opt_price_req_id} finished.")
                log.info("Option Price Result:")
                for key, value in app.opt_price_data.items():
                    log.info(f"  {key}: {value}")
            elif app.opt_price_error:
                log.error(f"Main thread: Option Price request {app.opt_price_req_id} failed: {app.opt_price_error}")
            else:
                log.warning(f"Main thread: Option Price request {app.opt_price_req_id} finished but no data or error captured.")
        else:
            log.warning(f"Main thread: Option Price request {app.opt_price_req_id} timed out!")
            if app.isConnected():
                log.info(f"Attempting to cancel Option Price request {app.opt_price_req_id} due to timeout.")
                app.cancelCalculateOptionPrice(app.opt_price_req_id)


    except Exception as e:
        log.exception("Unhandled exception in main:")
    finally:
        # Disconnect and wait for thread exit
        log.info("Main thread: Disconnecting...")
        if app.isConnected():
            app.disconnect()
        else:
            log.info("Main thread: Already disconnected or connection failed.")

        if 'thread' in locals() and thread.is_alive():
            join_timeout_secs = 10
            log.info(f"Main thread waiting for EClient thread to join (timeout {join_timeout_secs}s)...")
            thread.join(timeout=join_timeout_secs)
            if thread.is_alive():
                log.warning("EClient thread did not exit cleanly after disconnect and join timeout.")
            else:
                log.info("EClient thread exited cleanly.")
        else:
            log.info("EClient thread was not started or already finished.")

        log.info("Option Price Calculation Test finished.")


if __name__ == "__main__":
    main()
