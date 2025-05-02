#!/usr/bin/env python3
# -*- coding: utf-8 -*-

# broker_order_market.py
# Reference implementation for placing a market order, waiting for fill,
# verifying position, placing a closing market order, and verifying closure.
# Based on historical_data.py structure.

import argparse
import datetime
import logging
import sys
import time
import threading
from decimal import Decimal # Use Decimal for quantities

from ibapi import wrapper
from ibapi.client import EClient
from ibapi.contract import Contract
from ibapi.order import Order
from ibapi.order_state import OrderState
from ibapi.execution import Execution
from ibapi.utils import iswrapper

# --- Constants ---
TARGET_SYMBOL = "SPY"
ORDER_QUANTITY = Decimal("1.0") # Use Decimal for precision
WAIT_TIMEOUT_SECONDS = 30 # Timeout for waiting for order status/fill
POSITION_CHECK_DELAY_SECONDS = 3 # Delay before checking positions after order status update
DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 4002
DEFAULT_CLIENT_ID = 102 # Use a different ID than gen_goldens live tests

# --- Logging Setup ---
log = logging.getLogger(__name__)
# Set level to INFO by default, can be overridden if needed
log.setLevel(logging.INFO)
handler = logging.StreamHandler(sys.stdout)
formatter = logging.Formatter('%(asctime)s - %(name)s - %(levelname)s - %(message)s')
handler.setFormatter(formatter)
log.addHandler(handler)


