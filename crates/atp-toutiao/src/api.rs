//! Toutiao transport. Five hosts: `www` for JSON APIs (two feeds signed
//! with ByteDance's `a_bogus`, the rest plain), `m` for mobile pages that
//! carry their data as `RENDER_DATA` (the `www` pages hide theirs behind a
//! script challenge), `so` for search pages, `mp` for the creator backend
//! and `api.toutiaoapi.com`, the app host whose comment API the share
//! pages call. A signed call whose signature is refused answers an empty
//! body. [`Api`] owns the context.

use atp_bytedance::abogus::{self, TOUTIAO};
use atp_bytedance::query::{encode, query_string};
use atp_bytedance::visitor;
use atp_core::http::{Method, USER_AGENT};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};

pub const WWW: &str = "https://www.toutiao.com";
pub const MOBILE: &str = "https://m.toutiao.com";
pub const SEARCH: &str = "https://so.toutiao.com";
pub const MP: &str = "https://mp.toutiao.com";
/// The app API host Toutiao's share pages read comments from.
pub const SHARE: &str = "https://api.toutiaoapi.com";
/// Cookies of a logged-in session.
pub const LOGIN_COOKIES: &[&str] = &["sessionid"];
/// The creator backend's app id.
pub const MP_APP: &str = "1231";
const MOBILE_UA: &str = "Mozilla/5.0 (Linux; Android 13; Pixel 7) AppleWebKit/537.36 \
   (KHTML, like Gecko) Chrome/140.0.0.0 Mobile Safari/537.36";
const WALL_HINT: &str = "Toutiao refused a signed request; retry later, or log in with \
   `atp toutiao login`";
const LOGIN_HINT: &str = "log in with `atp toutiao login` (QR code) or `--browser`";

pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(mut ctx: Ctx) -> Self {
    ctx.http.set_header("referer", &format!("{WWW}/"));
    Self { ctx }
  }

  pub fn require_login(&self) -> Result<()> {
    self
      .ctx
      .require_login(LOGIN_COOKIES)
      .map_err(|e| e.with_hint(LOGIN_HINT))
  }

  /// A plain JSON call on `www`.
  pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    self.get_at(WWW, path, query).await
  }

  /// A plain JSON call on another host (`base`).
  pub async fn get_at(&self, base: &str, path: &str, query: &[(&str, String)]) -> Result<Value> {
    let resp = self
      .ctx
      .http
      .get(format!("{base}{path}"))
      .queries(query.iter().map(|(k, v)| (*k, v)))
      .send()
      .await?;
    self.absorb_token(&resp);
    json(&resp.text(), path)
  }

  /// A call signed with `a_bogus` (the query carries `aid` 24 and the
  /// visitor's `msToken`, and is sent exactly as signed).
  pub async fn signed(
    &self,
    method: Method,
    path: &str,
    query: &[(&str, String)],
    form: &[(&str, String)],
  ) -> Result<Value> {
    visitor::ensure(&self.ctx).await;
    let http = &self.ctx.http;
    let token = http.cookie("msToken").unwrap_or_else(visitor::ms_token);
    let mut pairs: Vec<(String, String)> = query
      .iter()
      .map(|(k, v)| ((*k).to_owned(), v.clone()))
      .collect();
    pairs.push(("aid".into(), TOUTIAO.aid.to_string()));
    pairs.push(("app_name".into(), "toutiao_web".into()));
    pairs.push(("msToken".into(), token));
    let body = query_string(
      &form
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect::<Vec<_>>(),
    );
    let query = query_string(&pairs);
    let now = jiff::Timestamp::now().as_millisecond();
    let a_bogus = abogus::sign(TOUTIAO, &query, &body, USER_AGENT, now);
    let url = format!("{WWW}{path}?{query}&a_bogus={}", encode(&a_bogus));
    let mut req = http.request(method, url);
    if !form.is_empty() {
      req = req.bytes(
        body.into_bytes(),
        "application/x-www-form-urlencoded; charset=UTF-8",
      );
    }
    let resp = req.send().await?;
    self.absorb_token(&resp);
    if resp.body.iter().all(u8::is_ascii_whitespace) {
      return Err(Error::new(ErrorCode::VerificationRequired, "empty answer").with_hint(WALL_HINT));
    }
    json(&resp.text(), path)
  }

  /// A mobile page (`m.toutiao.com`), which serves its data without the
  /// `www` script challenge.
  pub async fn mobile_page(&self, url: &str) -> Result<atp_core::http::Resp> {
    self
      .ctx
      .http
      .get(url)
      .header("user-agent", MOBILE_UA)
      .header("accept", "text/html,application/xhtml+xml")
      .send()
      .await
  }

  /// A creator-backend call (`mp.toutiao.com`, logged in).
  pub async fn mp(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    self.require_login()?;
    let resp = self
      .ctx
      .http
      .get(format!("{MP}{path}"))
      .query("app_id", MP_APP)
      .queries(query.iter().map(|(k, v)| (*k, v)))
      .header("referer", format!("{MP}/"))
      .send()
      .await?;
    let v = json(&resp.text(), path)?;
    match v.i64("code") {
      Some(0) | None => Ok(v),
      Some(code) => Err(mp_error(code, &v)),
    }
  }

  /// Keep the `msToken` every `www` answer hands out for the next call.
  fn absorb_token(&self, resp: &atp_core::http::Resp) {
    if let Some(token) = resp.header("x-ms-token").filter(|t| !t.is_empty()) {
      self.ctx.http.set_cookie("msToken", token);
    }
  }
}

fn json(text: &str, path: &str) -> Result<Value> {
  serde_json::from_str(text).map_err(|_| {
    Error::new(
      ErrorCode::VerificationRequired,
      format!("Toutiao answered {path} with a page instead of data"),
    )
    .with_hint(WALL_HINT)
  })
}

fn mp_error(code: i64, v: &Value) -> Error {
  let message = v.str("message").unwrap_or_else(|| format!("code {code}"));
  if matches!(code, 1 | 3 | 7) && message.contains("登录") {
    return Error::auth(message).with_hint(LOGIN_HINT);
  }
  Error::upstream(format!("creator backend: {message}"))
}

/// A `www` call's own status: `err_no` (comments) or `message` (feeds).
pub fn check(v: Value) -> Result<Value> {
  if let Some(code) = v.i64("err_no").filter(|c| *c != 0) {
    let message = v.str("message").unwrap_or_default();
    return Err(Error::upstream(format!("Toutiao error {code}: {message}")));
  }
  match v.str("message").as_deref() {
    None | Some("success") | Some("") => Ok(v),
    Some("error") | Some("fail") => Err(Error::upstream("Toutiao refused the request")),
    Some(_) => Ok(v),
  }
}

/// `GET` or `POST`, for [`Api::signed`].
pub const GET: Method = Method::GET;
pub const POST: Method = Method::POST;
