//! `login --token`: `SERVER|BOT_API_KEY` (a bot) or `SERVER|EMAIL|PASSWORD`
//! (a user, whose access and refresh tokens are kept).

use atp_core::credentials;
use atp_core::{Error, Result, Value, ValueExt, json};

use crate::api::{Api, server_url};

pub async fn login(api: &Api, raw: &str) -> Result<()> {
  match credentials::fields(raw).as_slice() {
    [server, email, password] => password_login(api, &server_url(server), email, password).await,
    [server, key] => bot_login(api, &server_url(server), key).await,
    _ => Err(Error::input(
      "expected `SERVER|BOT_API_KEY` or `SERVER|EMAIL|PASSWORD`",
    )),
  }
}

async fn password_login(api: &Api, server: &str, email: &str, password: &str) -> Result<()> {
  let resp = api
    .ctx
    .http
    .post(format!("{server}/api/token/login"))
    .no_cookies()
    .json(&json!({
      "credential": {"type": "password", "email": email, "password": password},
      "device": "all-the-platforms",
    }))
    .send()
    .await?;
  let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
  match (v.str("token"), v.str("refresh_token")) {
    (Some(t), Some(r)) if resp.status.is_success() => {
      api.save_user(server, &t, &r);
      Ok(())
    }
    _ => Err(Error::auth(format!(
      "{server} refused the login (HTTP {})",
      resp.status
    ))),
  }
}

async fn bot_login(api: &Api, server: &str, key: &str) -> Result<()> {
  let resp = api
    .ctx
    .http
    .get(format!("{server}/api/bot"))
    .no_cookies()
    .header("x-api-key", key)
    .send()
    .await?;
  if !resp.status.is_success() {
    return Err(Error::auth(format!(
      "{server} did not accept the bot API key (HTTP {})",
      resp.status
    )));
  }
  api.save_bot(server, key);
  Ok(())
}
