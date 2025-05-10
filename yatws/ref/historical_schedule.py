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
DEFAULT_CLIENT_ID = 103 # Use a different ID than other ref tests

# --- Logging Setup ---
log = logging.getLogger(__name__)
log.setLevel(logging.INFO) # Set to INFO, can be changed to DEBUG for more detail
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
        self.nextValidOrderId = -1 # Used for reqIds
        self._my_errors = {}

        # For historical schedule
        self.hist_schedule_req_id = -1
        self.hist_schedule_data = {
            "overall_start": "",
            "overall_end": "",
            "time_zone": "", # This will be the timezone from the first session, assuming it's consistent
            "sessions": [] # List of (session_start, session_end, session_timezone)
        }
        self.hist_schedule_finished = threading.Event()


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
        log.info("Executing requests")
        self.request_historical_schedule_test()
        log.info("Requests finished (background processing continues)")

    def keyboardInterrupt(self):
        log.info("Keyboard interrupt detected. Disconnecting...")
        self.disconnect()
        # Signal completion if waiting
        if not self.hist_schedule_finished.is_set():
            self.hist_schedule_finished.set()


    @iswrapper
    def error(self, reqId: int, errorCode: int, errorString: str, advancedOrderReject=""):
        super().error(reqId, errorCode, errorString, advancedOrderReject)
        if advancedOrderReject:
            log.error("Error. Id: %d, Code: %d, Msg: %s, AdvancedOrderReject: %s", reqId, errorCode, errorString, advancedOrderReject)
        else:
            log.error("Error. Id: %d, Code: %d, Msg: %s", reqId, errorCode, errorString)

        # Errors related to historical schedule request
        # 162 is "Historical data request pacing violation" or "Historical Market Data Service error message:API historical data query cancelled"
        # 2104 is "Market data farm connection is OK" (not an error)
        # 2106 is "HMDS data farm connection is OK" (not an error)
        # 2158 is "Sec-def data farm connection is OK" (not an error)
        # 321 is "Error validating request:-'bH' : cause - Please input a valid start date/time" (example of bad request)
        # 322 is "Error processing request:-'bH': cause - Historical data request failed:Timeout"
        if reqId == self.hist_schedule_req_id and errorCode not in [162, 2104, 2106, 2158]:
             log.error(f"Historical schedule request {reqId} resulted in error {errorCode}: {errorString}")
             self._my_errors[reqId] = (errorCode, errorString)
             self.hist_schedule_finished.set() # Signal completion on error

    # --- Historical Data Callbacks for SCHEDULE ---
    @iswrapper
    def historicalData(self, reqId: int, bar: wrapper.BarData):
        # For whatToShow = "SCHEDULE", the BarData fields are used differently:
        # bar.date: Start of the trading session (e.g., "20230301:093000")
        # bar.open: End of the trading session (e.g., "20230301:160000")
        # bar.high: Timezone of the trading session (e.g., "America/New_York")
        # Other fields (low, close, volume, barCount, wap) are not typically used or are 0/-1.
        if reqId == self.hist_schedule_req_id:
            session_start = bar.date
            session_end = str(bar.open) # bar.open is float, needs conversion
            session_timezone = str(bar.high) # bar.high is float, needs conversion

            log.info(f"CALLBACK historicalData (SCHEDULE): ReqId: {reqId}, SessionStart: {session_start}, SessionEnd: {session_end}, TimeZone: {session_timezone}")
            self.hist_schedule_data["sessions"].append((session_start, session_end, session_timezone))
            if not self.hist_schedule_data["time_zone"] and session_timezone != "0.0" and session_timezone != "-1.0": # Store first valid timezone
                self.hist_schedule_data["time_zone"] = session_timezone
        else:
            log.debug(f"HistoricalData for other reqId {reqId}: {bar}")


    @iswrapper
    def historicalDataEnd(self, reqId: int, start: str, end: str):
        super().historicalDataEnd(reqId, start, end)
        if reqId == self.hist_schedule_req_id:
            log.info(f"CALLBACK historicalDataEnd (SCHEDULE): ReqId: {reqId}, OverallStart: {start}, OverallEnd: {end}")
            self.hist_schedule_data["overall_start"] = start
            self.hist_schedule_data["overall_end"] = end
            self.hist_schedule_finished.set() # Signal completion
        else:
            log.debug(f"HistoricalDataEnd for other reqId {reqId}")

    # --- Test Logic ---
    def request_historical_schedule_test(self):
        self.hist_schedule_req_id = self.nextValidOrderId # Use next available ID
        self.nextValidOrderId += 1
        log.info(f"Requesting historical schedule with reqId: {self.hist_schedule_req_id}")

        contract = Contract()
        contract.symbol = "AAPL"
        contract.secType = "STK"
        contract.exchange = "SMART"
        contract.currency = "USD"
        # contract.primaryExchange = "NASDAQ" # Optional

        # For SCHEDULE:
        # endDateTime: The date part is used. Time part is ignored. If empty, current TWS date is used.
        # durationStr: Integer followed by space and unit 'D', 'W', or 'M'. E.g., "7 D", "4 W", "3 M".
        # barSizeSetting: Ignored, but API requires a valid value. "1 day" is fine.
        # whatToShow: Must be "SCHEDULE".
        # useRTH: Must be 0.
        # formatDate: 1 (yyyyMMdd{ }hh:mm:ss) or 2 (seconds since epoch).
        # keepUpToDate: Must be False.

        # Example: Request schedule for the next 30 days from today
        # queryTime = datetime.datetime.now().strftime("%Y%m%d %H:%M:%S") # End date for the period
        queryTime = "" # Let TWS use current date as end of period for duration calculation

        durationStr = "30 D" # Request 30 days of schedule
        barSizeSetting = "1 day" # Ignored for SCHEDULE, but must be valid
        whatToShow = "SCHEDULE"
        useRTH = 0 # Must be 0 for SCHEDULE
        formatDate = 1 # yyyyMMdd HH:mm:ss
        keepUpToDate = False # Must be False for SCHEDULE
        chartOptions = [] # No chart options

        log.info(f"Requesting Historical Schedule for {contract.symbol}: EndDateTime='{queryTime or 'Now'}' (date part used), Duration={durationStr}, What={whatToShow}, useRTH={useRTH}")

        # 20·1·0·AAPL·STK··0.0···SMART··USD···0··1 day·30 D·0·SCHEDULE·1·0··
        self.reqHistoricalData(self.hist_schedule_req_id, contract, queryTime,
                               durationStr, barSizeSetting, whatToShow,
                               useRTH, formatDate, keepUpToDate, chartOptions)

        log.info(f"Historical schedule request {self.hist_schedule_req_id} sent.")


