# Connects to TWS/Gateway and requests streaming market data for a symbol.
# Prints received ticks for a short duration, then cancels and disconnects.

from ibapi.client import EClient
from ibapi.wrapper import EWrapper
from ibapi.contract import Contract
from ibapi.ticktype import TickType, TickTypeEnum # For easy tick type lookup

import threading
import time

class IBapi(EWrapper, EClient):
    def __init__(self):
        EClient.__init__(self, self)
        self.next_req_id = 1 # Keep track of the next request ID

    def error(self, reqId, errorCode, errorString, advancedOrderReject=""):
        # Overriden method
        if reqId > 0:
            print(f"Error. Id: {reqId}, Code: {errorCode}, Msg: {errorString}, Advanced Order Reject: {advancedOrderReject}")
        else:
            # Usually connection-related messages
            print(f"System Error. Code: {errorCode}, Msg: {errorString}")

    def nextValidId(self, orderId: int):
        # Overriden method
        super().nextValidId(orderId)
        self.next_req_id = orderId # Store the next valid ID received at connection
        print(f"Connection acknowledged. Next valid ID: {self.next_req_id}")
        # You could start requests here if needed after connection confirmation

    def tickPrice(self, reqId, tickType, price, attrib):
        # Overriden method - Called when price data updates
        tick_name = TickTypeEnum.idx2name.get(tickType, f"UnknownTick({tickType})")
        print(f"Tick Price. ReqId: {reqId}, Type: {tick_name}({tickType}), Price: {price}, Attr: {attrib}")

    def tickSize(self, reqId, tickType, size):
        # Overriden method - Called when size data updates
        tick_name = TickTypeEnum.idx2name.get(tickType, f"UnknownTick({tickType})")
        print(f"Tick Size. ReqId: {reqId}, Type: {tick_name}({tickType}), Size: {size}")

    # Add other tick methods if needed (tickString, tickGeneric, etc.)
    # def tickString(self, reqId, tickType, value):
    #     tick_name = TickTypeEnum.idx2name.get(tickType, f"UnknownTick({tickType})")
    #     print(f"Tick String. ReqId: {reqId}, Type: {tick_name}({tickType}), Value: {value}")

    # def tickGeneric(self, reqId, tickType, value):
    #     tick_name = TickTypeEnum.idx2name.get(tickType, f"UnknownTick({tickType})")
    #     print(f"Tick Generic. ReqId: {reqId}, Type: {tick_name}({tickType}), Value: {value}")


def run_loop(app_instance):
    print("Starting IB API message loop...")
    app_instance.run()
    print("IB API message loop finished.")

# --- Main Execution ---
if __name__ == "__main__":
    app = IBapi()

    # Connection details (match your TWS/Gateway settings)
    host = '127.0.0.1'
    port = 3002 # Default paper trading port
    client_id = 102 # Choose a unique client ID

    print(f"Connecting to {host}:{port} with Client ID: {client_id}...")
    app.connect(host, port, client_id)

    # Start the socket processing in a separate thread
    api_thread = threading.Thread(target=run_loop, args=(app,), daemon=True)
    api_thread.start()

    # Wait for connection confirmation (nextValidId callback)
    # A more robust way would use an event or check app.isConnected()
    print("Waiting for connection confirmation...")
    time.sleep(2) # Simple wait, adjust as needed

    if not app.isConnected():
        print("Connection failed!")
        exit()

    # --- Define Contract ---
    contract = Contract()
    contract.symbol = "SPY"  # Example symbol
    contract.secType = "STK"
    contract.exchange = "SMART"
    contract.currency = "USD"

    # --- Request Market Data ---
    req_id = app.next_req_id # Use the next valid ID
    print(f"\nRequesting streaming market data for {contract.symbol} with Req ID: {req_id}...")
    # Generic Tick Types: Empty string requests a default set (including Bid/Ask/Last/Volume etc.)
    # Or specify like "100,101,104,106,165,221,225,233,236,258,293,294,295,318"
    # See: https://interactivebrokers.github.io/tws-api/tick_types.html
    generic_tick_list = ""
    snapshot = False # False for streaming data
    regulatory_snapshot = False # Set to True for regulatory snapshot (requires subscription)
    mkt_data_options = [] # Optional parameters

    app.reqMktData(req_id, contract, generic_tick_list, snapshot, regulatory_snapshot, mkt_data_options)

    # --- Wait for Data ---
    stream_duration = 15 # Seconds to stream data
    print(f"\nStreaming data for {stream_duration} seconds...")
    time.sleep(stream_duration)

    # --- Cancel Market Data ---
    print(f"\nCancelling market data request (Req ID: {req_id})...")
    app.cancelMktData(req_id)
    time.sleep(1) # Allow time for cancellation message processing

    # --- Disconnect ---
    print("Disconnecting...")
    app.disconnect()

    # Wait for the thread to potentially finish logging disconnection messages
    api_thread.join(timeout=2)
    print("Program finished.")
