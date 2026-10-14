//! Logins for both routes.
//!
//! - Web: a QR code from Baidu's passport (`getqrcode`), scanned with the
//!   Baidu or Baidu Netdisk app; `channel/unicast` reports the scan, and
//!   `qrbdusslogin` turns the confirmation into the session cookies
//!   (`BDUSS`, `STOKEN` …). Passport's single sign-on then hands the drive
//!   its own `STOKEN`, which shares need. Or `--browser` / `--cookie`.
//! - Open platform: `login --token APP_KEY:SECRET_KEY` (or `env` for
//!   `BAIDUPAN_APP_KEY` / `BAIDUPAN_SECRET_KEY`) of the user's own app,
//!   then the device-code consent (RFC 8628) on a phone.

use std::time::Duration;

use atp_core::oauth::{self, DeviceCode, Poll};
use atp_core::{Error, ErrorCode, QrStatus, QrTicket, Result, Value, ValueExt, credentials};

use crate::api::{Api, PAN};
use crate::open::{APP_KEY, ENV_KEY, ENV_SECRET, SECRET_KEY};

const PASSPORT: &str = "https://passport.baidu.com";
const TPL: &str = "netdisk";
/// Baidu asks for no faster polls of the device code than this.
const DEVICE_POLL: Duration = Duration::from_secs(5);
/// `channel/unicast` holds a poll open until something happens.
const LONG_POLL: Duration = Duration::from_secs(60);
const AFTER_LOGIN: &str = "https://pan.baidu.com/disk/main";
const APP_HINT: &str = "Baidu does not know this app: check the AppKey and SecretKey of your \
  app in the open platform's console (https://pan.baidu.com/union)";

/// After cookies came in (QR, browser, pasted): check the web drive takes
/// them. Skipped after `login --token`, which checks the token instead.
pub async fn prepare(api: &Api) -> Result<()> {
  if api.token_login.get() || !api.web_login() {
    return Ok(());
  }
  api.login_status().await.map(|_| ())
}

/// `login --token APP_KEY:SECRET_KEY`: the device-code consent of the
/// user's own app; the tokens and the app are kept with the session.
pub async fn token(api: &Api, token: &str) -> Result<()> {
  let (key, secret, given) = app(token)?;
  let open = api.open();
  let query = [
    ("response_type", "device_code".to_owned()),
    ("client_id", key.clone()),
    ("scope", "basic,netdisk".into()),
  ];
  let answer = open.oauth("device/code", &query).await?;
  let mut code = DeviceCode::parse(&answer).map_err(|e| match e.code {
    ErrorCode::NotAuthenticated => e.with_hint(APP_HINT),
    _ => e,
  })?;
  code.complete_url = Some(format!(
    "https://openapi.baidu.com/device?display=mobile&code={}",
    code.user_code
  ));
  code.interval = code.interval.max(DEVICE_POLL);
  let tokens = oauth::device_login(&api.ctx, &code, async || {
    let query = [
      ("grant_type", "device_token".to_owned()),
      ("code", code.device_code.clone()),
      ("client_id", key.clone()),
      ("client_secret", secret.clone()),
    ];
    Poll::parse(&open.oauth("token", &query).await?)
  })
  .await?;
  keep_app(api, given.then_some((&key, &secret)))?;
  tokens.save(&api.ctx)?;
  api.token_login.set(true);
  Ok(())
}

/// The app's key and secret, and whether they were given (not from the
/// environment).
fn app(token: &str) -> Result<(String, String, bool)> {
  if token.eq_ignore_ascii_case("env") {
    return match (credentials::env(ENV_KEY), credentials::env(ENV_SECRET)) {
      (Some(key), Some(secret)) => Ok((key, secret, false)),
      _ => Err(Error::input(format!(
        "set {ENV_KEY} and {ENV_SECRET}, or pass `--token APP_KEY:SECRET_KEY`"
      ))),
    };
  }
  let fields = credentials::expect(token, 2, 2, "APP_KEY:SECRET_KEY")?;
  Ok((fields[0].clone(), fields[1].clone(), true))
}

/// Keep the app the tokens belong to (they renew only through it), on disk
/// at once like the tokens; an app from the environment is not written.
fn keep_app(api: &Api, app: Option<(&String, &String)>) -> Result<()> {
  let (key, secret) = app.map_or((String::new(), String::new()), |(k, s)| {
    (k.clone(), s.clone())
  });
  credentials::save(&api.ctx, &[(APP_KEY, &key), (SECRET_KEY, &secret)]);
  api.ctx.store.update_session(|session| {
    for (name, value) in [(APP_KEY, &key), (SECRET_KEY, &secret)] {
      if value.is_empty() {
        session.extra.remove(name);
      } else {
        session.extra.insert(name.to_owned(), value.clone());
      }
    }
  })
}

