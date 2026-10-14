//! QR login, as accounts.douban.com's login page does it: `qrlogin_code`
//! hands out a code (and the link or image the QR code shows), which the
//! Douban app scans; `qrlogin_status?code=` says `scan` once it is scanned
//! and `login` once confirmed, and that answer sets `dbcl2` and `ck`.
//! `invalid` means the code expired; `cancel`, that it was declined on the
//! phone (the page then shows a fresh code).

use atp_core::{Error, QrStatus, QrTicket, Result, Value, ValueExt};

use crate::api::{ACCOUNTS, Api, LOGIN_COOKIES, MOBILE};

const QR: &str = "https://accounts.douban.com/j/mobile/login";

fn referer() -> String {
  format!("{ACCOUNTS}/passport/login")
}

/// The payload of a `{status: "success", payload}` answer.
fn payload(v: Value, what: &str) -> Result<Value> {
  if v.str("status").as_deref() == Some("success") {
    return Ok(v.at("payload").clone());
  }
  let message = v
    .first_str(&["description", "message", "msg"])
    .unwrap_or_else(|| format!("no {what}"));
  Err(Error::upstream(format!("Douban QR login: {message}")))
}

pub async fn qr_start(api: &Api) -> Result<QrTicket> {
  let http = &api.ctx.http;
  // The login page sets the cookies the next calls are checked against.
  http
    .get(referer())
    .header("accept", "text/html,application/xhtml+xml")
    .send()
    .await?;
  let v = http
    .post(format!("{QR}/qrlogin_code"))
    .form([("ck", api.ck())])
    .header("referer", referer())
    .header("origin", ACCOUNTS)
    .header("x-requested-with", "XMLHttpRequest")
    .send()
    .await?
    .check()?
    .value()?;
  let p = payload(v, "QR code")?;
  let code = p
    .str("code")
    .ok_or_else(|| Error::upstream("Douban's QR login gave no code"))?;
  let mut ticket = QrTicket {
    token: code,
    ..QrTicket::default()
  };
  match (p.str("login_url"), p.str("img")) {
    (Some(url), _) => ticket.url = url,
    (None, Some(img)) => ticket.image = Some(http.get(img).no_cookies().send().await?.body),
    (None, None) => return Err(Error::upstream("Douban's QR login gave no code to show")),
  }
  Ok(ticket)
}

pub async fn qr_poll(api: &Api, ticket: &QrTicket) -> Result<QrStatus> {
  let v = api
    .ctx
    .http
    .get(format!("{QR}/qrlogin_status"))
    .query("code", &ticket.token)
    .query("ck", api.ck())
    .header("referer", referer())
    .header("x-requested-with", "XMLHttpRequest")
    .send()
    .await?
    .check()?
    .value()?;
  let state = payload(v, "login state")?.str("login_status");
  match state.as_deref() {
    Some("scan") => Ok(QrStatus::Scanned),
    Some("invalid") => Ok(QrStatus::Expired),
    Some("login") => confirmed(api).await,
    _ => Ok(QrStatus::Waiting),
  }
}

/// After `login`: the cookies came with the answer, or come with the next page.
async fn confirmed(api: &Api) -> Result<QrStatus> {
  let has_all = || LOGIN_COOKIES.iter().all(|c| api.ctx.http.has_cookie(c));
  if !has_all() {
    api
      .ctx
      .http
      .get(format!("{MOBILE}/"))
      .header("accept", "text/html,application/xhtml+xml")
      .send()
      .await?;
  }
  if has_all() {
    Ok(QrStatus::Confirmed)
  } else {
    Err(Error::auth(
      "Douban confirmed the scan but issued no dbcl2 cookie; log in with `--browser`",
    ))
  }
}
