//! Logging in: an access token (`INSTANCE|TOKEN`), or MiAuth, where the user
//! opens a link on the instance and grants access and the session id then
//! turns into a token (`/api/miauth/<id>/check`).

use social_core::output::note;
use social_core::{Error, QrStatus, QrTicket, Result, Value, ValueExt, json};

use crate::api::{self, Api};

const PERMISSIONS: &str = "read:account,write:account,read:blocks,write:blocks,read:drive,\
write:drive,read:favorites,write:favorites,read:following,write:following,read:messaging,\
write:messaging,read:mutes,write:mutes,write:notes,read:notifications,write:notifications,\
read:reactions,write:reactions,write:votes,read:chat,write:chat";

/// `INSTANCE|TOKEN`, or a bare token for the instance in use; checked with
/// `i` before it is saved.
pub async fn token(api: &Api, raw: &str) -> Result<()> {
  let (instance, token) = match raw.split_once('|') {
    Some((instance, token)) => (api::host(instance), token.trim().to_owned()),
    None => (api.instance(), raw.trim().to_owned()),
  };
  if instance.is_empty() || token.is_empty() {
    return Err(Error::input(
      "expected `INSTANCE|TOKEN`, e.g. `misskey.io|abc123`",
    ));
  }
  let resp = api
    .ctx
    .http
    .post(format!("https://{instance}/api/i"))
    .no_cookies()
    .json(&json!({"i": token}))
    .send()
    .await?;
  if !resp.status.is_success() {
    let body: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
    return Err(Error::auth(format!(
      "{instance} did not accept the token ({})",
      body
        .str("error.message")
        .unwrap_or_else(|| format!("HTTP {}", resp.status))
    )));
  }
  api.save(&instance, &token);
  Ok(())
}

/// A random UUID (v4) for the MiAuth session.
fn session_id() -> String {
  let random: u128 = rand::random();
  let hex = format!("{random:032x}");
  format!(
    "{}-{}-4{}-a{}-{}",
    &hex[..8],
    &hex[8..12],
    &hex[13..16],
    &hex[17..20],
    &hex[20..32]
  )
}

pub fn start(api: &Api) -> QrTicket {
  let instance = api.instance();
  let session = session_id();
  let url = format!("https://{instance}/miauth/{session}?name=social-cli&permission={PERMISSIONS}");
  note(&format!(
    "Open this link signed in to {instance} and allow access (or scan the code):\n  {url}"
  ));
  QrTicket {
    url,
    token: session,
    extra: [("instance".to_owned(), instance)].into(),
    ..QrTicket::default()
  }
}

pub async fn poll(api: &Api, ticket: &QrTicket) -> Result<QrStatus> {
  let instance = ticket
    .extra
    .get("instance")
    .cloned()
    .unwrap_or_else(|| api.instance());
  let v = api
    .ctx
    .http
    .post(format!(
      "https://{instance}/api/miauth/{}/check",
      ticket.token
    ))
    .no_cookies()
    .json(&json!({}))
    .value()
    .await?;
  if v.bool("ok") != Some(true) {
    return Ok(QrStatus::Waiting);
  }
  let token = v
    .str("token")
    .ok_or_else(|| Error::upstream("MiAuth answered without a token"))?;
  api.save(&instance, &token);
  Ok(QrStatus::Confirmed)
}
