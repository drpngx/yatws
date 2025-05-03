#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import argparse
import datetime
import logging
import sys
import time
import threading

from ibapi import wrapper
from ibapi.client import EClient
from ibapi.contract import Contract
from ibapi.utils import iswrapper

# --- Constants ---
DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 4002
DEFAULT_CLIENT_ID = 102 # Use a different ID than gen_goldens live tests

# --- Logging Setup ---
log = logging.getLogger(__name__)
# Set level to DEBUG to see historicalData callback logs
log.setLevel(logging.DEBUG) # Keep DEBUG level
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

        # For historical data
        self.hist_data_req_id = -1
        self.hist_data_list = []
        self.hist_data_finished = threading.Event()


    @iswrapper
    def connectAck(self):
        log.info("Connection acknowledged.")

    @iswrapper
    def nextValidId(self, orderId: int):
        super().nextValidId(orderId)
        self.nextValidOrderId = orderId
        log.info("nextValidId: %d", orderId)
        # EClient's run() loop should handle sending StartAPI implicitly after this.
        # No need for an explicit self.startApi() call here.
        self.start() # Start the main logic after getting nextValidId


    def start(self):
        if self.started:
            return
        self.started = True
        log.info("Executing requests")
        self.request_historical_data_test()
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

        # Errors related to historical data
        if reqId == self.hist_data_req_id and errorCode != 162: # 162 is historical data end warning
             log.error(f"Historical data request {reqId} failed with error {errorCode}: {errorString}")
             self.hist_data_finished.set() # Signal completion on error

        # Store errors keyed by reqId
        if reqId > 0:
            self.reqId2nErr[reqId] = self.reqId2nErr.get(reqId, 0) + 1
            self._my_errors[reqId] = (errorCode, errorString)


    # --- Historical Data Callbacks ---
    @iswrapper
    def historicalData(self, reqId: int, bar: wrapper.BarData):
        # Add INFO level log here to ensure it's seen even if DEBUG is filtered
        log.info("CALLBACK historicalData: ReqId: %d, Date: %s", reqId, bar.date)
        log.debug("HistoricalData. ReqId: %d, Date: %s, Open: %f, High: %f, Low: %f, Close: %f, Volume: %d, Count: %d, WAP: %f",
                 reqId, bar.date, bar.open, bar.high, bar.low, bar.close, bar.volume, bar.barCount, bar.wap)
        if reqId == self.hist_data_req_id:
            self.hist_data_list.append(bar)

    @iswrapper
    def historicalDataEnd(self, reqId: int, start: str, end: str):
        super().historicalDataEnd(reqId, start, end)
        # Add INFO level log here
        log.info("CALLBACK historicalDataEnd: ReqId: %d from %s to %s", reqId, start, end)
        if reqId == self.hist_data_req_id:
            self.hist_data_finished.set() # Signal completion

    # --- Test Logic ---
    def request_historical_data_test(self):
        self.hist_data_req_id = self.nextValidOrderId # Use next available ID
        self.nextValidOrderId += 1
        log.info(f"Requesting historical data with reqId: {self.hist_data_req_id}")

        contract = Contract()
        contract.symbol = "IBM"
        contract.secType = "STK"
        contract.exchange = "SMART"
        contract.currency = "USD"

        # Calculate endDateTime - TWS API expects "YYYYMMDD HH:MM:SS [zzz]" format
        # Let's request up to now. Leave empty for TWS to use current time.
        queryTime = ""
        # queryTime = datetime.datetime.now().strftime("%Y%m%d %H:%M:%S") # Example if specific end time needed

        durationStr = "3 D" # Duration (e.g., 3 days)
        barSizeSetting = "1 hour" # Bar size
        whatToShow = "TRADES"
        useRTH = 1 # 1 for RTH only, 0 for all hours
        formatDate = 1 # 1 for yyyyMMdd HH:mm:ss, 2 for seconds since epoch
        keepUpToDate = False # Don't subscribe for updates
        chartOptions = [] # No chart options

        log.info(f"Requesting Historical Data for {contract.symbol}: End={queryTime or 'Now'}, Duration={durationStr}, BarSize={barSizeSetting}, What={whatToShow}, RTH={useRTH}")

        self.reqHistoricalData(self.hist_data_req_id, contract, queryTime,
                               durationStr, barSizeSetting, whatToShow,
                               useRTH, formatDate, keepUpToDate, chartOptions)

        # Request initiated. The waiting logic is now handled in main().
        log.info(f"Historical data request {self.hist_data_req_id} sent. Background thread will process response.")
        # Do NOT wait or disconnect here.


