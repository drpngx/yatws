#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import argparse
import logging
import sys
import threading

from ibapi import wrapper
from ibapi.client import EClient
from ibapi.scanner import ScannerSubscription
from ibapi.utils import iswrapper

# --- Constants ---
DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 4002
DEFAULT_CLIENT_ID = 102

# --- Logging Setup ---
log = logging.getLogger(__name__)
log.setLevel(logging.DEBUG)
handler = logging.StreamHandler(sys.stdout)
formatter = logging.Formatter('%(asctime)s - %(name)s - %(levelname)s - %(message)s')
handler.setFormatter(formatter)
log.addHandler(handler)


# --- Test Application Class ---
class MarketScannerApp(wrapper.EWrapper, EClient):
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

        # For scanner data
        self.scanner_req_id = -1
        self.scanner_data_list = []
        self.scanner_data_finished = threading.Event()


    @iswrapper
    def connectAck(self):
        log.info("Connection acknowledged.")

    @iswrapper
    def nextValidId(self, orderId: int):
        super().nextValidId(orderId)
        self.nextValidOrderId = orderId
        log.info("nextValidId: %d", orderId)
        self.start()


    def start(self):
        if self.started:
            return
        self.started = True
        log.info("Executing requests")
        self.request_market_scanner()
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

        # Errors related to scanner data
        if reqId == self.scanner_req_id:
            log.error(f"Market scanner request {reqId} failed with error {errorCode}: {errorString}")
            self.scanner_data_finished.set()  # Signal completion on error

        # Store errors keyed by reqId
        if reqId > 0:
            self.reqId2nErr[reqId] = self.reqId2nErr.get(reqId, 0) + 1
            self._my_errors[reqId] = (errorCode, errorString)


    # --- Market Scanner Callbacks ---
    @iswrapper
    def scannerData(self, reqId: int, rank: int, contractDetails, distance: str,
                    benchmark: str, projection: str, legsStr: str):
        super().scannerData(reqId, rank, contractDetails, distance, benchmark, projection, legsStr)
        log.info(f"CALLBACK scannerData: ReqId: {reqId}, Rank: {rank}, Symbol: {contractDetails.contract.symbol}, "
                 f"SecType: {contractDetails.contract.secType}, Exchange: {contractDetails.contract.exchange}, "
                 f"Currency: {contractDetails.contract.currency}, Distance: {distance}, Benchmark: {benchmark}")

        if reqId == self.scanner_req_id:
            self.scanner_data_list.append({
                'rank': rank,
                'symbol': contractDetails.contract.symbol,
                'secType': contractDetails.contract.secType,
                'exchange': contractDetails.contract.exchange,
                'currency': contractDetails.contract.currency,
                'distance': distance,
                'benchmark': benchmark,
                'projection': projection,
                'legsStr': legsStr
            })

    @iswrapper
    def scannerDataEnd(self, reqId: int):
        super().scannerDataEnd(reqId)
        log.info(f"CALLBACK scannerDataEnd: ReqId: {reqId}")
        if reqId == self.scanner_req_id:
            self.scanner_data_finished.set()  # Signal completion

    # --- Market Scanner Request ---
    def request_market_scanner(self):
        self.scanner_req_id = self.nextValidOrderId  # Use next available ID
        self.nextValidOrderId += 1
        log.info(f"Requesting market scanner with reqId: {self.scanner_req_id}")

        # Create a ScannerSubscription object
        scanner_sub = ScannerSubscription()

        # Configure the scanner parameters
        scanner_sub.instrument = "STK"  # Stocks
        scanner_sub.locationCode = "STK.US.MAJOR"  # US major exchanges
        scanner_sub.scanCode = "TOP_PERC_GAIN"  # Top percentage gainers
        scanner_sub.abovePrice = 1
        scanner_sub.aboveVolume = 10000
        scanner_sub.numberOfRows = 10

        # Optional filters
        scanner_sub.stockTypeFilter = "ALL"  # ALL, STOCK, ETF

        # Scanner subscription options - empty for default behavior
        scannerSubscriptionOptions = []

        # Filter options (empty list for default)
        scannerSubscriptionFilterOptions = []

        log.info(f"Market Scanner parameters: Instrument={scanner_sub.instrument}, "
                 f"Location={scanner_sub.locationCode}, ScanCode={scanner_sub.scanCode}, "
                 f"AbovePrice={scanner_sub.abovePrice}, AboveVolume={scanner_sub.aboveVolume}")

        # Make the request
        # 22·1·10·STK·STK.US.MAJOR·TOP_PERC_GAIN·1··10000···········0···ALL···
        self.reqScannerSubscription(self.scanner_req_id, scanner_sub,
                                    scannerSubscriptionOptions,
                                    scannerSubscriptionFilterOptions)

        log.info(f"Market scanner request {self.scanner_req_id} sent. Background thread will process response.")


# --- Main Execution ---
def main():
    parser = argparse.ArgumentParser(description="IB API Market Scanner Test")
    parser.add_argument("--host", default=DEFAULT_HOST, help="Host address")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help="Port number")
    parser.add_argument("--clientId", type=int, default=DEFAULT_CLIENT_ID, help="Client ID")
    args = parser.parse_args()

    log.info("Starting Market Scanner Test")
    log.info(f"Connecting to {args.host}:{args.port} with clientId {args.clientId}")

    try:
        app = MarketScannerApp()
        log.info(f"Logger level set to: {logging.getLevelName(log.level)}")

        app.connect(args.host, args.port, args.clientId)
        log.info("Connection initiated. Server version: %s", app.serverVersion())

        # Start the EClient message loop in a separate thread
        log.info("Starting EClient.run() message loop in background thread...")
        thread = threading.Thread(target=app.run, daemon=True)
        thread.start()
        log.info("EClient.run() thread started.")

        # Wait for the scanner results
        wait_timeout_secs = 60  # Timeout for the scanner data
        log.info(f"Main thread waiting up to {wait_timeout_secs}s for scanner data end signal (reqId: {app.scanner_req_id})...")
        wait_successful = app.scanner_data_finished.wait(timeout=wait_timeout_secs)

        if wait_successful:
            log.info(f"Main thread: Market scanner request {app.scanner_req_id} finished (end signal received).")
            log.info(f"Main thread: Received {len(app.scanner_data_list)} scanner results.")

            # Print out the results
            if app.scanner_data_list:
                log.info("Scanner Results:")
                for idx, item in enumerate(app.scanner_data_list):
                    log.info(f"  {idx+1}. Rank: {item['rank']}, Symbol: {item['symbol']}, "
                             f"Exchange: {item['exchange']}, Distance: {item['distance']}")
        else:
            log.warning(f"Main thread: Market scanner request {app.scanner_req_id} timed out waiting for end signal!")
            log.warning(f"  Received {len(app.scanner_data_list)} results before timeout.")
            # Attempt to cancel if timed out
            if app.isConnected() and not app._my_errors.get(app.scanner_req_id):
                log.info(f"Main thread: Attempting to cancel market scanner request {app.scanner_req_id} due to timeout.")
                app.cancelScannerSubscription(app.scanner_req_id)

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

        log.info("Market Scanner Test finished.")

    except Exception as e:
        log.exception("Unhandled exception in main:")
    finally:
        log.info("Exiting.")


if __name__ == "__main__":
    main()
