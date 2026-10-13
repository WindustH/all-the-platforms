//! The admin's safety scan (安全验证) the backend may require before
//! publishing: a QR code the account admin scans with WeChat, then polled
//! until confirmed.

use std::time::Duration;

use atp_core::output::note;
use atp_core::{Error, ErrorCode, Result, ValueExt};

use crate::api::Api;
use crate::backend::MP;

/// The admin's safety scan: show the QR code and wait for the confirmation.
/// Returns the confirmation code and a refreshed `operation_seq`.
pub async fn scan(api: &Api, appmsgid: &str, notify: bool) -> Result<(String, Option<String>)> {
  let backend = api.backend();
  let ticket_page = backend
    .post(
      "/misc/safeassistant",
      vec![("1", "1".into())],
      vec![("action", "get_ticket".into())],
    )
    .await?;
  let ticket = ticket_page
    .str("ticket")
    .ok_or_else(|| Error::upstream("the safety check gave no ticket"))?;
  let conn = backend
    .post(
      "/safe/safeqrconnect",
      vec![("1", "1".into())],
      vec![
        ("state", "0".into()),
        ("login_type", "safe_center".into()),
        ("type", "json".into()),
        ("ticket", ticket.clone()),
      ],
    )
    .await?;
  let uuid = conn
    .str("uuid")
    .ok_or_else(|| Error::upstream("the safety check gave no uuid"))?;
  show_safety_qr(api, &ticket, &uuid, appmsgid, notify).await?;
  let code = wait_for_scan(api, &uuid).await?;
  Ok((code, ticket_page.str("operation_seq")))
}

async fn show_safety_qr(
  api: &Api,
  ticket: &str,
  uuid: &str,
  appmsgid: &str,
  notify: bool,
) -> Result<()> {
  let mut query = vec![
    ("ticket", ticket.to_owned()),
    ("uuid", uuid.to_owned()),
    ("action", "check".to_owned()),
    ("service_type", "1".into()),
    ("type", "msgs".into()),
    ("msgid", appmsgid.into()),
  ];
  if !notify {
    query.push(("publish_type", "1".into()));
  }
  let image = api
    .ctx
    .http
    .get(format!("{MP}/safe/safeqrcode"))
    .queries(query)
    .send()
    .await?
    .check()?
    .body;
  let path = atp_core::qr::show_image(&image, api.ctx.store.cache_dir())?;
  note(&format!(
    "The account admin must scan this with WeChat to allow publishing. QR image: {}",
    path.display()
  ));
  Ok(())
}

/// Poll until the admin confirms (405) or cancels (403); the confirmation code.
async fn wait_for_scan(api: &Api, uuid: &str) -> Result<String> {
  let started = std::time::Instant::now();
  while started.elapsed() < Duration::from_secs(180) {
    tokio::time::sleep(Duration::from_secs(2)).await;
    let now = jiff::Timestamp::now().as_millisecond().to_string();
    let v = api
      .backend()
      .post_raw(
        "/safe/safeuuid",
        vec![("timespam", now)],
        vec![
          ("uuid", uuid.to_owned()),
          ("action", "json".into()),
          ("type", "json".into()),
        ],
      )
      .await?;
    match v.i64("errcode") {
      Some(405) => return Ok(v.str("code").unwrap_or_default()),
      Some(403) => return Err(Error::auth("the admin cancelled the safety check")),
      _ => {}
    }
  }
  Err(Error::new(
    ErrorCode::VerificationRequired,
    "timed out waiting for the admin's safety scan",
  ))
}