# --- Main Execution ---
def main():
    parser = argparse.ArgumentParser(description="IB API Historical Data Test")
    parser.add_argument("--host", default=DEFAULT_HOST, help="Host address")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help="Port number")
    parser.add_argument("--clientId", type=int, default=DEFAULT_CLIENT_ID, help="Client ID")
    args = parser.parse_args()

    log.info("Starting Historical Data Test")
    log.info(f"Connecting to {args.host}:{args.port} with clientId {args.clientId}")

    try:
        app = TestApp()
        # Ensure logger level is set before connect/run
        log.info(f"Logger level set to: {logging.getLevelName(log.level)}") # Add this check

        app.connect(args.host, args.port, args.clientId)
        log.info("Connection initiated. Server version: %s", app.serverVersion()) # This might be 0 if connect returns before ack

        # Start the EClient message loop in a separate thread
        log.info("Starting EClient.run() message loop in background thread...") # Add this log
        thread = threading.Thread(target=app.run, daemon=True)
        thread.start()
        log.info("EClient.run() thread started.")

        # --- Wait for the specific test event in the main thread ---
        wait_timeout_secs = 90 # Timeout for the historical data itself
        log.info(f"Main thread waiting up to {wait_timeout_secs}s for historical data end signal (reqId: {app.hist_data_req_id})...")
        wait_successful = app.hist_data_finished.wait(timeout=wait_timeout_secs)

        if wait_successful:
            log.info(f"Main thread: Historical data request {app.hist_data_req_id} finished (end signal received).")
            # Log results here if needed, accessing app.hist_data_list
            log.info(f"Main thread: Received {len(app.hist_data_list)} bars.")
            if app.hist_data_list:
                 log.info(f"  First Bar: {app.hist_data_list[0].date}")
                 log.info(f"  Last Bar:  {app.hist_data_list[-1].date}")
        else:
            log.warning(f"Main thread: Historical data request {app.hist_data_req_id} timed out waiting for end signal!")
            log.warning(f"  Received {len(app.hist_data_list)} bars before timeout.")
            # Attempt to cancel if timed out (best effort)
            if app.isConnected() and not app._my_errors.get(app.hist_data_req_id):
                 log.info(f"Main thread: Attempting to cancel historical data request {app.hist_data_req_id} due to timeout.")
                 app.cancelHistoricalData(app.hist_data_req_id)

        # --- Disconnect and wait for thread exit ---
        log.info("Main thread: Disconnecting...")
        if app.isConnected():
            app.disconnect()
        else:
            log.info("Main thread: Already disconnected.")

        # Wait for the EClient thread to finish after disconnect
        join_timeout_secs = 10 # Short timeout for join after disconnect
        log.info(f"Main thread waiting for EClient thread to join (timeout {join_timeout_secs}s)...")
        thread.join(timeout=join_timeout_secs)
        log.info("Main thread finished waiting for EClient thread.")

        if thread.is_alive():
             log.warning("EClient thread did not exit cleanly after disconnect and join timeout.")
             # Attempt disconnect again if stuck
             if app.isConnected():
                 log.warning("Attempting disconnect again...")
                 app.disconnect()
        else:
             log.info("EClient thread exited cleanly.")


        log.info("Historical Data Test finished.")

    except Exception as e:
        log.exception("Unhandled exception in main:")
    finally:
        log.info("Exiting.")


if __name__ == "__main__":
    main()
