#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import argparse
import logging
import sys
import threading

from ibapi import wrapper
from ibapi.client import EClient
from ibapi.contract import Contract
from ibapi.utils import iswrapper

# --- Constants ---
DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 4002
DEFAULT_CLIENT_ID = 103  # Use a different ID than other tests

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
        self.reqId2nErr = {}
        self._my_errors = {}

        # For market data to get exchange mapping
        self.mkt_data_req_id = -1
        self.exchange_mapping_code = None
        self.exchange_mapping_received = threading.Event()

        # For SMART components
        self.smart_components_req_id = -1
        self.smart_components_map = {}
        self.smart_components_finished = threading.Event()

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
        log.info("Executing SMART components test workflow")
        self.request_market_data_for_exchange_mapping()

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

        # Handle errors for both requests
        if reqId == self.mkt_data_req_id:
            log.error(f"Market data request {reqId} failed with error {errorCode}: {errorString}")
            self.exchange_mapping_received.set()  # Signal completion on error
        elif reqId == self.smart_components_req_id:
            log.error(f"SMART components request {reqId} failed with error {errorCode}: {errorString}")
            self.smart_components_finished.set()  # Signal completion on error

        # Store errors keyed by reqId
        if reqId > 0:
            self.reqId2nErr[reqId] = self.reqId2nErr.get(reqId, 0) + 1
            self._my_errors[reqId] = (errorCode, errorString)

    # --- Market Data Callbacks (to get exchange mapping) ---
    @iswrapper
    def tickReqParams(self, tickerId: int, minTick: float, bboExchange: str, snapshotPermissions: int):
        """
        Callback that provides the exchange mapping code (bboExchange parameter).
        This is what we need to pass to reqSmartComponents.
        """
        log.info("CALLBACK tickReqParams: TickerID: %d, BboExchange: '%s'", tickerId, bboExchange)

        if tickerId == self.mkt_data_req_id:
            self.exchange_mapping_code = bboExchange
            log.info(f"Received exchange mapping code: '{bboExchange}' for market data request {tickerId}")

            # Cancel the market data request since we only needed the mapping code
            self.cancelMktData(self.mkt_data_req_id)

            # Signal that we have the mapping code
            self.exchange_mapping_received.set()

    @iswrapper
    def tickPrice(self, reqId: int, tickType: int, price: float, attrib):
        # We don't need the actual price data, just the exchange mapping
        pass

    @iswrapper
    def tickSize(self, reqId: int, tickType: int, size: int):
        # We don't need the actual size data, just the exchange mapping
        pass

    # --- SMART Components Callbacks ---
    @iswrapper
    def smartComponents(self, reqId: int, theMap):
        """
        Callback for SMART components response.
        theMap is a dictionary where:
        - key: bit number (int)
        - value: SmartComponent object with exchange and exchangeLetter attributes
        """
        log.info("CALLBACK smartComponents: ReqId: %d", reqId)
        log.debug("SmartComponents. ReqId: %d, Components received: %d", reqId, len(theMap))

        if reqId == self.smart_components_req_id:
            self.smart_components_map = {}
            for bit_number, smart_component in theMap.items():
                self.smart_components_map[bit_number] = {
                    'exchange': smart_component.exchange,
                    'exchange_letter': smart_component.exchangeLetter
                }
                log.debug("  Bit %d: Exchange='%s', Letter='%s'",
                         bit_number, smart_component.exchange, smart_component.exchangeLetter)

            log.info("SMART components received for request %d. Setting completion event.", reqId)
            self.smart_components_finished.set()  # Signal completion

    # --- Test Logic ---
    def request_market_data_for_exchange_mapping(self):
        """
        Step 1: Request market data for a common stock to get the exchange mapping code
        """
        self.mkt_data_req_id = self.nextValidOrderId
        self.nextValidOrderId += 1

        log.info(f"Step 1: Requesting market data to get exchange mapping code (reqId: {self.mkt_data_req_id})")

        # Create a contract for a common stock
        contract = Contract()
        contract.symbol = "AAPL"
        contract.secType = "STK"
        contract.exchange = "SMART"
        contract.currency = "USD"

        # Request market data - this will trigger tickReqParams with the exchange mapping code
        self.reqMktData(self.mkt_data_req_id, contract, "", False, False, [])

        log.info(f"Market data request sent. Waiting for tickReqParams callback...")

    def request_smart_components_with_mapping(self):
        """
        Step 2: Use the exchange mapping code to request SMART components
        """
        if not self.exchange_mapping_code:
            log.error("No exchange mapping code available for SMART components request")
            return

        self.smart_components_req_id = self.nextValidOrderId
        self.nextValidOrderId += 1

        log.info(f"Step 2: Requesting SMART components with mapping code '{self.exchange_mapping_code}' (reqId: {self.smart_components_req_id})")

        # Request SMART components using the exchange mapping code
        self.reqSmartComponents(self.smart_components_req_id, self.exchange_mapping_code)

        log.info(f"SMART components request sent for mapping code '{self.exchange_mapping_code}'")


# --- Main Execution ---
def main():
    parser = argparse.ArgumentParser(description="IB API SMART Components Test")
    parser.add_argument("--host", default=DEFAULT_HOST, help="Host address")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help="Port number")
    parser.add_argument("--clientId", type=int, default=DEFAULT_CLIENT_ID, help="Client ID")
    args = parser.parse_args()

    log.info("Starting SMART Components Test")
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

        # --- Step 1: Wait for exchange mapping code ---
        mapping_timeout_secs = 10
        log.info(f"Main thread waiting up to {mapping_timeout_secs}s for exchange mapping code...")
        mapping_received = app.exchange_mapping_received.wait(timeout=mapping_timeout_secs)

        if not mapping_received:
            log.error("Failed to receive exchange mapping code within timeout")
            return

        if not app.exchange_mapping_code:
            log.error("Exchange mapping code is empty or None")
            return

        log.info(f"Successfully received exchange mapping code: '{app.exchange_mapping_code}'")

        # --- Step 2: Request SMART components with the mapping code ---
        app.request_smart_components_with_mapping()

        # --- Step 3: Wait for SMART components response ---
        components_timeout_secs = 20
        log.info(f"Main thread waiting up to {components_timeout_secs}s for SMART components response...")
        components_received = app.smart_components_finished.wait(timeout=components_timeout_secs)

        if components_received:
            log.info(f"Main thread: SMART components request finished (response received).")
            # Log results
            components_count = len(app.smart_components_map)
            log.info(f"Main thread: Received {components_count} SMART components for mapping '{app.exchange_mapping_code}'.")

            if app.smart_components_map:
                log.info("SMART Components Details:")
                # Sort by bit number for consistent output
                sorted_components = sorted(app.smart_components_map.items())

                # Show first 10 components to avoid overwhelming output
                for i, (bit_number, component_info) in enumerate(sorted_components[:10]):
                    log.info(f"  Bit {bit_number}: Exchange='{component_info['exchange']}', Letter='{component_info['exchange_letter']}'")

                if components_count > 10:
                    log.info(f"  ... and {components_count - 10} more components")

                # Show summary statistics
                exchanges = set(comp['exchange'] for comp in app.smart_components_map.values())
                log.info(f"  Total unique exchanges: {len(exchanges)}")
                log.info(f"  Exchange names: {sorted(exchanges)}")
            else:
                log.warning("No SMART components received (empty response)")
        else:
            log.warning(f"Main thread: SMART components request timed out!")

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

        log.info("SMART Components Test finished.")

    except Exception as e:
        log.exception("Unhandled exception in main:")
    finally:
        log.info("Exiting.")


if __name__ == "__main__":
    main()
