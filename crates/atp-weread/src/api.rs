//! WeRead transport: the JSON routes of weread.qq.com's web reader
//! (`/web/…`, `/api/…`) with the session cookies `wr_vid` (the user
//! number), `wr_skey` (a short-lived key) and `wr_rt` (the refresh token).
//! Errors arrive as `{errCode, errMsg}`. `-2012` (登录超时) means the key
//! expired: like the reader, ATP renews it once through
//! `/web/login/renewal` and repeats the call.

use atp_core::http::{Method, Req};
use atp_core::text::{one_line, truncate};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};

pub const BASE: &str = "https://weread.qq.com";
/// Cookies of a logged-in session.
pub const LOGIN_COOKIES: &[&str] = &["wr_vid", "wr_skey"];
/// `errCode` of an expired key.
const EXPIRED: i64 = -2012;
const LOGIN_HINT: &str = "log in again: `atp weread login` (QR code) or `--browser`";

pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(mut ctx: Ctx) -> Self {
    ctx.http.set_header("referer", &format!("{BASE}/"));
    Self { ctx }
  }

  pub fn require_login(&self) -> Result<()> {
    self
      .ctx
      .require_login(LOGIN_COOKIES)
      .map_err(|e| e.with_hint(LOGIN_HINT))
  }

  /// The logged-in user number.
  pub fn vid(&self) -> Option<String> {
    self.ctx.http.cookie("wr_vid")
  }

  /// `GET <BASE><path>`.
  pub async fn get(&self, path: &str, params: &[(&str, String)]) -> Result<Value> {
    let build = || {
      self
        .ctx
        .http
        .get(format!("{BASE}{path}"))
        .queries(params.iter().map(|(k, v)| (*k, v)))
    };
    self.call(build, path).await
  }

  /// Send, renewing an expired key once.
  async fn call<'a>(&'a self, build: impl Fn() -> Req<'a>, path: &str) -> Result<Value> {
    let v = read(build().send().await?, path)?;
    if code(&v) == Some(EXPIRED) && self.ctx.http.has_cookie("wr_rt") && self.renew().await? {
      let again = read(build().send().await?, path)?;
      return self.check(again);
    }
    self.check(v)
  }

  /// A fresh `wr_skey` from `wr_rt`, as the reader asks for one.
  async fn renew(&self) -> Result<bool> {
    let resp = self
      .ctx
      .http
      .request(Method::POST, format!("{BASE}/web/login/renewal"))
      .header("origin", BASE)
      .json(&json!({"rq": "%2Fweb%2Fbook%2Fread", "ql": false}))
      .send()
      .await?;
    let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
    Ok(v.i64("succ") == Some(1))
  }

  /// WeRead's error envelope as a shared error.
  fn check(&self, v: Value) -> Result<Value> {
    let Some(code) = code(&v) else {
      return Ok(v);
    };
    let message = v.first_str(&["errMsg", "errmsg"]).unwrap_or_default();
    let text = format!("WeRead: {message} ({code})");
    Err(match code {
      -2010 | -2013 | -12013 => Error::auth(text).with_hint(LOGIN_HINT),
      EXPIRED => Error::auth(format!(
        "{text}: the session expired and could not be renewed"
      ))
      .with_hint(LOGIN_HINT),
      -2003 | -2004 => Error::not_found(text),
      _ => Error::upstream(text),
    })
  }
}

/// The error code of an answer, if it is an error.
fn code(v: &Value) -> Option<i64> {
  v.first_str(&["errCode", "errcode"])
    .and_then(|c| c.parse::<i64>().ok())
    .filter(|c| *c != 0)
}

fn read(resp: atp_core::http::Resp, path: &str) -> Result<Value> {
  serde_json::from_slice(&resp.body).map_err(|_| {
    if !resp.status.is_success() {
      return atp_core::http::status_error(resp.status, &resp.text());
    }
    Error::new(
      ErrorCode::UpstreamError,
      format!(
        "WeRead answered {path} with a page instead of data: {}",
        truncate(&one_line(&resp.text()), 120)
      ),
    )
  })
}
