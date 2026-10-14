//! QR login through a ByteDance SSO passport (`/get_qrcode/` then
//! `/check_qrconnect/`), as `sso.toutiao.com` serves it. The code is scanned
//! with the site's app (Toutiao or Douyin); a confirmed scan hands back a
//! redirect that sets the session cookies.

use atp_core::http::Http;
use atp_core::{Error, ErrorCode, Result, Value, ValueExt};

/// One SSO passport: its host, the service it logs into and the app id.
#[derive(Debug, Clone, Copy)]
pub struct Sso {
  pub host: &'static str,
  pub service: &'static str,
  pub aid: u64,
}

pub const TOUTIAO: Sso = Sso {
  host: "https://sso.toutiao.com",
  service: "https://www.toutiao.com",
  aid: 24,
};

/// A started QR login: what the QR code encodes, and its token for polling.
pub struct Qr {
  pub url: String,
  pub token: String,
}

pub enum QrState {
  Waiting,
  Scanned,
  /// Confirmed on the phone; the redirect still has to be followed.
  Confirmed(String),
  Expired,
}

pub async fn qr_start(http: &Http, sso: Sso) -> Result<Qr> {
  let v = call(http, sso, "/get_qrcode/", None).await?;
  let data = v.at("data");
  match (data.str("qrcode_index_url"), data.str("token")) {
    (Some(url), Some(token)) => Ok(Qr { url, token }),
    _ => Err(failure(&v, "the passport issued no QR code")),
  }
}

pub async fn qr_poll(http: &Http, sso: Sso, token: &str) -> Result<QrState> {
  let v = call(http, sso, "/check_qrconnect/", Some(token)).await?;
  let data = v.at("data");
  // 7: polled too often; keep waiting.
  if data.i64("error_code") == Some(7) {
    return Ok(QrState::Waiting);
  }
  // Newer passports answer words, older ones numbers.
  match data.str("status").as_deref() {
    Some("new" | "1") => Ok(QrState::Waiting),
    Some("scanned" | "2") => Ok(QrState::Scanned),
    Some("confirmed" | "3") => match data.str("redirect_url") {
      Some(url) => Ok(QrState::Confirmed(url)),
      None => Err(Error::upstream(
        "the passport confirmed the scan without a redirect",
      )),
    },
    Some("expired" | "5") => Ok(QrState::Expired),
    _ => Err(failure(&v, "unexpected QR state")),
  }
}

/// Follow the confirmation redirect so the session cookies land in the jar.
pub async fn finish(http: &Http, redirect: &str) -> Result<()> {
  http
    .get(redirect)
    .header("accept", "text/html")
    .send()
    .await?;
  Ok(())
}

async fn call(http: &Http, sso: Sso, path: &str, token: Option<&str>) -> Result<Value> {
  let mut req = http
    .get(format!("{}{path}", sso.host))
    .query("service", sso.service)
    .query("need_logo", "false")
    .query("aid", sso.aid)
    .query("account_sdk_source", "sso")
    .query("language", "zh");
  if let Some(token) = token {
    req = req.query("token", token);
  }
  let resp = req
    .header("referer", format!("{}/", sso.service))
    .send()
    .await?;
  resp.value().map_err(|_| {
    Error::new(
      ErrorCode::VerificationRequired,
      "the passport answered with a page instead of data (risk control)",
    )
    .with_hint("log in with `--browser` from a browser where you are logged in")
  })
}

/// The passport's own error, or `fallback`. A demanded identity check (scan
/// with the Douyin app) is `verification_required`.
fn failure(v: &Value, fallback: &str) -> Error {
  let message = v
    .first_str(&["data.description", "description", "message"])
    .unwrap_or_else(|| fallback.to_owned());
  if message.contains("验证") {
    return Error::new(ErrorCode::VerificationRequired, message)
      .with_hint("finish the identity check in the app, then log in again");
  }
  Error::upstream(format!("QR login: {message}"))
}
