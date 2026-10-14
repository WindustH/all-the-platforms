//! NetEase Cloud Music transport: the web player's `weapi` calls on
//! `music.163.com` and the desktop client's `eapi` calls on
//! `interface.music.163.com`, both encrypted by [`sign`], and the
//! `{code, ...}` envelope every answer comes in.
//!
//! Most calls go the web player's way. A few the web player no longer
//! answers without a login (search) or never had (cloud uploads, comment
//! pages with cursors) go the desktop client's way, with its User-Agent and
//! its client cookies (`os`, `appver` ...) next to the session's.

use std::time::Duration;

use atp_core::http::Req;
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};
use serde_json::Map;

use crate::sign;

pub const WEB: &str = "https://music.163.com";
const CLIENT: &str = "https://interface.music.163.com";

/// Session cookies of a logged-in account.
pub const LOGIN_COOKIES: &[&str] = &["MUSIC_U"];
/// Session cookies the desktop calls carry along.
const SESSION_COOKIES: &[&str] = &["MUSIC_U", "MUSIC_A", "__csrf", "NMTID"];

const CLIENT_UA: &str = "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) Safari/537.36 Chrome/91.0.4472.164 NeteaseMusicDesktop/3.1.17.204416";
/// What the desktop client says about itself, in its cookies and in the
/// `header` of every eapi body.
const CLIENT_INFO: &[(&str, &str)] = &[
  ("os", "pc"),
  ("appver", "3.1.17.204416"),
  (
    "osver",
    "Microsoft-Windows-10-Professional-build-19045-64bit",
  ),
  ("channel", "netease"),
];

/// The transport: the context with the web player's headers.
pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(mut ctx: Ctx) -> Self {
    ctx.http.set_header("referer", WEB);
    ctx.http.set_header("origin", WEB);
    Self { ctx }
  }

  /// A web player call: `path` without its `/weapi/` prefix.
  pub fn web(&self, path: &str) -> Call<'_> {
    Call::new(self, Route::Web, path)
  }

  /// A desktop client call: `path` without its `/eapi/` prefix.
  pub fn client(&self, path: &str) -> Call<'_> {
    Call::new(self, Route::Client, path)
  }

  /// Fail with `not_authenticated` unless the session cookie is present.
  pub fn require_login(&self) -> Result<()> {
    self.ctx.require_login(LOGIN_COOKIES)
  }

  /// The error an answer's `code` stands for.
  pub fn error(&self, code: i64, v: &Value) -> Error {
    error(&self.ctx.login_hint(), code, v)
  }

  fn csrf(&self) -> String {
    self.ctx.http.cookie("__csrf").unwrap_or_default()
  }
}

#[derive(Clone, Copy)]
enum Route {
  Web,
  Client,
}

pub struct Call<'a> {
  api: &'a Api,
  route: Route,
  path: String,
  data: Map<String, Value>,
  write: bool,
}

impl<'a> Call<'a> {
  fn new(api: &'a Api, route: Route, path: &str) -> Self {
    Self {
      api,
      route,
      path: path.trim_start_matches('/').to_owned(),
      data: Map::new(),
      write: false,
    }
  }

  /// A field of the JSON body.
  pub fn arg(mut self, key: &str, value: impl Into<Value>) -> Self {
    self.data.insert(key.to_owned(), value.into());
    self
  }

  /// A change to the account: needs a login and is paced like a person.
  pub fn write(mut self) -> Self {
    self.write = true;
    self
  }

  /// Send and return the whole answer, or the error its `code` stands for.
  pub async fn send(self) -> Result<Value> {
    let api = self.api;
    let v = self.send_raw().await?;
    match v.i64("code") {
      None | Some(200) => Ok(v),
      Some(code) => Err(api.error(code, &v)),
    }
  }

  /// Send and return the whole answer, whatever its `code`.
  pub async fn send_raw(self) -> Result<Value> {
    let http = &self.api.ctx.http;
    if self.write {
      self.api.require_login()?;
      http
        .pause(Duration::from_millis(500), Duration::from_millis(1500))
        .await;
    }
    let req = match self.route {
      Route::Web => self.web_request(),
      Route::Client => self.client_request(),
    };
    req.send().await?.check()?.value()
  }

  fn web_request(&self) -> Req<'a> {
    let csrf = self.api.csrf();
    let mut data = self.data.clone();
    data.insert("csrf_token".into(), json!(csrf));
    let text = Value::Object(data).to_string();
    self
      .api
      .ctx
      .http
      .post(format!("{WEB}/weapi/{}", self.path))
      .query("csrf_token", csrf)
      .form(sign::weapi(&text))
  }

  fn client_request(&self) -> Req<'a> {
    let http = &self.api.ctx.http;
    let csrf = self.api.csrf();
    let mut header: Map<String, Value> = CLIENT_INFO
      .iter()
      .map(|(k, v)| ((*k).to_owned(), json!(v)))
      .collect();
    header.insert("__csrf".into(), json!(csrf));
    header.insert("requestId".into(), json!(request_id()));
    let mut cookie: Vec<String> = CLIENT_INFO
      .iter()
      .map(|(k, v)| format!("{k}={v}"))
      .collect();
    for name in SESSION_COOKIES {
      if let Some(value) = http.cookie(name) {
        header.insert((*name).to_owned(), json!(value));
        cookie.push(format!("{name}={value}"));
      }
    }
    let mut data = self.data.clone();
    data.insert("header".into(), Value::Object(header));
    data.insert("e_r".into(), json!(false));
    let text = Value::Object(data).to_string();
    let params = sign::eapi(&format!("/api/{}", self.path), &text);
    // The session cookies go by hand next to the client's own, so the
    // jar neither sends them twice nor keeps the guest cookies answers set.
    http
      .post(format!("{CLIENT}/eapi/{}", self.path))
      .no_cookies()
      .header("cookie", cookie.join("; "))
      .header("user-agent", CLIENT_UA)
      .form([("params", params)])
  }
}

/// `<millis>_<4 digits>`, the desktop client's request id.
fn request_id() -> String {
  let n = rand::random_range(0..10000);
  format!("{}_{n:04}", jiff::Timestamp::now().as_millisecond())
}

fn error(login_hint: &str, code: i64, v: &Value) -> Error {
  let message = v
    .first_str(&[
      "message",
      "msg",
      "data.blockText",
      "data.message",
      "data.msg",
    ])
    .unwrap_or_else(|| "request refused".into());
  let text = format!("{message} ({code})");
  match code {
    301 | 50000005 => Error::auth(text).with_hint(login_hint),
    -462 | -463 | 8821 => verification(text, v),
    -460 => Error::new(ErrorCode::IpBlocked, text)
      .with_hint("NetEase refused requests from this network for now; retry later or log in"),
    405 | 406 | -447 => Error::new(ErrorCode::RateLimited, text),
    -2 | 401 | 403 => Error::new(ErrorCode::PermissionDenied, text),
    400 => Error::input(text),
    404 | -404 => Error::not_found(text),
    _ => Error::upstream(text),
  }
}

/// A risk check: `verification_required`, with the page that clears it
/// when the answer names one.
fn verification(text: String, v: &Value) -> Error {
  let error = Error::new(ErrorCode::VerificationRequired, text)
    .with_hint("NetEase wants a human check: open the page, or pass it in the app and retry");
  let page = v
    .first_str(&[
      "data.url",
      "data.verifyUrl",
      "data.redirectUrl",
      "url",
      "redirectUrl",
    ])
    .filter(|u| u.starts_with("http"));
  match page {
    Some(url) => error.with_url(url),
    None => error,
  }
}
