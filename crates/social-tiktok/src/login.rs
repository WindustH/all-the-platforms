//! QR login with the TikTok app (Profile → ☰ → scan), the passport flow of
//! the web login page: `get_qrcode` hands out the link to encode and a token,
//! `check_qrconnect` reports `new`, `scanned`, `confirmed` or `expired`. On
//! confirmation the session cookies arrive, directly or through the
//! `redirect_url` the answer names.

use social_core::{Error, ErrorCode, QrStatus, QrTicket, Result, Value, ValueExt};

use crate::api::{Api, SESSION_COOKIE, WWW};

const AID: &str = "1459";

async fn passport(api: &Api, path: &str, token: Option<&str>) -> Result<Value> {
  let mut req = api
    .ctx
    .http
    .get(format!("{WWW}/passport/web/{path}/"))
    .query("next", WWW)
    .query("aid", AID)
    .query("account_sdk_source", "web")
    .header("referer", format!("{WWW}/login/qrcode"));
  if let Some(t) = token {
    req = req.query("token", t);
  }
  let v = req.send().await?.check()?.value()?;
  if v.str("message").as_deref() == Some("success") {
    return Ok(v.at("data").clone());
  }
  let code = v.i64("data.error_code").unwrap_or(-1);
  let text = v
    .str("data.description")
    .unwrap_or_else(|| "the QR login failed".into());
  Err(match code {
    7 => Error::new(ErrorCode::RateLimited, format!("TikTok: {text}"))
      .with_hint("wait a while, or use `social tiktok login --browser`"),
    _ => Error::upstream(format!("TikTok QR login error {code}: {text}")),
  })
}

pub async fn qr_start(api: &Api) -> Result<QrTicket> {
  let data = passport(api, "get_qrcode", None).await?;
  let (Some(url), Some(token)) = (data.str("qrcode_index_url"), data.str("token")) else {
    return Err(Error::upstream("TikTok handed out no QR code"));
  };
  Ok(QrTicket {
    url,
    token,
    ..QrTicket::default()
  })
}

pub async fn qr_poll(api: &Api, ticket: &QrTicket) -> Result<QrStatus> {
  let data = passport(api, "check_qrconnect", Some(&ticket.token)).await?;
  Ok(match data.str("status").as_deref() {
    Some("scanned") => QrStatus::Scanned,
    Some("expired") => QrStatus::Expired,
    Some("confirmed") => {
      if !api.ctx.http.has_cookie(SESSION_COOKIE)
        && let Some(url) = data.str("redirect_url")
      {
        api.ctx.http.get(url).send().await?;
      }
      if !api.ctx.http.has_cookie(SESSION_COOKIE) {
        return Err(
          Error::auth("TikTok confirmed the scan but set no session")
            .with_hint("use `social tiktok login --browser` or `--cookie`"),
        );
      }
      QrStatus::Confirmed
    }
    _ => QrStatus::Waiting,
  })
}
