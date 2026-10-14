//! QR login on id.kuaishou.com, and the per-site tickets a login unlocks.
//!
//! The QR flow: `qr/start` gives the code; `qr/scanResult` waits (a long
//! poll) until the app scans it, `qr/acceptResult` until the user confirms;
//! `qr/callback` trades the confirmation for `passToken`, `userId` and the
//! www ticket. The live site and the creator center take their own
//! tickets, minted from `passToken` on first use.

use std::time::Duration;

use atp_core::{Error, ErrorCode, QrStatus, QrTicket, Result, Value, ValueExt, json};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::api::{Api, CP, LIVE, LOGIN_HINT, WWW};

const SID_WWW: &str = "kuaishou.server.webday7";
const SID_LIVE: &str = "kuaishou.live.web";
const SID_CP: &str = "kuaishou.web.cp.api";
const CHANNEL: &str = "UNKNOWN";
/// The long polls answer within about 70 s.
const LONG_POLL: Duration = Duration::from_secs(75);

pub async fn qr_start(api: &Api) -> Result<QrTicket> {
  api.scanned.set(false);
  let v = api
    .login("/rest/c/infra/ks/qr/start")
    .form(&[
      ("sid", SID_WWW),
      ("channelType", CHANNEL),
      ("isWebSig4", "true"),
    ])
    .send()
    .await?;
  let (Some(url), Some(token), Some(signature)) = (
    v.str("qrUrl"),
    v.str("qrLoginToken"),
    v.str("qrLoginSignature"),
  ) else {
    return Err(failed("qr/start", &v));
  };
  let mut ticket = QrTicket {
    url,
    token,
    image: v.str("imageData").and_then(|d| STANDARD.decode(d).ok()),
    ..QrTicket::default()
  };
  ticket.extra.insert("signature".into(), signature);
  Ok(ticket)
}

pub async fn qr_poll(api: &Api, ticket: &QrTicket) -> Result<QrStatus> {
  let signature = ticket.extra.get("signature").cloned().unwrap_or_default();
  let mut fields = vec![
    ("qrLoginToken", ticket.token.as_str()),
    ("qrLoginSignature", signature.as_str()),
  ];
  let scanned = api.scanned.get();
  let path = if scanned {
    fields.push(("sid", SID_WWW));
    "/rest/c/infra/ks/qr/acceptResult"
  } else {
    "/rest/c/infra/ks/qr/scanResult"
  };
  fields.extend([("channelType", CHANNEL), ("isWebSig4", "true")]);
  let v = match api
    .login(path)
    .form(&fields)
    .timeout(LONG_POLL)
    .send()
    .await
  {
    Ok(v) => v,
    // A long poll that ran out: nothing happened yet.
    Err(e) if e.code == ErrorCode::NetworkError => return Ok(waiting(scanned)),
    Err(e) => return Err(e),
  };
  match v.i64("result") {
    Some(1) if scanned => {
      let token = v
        .first_str(&["qrToken", "qr_token"])
        .ok_or_else(|| failed("qr/acceptResult", &v))?;
      callback(api, &token).await?;
      Ok(QrStatus::Confirmed)
    }
    Some(1) => {
      api.scanned.set(true);
      Ok(QrStatus::Scanned)
    }
    Some(707) => Ok(QrStatus::Expired),
    Some(711 | 712) => Err(Error::auth("the login was cancelled on the phone")),
    _ => Ok(waiting(scanned)),
  }
}

fn waiting(scanned: bool) -> QrStatus {
  if scanned {
    QrStatus::Scanned
  } else {
    QrStatus::Waiting
  }
}

/// Trade the confirmed code for the session.
async fn callback(api: &Api, qr_token: &str) -> Result<()> {
  let v = api
    .login("/pass/kuaishou/login/qr/callback")
    .form(&[
      ("qrToken", qr_token),
      ("sid", SID_WWW),
      ("channelType", CHANNEL),
      ("isWebSig4", "true"),
    ])
    .send()
    .await?;
  if v.i64("result") != Some(1) {
    return Err(failed("qr/callback", &v));
  }
  absorb(api, &v);
  sts(api, SID_WWW, WWW).await;
  Ok(())
}

