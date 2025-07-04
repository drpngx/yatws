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
from ibapi.common import OrderId

# --- Constants ---
DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 4002
DEFAULT_CLIENT_ID = 103  # Use a different ID than historical data test

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

        # For order placement
        self.order_id = -1
        self.order_placed = threading.Event()
        self.order_status_received = threading.Event()
        self.order_states = {}  # Track order states by order ID

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
        log.info("Executing order placement test")
        self.place_order_test()
        log.info("Order placement request finished")

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

        # Store errors keyed by reqId
        if reqId > 0:
            self.reqId2nErr[reqId] = self.reqId2nErr.get(reqId, 0) + 1
            self._my_errors[reqId] = (errorCode, errorString)

        # If this is our order, signal completion
        if reqId == self.order_id:
            log.error(f"Order {reqId} failed with error {errorCode}: {errorString}")
            self.order_status_received.set()

    # --- Order Callbacks ---
    @iswrapper
    def openOrder(self, orderId: OrderId, contract: Contract, order: Order, orderState):
        log.info("CALLBACK openOrder: OrderId: %d, Symbol: %s, Status: %s",
                orderId, contract.symbol, orderState.status)
        log.debug("OpenOrder. OrderId: %d, Symbol: %s, SecType: %s, Side: %s, Quantity: %f, "
                 "OrderType: %s, LimitPrice: %s, AuxPrice: %s, Status: %s",
                 orderId, contract.symbol, contract.secType, order.action, order.totalQuantity,
                 order.orderType, order.lmtPrice, order.auxPrice, orderState.status)

        if orderId == self.order_id:
            self.order_states[orderId] = orderState
            self.order_placed.set()

    @iswrapper
    def orderStatus(self, orderId: OrderId, status: str, filled: float,
                   remaining: float, avgFillPrice: float, permId: int,
                   parentId: int, lastFillPrice: float, clientId: int, whyHeld: str,
                   mktCapPrice: float):
        log.info("CALLBACK orderStatus: OrderId: %d, Status: %s, Filled: %f, Remaining: %f, "
                "AvgFillPrice: %f, PermId: %d",
                orderId, status, filled, remaining, avgFillPrice, permId)

        if orderId == self.order_id:
            # Store status information
            self.order_states[orderId] = {
                'status': status,
                'filled': filled,
                'remaining': remaining,
                'avgFillPrice': avgFillPrice,
                'permId': permId,
                'parentId': parentId,
                'lastFillPrice': lastFillPrice,
                'clientId': clientId,
                'whyHeld': whyHeld,
                'mktCapPrice': mktCapPrice
            }

            # Signal completion for terminal states
            if status in ['Filled', 'Cancelled', 'ApiCancelled', 'Inactive']:
                log.info(f"Order {orderId} reached terminal status: {status}")
                self.order_status_received.set()

    @iswrapper
    def execDetails(self, reqId: int, contract: Contract, execution):
        log.info("CALLBACK execDetails: ReqId: %d, Symbol: %s, ExecId: %s, Side: %s, "
                "Shares: %f, Price: %f",
                reqId, contract.symbol, execution.execId, execution.side,
                execution.shares, execution.price)

    @iswrapper
    def commissionReport(self, commissionReport):
        log.info("CALLBACK commissionReport: ExecId: %s, Commission: %f, Currency: %s",
                commissionReport.execId, commissionReport.commission,
                commissionReport.currency)

    # --- Test Logic ---
    def place_order_test(self):
        self.order_id = self.nextValidOrderId
        self.nextValidOrderId += 1
        log.info(f"Placing order with orderId: {self.order_id}")

        # Create MSFT contract matching the Rust data
        contract = Contract()
        contract.conId = 272093  # Specific contract ID from Rust data
        contract.symbol = "MSFT"
        contract.secType = "STK"
        contract.exchange = "SMART"
        contract.currency = "USD"
        contract.localSymbol = "MSFT"
        contract.tradingClass = "NMS"

        # Create stop-limit order matching the Rust data
        order = Order()
        order.orderId = self.order_id
        order.orderType = "STP LMT"  # StopLimit
        order.action = "SELL"        # Sell
        order.totalQuantity = 1.0    # quantity: 1.0
        order.lmtPrice = 457.0       # limit_price: Some(457.0)
        order.auxPrice = 461.0       # aux_price: Some(461.0) - stop price
        order.tif = "DAY"           # time_in_force: Day
        order.outsideRth = True     # outside_rth: true
        order.transmit = True       # transmit: true

        # Additional parameters from Rust data
        order.minQty = 0            # min_quantity: Some(0)
        order.percentOffset = 0.0   # percent_offset: Some(0.0)
        order.trailingPercent = 0.0 # trailing_percent: Some(0.0)
        order.trailStopPrice = 471.0 # trailing_stop_price: Some(471.0)
        order.ocaType = 3           # oca_type: Some(3)
        order.triggerMethod = 0     # trigger_method: Some(0)
        order.clearingIntent = "IB" # clearing_intent: Some("IB")
        order.openClose = "O"       # open_close: Some("O")
        order.origin = 0            # origin: 0

        # Scale and other parameters
        order.scaleInitLevelSize = 0     # scale_init_level_size: None -> 0
        # order.scalePriceIncrement = 0.0  # scale_price_increment: Some(0.0)
        # order.startingPrice = 0.0        # starting_price: Some(0.0)
        # order.stockRefPrice = 0.0        # stock_ref_price: Some(0.0)
        # order.delta = 0.0               # delta: Some(0.0)
        # order.stockRangeLower = 0.0     # stock_range_lower: Some(0.0)
        # order.stockRangeUpper = 0.0     # stock_range_upper: Some(0.0)
        # order.volatility = 0.0          # volatility: Some(0.0)
        order.volatilityType = 0        # volatility_type: Some(0)
        # order.continuousUpdate = 0      # continuous_update: Some(0)
        # order.referencePriceType = 0    # reference_price_type: Some(0)
        # order.basisPoints = 0.0         # basis_points: Some(0.0)
        # order.basisPointsType = 0       # basis_points_type: Some(0)

        # Delta neutral parameters
        order.deltaNeutralOrderType = "None"  # delta_neutral_order_type: Some("None")
        # order.deltaNeutralAuxPrice = 0.0      # delta_neutral_aux_price: Some(0.0)
        order.deltaNeutralConId = 0           # delta_neutral_con_id: Some(0)
        order.deltaNeutralShortSale = False   # delta_neutral_short_sale: false
        order.deltaNeutralShortSaleSlot = 0   # delta_neutral_short_sale_slot: Some(0)

        # Additional flags
        order.dontUseAutoPriceForHedge = True  # dont_use_auto_price_for_hedge: true
        order.usePriceMgmtAlgo = False         # use_price_mgmt_algo: Some(false)
        order.duration = 0                     # duration: Some(0)
        order.postToAts = 0                    # post_to_ats: Some(0)
        order.minTradeQty = 0                  # min_trade_qty: Some(0)
        order.minCompeteSize = 100             # min_compete_size: Some(100)
        # order.competeAgainstBestOffset = 0.02  # compete_against_best_offset: Some(0.02)
        order.midOffsetAtWhole = 0.0           # mid_offset_at_whole: Some(0.0)
        order.midOffsetAtHalf = 0.0            # mid_offset_at_half: Some(0.0)
        order.cashQty = 0.0                    # cash_qty: Some(0.0)
        order.adjustableTrailingUnit = 0       # adjustable_trailing_unit: Some(0)

        log.info(f"Placing {order.action} {order.totalQuantity} {contract.symbol} "
                f"{order.orderType} order: Stop=${order.auxPrice}, Limit=${order.lmtPrice}")

        # Place the order
        self.placeOrder(self.order_id, contract, order)
        log.info(f"Order {self.order_id} placement request sent.")


