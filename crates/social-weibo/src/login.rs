//! QR-code login through passport.weibo.com (`/sso/v2/qrcode/...`): a CSRF
//! token from the sign-in page, a QR code to scan with the Weibo app, then
//! polling until the scan is confirmed. The confirmation hands out an `alt`
//! ticket, exchanged at login.sina.com.cn for the cross-domain URLs that set
//! the weibo.com session cookies.

use social_core::{Error, QrStatus, QrTicket, Result, Value, ValueExt};

use crate::api::Client;

const PASSPORT: &str = "https://passport.weibo.com";
const SIGNIN_REFERER: &str = "https://passport.weibo.com/sso/signin?entry=miniblog&source=miniblog&url=https%3A%2F%2Fweibo.com%2F";
const OK: i64 = 20000000;
const WAITING: i64 = 50114001;
const SCANNED: i64 = 50114002;
const EXPIRED: i64 = 50114004;
/// Passport client version the web page sends when polling.
const VERSION: &str = "20250520";

async fn passport(c: &Client, path: &str, query: &[(&str, &str)]) -> Result<Value> {
  let http = &c.ctx.http;
  let mut req = http
    .get(format!("{PASSPORT}{path}"))
    .header("referer", SIGNIN_REFERER)
    .header("x-requested-with", "XMLHttpRequest")
    .queries(query.iter().copied());
  if let Some(token) = http.cookie("X-CSRF-TOKEN") {
    req = req.header("x-csrf-token", token);
  }
  req.send().await?.value()
}

pub async fn qr_start(c: &Client) -> Result<QrTicket> {
  // The sign-in page sets the X-CSRF-TOKEN cookie the QR calls echo.
  c.ctx
    .http
    .get(format!("{PASSPORT}/sso/signin"))
    .header("accept", "text/html")
    .queries([
      ("entry", "miniblog"),
      ("source", "miniblog"),
      ("url", "https://weibo.com/"),
    ])
    .send()
    .await?;
  let v = passport(
    c,
    "/sso/v2/qrcode/image",
    &[("entry", "miniblog"), ("size", "180")],
  )
  .await?;
  if v.i64("retcode") != Some(OK) {
    return Err(Error::upstream(format!(
      "Weibo did not hand out a QR code: {}",
      v.str("msg").unwrap_or_default()
    )));
  }
  let qrid = v
    .str("data.qrid")
    .ok_or_else(|| Error::upstream("QR login: no qrid"))?;
  // The image URL carries what the code encodes in its `data` parameter.
  let url = v
    .str("data.image")
    .and_then(|img| {
      let img = if img.starts_with("//") {
        format!("https:{img}")
      } else {
        img
      };
      url::Url::parse(&img)
        .ok()?
        .query_pairs()
        .find_map(|(k, v)| (k == "data").then(|| v.into_owned()))
    })
    .unwrap_or_else(|| format!("https://passport.weibo.cn/signin/qrcode/scan?qr={qrid}"));
  Ok(QrTicket {
    url,
    token: qrid,
    ..QrTicket::default()
  })
}

pub async fn qr_poll(c: &Client, ticket: &QrTicket) -> Result<QrStatus> {
  let v = passport(
    c,
    "/sso/v2/qrcode/check",
    &[
      ("entry", "miniblog"),
      ("source", "miniblog"),
      ("url", "https://weibo.com/"),
      ("qrid", &ticket.token),
      ("rid", ""),
      ("ver", VERSION),
    ],
  )
  .await?;
  match v.i64("retcode") {
    Some(OK) => {
      finish(c, &v).await?;
      Ok(QrStatus::Confirmed)
    }
    Some(WAITING) => Ok(QrStatus::Waiting),
    Some(SCANNED) => Ok(QrStatus::Scanned),
    Some(EXPIRED) => Ok(QrStatus::Expired),
    _ => {
      let msg = v.str("msg").unwrap_or_default();
      if msg.contains("过期") || msg.contains("失效") {
        Ok(QrStatus::Expired)
      } else if msg.contains("扫") {
        Ok(QrStatus::Scanned)
      } else {
        Err(Error::auth(format!("QR login failed: {msg}")))
      }
    }
  }
}

/// Turn the confirmation into weibo.com cookies.
async fn finish(c: &Client, v: &Value) -> Result<()> {
  let http = &c.ctx.http;
  if let Some(alt) = v.str("data.alt") {
    let resp = http
      .get("https://login.sina.com.cn/sso/login.php")
      .header("referer", "https://weibo.com/")
      .queries([
        ("entry", "miniblog"),
        ("returntype", "TEXT"),
        ("crossdomain", "1"),
        ("cdult", "3"),
        ("domain", "weibo.com"),
        ("savestate", "30"),
        ("alt", alt.as_str()),
      ])
      .send()
      .await?;
    let ticket: Value = resp.value().unwrap_or_default();
    for url in ticket.list("crossDomainUrlList") {
      if let Some(url) = url.as_str() {
        // With a callback the hop answers with script instead of redirecting,
        // so its Set-Cookie headers reach the jar.
        let sep = if url.contains('?') { '&' } else { '?' };
        let url = format!(
          "{url}{sep}callback=sinaSSOController.doCrossDomainCallBack&scriptId=ssoscript0&client=ssologin.js(v1.4.19)"
        );
        let _ = http
          .get(url)
          .header("referer", "https://weibo.com/")
          .send()
          .await;
      }
    }
  }
  if let Some(url) = v.str("data.url") {
    let _ = http
      .get(url)
      .header("referer", "https://weibo.com/")
      .send()
      .await;
  }
  if !c.logged_in() {
    return Err(
      Error::auth("the scan was confirmed but Weibo set no session cookie")
        .with_hint("log in with `social weibo login --browser` or `--cookie '...'`"),
    );
  }
  Ok(())
}
