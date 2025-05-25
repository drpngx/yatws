"""Parse a message manually.

You can copy the message from the debug output.

For debugging, go directly in the source and print debug statements from there.
Source: `IBJts/source/pythonclient/ibapi/decoder.py`
"""

from ibapi.decoder import Decoder
from ibapi.wrapper import EWrapper


class Fields:
  def __init__(self, msg: str):
    # We strip the last field because the message uses \0 as terminator,
    # not separator: there is a trailing one.
    self._fields = [x.encode('utf-8') for x in msg.split('·')][:-1]
    self._current = 0

  def __iter__(self):
    return self

  def __next__(self):
    if self._current >= len(self._fields):
      raise StopIteration
    value = self._fields[self._current]
    self._current += 1
    return value

  @property
  def index(self):
    return self._current

  @property
  def length(self):
    return len(self._fields)

  def safe_peek(self, idx = None):
    if idx is None:
      idx = self._current
    if idx < 0:
      idx = len(self._fields) + idx
    if self._current < len(self._current):
      return self._fields[idx]
    return None


def DecodeContractData():
  dec = Decoder(EWrapper(), 187)

  msg = "10·6·ES·FUT·20300621 08:30:00 US/Central·20300621·0··QBALGO·USD·ESM0·ES·ES·770561177·0.25·50·AD,ALERT,ALLOC,AVGCOST,BASKET,BENCHPX,DAY,DEACT,DEACTDIS,LMT,MKT,NGCOMB,OPENCLOSE,WHATIF·CME,QBALGO·1·11004968·E-mini S&P 500··203006····US/Central·20250503:CLOSED;20250504:1700-20250505:1600;20250505:1700-20250506:1600;20250506:1700-20250507:1600;20250507:1700-20250508:1600;20250508:1700-20250509:1600·20250503:CLOSED;20250504:1700-20250505:1600;20250505:1700-20250506:1600;20250506:1700-20250507:1600;20250507:1700-20250508:1600;20250508:1700-20250509:1600···0·2147483647·ES·IND·67,67·20300621··1·1·1·0·"
  fields = Fields(msg)
  dec.processContractDataMsg(fields)


def DecodeOpenOrderVwap():
  # order_state(status="PreSubmitted", initMarginBefore="0.0", maintMarginBefore="0.0", equityWithLoanBefore="1124466.4", initMarginChange="323301.0", maintMarginChange="293910.0", equityWithLoanChange="0.0", initMarginAfter="323301.0", maintMarginAfter="293910.0", equityWithLoanAfter="1124466.4", commission=1.7976931348623157e+308, minCommission=-3.31765, maxCommission=26.18235, commissionCurrency="USD", warningText="", completedTime="", completedStatus="")
  def PrintOpenOrder(order_id, contract, order, order_state):
    print(f'order id: {order_id}')
    print(f'contract: {contract}')
    print(f'order: {order}')
    print('order_state(' + ', '.join(f'{k}="{v}"' if isinstance(v, str) else f'{k}={v}' for k, v in vars(order_state).items()) + ')')

  dec = Decoder(EWrapper(), 187)
  dec.wrapper.openOrder = PrintOpenOrder

  msg = "5·1·265598·AAPL·STK··0·?··SMART·USD·AAPL·NMS·BUY·5000·LMT·150.0·0.0·DAY··DU1234567··0··103·1056084862·0·0·0··1056084862.0/DU1234567/100·········0··-1·0······2147483647·0·0·0··3·0·0··0·0··0·None··0····?·0·0··0·0······0·0·0·2147483647·2147483647···0··IB·0·0·Vwap·6·noTakeLiq·0·allowPastEndTime·0·speedUp·0·startTime·20250525 14:44:23 US/Eastern·maxPctVol·0.10·endTime·20250525 20:44:23 US/Eastern·0·1·PreSubmitted·0.0·0.0·1124466.4·323301.0·293910.0·0.0·323301.0·293910.0·1124466.4··-3.31765·26.18235·USD··0·0·0·None·1.7976931348623157E308·151.0·1.7976931348623157E308·1.7976931348623157E308·1.7976931348623157E308·1.7976931348623157E308·0····0·1·0·0·0···0··100·0.02····0··"
  fields = Fields(msg)
  dec.processOpenOrder(fields)


def Run():
  DecodeOpenOrderVwap()


if __name__ == "__main__":
  Run()
