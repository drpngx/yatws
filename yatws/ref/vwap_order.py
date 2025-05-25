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
from ibapi.order import Order
from ibapi.utils import iswrapper

# --- Constants ---
DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 4002
DEFAULT_CLIENT_ID = 103  # Use a different ID than other test scripts

# --- Logging Setup ---
log = logging.getLogger(__name__)
log.setLevel(logging.DEBUG)  # Keep DEBUG level
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

        # For what-if order results
        self.what_if_order_id = -1
        self.what_if_results = {}
        self.what_if_finished = threading.Event()

    @iswrapper
    def connectAck(self):
        log.info("Connection acknowledged.")

    @iswrapper
    def nextValidId(self, orderId: int):
        super().nextValidId(orderId)
        self.nextValidOrderId = orderId
        log.info("nextValidId: %d", orderId)
        self.start()  # Start the main logic after getting nextValidId

    def start(self):
        if self.started:
            return
        self.started = True
        log.info("Executing what-if order request")
        self.request_vwap_what_if_order_test()
        log.info("Request finished")

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
            log.error("Error. Id: %d, Code: %d, Msg: %s, AdvancedOrderReject: %s",
                     reqId, errorCode, errorString, advancedOrderReject)
        else:
            log.error("Error. Id: %d, Code: %d, Msg: %s", reqId, errorCode, errorString)

        # Errors related to what-if order
        if reqId == self.what_if_order_id:
            log.error(f"What-if order request {reqId} failed with error {errorCode}: {errorString}")
            self.what_if_finished.set()  # Signal completion on error

        # Store errors keyed by reqId
        if reqId > 0:
            self.reqId2nErr[reqId] = self.reqId2nErr.get(reqId, 0) + 1
            self._my_errors[reqId] = (errorCode, errorString)

    # --- Order Status Callbacks ---
    @iswrapper
    def orderStatus(self, orderId: int, status: str, filled: float, remaining: float,
                   avgFillPrice: float, permId: int, parentId: int, lastFillPrice: float,
                   clientId: int, whyHeld: str, mktCapPrice: float):
        log.info("CALLBACK orderStatus: OrderId: %d, Status: %s, Filled: %g, Remaining: %g, "
                "AvgFillPrice: %g, PermId: %d, ParentId: %d, LastFillPrice: %g, "
                "ClientId: %d, WhyHeld: %s, MktCapPrice: %g",
                orderId, status, filled, remaining, avgFillPrice, permId, parentId,
                lastFillPrice, clientId, whyHeld, mktCapPrice)

    @iswrapper
    def openOrder(self, orderId: int, contract: Contract, order: Order, orderState):
        log.info("CALLBACK openOrder: OrderId: %d, Symbol: %s, SecType: %s, Status: %s",
                orderId, contract.symbol, contract.secType, orderState.status)

        if orderId == self.what_if_order_id and order.whatIf:
            log.info("Received what-if order results for order %d", orderId)

            # Extract the margin and commission information
            self.what_if_results = {
                'orderId': orderId,
                'status': orderState.status,
                'initMarginBefore': orderState.initMarginBefore,
                'maintMarginBefore': orderState.maintMarginBefore,
                'equityWithLoanBefore': orderState.equityWithLoanBefore,
                'initMarginAfter': orderState.initMarginAfter,
                'maintMarginAfter': orderState.maintMarginAfter,
                'equityWithLoanAfter': orderState.equityWithLoanAfter,
                'initMarginChange': orderState.initMarginChange,
                'maintMarginChange': orderState.maintMarginChange,
                'equityWithLoanChange': orderState.equityWithLoanChange,
                'commission': orderState.commission,
                'commissionCurrency': orderState.commissionCurrency,
                'warningText': orderState.warningText
            }

            log.info("What-if results:")
            log.info("  Initial Margin After: %s", orderState.initMarginAfter)
            log.info("  Maintenance Margin After: %s", orderState.maintMarginAfter)
            log.info("  Commission: %s %s", orderState.commission, orderState.commissionCurrency)
            if orderState.warningText:
                log.info("  Warning: %s", orderState.warningText)

            self.what_if_finished.set()  # Signal completion

    @iswrapper
    def openOrderEnd(self):
        log.info("CALLBACK openOrderEnd")

    # --- Test Logic ---
    # 3·1·0·AAPL·STK··0.0···SMART··USD·····BUY·5000·LMT·150.0······0··1·0·0·0·0·0·0·0··0·······0··-1·0···0···0·0··0······0·····0···········0···0·0·Vwap·6·maxPctVol·0.10·startTime·20250525-18:44:23·endTime·20250526-00:44:23·allowPastEndTime·0·noTakeLiq·0·speedUp·0··1··0·0·0·0··1.7976931348623157e+308·1.7976931348623157e+308·1.7976931348623157e+308·1.7976931348623157e+308·1.7976931348623157e+308·0····1.7976931348623157e+308·····0·0·0··2147483647·2147483647·0····0··2147483647·
    def request_vwap_what_if_order_test(self):
        self.what_if_order_id = self.nextValidOrderId
        self.nextValidOrderId += 1
        log.info(f"Requesting VWAP what-if order with orderId: {self.what_if_order_id}")

        # Create contract for AAPL stock
        contract = Contract()
        contract.symbol = "AAPL"
        contract.secType = "STK"
        contract.exchange = "SMART"
        contract.currency = "USD"

        # Create VWAP algorithm order
        order = Order()
        order.action = "BUY"
        order.totalQuantity = 5000
        order.orderType = "LMT"
        order.lmtPrice = 150.0
        order.whatIf = True  # This makes it a what-if order
        order.transmit = True

        # Set up VWAP algorithm parameters
        order.algoStrategy = "Vwap"
        order.algoParams = []

        # Calculate start and end times (1 hour from now, ending 6 hours later)
        now = datetime.datetime.utcnow()
        start_time = now + datetime.timedelta(hours=1)
        end_time = start_time + datetime.timedelta(hours=6)

        # Format times as TWS expects: "YYYYMMDD-HH:MM:SS" (UTC notation with dash)
        start_time_str = start_time.strftime("%Y%m%d-%H:%M:%S")
        end_time_str = end_time.strftime("%Y%m%d-%H:%M:%S")

        # Add VWAP algorithm parameters
        from ibapi.tag_value import TagValue
        order.algoParams.append(TagValue("maxPctVol", "0.10"))  # 10% of volume
        order.algoParams.append(TagValue("startTime", start_time_str))
        order.algoParams.append(TagValue("endTime", end_time_str))
        order.algoParams.append(TagValue("allowPastEndTime", "0"))  # false
        order.algoParams.append(TagValue("noTakeLiq", "0"))  # false
        order.algoParams.append(TagValue("speedUp", "0"))  # false

        log.info(f"VWAP Algorithm Parameters:")
        log.info(f"  Max % of Volume: 10%")
        log.info(f"  Start Time: {start_time_str}")
        log.info(f"  End Time: {end_time_str}")
        log.info(f"  Allow Past End Time: False")
        log.info(f"  No Take Liquidity: False")
        log.info(f"  Speed Up: False")

        log.info(f"Placing what-if order for {order.totalQuantity} shares of {contract.symbol} "
                f"at ${order.lmtPrice} using VWAP algorithm")

        # Place the what-if order
        self.placeOrder(self.what_if_order_id, contract, order)

        log.info(f"What-if order request sent for ID: {self.what_if_order_id}")


