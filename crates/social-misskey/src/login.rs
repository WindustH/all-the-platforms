//! MiAuth: the user opens a link on the instance and grants access; the
//! session id then turns into a token (`/api/miauth/<id>/check`).

use social_core::output::note;
use social_core::{Error, QrStatus, QrTicket, Result, ValueExt, json};

use crate::api::Api;

const PERMISSIONS: &str = "read:account,write:account,read:blocks,write:blocks,read:drive,\
write:drive,read:favorites,write:favorites,read:following,write:following,read:messaging,\
write:messaging,read:mutes,write:mutes,write:notes,read:notifications,write:notifications,\
read:reactions,write:reactions,write:votes,read:chat,write:chat";

fn session_id() -> String {
  let a: u128 = rand::random();
  let h = format!("{a:032x}");
  format!(
    "{}-{}-4{}-a{}-{}",
    &h[..8],
    &h[8..12],
    &h[13..16],
    &h[17..20],
    &h[20..32]
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