# --- Main Execution ---
def main():
    parser = argparse.ArgumentParser(description="IB API Order Placement Test")
    parser.add_argument("--host", default=DEFAULT_HOST, help="Host address")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help="Port number")
    parser.add_argument("--clientId", type=int, default=DEFAULT_CLIENT_ID, help="Client ID")
    args = parser.parse_args()

    log.info("Starting Order Placement Test")
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

        # Wait for order placement confirmation
        placement_timeout_secs = 30
        log.info(f"Main thread waiting up to {placement_timeout_secs}s for order placement confirmation...")
        placement_successful = app.order_placed.wait(timeout=placement_timeout_secs)

        if placement_successful:
            log.info(f"Main thread: Order {app.order_id} placement confirmed.")

            # Wait for order status updates
            status_timeout_secs = 60
            log.info(f"Main thread waiting up to {status_timeout_secs}s for order status updates...")
            status_received = app.order_status_received.wait(timeout=status_timeout_secs)

            if status_received:
                log.info(f"Main thread: Order {app.order_id} reached terminal status.")
                if app.order_id in app.order_states:
                    state = app.order_states[app.order_id]
                    if isinstance(state, dict):
                        log.info(f"  Final Status: {state.get('status', 'Unknown')}")
                        log.info(f"  Filled: {state.get('filled', 0.0)}")
                        log.info(f"  Remaining: {state.get('remaining', 0.0)}")
                        log.info(f"  PermId: {state.get('permId', 0)}")
                    else:
                        log.info(f"  Order State: {state.status}")
            else:
                log.warning(f"Main thread: Order {app.order_id} status update timed out!")

                # Attempt to cancel if no terminal status received
                if app.isConnected() and not app._my_errors.get(app.order_id):
                    log.info(f"Main thread: Attempting to cancel order {app.order_id} due to timeout.")
                    app.cancelOrder(app.order_id, "")
        else:
            log.warning(f"Main thread: Order {app.order_id} placement timed out!")

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

        log.info("Order Placement Test finished.")

    except Exception as e:
        log.exception("Unhandled exception in main:")
    finally:
        log.info("Exiting.")


if __name__ == "__main__":
    main()
