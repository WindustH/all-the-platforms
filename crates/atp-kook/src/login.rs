//! `login --token`: a bot token, checked against `/user/me` before it is kept.

use atp_core::Result;

use crate::api::Api;

pub async fn login_token(api: &Api, token: &str) -> Result<()> {
  api.me_with(token).await?;
  api.save_token(token);
  Ok(())
}
