# Yet Another TWS API in rust
TODO: order execution processing.

Mkae sure we use this:
443  |   pub fn encode_request_ids(&self) -> Result<Vec<u8>, IBKRError> {
     |          ^^^^^^^^^^^^^^^^^^
...
1333 |   fn encode_contract_for_order(&self, cursor: &mut Cursor<Vec<u8>>, contract: &Contract) -> Result<(), IBKRError> {
     |      ^^^^^^^^^^^^^^^^^^^^^^^^^
...
1468 |   pub fn encode_request_all_open_orders(&self) -> Result<Vec<u8>, IBKRError> {
     |          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
1475 |   pub fn encode_request_open_orders(&self) -> Result<Vec<u8>, IBKRError> {
     |          ^^^^^^^^^^^^^^^^^^^^^^^^^^
...
1586 |   pub fn encode_request_managed_accounts(&self) -> Result<Vec<u8>, IBKRError> {
     |          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Is never used: SecDefOptParamsResult, HistoricalScheduleResult

Is never used DataRefManager::_handle_error, and News etc
