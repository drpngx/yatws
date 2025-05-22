#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# Query the price of a box spread

import argparse
import datetime
import logging
import sys
import time
import threading

from ibapi import wrapper
from ibapi.client import EClient
from ibapi.contract import Contract, ComboLeg
from ibapi.utils import iswrapper

# --- Constants ---
DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 4002
DEFAULT_CLIENT_ID = 103 # Use a different ID than historical data test

# --- Logging Setup ---
log = logging.getLogger(__name__)
log.setLevel(logging.DEBUG)
handler = logging.StreamHandler(sys.stdout)
formatter = logging.Formatter('%(asctime)s - %(name)s - %(levelname)s - %(message)s')
handler.setFormatter(formatter)
log.addHandler(handler)


# --- Test Application Class ---
class TestApp(wrapper.EWrapper, EClient):
    def __init__(self):
        wrapper.EWrapper.__init__(self)
        EClient.__init__(self, wrapper=self)
        self.nKeybInt = 0
        self.started = False
        self.nextValidOrderId = -1
        self.permId2ord = {}
        self.reqId2nErr = {}
        self.globalCancelOnly = False
        self._my_errors = {}

        # For market data
        self.mkt_data_req_id = -1
        self.quote_data = {}
        self.quote_received = threading.Event()
        self.quote_complete = False


    @iswrapper
    def connectAck(self):
        log.info("Connection acknowledged.")

    @iswrapper
    def nextValidId(self, orderId: int):
        super().nextValidId(orderId)
        self.nextValidOrderId = orderId
        log.info("nextValidId: %d", orderId)
        self.start() # Start the main logic after getting nextValidId

    def start(self):
        if self.started:
            return
        self.started = True
        log.info("Setting market data type to delayed")
        # Set market data type to delayed (3 = delayed, 4 = delayed frozen)
        self.reqMarketDataType(3)
        time.sleep(1)  # Give it a moment to process

        log.info("Executing requests")
        self.request_delayed_quote_test()
        log.info("Requests finished")

    def keyboardInterrupt(self):
        self.nKeybInt += 1
        if self.nKeybInt == 1:
            log.info("Keyboard interrupt detected. Disconnecting...")
            self.done = True
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

        # Errors related to market data
        if reqId == self.mkt_data_req_id:
             log.error(f"Market data request {reqId} failed with error {errorCode}: {errorString}")
             self.quote_received.set() # Signal completion on error

        # Store errors keyed by reqId
        if reqId > 0:
            self.reqId2nErr[reqId] = self.reqId2nErr.get(reqId, 0) + 1
            self._my_errors[reqId] = (errorCode, errorString)

    @iswrapper
    def marketDataType(self, reqId: int, marketDataType: int):
        super().marketDataType(reqId, marketDataType)
        log.info("Market data type set. ReqId: %d, Type: %d", reqId, marketDataType)

    # --- Market Data Callbacks ---
    @iswrapper
    def tickPrice(self, reqId: int, tickType: int, price: float, attrib):
        super().tickPrice(reqId, tickType, price, attrib)
        log.info("CALLBACK tickPrice: ReqId: %d, TickType: %d, Price: %f", reqId, tickType, price)
        if reqId == self.mkt_data_req_id:
            tick_name = self.get_tick_type_name(tickType)
            self.quote_data[f"price_{tick_name}"] = price
            self.quote_data[f"price_{tickType}"] = price
            # Signal we got some data
            if not self.quote_complete:
                self.quote_received.set()

    @iswrapper
    def tickSize(self, reqId: int, tickType: int, size: int):
        super().tickSize(reqId, tickType, size)
        log.info("CALLBACK tickSize: ReqId: %d, TickType: %d, Size: %d", reqId, tickType, size)
        if reqId == self.mkt_data_req_id:
            tick_name = self.get_tick_type_name(tickType)
            self.quote_data[f"size_{tick_name}"] = size
            self.quote_data[f"size_{tickType}"] = size

    @iswrapper
    def tickGeneric(self, reqId: int, tickType: int, value: float):
        super().tickGeneric(reqId, tickType, value)
        log.info("CALLBACK tickGeneric: ReqId: %d, TickType: %d, Value: %f", reqId, tickType, value)
        if reqId == self.mkt_data_req_id:
            tick_name = self.get_tick_type_name(tickType)
            self.quote_data[f"generic_{tick_name}"] = value
            self.quote_data[f"generic_{tickType}"] = value

    @iswrapper
    def tickString(self, reqId: int, tickType: int, value: str):
        super().tickString(reqId, tickType, value)
        log.info("CALLBACK tickString: ReqId: %d, TickType: %d, Value: %s", reqId, tickType, value)
        if reqId == self.mkt_data_req_id:
            tick_name = self.get_tick_type_name(tickType)
            self.quote_data[f"string_{tick_name}"] = value
            self.quote_data[f"string_{tickType}"] = value

    def get_tick_type_name(self, tick_type: int) -> str:
        """Convert tick type number to readable name"""
        tick_names = {
            0: "BID_SIZE",
            1: "BID",
            2: "ASK",
            3: "ASK_SIZE",
            4: "LAST",
            5: "LAST_SIZE",
            6: "HIGH",
            7: "LOW",
            8: "VOLUME",
            9: "CLOSE",
            10: "BID_OPTION_COMPUTATION",
            11: "ASK_OPTION_COMPUTATION",
            12: "LAST_OPTION_COMPUTATION",
            13: "MODEL_OPTION",
            14: "OPEN",
            15: "LOW_13_WEEK",
            16: "HIGH_13_WEEK",
            17: "LOW_26_WEEK",
            18: "HIGH_26_WEEK",
            19: "LOW_52_WEEK",
            20: "HIGH_52_WEEK",
            21: "AVG_VOLUME",
            # Add more as needed
        }
        return tick_names.get(tick_type, f"UNKNOWN_{tick_type}")

    # --- Test Logic ---
    # 1·11·1·0·ES·BAG··0.0···CME··USD···4·654505646·1·BUY·CME·654505738·1·SELL·CME·654505869·1·BUY·CME·654505848·1·SELL·CME·0··0·0··
    # 1·11·1·0·ES·BAG··0.0···CME··USD···4·654505646·1·BUY·CME·654505738·1·SELL·CME·654505869·1·BUY·CME·654505848·1·SELL·CME·0··1·0··
    def request_delayed_quote_test(self):
        self.mkt_data_req_id = self.nextValidOrderId
        self.nextValidOrderId += 1
        log.info(f"Requesting delayed quote with reqId: {self.mkt_data_req_id}")

        # Create the ES combo contract based on the Rust structure
        contract = Contract()
        contract.symbol = "ES"
        contract.secType = "BAG"  # Combo type in IBAPI Python
        contract.exchange = "CME"
        contract.currency = "USD"

        # Create combo legs based on the Rust ComboLeg structure
        # ComboLeg 1: con_id=654505646, ratio=1, action="BUY"
        leg1 = ComboLeg()
        leg1.conId = 654505646
        leg1.ratio = 1
        leg1.action = "BUY"
        leg1.exchange = "CME"
        leg1.openClose = 0  # Same
        leg1.shortSaleSlot = 0
        leg1.designatedLocation = ""
        leg1.exemptCode = -1

        # ComboLeg 2: con_id=654505738, ratio=1, action="SELL"
        leg2 = ComboLeg()
        leg2.conId = 654505738
        leg2.ratio = 1
        leg2.action = "SELL"
        leg2.exchange = "CME"
        leg2.openClose = 0  # Same
        leg2.shortSaleSlot = 0
        leg2.designatedLocation = ""
        leg2.exemptCode = -1

        # ComboLeg 3: con_id=654505869, ratio=1, action="BUY"
        leg3 = ComboLeg()
        leg3.conId = 654505869
        leg3.ratio = 1
        leg3.action = "BUY"
        leg3.exchange = "CME"
        leg3.openClose = 0  # Same
        leg3.shortSaleSlot = 0
        leg3.designatedLocation = ""
        leg3.exemptCode = -1

        # ComboLeg 4: con_id=654505848, ratio=1, action="SELL"
        leg4 = ComboLeg()
        leg4.conId = 654505848
        leg4.ratio = 1
        leg4.action = "SELL"
        leg4.exchange = "CME"
        leg4.openClose = 0  # Same
        leg4.shortSaleSlot = 0
        leg4.designatedLocation = ""
        leg4.exemptCode = -1

        # Add all legs to the contract
        contract.comboLegs = [leg1, leg2, leg3, leg4]

        log.info(f"Requesting Market Data for {contract.symbol} combo contract:")
        log.info(f"  SecType: {contract.secType}, Exchange: {contract.exchange}, Currency: {contract.currency}")
        log.info(f"  Number of combo legs: {len(contract.comboLegs)}")
        for i, leg in enumerate(contract.comboLegs):
            log.info(f"    Leg {i+1}: ConId={leg.conId}, Ratio={leg.ratio}, Action={leg.action}, Exchange={leg.exchange}")

        # Request market data (delayed quotes)
        genericTickList = ""  # Empty for basic quotes
        snapshot = False  # Streaming data, not snapshot
        regulatorySnapshot = False
        mktDataOptions = []

        self.reqMktData(self.mkt_data_req_id, contract, genericTickList,
                       snapshot, regulatorySnapshot, mktDataOptions)

        log.info(f"Market data request {self.mkt_data_req_id} sent. Background thread will process response.")