# --- Main Execution ---
def main():
    parser = argparse.ArgumentParser(description="IB API VWAP What-If Order Test")
    parser.add_argument("--host", default=DEFAULT_HOST, help="Host address")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help="Port number")
    parser.add_argument("--clientId", type=int, default=DEFAULT_CLIENT_ID, help="Client ID")
    args = parser.parse_args()

    log.info("Starting VWAP What-If Order Test")
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

        # --- Wait for the what-if order results ---
        wait_timeout_secs = 60  # Timeout for the what-if order
        log.info(f"Main thread waiting up to {wait_timeout_secs}s for what-if order results...")
        wait_successful = app.what_if_finished.wait(timeout=wait_timeout_secs)

        if wait_successful:
            log.info(f"Main thread: What-if order {app.what_if_order_id} completed.")

            if app.what_if_results:
                results = app.what_if_results
                log.info("=== WHAT-IF ORDER RESULTS ===")
                log.info(f"Order ID: {results['orderId']}")
                log.info(f"Status: {results['status']}")

                if results['initMarginAfter']:
                    log.info(f"Initial Margin After: ${results['initMarginAfter']}")
                if results['maintMarginAfter']:
                    log.info(f"Maintenance Margin After: ${results['maintMarginAfter']}")
                if results['commission']:
                    log.info(f"Estimated Commission: {results['commission']} {results['commissionCurrency']}")

                if results['initMarginChange']:
                    log.info(f"Initial Margin Change: ${results['initMarginChange']}")
                if results['maintMarginChange']:
                    log.info(f"Maintenance Margin Change: ${results['maintMarginChange']}")
                if results['equityWithLoanChange']:
                    log.info(f"Equity with Loan Change: ${results['equityWithLoanChange']}")

                if results['warningText']:
                    log.warning(f"Warning: {results['warningText']}")

                log.info("=============================")
            else:
                log.warning("What-if order completed but no results received")
        else:
            log.warning(f"Main thread: What-if order {app.what_if_order_id} timed out!")

        # --- Disconnect and wait for thread exit ---
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

        log.info("VWAP What-If Order Test finished.")

    except Exception as e:
        log.exception("Unhandled exception in main:")
    finally:
        log.info("Exiting.")


if __name__ == "__main__":
    main()