# --- Test Application Class ---
class MarketOrderTestApp(wrapper.EWrapper, EClient):
    def __init__(self):
        wrapper.EWrapper.__init__(self)
        EClient.__init__(self, wrapper=self)
        self.nKeybInt = 0
        self.started = False
        self.nextValidOrderId = -1 # Will be set by nextValidId callback
        self.done = False # Flag to signal completion or error

        # State management for market order test
        self.order_status = {} # key: orderId, value: status string
        self.order_filled = {} # key: orderId, value: boolean
        self.open_positions = {} # key: conId, value: dict {contract, quantity, average_cost}
        self.exec_details = {} # key: execId, value: Execution
        self.buy_order_id = None
        self.sell_order_id = None
        self.test_success = False # Track overall test success

        # Threading events for synchronization
        self.order_status_events = {} # key: orderId, value: threading.Event()
        self.position_end_event = threading.Event()
        self.exec_details_end_event = threading.Event()

        # Store errors keyed by reqId or orderId
        self._my_errors = {}


    @iswrapper
    def connectAck(self):
        log.info("Connection acknowledged.")
        # Note: StartAPI is implicitly handled by EClient's run loop

    @iswrapper
    def nextValidId(self, orderId: int):
        """Receives the next valid order ID and starts the test logic."""
        super().nextValidId(orderId)
        self.nextValidOrderId = orderId
        log.info("nextValidId: %d", orderId)
        # Start the main test logic after getting the first valid order ID
        self.start()


    def start(self):
        """Initiates the market order test sequence."""
        if self.started:
            return
        self.started = True
        log.info("Executing market order test sequence...")
        # Run the test logic in a separate thread to avoid blocking the EClient loop
        test_thread = threading.Thread(target=self.run_market_order_test_sequence)
        test_thread.daemon = True # Allow main thread to exit even if this fails badly
        test_thread.start()
        # The start method returns immediately; test runs in background.
        # The main loop in main() will wait for the EClient thread to finish.

    def keyboardInterrupt(self):
        self.nKeybInt += 1
        if self.nKeybInt == 1:
            log.warning("Keyboard interrupt detected. Signaling test to stop and disconnecting...")
            self.done = True # Signal test loop to stop if possible
            # Attempt cleanup if orders were placed
            if self.buy_order_id or self.sell_order_id:
                 log.warning("Attempting emergency cleanup due to interrupt...")
                 # Create contract again for cleanup attempt
                 contract = self.create_stock_contract(TARGET_SYMBOL)
                 self.attempt_cleanup(contract) # Best effort cleanup
            self.disconnect()
        else:
            log.info("Forcing exit...")
            sys.exit(0)

    @iswrapper
    def error(self, reqId: int, errorCode: int, errorString: str, advancedOrderReject=""):
        """Handles errors from TWS."""
        super().error(reqId, errorCode, errorString, advancedOrderReject)
        if advancedOrderReject:
            log.error("Error. Id: %d, Code: %d, Msg: %s, AdvancedOrderReject: %s", reqId, errorCode, errorString, advancedOrderReject)
        else:
            log.error("Error. Id: %d, Code: %d, Msg: %s", reqId, errorCode, errorString)

        # Store error keyed by ID (reqId or orderId)
        error_key = reqId if reqId != -1 else f"general_{errorCode}" # Use -1 for general errors
        self._my_errors[error_key] = (errorCode, errorString)

        # If error is related to an order, signal its event to unblock waiter
        if reqId > 0 and reqId in self.order_status_events:
            log.warning(f"Signaling order event for {reqId} due to error {errorCode}.")
            self.order_status[reqId] = "Error" # Mark status as Error
            self.order_status_events[reqId].set()

        # Specific connection errors might require disconnect
        connection_failure_codes = [502, 504, 509, 1100, 1300, 2103, 2105, 2110]
        if errorCode in connection_failure_codes:
            log.critical(f"Connection failure detected (Code: {errorCode}). Signaling done and disconnecting.")
            self.done = True
            self.disconnect()


    # --- Order and Position Callbacks ---
    @iswrapper
    def orderStatus(self, orderId: int, status: str, filled: Decimal,
                    remaining: Decimal, avgFillPrice: float, permId: int,
                    parentId: int, lastFillPrice: float, clientId: int,
                    whyHeld: str, mktCapPrice: float):
        """Receives order status updates."""
        super().orderStatus(orderId, status, filled, remaining, avgFillPrice, permId, parentId, lastFillPrice, clientId, whyHeld, mktCapPrice)
        log.info(f"OrderStatus - ID: {orderId}, Status: {status}, Filled: {filled}, Remaining: {remaining}, AvgPx: {avgFillPrice}, LastPx: {lastFillPrice}")
        self.order_status[orderId] = status
        if status == "Filled":
            self.order_filled[orderId] = True
        # Signal any waiting thread for this specific order
        if orderId in self.order_status_events:
            log.debug(f"Signaling order event for {orderId} due to status update: {status}")
            self.order_status_events[orderId].set()

    @iswrapper
    def openOrder(self, orderId: int, contract: Contract, order: Order,
                  orderState: OrderState):
        """Receives open order details."""
        super().openOrder(orderId, contract, order, orderState)
        log.info(f"OpenOrder - ID: {orderId}, Symbol: {contract.symbol}, Status: {orderState.status}")
        # Update status if needed, especially for initial state or if status changed
        if orderId not in self.order_status or self.order_status.get(orderId) != orderState.status:
             log.info(f"Updating order {orderId} status from OpenOrder: {orderState.status}")
             self.order_status[orderId] = orderState.status
             if orderState.status == "Filled":
                 self.order_filled[orderId] = True
             # Signal any waiting thread
             if orderId in self.order_status_events:
                 log.debug(f"Signaling order event for {orderId} due to OpenOrder update: {orderState.status}")
                 self.order_status_events[orderId].set()

    @iswrapper
    def position(self, account: str, contract: Contract, position: Decimal, avgCost: float):
        """Receives position information."""
        super().position(account, contract, position, avgCost)
        log.info(f"Position - Account: {account}, Symbol: {contract.symbol}, ConId: {contract.conId}, Qty: {position}, AvgCost: {avgCost}")
        # Store position data, keyed by conId for easy lookup
        self.open_positions[contract.conId] = {
            "contract": contract,
            "quantity": position,
            "average_cost": avgCost
        }

    @iswrapper
    def positionEnd(self):
        """Indicates the end of position stream."""
        super().positionEnd()
        log.info("PositionEnd received.")
        self.position_end_event.set() # Signal end of positions

    @iswrapper
    def execDetails(self, reqId: int, contract: Contract, execution: Execution):
        """Receives execution details."""
        super().execDetails(reqId, contract, execution)
        log.info(f"ExecDetails - ReqId: {reqId}, ExecId: {execution.execId}, OrderId: {execution.orderId}, Symbol: {contract.symbol}, Side: {execution.side}, Qty: {execution.shares}, Px: {execution.price}")
        self.exec_details[execution.execId] = execution

    @iswrapper
    def execDetailsEnd(self, reqId: int):
        """Indicates the end of execution details stream."""
        super().execDetailsEnd(reqId)
        log.info(f"ExecDetailsEnd - ReqId: {reqId}")
        self.exec_details_end_event.set()


    # --- Helper methods ---
    def get_next_order_id(self):
        """Gets and increments the next valid order ID."""
        if self.nextValidOrderId == -1:
            # This should not happen if start() is called after nextValidId callback
            log.error("Attempted to get next order ID before it was initialized.")
            raise ConnectionError("Next valid order ID not received yet.")
        order_id = self.nextValidOrderId
        self.nextValidOrderId += 1
        return order_id

    def create_stock_contract(self, symbol):
        """Creates a simple stock contract."""
        contract = Contract()
        contract.symbol = symbol
        contract.secType = "STK"
        contract.currency = "USD"
        contract.exchange = "SMART"
        # Optional: Specify Primary Exchange if needed, e.g., for ARCA:
        # contract.primaryExchange = "ARCA"
        return contract

    def create_market_order(self, action, quantity):
        """Creates a simple market order."""
        order = Order()
        order.action = action # "BUY" or "SELL"
        order.orderType = "MKT"
        order.totalQuantity = quantity # Use Decimal
        order.transmit = True # Transmit immediately
        return order

    def wait_for_order_fill(self, order_id):
        """Waits for a specific order to reach 'Filled' status."""
        log.info(f"Waiting up to {WAIT_TIMEOUT_SECONDS}s for order {order_id} to fill...")
        if order_id not in self.order_status_events:
            self.order_status_events[order_id] = threading.Event()
        else:
            self.order_status_events[order_id].clear() # Ensure event is clear before wait

        start_time = time.time()
        while time.time() - start_time < WAIT_TIMEOUT_SECONDS:
            if self.done: # Check if keyboard interrupt occurred
                 log.warning(f"Wait for order {order_id} interrupted.")
                 return False

            # Check status before waiting
            current_status = self.order_status.get(order_id)
            if current_status == "Filled":
                log.info(f"Order {order_id} confirmed Filled.")
                return True
            elif current_status in ["Cancelled", "ApiCancelled", "Inactive", "Error"]:
                log.error(f"Order {order_id} reached terminal state '{current_status}' before filling.")
                return False

            # Wait for event with timeout
            log.debug(f"Waiting on event for order {order_id}...")
            signaled = self.order_status_events[order_id].wait(timeout=1) # Wait 1s, then re-check loop condition
            if signaled:
                log.debug(f"Event signaled for order {order_id}. Re-checking status.")
                self.order_status_events[order_id].clear() # Reset event after processing signal
                # Loop will re-check status immediately
            else:
                 log.debug(f"Timeout waiting for event signal for order {order_id}. Loop continues.")

        # If loop finishes without returning, it timed out
        log.error(f"Timeout waiting for order {order_id} to fill. Last status: {self.order_status.get(order_id)}")
        return False

    def check_position(self, contract, expected_quantity):
        """Requests positions and checks if the target contract has the expected quantity."""
        if self.done: return False # Check interrupt flag
        log.info(f"Requesting positions to verify {contract.symbol} quantity (expected: {expected_quantity})...")
        self.open_positions.clear() # Clear previous position data
        self.position_end_event.clear()
        self.reqPositions() # Request position updates

        # Wait for positionEnd signal
        log.info("Waiting for PositionEnd signal...")
        if not self.position_end_event.wait(timeout=10):
            log.error("Timeout waiting for PositionEnd signal.")
            # Attempt cancellation of position subscription (best effort)
            try:
                self.cancelPositions()
            except Exception as e:
                log.warning(f"Error cancelling position subscription: {e}")
            return False # Cannot verify position

        if self.done: return False # Check interrupt flag again after wait

        log.info("Position data received.")
        # Check the specific position
        position_data = self.open_positions.get(contract.conId)

        if expected_quantity == 0:
            if position_data is None or position_data["quantity"] == 0:
                log.info(f"Verified: Position for {contract.symbol} is closed (Qty: {position_data['quantity'] if position_data else 0}).")
                return True
            else:
                log.error(f"Verification FAILED: Position for {contract.symbol} not closed. Expected Qty: 0, Actual Qty: {position_data['quantity']}")
                return False
        else: # Expected non-zero quantity
            if position_data and position_data["quantity"] == expected_quantity:
                log.info(f"Verified: Position for {contract.symbol}. Expected Qty: {expected_quantity}, Actual Qty: {position_data['quantity']}")
                return True
            else:
                actual_qty_str = str(position_data['quantity']) if position_data else 'Not Found'
                log.error(f"Verification FAILED: Position check for {contract.symbol}. Expected Qty: {expected_quantity}, Actual Qty: {actual_qty_str}")
                return False

    def request_day_executions(self):
        """Requests and logs execution details for the current session."""
        if self.done: return # Check interrupt flag
        log.info("--- Requesting Today's Executions ---")
        try:
            req_id = 9001 # Arbitrary ID for execution request
            self.exec_details.clear()
            self.exec_details_end_event.clear()
            # Default filter requests executions for this client ID only
            self.reqExecutions(req_id, {}) # Empty ExecutionFilter() uses defaults

            log.info("Waiting for ExecDetailsEnd signal...")
            if not self.exec_details_end_event.wait(timeout=10):
                log.warning("Timeout waiting for ExecDetailsEnd.")
                return

            if self.done: return # Check interrupt flag again

            log.info(f"Received {len(self.exec_details)} execution details:")
            # Sort by time for better readability
            try:
                # Ensure exec.time is comparable (it should be a string like 'YYYYMMDD  HH:MM:SS')
                sorted_execs = sorted(self.exec_details.values(), key=lambda exec: exec.time)
            except Exception as e:
                log.error(f"Could not sort executions by time: {e}. Printing unsorted.")
                sorted_execs = self.exec_details.values()

            for exec_detail in sorted_execs:
                 log.info(f"  ExecId: {exec_detail.execId}, OrderId: {exec_detail.orderId}, Time: {exec_detail.time}, Symbol: {exec_detail.contract.symbol}, Side: {exec_detail.side}, Qty: {exec_detail.shares}, Px: {exec_detail.price}, Comm: {exec_detail.commission}")

        except Exception as e:
            log.error(f"Error requesting or processing executions: {e}")

    def attempt_cleanup(self, contract):
        """Attempts to place a closing market order if a position exists. Best effort."""
        # This runs potentially during disconnect, so avoid long waits
        log.warning("--- Attempting Emergency Cleanup ---")
        if not self.isConnected():
             log.warning("Cleanup skipped: Not connected.")
             return
        try:
            # Request positions quickly - might not get full response during shutdown
            log.info("Cleanup: Requesting current positions...")
            self.open_positions.clear()
            self.position_end_event.clear()
            self.reqPositions()
            # Short wait for potential position data
            self.position_end_event.wait(timeout=2)

            position_data = self.open_positions.get(contract.conId)
            if position_data and position_data["quantity"] != 0:
                qty_to_close = abs(position_data["quantity"])
                action = "SELL" if position_data["quantity"] > 0 else "BUY"
                log.warning(f"Cleanup: Found position {position_data['quantity']} {contract.symbol}. Placing {action} MKT order for {qty_to_close}.")
                cleanup_order = self.create_market_order(action, qty_to_close)
                # Get next ID carefully - might fail if disconnected
                try:
                    cleanup_order_id = self.get_next_order_id()
                    self.placeOrder(cleanup_order_id, contract, cleanup_order)
                    log.warning(f"Cleanup order {cleanup_order_id} placed. Monitor manually.")
                except Exception as id_err:
                    log.error(f"Cleanup: Failed to get order ID or place order: {id_err}")
            else:
                log.info("Cleanup: No open position found for {} or quantity is zero.".format(contract.symbol))

            # Cancel position subscription if active
            try:
                self.cancelPositions()
            except Exception as e:
                log.warning(f"Cleanup: Error cancelling position subscription: {e}")

        except Exception as e:
            log.error(f"Error during cleanup attempt: {e}")


    # --- Main Test Logic ---
    def run_market_order_test_sequence(self):
        """Executes the buy, verify, sell, verify sequence. Runs in a separate thread."""
        log.info("Market order test thread started.")
        self.test_success = False # Assume failure until success
        contract = None # Define contract in outer scope for cleanup

        try:
            # Ensure we have a valid order ID before proceeding
            if self.nextValidOrderId == -1:
                 log.error("Test sequence cannot start: nextValidOrderId not set.")
                 self.done = True
                 return # Exit thread

            log.warning("Ensure market is open and liquid for SPY stock for this test.")
            log.warning(f"This test will BUY {ORDER_QUANTITY} share(s) of {TARGET_SYMBOL} and then SELL it.")
            log.info("Starting test sequence in 5 seconds...")
            time.sleep(5)
            if self.done: return # Check interrupt

            contract = self.create_stock_contract(TARGET_SYMBOL)

            # --- BUY Order ---
            log.info(f"--- Placing BUY MKT order for {ORDER_QUANTITY} {TARGET_SYMBOL} ---")
            buy_order = self.create_market_order("BUY", ORDER_QUANTITY)
            self.buy_order_id = self.get_next_order_id()
            log.info(f"Placing BUY order with ID: {self.buy_order_id}")
            self.placeOrder(self.buy_order_id, contract, buy_order)

            if not self.wait_for_order_fill(self.buy_order_id):
                log.error("BUY order did not fill. Test failed.")
                self.attempt_cleanup(contract) # Attempt cleanup
                self.done = True
                return # Exit thread
            if self.done: return # Check interrupt

            log.info("BUY order filled.")
            log.info(f"Waiting {POSITION_CHECK_DELAY_SECONDS}s before position check...")
            time.sleep(POSITION_CHECK_DELAY_SECONDS)
            if self.done: return # Check interrupt

            # --- Verify Position after BUY ---
            if not self.check_position(contract, ORDER_QUANTITY):
                log.error("Position verification failed after BUY. Test failed.")
                self.attempt_cleanup(contract) # Attempt cleanup
                self.done = True
                return # Exit thread
            if self.done: return # Check interrupt

            log.info("Position verified after BUY.")

            # --- SELL Order ---
            log.info(f"--- Placing SELL MKT order for {ORDER_QUANTITY} {TARGET_SYMBOL} ---")
            sell_order = self.create_market_order("SELL", ORDER_QUANTITY)
            self.sell_order_id = self.get_next_order_id()
            log.info(f"Placing SELL order with ID: {self.sell_order_id}")
            self.placeOrder(self.sell_order_id, contract, sell_order)

            if not self.wait_for_order_fill(self.sell_order_id):
                log.error("SELL order did not fill. Manual intervention likely required. Test failed.")
                # Don't return immediately, check final position state below
            else:
                log.info("SELL order filled.")
            if self.done: return # Check interrupt

            log.info(f"Waiting {POSITION_CHECK_DELAY_SECONDS}s before final position check...")
            time.sleep(POSITION_CHECK_DELAY_SECONDS)
            if self.done: return # Check interrupt

            # --- Verify Position after SELL ---
            if not self.check_position(contract, 0):
                log.error("Position verification failed after SELL. Position may still be open! Test failed.")
                # No cleanup attempt here as the goal was to close it. Manual check needed.
                self.done = True
                return # Exit thread
            if self.done: return # Check interrupt

            log.info("Position verified closed after SELL.")
            log.info("--- Market Order Test Completed Successfully ---")
            self.test_success = True # Mark success

        except ConnectionError as ce:
             log.error(f"Connection error during test sequence: {ce}")
        except Exception as e:
            log.exception(f"An unexpected error occurred during the test sequence: {e}")
            # Attempt cleanup if contract was defined
            if contract:
                 log.warning("Attempting emergency cleanup due to error...")
                 self.attempt_cleanup(contract)
        finally:
            # Request executions regardless of success/failure, if not interrupted
            if not self.done:
                self.request_day_executions()

            log.info("Market order test thread finished.")
            self.done = True # Signal that the test sequence is complete
            # Disconnect is handled by the main thread after run() finishes
            # Nope. Exit by hand.
            sys.exit()


