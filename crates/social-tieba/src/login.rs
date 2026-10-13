//! QR login through Baidu passport, scanned with the Tieba or Baidu app.
//!
//! `getqrcode` hands out a sign and the QR image; `channel/unicast` is long
//! polled for the scan (`errno` 1 waiting, 2 scanned, -1 expired; 0 carries a
//! `channel_v` whose `status` 0 has the one-time code `v`); `qrbdusslogin`
//! trades `v` for the session: `BDUSS` and, in `stokenList`, Tieba's
//! `STOKEN` (`tb#…`). Flow as in TiebaMeow/WebTiebaManager.

use std::time::Duration;

use social_core::{Error, ErrorCode, QrStatus, QrTicket, Result, Value, ValueExt};

use crate::api::Api;

const PASSPORT: &str = "https://passport.baidu.com";
/// The unicast poll holds the connection until something happens.
const POLL_TIMEOUT: Duration = Duration::from_secs(45);

fn now() -> String {
  jiff::Timestamp::now().as_second().to_string()
}

/// JSON out of a JSONP or single-quoted passport answer.
fn loose_json(text: &str) -> Option<Value> {
  let start = text.find('{')?;
  let end = text.rfind('}')?;
  let body = &text[start..=end];
  serde_json::from_str(body)
    .ok()
    .or_else(|| serde_json::from_str(&body.replace("\\&", "&").replace('\'', "\"")).ok())
}

pub async fn qr_start(api: &Api) -> Result<QrTicket> {
  let resp = api
    .ctx
    .http
    .get(format!("{PASSPORT}/v2/api/getqrcode"))
    .query("lp", "pc")
    .query("qrloginfrom", "pc")
    .send()
    .await?;
  let v = loose_json(&resp.check()?.text())
    .ok_or_else(|| Error::upstream("unexpected answer from Baidu passport getqrcode"))?;
  let (Some(sign), Some(img)) = (v.str("sign"), v.str("imgurl")) else {
    return Err(Error::upstream("Baidu passport sent no QR code"));
  };
  let img = if img.starts_with("http") {
    img
  } else {
    format!("https://{img}")
  };
  let image = api.ctx.http.get(&img).send().await?.check()?.body;
  Ok(QrTicket {
    url: img,
    token: sign,
    image: Some(image),
    ..QrTicket::default()
  })
}

pub async fn qr_poll(api: &Api, ticket: &QrTicket) -> Result<QrStatus> {
  let sent = api
    .ctx
    .http
    .get(format!("{PASSPORT}/channel/unicast"))
    .query("channel_id", &ticket.token)
    .query("tpl", "tb")
    .query("apiver", "v3")
    .query("callback", "")
    .query("tt", now())
    .timeout(POLL_TIMEOUT)
    .retries(0)
    .send()
    .await;
  let resp = match sent {
    Ok(r) => r,
    // A long poll that ended without news.
    Err(e) if e.code == ErrorCode::NetworkError => return Ok(QrStatus::Waiting),
    Err(e) => return Err(e),
  };
  let Some(v) = loose_json(&resp.text()) else {
    return Ok(QrStatus::Waiting);
  };
  match v.i64("errno") {
    Some(-1) => return Ok(QrStatus::Expired),
    Some(0) => {}
    Some(2) => return Ok(QrStatus::Scanned),
    _ => return Ok(QrStatus::Waiting),
  }
  let channel = v
    .str("channel_v")
    .and_then(|s| serde_json::from_str::<Value>(&s).ok())
    .unwrap_or_default();
  match channel.i64("status") {
    Some(0) => {}
    Some(1) => return Ok(QrStatus::Scanned),
    Some(_) => return Ok(QrStatus::Expired),
    None => return Ok(QrStatus::Waiting),
  }
  let code = channel
    .str("v")
    .ok_or_else(|| Error::auth("the QR login was confirmed without a login code"))?;
  finish(api, &code).await?;
  Ok(QrStatus::Confirmed)
}

/// Trade the one-time code for the session cookies.
async fn finish(api: &Api, code: &str) -> Result<()> {
  let tt = now();
  let resp = api
    .ctx
    .http
    .get(format!("{PASSPORT}/v3/login/main/qrbdusslogin"))
    .query("v", &tt)
    .query("bduss", code)
    .query("u", crate::api::WEB)
    .query("loginVersion", "v4")
    .query("qrcode", "1")
    .query("tpl", "tb")
    .query("apiver", "v3")
    .query("tt", &tt)
    .query("traceid", "")
    .send()
    .await?;
  let v = loose_json(&resp.check()?.text())
    .ok_or_else(|| Error::upstream("unexpected answer from Baidu passport qrbdusslogin"))?;
  let session = v.at("data.session");
  let bduss = session
    .str("bduss")
    .filter(|_| v.str("code").as_deref() == Some("110000"));
  let Some(bduss) = bduss else {
    let msg = v.str("errInfo.msg").unwrap_or_default();
    return Err(Error::auth(
      format!("Baidu refused the QR login {msg}")
        .trim()
        .to_owned(),
    ));
  };
  api.ctx.http.set_cookie("BDUSS", &bduss);
  if let Some(stoken) = session.str("stokenList").and_then(|l| tieba_stoken(&l)) {
    api.ctx.http.set_cookie("STOKEN", &stoken);
  }
  Ok(())
}

/// Tieba's entry (`tb#…`) of passport's per-product STOKEN list.
fn tieba_stoken(list: &str) -> Option<String> {
  let items: Vec<String> = serde_json::from_str(&list.replace("&quot;", "\"")).ok()?;
  items
    .iter()
    .find_map(|i| i.strip_prefix("tb#").map(str::to_owned))
}
