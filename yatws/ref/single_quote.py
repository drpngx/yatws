from ibapi.client import EClient
from ibapi.wrapper import EWrapper
from ibapi.contract import Contract
import threading
import time

class IBapi(EWrapper, EClient):
  def __init__(self):
    EClient.__init__(self, self)
    self.data = {}  # Store market data here

  def tickPrice(self, reqId, tickType, price, attrib):
    # Called when price data updates
    self.data[reqId] = self.data.get(reqId, {})

    # IB API tick types
    # 1 = Bid, 2 = Ask, 4 = Last, 6 = High, 7 = Low, 9 = Close
    if tickType == 1:
      self.data[reqId]['bid'] = price
    elif tickType == 2:
      self.data[reqId]['ask'] = price
    elif tickType == 4:
      self.data[reqId]['last'] = price

    # Print the current data we have
    print(f"Stock Data Update for {reqId}: {self.data[reqId]}")

def run_loop():
  app.run()

# Create an instance of the app
app = IBapi()
app.connect('127.0.0.1', 3002, 0)

# Start the socket in a thread
api_thread = threading.Thread(target=run_loop, daemon=True)
api_thread.start()

# Allow time for connection to server
time.sleep(1)

# Create a contract for the stock
def create_stock_contract(symbol):
  contract = Contract()
  contract.symbol = symbol
  contract.secType = 'STK'
  contract.exchange = 'SMART'
  contract.currency = 'USD'
  return contract

# Request market data
def request_market_data(symbol, req_id):
  contract = create_stock_contract(symbol)
  # False = no snapshot, empty string list = everything, False = regulatorySnapshot
  # [] = Specify only specific tick types like: [1, 2, 4] for bid/ask/last
  app.reqMktData(req_id, contract, '', False, False, [])

# Example usage
symbol = 'AAPL'  # Replace with your stock symbol
request_market_data(symbol, 1)

# Keep the program running to receive data
try:
  while True:
    time.sleep(0.5)
except KeyboardInterrupt:
  print("Canceling market data request...")
  app.cancelMktData(1)
  app.disconnect()
  print("Disconnected")
