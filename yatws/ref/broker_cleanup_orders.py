#!/usr/bin/env python3

import time
import logging

from ibapi.client import EClient
from ibapi.wrapper import EWrapper
from ibapi.contract import Contract
from ibapi.order import Order
from ibapi.utils import iswrapper #just for decorator


# Setup logging
logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(name)s - %(levelname)s - %(message)s')
log = logging.getLogger("cancel_orders_and_summary")


class TestApp(EWrapper, EClient):
    def __init__(self):
        EClient.__init__(self, self)
        self.nextOrderId = None
        self.open_orders_received = False
        self.account_summary_requested = False

    @iswrapper
    def error(self, reqId, errorCode, errorString, advancedOrderRejectJson=""):
        # Ignore informational messages unless they are connection related
        if reqId == -1 and errorCode in [2104, 2106, 2158]: # Market data farm connection messages
             log.info(f"Info: {errorCode} - {errorString}")
        elif errorCode == 1100: # Connectivity between IB and TWS has been lost
             log.error(f"Connectivity Lost: {errorCode} - {errorString}")
        elif errorCode == 1101 or errorCode == 1102: # Connectivity restored
             log.info(f"Connectivity Restored: {errorCode} - {errorString}")
        elif errorCode == 2105: # HMDS data farm connection inactive but should be available
             log.warning(f"Warning: {errorCode} - {errorString}")
        elif errorCode == 1300: # Socket connection has been dropped
             log.error(f"Socket Dropped: {errorCode} - {errorString}")
        elif errorCode == 502: # Couldn't connect to TWS
             log.error(f"Connection Error: {errorCode} - {errorString}")
             # Potentially trigger a disconnect/exit here if needed
        elif errorCode == 504: # Not connected
             log.warning(f"Not Connected: {errorCode} - {errorString}")
             # Potentially trigger a disconnect/exit here if needed
        elif errorCode == 202: # Order cancelled
             log.info(f"Order Cancelled Confirmation: {errorCode} - {errorString}")
        else:
             log.error(f"Error. Id: {reqId}, Code: {errorCode}, Msg: {errorString}, Advanced Order Reject:
{advancedOrderRejectJson}")


    @iswrapper
    def nextValidId(self, orderId: int):
        super().nextValidId(orderId)
        self.nextOrderId = orderId
        log.info(f"nextValidId: {orderId}")
        # Request open orders once we have a valid ID
        log.info("Requesting open orders...")
        self.reqOpenOrders()

    @iswrapper
    def openOrder(self, orderId, contract: Contract, order: Order, orderState):
        super().openOrder(orderId, contract, order, orderState)
        log.info(f"OpenOrder. PermId: {order.permId}, ClientId: {order.clientId}, OrderId: {orderId}, "
                 f"Account: {order.account}, Symbol: {contract.symbol}, SecType: {contract.secType}, "
                 f"Exchange: {contract.exchange}, Action: {order.action}, OrderType: {order.orderType}, "
                 f"TotalQty: {order.totalQuantity}, CashQty: {order.cashQty}, LmtPrice: {order.lmtPrice}, "
                 f"AuxPrice: {order.auxPrice}, Status: {orderState.status}")

        # Cancel the order
        log.info(f"Cancelling order {orderId}...")
        self.cancelOrder(orderId, "") # Pass empty manual order cancel time

    @iswrapper
    def openOrderEnd(self):
        super().openOrderEnd()
        log.info("OpenOrderEnd received.")
        self.open_orders_received = True
        # Now request account summary
        if not self.account_summary_requested:
            log.info("Requesting account summary...")
            self.account_summary_requested = True
            # Use a unique reqId for the summary request
            summary_req_id = self.nextOrderId
            self.nextOrderId += 1 # Increment for potential future use
            # Request all summary tags for all accounts
            self.reqAccountSummary(summary_req_id, "All", "$LEDGER") # Using $LEDGER as an example tag set

    @iswrapper
    def orderStatus(self, orderId, status, filled, remaining, avgFillPrice, permId, parentId, lastFillPrice, clientId, whyHeld,
mktCapPrice):
        super().orderStatus(orderId, status, filled, remaining, avgFillPrice, permId, parentId, lastFillPrice, clientId, whyHeld,
mktCapPrice)
        log.info(f"OrderStatus. Id: {orderId}, Status: {status}, Filled: {filled}, Remaining: {remaining}, "
                 f"AvgFillPrice: {avgFillPrice}, PermId: {permId}, ParentId: {parentId}, LastFillPrice: {lastFillPrice}, "
                 f"ClientId: {clientId}, WhyHeld: {whyHeld}, MktCapPrice: {mktCapPrice}")

    @iswrapper
    def accountSummary(self, reqId: int, account: str, tag: str, value: str, currency: str):
        super().accountSummary(reqId, account, tag, value, currency)
        log.info(f"AccountSummary. ReqId: {reqId}, Account: {account}, Tag: {tag}, Value: {value}, Currency: {currency}")

    @iswrapper
    def accountSummaryEnd(self, reqId: int):
        super().accountSummaryEnd(reqId)
        log.info(f"AccountSummaryEnd. ReqId: {reqId}")
        log.info("Disconnecting...")
        self.disconnect()


def main():
    app = TestApp()
    # Connect to TWS or Gateway. Default port 7497 for TWS, 7496 for Paper TWS. Gateway default 4001, Paper Gateway 4002.
    # Use 127.0.0.1 for local connection.
    # clientId=0 is standard for manual connection testing.
    app.connect("127.0.0.1", 7497, clientId=0)
    log.info(f"IB TWS connection state: {app.isConnected()}")

    # Start the message loop in a separate thread
    app.run()

    # Keep the main thread alive until disconnected
    # Or implement a more robust shutdown mechanism
    # For this example, we rely on accountSummaryEnd to disconnect.
    # A timeout could be added here for safety.
    # while app.isConnected():
    #     time.sleep(1)

    log.info("Exiting application.")


if __name__ == "__main__":
    main()