# --- Main Execution ---
def main():
    parser = argparse.ArgumentParser(description="IB API Market Order Test")
    parser.add_argument("--host", default=DEFAULT_HOST, help="Host address")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help="Port number")
    parser.add_argument("--clientId", type=int, default=DEFAULT_CLIENT_ID, help="Client ID")
    args = parser.parse_args()

    log.info("Starting Market Order Test")
    log.info(f"Connecting to {args.host}:{args.port} with clientId {args.clientId}")

    app = MarketOrderTestApp()
    exit_code = 1 # Default to failure

    try:
        # Connect to TWS/Gateway
        app.connect(args.host, args.port, args.clientId)
        log.info("Connection initiated. Server version: %s", app.serverVersion())

        # Start the EClient message loop in a background thread
        log.info("Starting EClient.run() message loop in background thread...")
        thread = threading.Thread(target=app.run, daemon=True)
        thread.start()
        log.info("EClient.run() thread started.")

        # Wait for the EClient thread to finish.
        # The test logic runs in a separate thread started by app.start(),
        # and signals completion by setting app.done = True.
        # The EClient thread (app.run) will exit when app.disconnect() is called
        # or if a fatal error occurs.
        log.info("Main thread waiting for EClient thread to complete...")
        thread.join() # Wait indefinitely for the EClient thread
        log.info("EClient thread has finished.")

        # Check test result
        if app.test_success:
             log.info("Overall Test Result: PASSED")
             exit_code = 0
        else:
             log.error("Overall Test Result: FAILED")
             # Log any captured errors
             if app._my_errors:
                  log.error("Captured errors during test:")
                  for key, (code, msg) in app._my_errors.items():
                       log.error(f"  ID {key}: Code={code}, Msg={msg}")

    except Exception as e:
        log.exception("Unhandled exception in main:")
    except KeyboardInterrupt:
        log.warning("Main thread caught KeyboardInterrupt.")
        # app.keyboardInterrupt() should have been called in the EClient thread
        # if the interrupt happened while connected.
    finally:
        # Ensure disconnect is attempted if app still thinks it's connected
        if app.isConnected():
            log.warning("Disconnecting from main thread finally block (should have disconnected earlier).")
            app.disconnect()
        log.info("Exiting.")
        sys.exit(exit_code)


if __name__ == "__main__":
    main()