/// Put the tickets of a login answer into the jar: `passToken`, `userId`
/// and every `<sid>_st` / `<sid>_ph` / `<sid>.at`.
fn absorb(api: &Api, v: &Value) {
  let Some(obj) = v.as_object() else {
    return;
  };
  for (key, value) in obj {
    let ticket = ["_st", "_ph", ".at"].iter().any(|s| key.ends_with(s));
    if !(ticket || matches!(key.as_str(), "passToken" | "userId")) {
      continue;
    }
    let value = match value {
      Value::String(s) => s.clone(),
      Value::Number(n) => n.to_string(),
      _ => continue,
    };
    if !value.is_empty() {
      api.ctx.http.set_cookie(key, &value);
    }
  }
}

/// The site's own ticket from its `<sid>.at` (best effort: the browser
/// gets it through a redirect whose cookies only the first hop sets).
async fn sts(api: &Api, sid: &str, site: &str) {
  let Some(token) = api.ctx.http.cookie(&format!("{sid}.at")) else {
    return;
  };
  let url = format!(
    "{site}/rest/infra/sts?authToken={}&sid={sid}",
    crate::sign::component(&token)
  );
  if let Err(e) = api.ctx.http.get(url).send().await {
    tracing::debug!("{sid} ticket: {e}");
  }
}

/// Mint a site's tickets from `passToken`.
async fn tickets(api: &Api, sid: &str, page: &str) -> Result<Value> {
  if !api.ctx.http.has_cookie("passToken") {
    return Err(
      Error::auth(format!(
        "the session has no {sid} ticket and no passToken to mint one"
      ))
      .with_hint(LOGIN_HINT),
    );
  }
  let v = api
    .login("/pass/kuaishou/login/passToken")
    .form(&[
      ("sid", sid),
      ("channelType", CHANNEL),
      ("encryptHeaders", ""),
    ])
    .referer(page)
    .send()
    .await?;
  if v.i64("result") != Some(1) {
    return Err(failed("passToken", &v).with_hint(LOGIN_HINT));
  }
  absorb(api, &v);
  Ok(v)
}

/// Make sure the live site's tickets are there (`listen` needs them).
pub async fn live_tickets(api: &Api) -> Result<()> {
  if api.ctx.http.has_cookie("kuaishou.live.web_st") {
    return Ok(());
  }
  let v = tickets(api, SID_LIVE, &format!("{LIVE}/")).await?;
  let token = v
    .get(format!("{SID_LIVE}.at"))
    .and_then(Value::as_str)
    .ok_or_else(|| failed("passToken", &v))?;
  api
    .live_post("/live_api/baseuser/userLogin")
    .json(json!({ "userLoginInfo": { "authToken": token, "sid": SID_LIVE } }))
    .send()
    .await?;
  Ok(())
}

/// Mint the live site's tickets again from `passToken`: stale ones get no
/// socket token. Without `passToken` the session is left as it is.
pub async fn renew_live_tickets(api: &Api) -> Result<()> {
  if !api.ctx.http.has_cookie("passToken") {
    return Err(
      Error::auth("Kuaishou gave no live socket token and the session has no passToken")
        .with_hint(LOGIN_HINT),
    );
  }
  for name in ["kuaishou.live.web_st", "kuaishou.live.web_ph"] {
    api.ctx.http.remove_cookie(name);
  }
  live_tickets(api).await
}

/// Make sure the creator center's tickets are there.
pub async fn cp_tickets(api: &Api) -> Result<()> {
  if api.ctx.http.has_cookie("kuaishou.web.cp.api_ph") {
    return Ok(());
  }
  tickets(api, SID_CP, &format!("{CP}/")).await?;
  sts(api, SID_CP, CP).await;
  Ok(())
}

fn failed(step: &str, v: &Value) -> Error {
  let msg = v
    .first_str(&["error_msg", "message"])
    .unwrap_or_else(|| format!("result {}", v.i64("result").unwrap_or(-1)));
  Error::upstream(format!("Kuaishou login ({step}): {msg}"))
}