/// A QR code of passport, in a fresh jar: the scan logs in whoever scans
/// it, and passport's own cookies (`BAIDUID`) carry the QR session along.
pub async fn qr_start(api: &Api) -> Result<QrTicket> {
  api.ctx.http.replace_cookies(Default::default());
  let now = millis();
  let v = api
    .ctx
    .http
    .get(format!("{PASSPORT}/v2/api/getqrcode"))
    .query("lp", "pc")
    .query("qrloginfrom", "pc")
    .query("apiver", "v3")
    .query("tpl", TPL)
    .query("tt", now)
    .header("referer", format!("{PAN}/"))
    .send()
    .await?
    .check()?
    .value()?;
  let (Some(sign), Some(image_url)) = (v.str("sign"), v.str("imgurl")) else {
    return Err(failed("getqrcode", &v));
  };
  let image_url = if image_url.starts_with("http") {
    image_url
  } else {
    format!("https://{image_url}")
  };
  let image = api
    .ctx
    .http
    .get(&image_url)
    .no_cookies()
    .header("referer", format!("{PAN}/"))
    .send()
    .await?
    .check()?
    .body;
  Ok(QrTicket {
    url: image_url,
    token: sign,
    image: Some(image),
    ..QrTicket::default()
  })
}

pub async fn qr_poll(api: &Api, ticket: &QrTicket) -> Result<QrStatus> {
  let now = millis();
  let sent = api
    .ctx
    .http
    .get(format!("{PASSPORT}/channel/unicast"))
    .query("channel_id", &ticket.token)
    .query("tpl", TPL)
    .query("apiver", "v3")
    .query("tt", now)
    .query("_", now)
    .timeout(LONG_POLL)
    .retries(0)
    .send()
    .await;
  let v = match sent {
    Ok(resp) => resp.value()?,
    // The long poll ran out without news.
    Err(e) if e.code == ErrorCode::NetworkError => return Ok(QrStatus::Waiting),
    Err(e) => return Err(e),
  };
  // `channel_v` is JSON in a string.
  let channel: Value = v
    .str("channel_v")
    .and_then(|c| serde_json::from_str(&c).ok())
    .unwrap_or_default();
  tracing::debug!(
    "unicast: errno {:?}, status {:?}",
    v.i64("errno"),
    channel.i64("status")
  );
  match v.i64("errno") {
    Some(0) => {}
    Some(1) => return Ok(QrStatus::Waiting),
    _ => return Err(failed("channel/unicast", &v)),
  }
  if let Some(code) = channel.str("v") {
    confirm(api, &code).await?;
    return Ok(QrStatus::Confirmed);
  }
  Ok(match channel.i64("status") {
    Some(1) => QrStatus::Scanned,
    Some(s) if s < 0 => QrStatus::Expired,
    _ => QrStatus::Waiting,
  })
}

/// Turn a confirmed scan into the session cookies.
async fn confirm(api: &Api, code: &str) -> Result<()> {
  let http = &api.ctx.http;
  let now = millis();
  http
    .get(format!("{PASSPORT}/v3/login/main/qrbdusslogin"))
    .query("v", now)
    .query("bduss", code)
    .query("u", AFTER_LOGIN)
    .query("qrcode", "1")
    .query("tpl", TPL)
    .query("apiver", "v3")
    .query("tt", now)
    .header("referer", format!("{PAN}/"))
    .send()
    .await?;
  if !http.has_cookie("BDUSS") {
    return Err(
      Error::auth("Baidu accepted the scan but set no BDUSS cookie")
        .with_hint("log in with `atp baidupan login --browser` or `--cookie '...'` instead"),
    );
  }
  // Single sign-on: passport hands pan.baidu.com its own STOKEN.
  let sso = http
    .get(format!("{PASSPORT}/v3/login/api/auth/"))
    .query("return_type", "5")
    .query("tpl", TPL)
    .query("u", "https://pan.baidu.com/disk/home")
    .send()
    .await;
  if let Err(e) = sso {
    tracing::debug!("passport sign-on to the drive failed: {}", e.message);
  }
  Ok(())
}

fn failed(step: &str, v: &Value) -> Error {
  let answer: String = v.to_string().chars().take(200).collect();
  Error::upstream(format!("Baidu login {step}: {answer}"))
}

fn millis() -> i64 {
  jiff::Timestamp::now().as_millisecond()
}