# --- Main Execution ---
def main():
    parser = argparse.ArgumentParser(description="IB API Historical Schedule Test")
    parser.add_argument("--host", default=DEFAULT_HOST, help="Host address")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help="Port number")
    parser.add_argument("--clientId", type=int, default=DEFAULT_CLIENT_ID, help="Client ID")
    args = parser.parse_args()

    log.info("Starting Historical Schedule Test")
    log.info(f"Connecting to {args.host}:{args.port} with clientId {args.clientId}")

    app = TestApp()
    try:
        app.connect(args.host, args.port, args.clientId)
        log.info(f"Connection initiated. Server version: {app.serverVersion()}")

        log.info("Starting EClient.run() message loop in background thread...")
        thread = threading.Thread(target=app.run, daemon=True)
        thread.start()
        log.info("EClient.run() thread started.")

        # Wait for the historical schedule data to be finished
        wait_timeout_secs = 60 # Timeout for the schedule data
        log.info(f"Main thread waiting up to {wait_timeout_secs}s for historical schedule end signal (reqId: {app.hist_schedule_req_id})...")

        wait_successful = app.hist_schedule_finished.wait(timeout=wait_timeout_secs)

        if wait_successful:
            if app._my_errors.get(app.hist_schedule_req_id):
                log.error(f"Main thread: Historical schedule request {app.hist_schedule_req_id} failed. Error: {app._my_errors[app.hist_schedule_req_id]}")
            else:
                log.info(f"Main thread: Historical schedule request {app.hist_schedule_req_id} finished.")
                log.info(f"  Overall Period Start: {app.hist_schedule_data['overall_start']}")
                log.info(f"  Overall Period End:   {app.hist_schedule_data['overall_end']}")
                log.info(f"  Primary TimeZone:     {app.hist_schedule_data['time_zone']}")
                log.info(f"  Number of Sessions:   {len(app.hist_schedule_data['sessions'])}")
                for i, (s_start, s_end, s_tz) in enumerate(app.hist_schedule_data["sessions"][:5]): # Log first 5 sessions
                    log.info(f"    Session {i+1}: Start='{s_start}', End='{s_end}', TZ='{s_tz}'")
        else:
            log.warning(f"Main thread: Historical schedule request {app.hist_schedule_req_id} timed out waiting for end signal!")
            log.warning(f"  Received {len(app.hist_schedule_data['sessions'])} sessions before timeout.")
            if app.isConnected() and not app._my_errors.get(app.hist_schedule_req_id):
                 log.info(f"Main thread: Attempting to cancel historical schedule request {app.hist_schedule_req_id} due to timeout.")
                 app.cancelHistoricalData(app.hist_schedule_req_id) # cancelHistoricalData is used for SCHEDULE too

    except KeyboardInterrupt:
        log.info("Main thread: Keyboard interrupt received.")
    except Exception as e:
        log.exception("Unhandled exception in main:")
    finally:
        log.info("Main thread: Disconnecting...")
        if app.isConnected():
            app.disconnect()

        # Wait for the EClient thread to finish
        if 'thread' in locals() and thread.is_alive():
            log.info("Main thread waiting for EClient thread to join...")
            thread.join(timeout=10)
            if thread.is_alive():
                log.warning("EClient thread did not exit cleanly.")
            else:
                log.info("EClient thread exited cleanly.")

        log.info("Historical Schedule Test finished.")


if __name__ == "__main__":
    main()
