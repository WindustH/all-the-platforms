//! `login --token 'URL[|ACCESS_TOKEN][|WS_URL]'`: the endpoint is tried with
//! `get_login_info` before it is kept.

use atp_core::{Result, json};

use crate::api::{Api, Endpoint};

pub async fn login_token(api: &Api, token: &str) -> Result<()> {
  let endpoint = Endpoint::parse(token)?;
  api.call_at(&endpoint, "get_login_info", json!({})).await?;
  api.save(&endpoint);
  Ok(())
}
