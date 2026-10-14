//! QR login, as weread.qq.com does it: `/api/auth/getLoginUid` hands out a
//! uid; the QR code holds `https://weread.qq.com/web/confirm?uid=<uid>`,
//! which WeChat opens when it scans it; `/api/auth/getLoginInfo?uid=`
//! waits (up to about a minute) until the login is confirmed and answers
//! `{succeed, webLoginVid, accessToken, refreshToken}`, which become the
//! session cookies `wr_vid`, `wr_skey` and `wr_rt`. When WeRead wants the
//! four-digit code WeChat shows (`logicCode` `NEED_OTP`), it is asked for
//! on the terminal and sent along. A random `wr_fp` cookie identifies this
//! install as the site's browser fingerprint does, so a new login here does
//! not end sessions elsewhere.

use std::io::{BufRead, IsTerminal, Write};
use std::time::Duration;

use atp_core::text::encode_uri_component;
use atp_core::{Error, ErrorCode, QrStatus, QrTicket, Result, Value, ValueExt};
use rand::Rng;

use crate::api::{Api, BASE};

/// The page the web logs in from.
const FROM: &str = "https://weread.qq.com/r/weread-skills";
/// How long one `getLoginInfo` may wait for the phone.
const LONG_POLL: Duration = Duration::from_secs(70);
/// How often a wrong code may be typed again.
const CODE_TRIES: usize = 3;

pub async fn qr_start(api: &Api) -> Result<QrTicket> {
  let http = &api.ctx.http;
  if !http.has_cookie("wr_fp") {
    http.set_cookie("wr_fp", &rand::rng().random::<u32>().to_string());
  }
  http
    .get(FROM)
    .header("accept", "text/html,application/xhtml+xml")
    .send()
    .await?;
  let v = http
    .get(format!("{BASE}/api/auth/getLoginUid"))
    .header("referer", FROM)
    .send()
    .await?
    .check()?
    .value()?;
  let uid = v
    .str("uid")
    .ok_or_else(|| Error::upstream("WeRead gave no login uid"))?;
  Ok(QrTicket {
    url: format!("{BASE}/web/confirm?uid={}", encode_uri_component(&uid)),
    token: uid,
    ..QrTicket::default()
  })
}

pub async fn qr_poll(api: &Api, ticket: &QrTicket) -> Result<QrStatus> {
  let mut otp = String::new();
  for _ in 0..=CODE_TRIES {
    let Some(v) = login_info(api, &ticket.token, &otp).await? else {
      return Ok(QrStatus::Waiting);
    };
    if v.bool("succeed") == Some(true) {
      return install(api, &v);
    }
    match v.str("logicCode").as_deref() {
      Some("NEED_OTP") => otp = ask_code("Type the four-digit code WeChat shows: ")?,
      Some("OTP_NOT_MATCH") => otp = ask_code("That code did not match; type it again: ")?,
      Some("LOGIN_TIMEOUT" | "OTP_EXPIRED") => return Ok(QrStatus::Expired),
      _ => return Ok(QrStatus::Waiting),
    }
  }
  Err(Error::auth("the code WeChat shows was not accepted"))
}

/// One long poll; `None` when it ended without an answer.
async fn login_info(api: &Api, uid: &str, otp: &str) -> Result<Option<Value>> {
  let sent = api
    .ctx
    .http
    .get(format!("{BASE}/api/auth/getLoginInfo"))
    .query("uid", uid)
    .query("otp", otp)
    .header("referer", FROM)
    .timeout(LONG_POLL)
    .retries(0)
    .send()
    .await;
  match sent {
    Ok(resp) if resp.status.is_success() => Ok(serde_json::from_slice(&resp.body).ok()),
    Ok(_) => Ok(None),
    Err(e) if e.code == ErrorCode::NetworkError => Ok(None),
    Err(e) => Err(e),
  }
}

/// The confirmed login's credentials as the cookies the web keeps.
fn install(api: &Api, v: &Value) -> Result<QrStatus> {
  let (Some(vid), Some(key)) = (v.str("webLoginVid"), v.str("accessToken")) else {
    return Err(Error::upstream(
      "WeRead confirmed the login without credentials",
    ));
  };
  let http = &api.ctx.http;
  http.set_cookie("wr_vid", &vid);
  http.set_cookie("wr_skey", &key);
  http.set_cookie("wr_ql", "0");
  if let Some(refresh) = v.str("refreshToken") {
    http.set_cookie("wr_rt", &encode_uri_component(&refresh));
  }
  Ok(QrStatus::Confirmed)
}

/// The four-digit code, typed on the terminal.
fn ask_code(question: &str) -> Result<String> {
  let stdin = std::io::stdin();
  if !stdin.is_terminal() {
    return Err(Error::auth(
      "WeRead wants the four-digit code WeChat shows, which needs a terminal; \
       log in with `--browser` instead",
    ));
  }
  eprint!("{question}");
  std::io::stderr().flush()?;
  let mut line = String::new();
  stdin.lock().read_line(&mut line)?;
  Ok(line.trim().to_owned())
}
