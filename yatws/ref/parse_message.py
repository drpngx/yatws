"""Parse a message manually.

You can copy the message from the debug output.

For debugging, go directly in the source and print debug statements from there.
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


def Run():
  dec = Decoder(EWrapper(), 187)

  msg = "10·6·ES·FUT·20300621 08:30:00 US/Central·20300621·0··QBALGO·USD·ESM0·ES·ES·770561177·0.25·50·AD,ALERT,ALLOC,AVGCOST,BASKET,BENCHPX,DAY,DEACT,DEACTDIS,LMT,MKT,NGCOMB,OPENCLOSE,WHATIF·CME,QBALGO·1·11004968·E-mini S&P 500··203006····US/Central·20250503:CLOSED;20250504:1700-20250505:1600;20250505:1700-20250506:1600;20250506:1700-20250507:1600;20250507:1700-20250508:1600;20250508:1700-20250509:1600·20250503:CLOSED;20250504:1700-20250505:1600;20250505:1700-20250506:1600;20250506:1700-20250507:1600;20250507:1700-20250508:1600;20250508:1700-20250509:1600···0·2147483647·ES·IND·67,67·20300621··1·1·1·0·"
  fields = Fields(msg)
  dec.processContractDataMsg(fields)


if __name__ == "__main__":
  Run()
