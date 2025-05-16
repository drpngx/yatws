// yatws/test_general.rs
use anyhow::{Result};
use log::{error, info};
use yatws::{
  IBKRClient,
};

pub(crate) fn time_impl(client: &IBKRClient, _is_live: bool) -> Result<()> {
  info!("--- Testing Get Server Time ---");
  let cl = client.client();
  match cl.request_current_time() {
    Ok(time) => {
      info!("Successfully received server time: {:?}", time);
      Ok(())
    }
    Err(e) => {
      error!("Failed to get server time: {:?}", e);
      Err(e.into())
    }
  }
}
