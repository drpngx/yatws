#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import argparse
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
DEFAULT_CLIENT_ID = 104 # Use a different ID than other tests

# --- Logging Setup ---
log = logging.getLogger(__name__)
log.setLevel(logging.DEBUG)
handler = logging.StreamHandler(sys.stdout)
formatter = logging.Formatter('%(asctime)s - %(name)s - %(levelname)s - %(message)s')
handler.setFormatter(formatter)
log.addHandler(handler)


def create_stop_limit_order(action: str, quantity: int, stop_price: float, limit_price: float) -> Order:
    """Creates a Stop-Limit order."""
    order = Order()
    order.action = action
    order.orderType = "STP LMT"  # Stop Limit
    order.totalQuantity = quantity
    order.lmtPrice = limit_price
    order.auxPrice = stop_price  # Stop price
    order.transmit = True
    return order


# --- Test Application Class ---
class TestApp(wrapper.EWrapper, EClient):
    def __init__(self):
        wrapper.EWrapper.__init__(self)
        EClient.__init__(self, wrapper=self)
        self.started = False
        self.nextValidOrderId = -1
        self._my_errors = {}
        self.test_finished = threading.Event()

    @iswrapper
    def connectAck(self):
        log.info("Connection acknowledged.")

    @iswrapper
    def nextValidId(self, orderId: int):
        super().nextValidId(orderId)
        self.nextValidOrderId = orderId
        log.info("nextValidId: %d", orderId)
        # Start the main logic after getting a valid order ID
        self.start()

    def start(self):
        if self.started:
            return
        self.started = True
        log.info("Executing requests")
        self.place_stop_limit_order_test()
        log.info("Requests finished. Waiting for server responses...")
        # Give TWS time to process and send back status messages
        time.sleep(5)
        self.test_finished.set()

    def keyboardInterrupt(self):
        log.info("Keyboard interrupt detected. Disconnecting...")
        self.test_finished.set()

    @iswrapper
    def error(self, reqId: int, errorCode: int, errorString: str, advancedOrderReject=""):
        super().error(reqId, errorCode, errorString, advancedOrderReject)
        if advancedOrderReject:
            log.error("Error. Id: %d, Code: %d, Msg: %s, AdvancedOrderReject: %s", reqId, errorCode, errorString, advancedOrderReject)
        else:
            log.error("Error. Id: %d, Code: %d, Msg: %s", reqId, errorCode, errorString)

        # Store errors keyed by reqId for debugging
        if reqId > 0:
            self._my_errors[reqId] = (errorCode, errorString)

    # --- Order Callbacks ---
    @iswrapper
    def openOrder(self, orderId, contract, order, orderState):
        super().openOrder(orderId, contract, order, orderState)
        log.info("CALLBACK openOrder: Id: %d, %s, %s, %s, %s, State: %s",
                 orderId, contract.symbol, contract.secType,
                 order.action, order.orderType, orderState.status)
        log.debug("  Order Details: %s", order)
        log.debug("  Order State: %s", orderState)

    @iswrapper
    def orderStatus(self, orderId, status, filled, remaining, avgFillPrice, permId, parentId, lastFillPrice, clientId, whyHeld, mktCapPrice):
        super().orderStatus(orderId, status, filled, remaining, avgFillPrice, permId, parentId, lastFillPrice, clientId, whyHeld, mktCapPrice)
        log.info("CALLBACK orderStatus: Id: %d, Status: %s, Filled: %f, Remaining: %f, AvgFillPrice: %f, ParentId: %d",
                 orderId, status, filled, remaining, avgFillPrice, parentId)

    # --- Test Logic ---
    # 3·1·0·MSFT·STK··0.0···SMART··USD·····BUY·1·STP LMT·301.0·300.0·····0··1·0·0·0·0·0·0·0··0·······0··-1·0···0···0·0··0······0·····0···········0···0·0···0··0·0·0·0··1.7976931348623157e+308·1.7976931348623157e+308·1.7976931348623157e+308·1.7976931348623157e+308·1.7976931348623157e+308·0····1.7976931348623157e+308·····0·0·0··2147483647·2147483647·0····0··2147483647·
    def place_stop_limit_order_test(self):
        if self.nextValidOrderId == -1:
            log.error("nextValidOrderId not received yet. Cannot place order.")
            return

        # Define the contract for MSFT stock
        contract = Contract()
        contract.symbol = "MSFT"
        contract.secType = "STK"
        contract.exchange = "SMART"
        contract.currency = "USD"

        # Use the provided function to create the Stop Limit order
        # Action: BUY, Quantity: 1, Stop Price: 300.0, Limit Price: 301.0
        # This means: "If MSFT price rises to $300, submit a LMT order to buy at $301 or better"
        # Prices are set far from the current market to prevent actual execution.
        order = create_stop_limit_order("BUY", 1, 300.0, 301.0)

        # Assign the next valid order ID
        order.orderId = self.nextValidOrderId
        self.nextValidOrderId += 1

        log.info(f"Submitting Stop-Limit Order (ID: {order.orderId}): {order.action} {order.totalQuantity} {contract.symbol} with StopPrice {order.auxPrice} and LimitPrice {order.lmtPrice}")
        self.placeOrder(order.orderId, contract, order)

        log.info("Stop-Limit order request has been sent.")


# --- Main Execution ---
def main():
    parser = argparse.ArgumentParser(description="IB API Stop-Limit Order Test")
    parser.add_argument("--host", default=DEFAULT_HOST, help="Host address")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help="Port number")
    parser.add_argument("--clientId", type=int, default=DEFAULT_CLIENT_ID, help="Client ID")
    args = parser.parse_args()

    log.info("Starting Stop-Limit Order Test")
    log.info(f"Connecting to {args.host}:{args.port} with clientId {args.clientId}")

    try:
        app = TestApp()
        app.connect(args.host, args.port, args.clientId)
        log.info(f"Connection initiated. Server version: {app.serverVersion()}")

        # Start the EClient message loop in a background thread
        thread = threading.Thread(target=app.run, daemon=True)
        thread.start()
        log.info("EClient.run() thread started.")

        # Wait for the test to signal completion or for a timeout
        wait_timeout_secs = 30
        log.info(f"Main thread waiting up to {wait_timeout_secs}s for test completion...")
        app.test_finished.wait(timeout=wait_timeout_secs)

        if not app.test_finished.is_set():
            log.warning("Test did not complete within the timeout.")

    except Exception as e:
        log.exception("Unhandled exception in main:")
    finally:
        log.info("Main thread: Disconnecting...")
        if app.isConnected():
            app.disconnect()
        log.info("Exiting.")


if __name__ == "__main__":
    main()