# --- Main Execution ---
def main():
    parser = argparse.ArgumentParser(description="IB API Delayed Quote Test for ES Combo")
    parser.add_argument("--host", default=DEFAULT_HOST, help="Host address")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help="Port number")
    parser.add_argument("--clientId", type=int, default=DEFAULT_CLIENT_ID, help="Client ID")
    args = parser.parse_args()

    log.info("Starting Delayed Quote Test for ES Combo Contract")
    log.info(f"Connecting to {args.host}:{args.port} with clientId {args.clientId}")

    try:
        app = TestApp()
        log.info(f"Logger level set to: {logging.getLevelName(log.level)}")

        app.connect(args.host, args.port, args.clientId)
        log.info("Connection initiated. Server version: %s", app.serverVersion())

        # Start the EClient message loop in a separate thread
        log.info("Starting EClient.run() message loop in background thread...")
        thread = threading.Thread(target=app.run, daemon=True)
        thread.start()
        log.info("EClient.run() thread started.")

        # Wait for quote data in the main thread
        wait_timeout_secs = 30  # Timeout for quote data
        log.info(f"Main thread waiting up to {wait_timeout_secs}s for quote data (reqId: {app.mkt_data_req_id})...")
        wait_successful = app.quote_received.wait(timeout=wait_timeout_secs)

        if wait_successful:
            log.info(f"Main thread: Quote data received for request {app.mkt_data_req_id}.")
            log.info(f"Quote data summary:")
            for key, value in app.quote_data.items():
                log.info(f"  {key}: {value}")

            # Let it run for a bit more to collect more data
            log.info("Collecting data for 10 more seconds...")
            time.sleep(10)
            app.quote_complete = True

            log.info("Final quote data:")
            for key, value in app.quote_data.items():
                log.info(f"  {key}: {value}")

        else:
            log.warning(f"Main thread: Quote request {app.mkt_data_req_id} timed out!")
            log.warning(f"  Received data: {app.quote_data}")

        # Cancel market data subscription
        log.info(f"Cancelling market data subscription {app.mkt_data_req_id}...")
        if app.isConnected():
            app.cancelMktData(app.mkt_data_req_id)
            time.sleep(1)  # Give it a moment to process cancellation

        # Disconnect and wait for thread exit
        log.info("Main thread: Disconnecting...")
        if app.isConnected():
            app.disconnect()
        else:
            log.info("Main thread: Already disconnected.")

        # Wait for the EClient thread to finish after disconnect
        join_timeout_secs = 10
        log.info(f"Main thread waiting for EClient thread to join (timeout {join_timeout_secs}s)...")
        thread.join(timeout=join_timeout_secs)
        log.info("Main thread finished waiting for EClient thread.")

        if thread.is_alive():
             log.warning("EClient thread did not exit cleanly after disconnect and join timeout.")
             if app.isConnected():
                 log.warning("Attempting disconnect again...")
                 app.disconnect()
        else:
             log.info("EClient thread exited cleanly.")

        log.info("Delayed Quote Test finished.")

    except Exception as e:
        log.exception("Unhandled exception in main:")
    finally:
        log.info("Exiting.")


if __name__ == "__main__":
    main()
