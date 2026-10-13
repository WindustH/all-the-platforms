//! Zhihu transport: base URLs, request headers and the error envelope.
//!
//! Every request carries `x-requested-with: fetch` and, once the `_xsrf`
//! cookie is known, the matching `x-xsrftoken` that write endpoints require.
//! Upstream failures (`{"error": {"code", "message"}}`) map to shared error codes.
//!
//! Zhihu additionally signs web requests with `x-zse-93` / `x-zse-96`
//! (derived from the `d_c0` cookie and the path). Logged-in sessions work
//! without it; anonymous API calls are refused (`10003`), and after a few of
//! them the IP is flagged (`40352`). So [`Api::call`] requires a session and
//! only the hot list and the login flow go through [`Api::call_public`].

use std::time::Duration;

use atp_core::http::{Method, Req};
use atp_core::text::{one_line, truncate};
use atp_core::{Ctx, Error, ErrorCode, Page, PageReq, Post, Result, Value, ValueExt};

pub const WWW: &str = "https://www.zhihu.com";
pub const V4: &str = "https://www.zhihu.com/api/v4";
pub const V3: &str = "https://www.zhihu.com/api/v3";
pub const ZHUANLAN: &str = "https://zhuanlan.zhihu.com/api";
/// The mobile API host; serves the hot list without a login.
pub const MOBILE: &str = "https://api.zhihu.com";

/// The Zhihu transport: owns the context; every request goes through it.
pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(mut ctx: Ctx) -> Self {
    ctx.http.set_header("referer", "https://www.zhihu.com/");
    Self { ctx }
  }

  /// Fail early without the `z_c0` session cookie.
  pub fn need_login(&self) -> Result<()> {
    self.ctx.require_login(&["z_c0"]).map_err(|e| Error {
      message: "Zhihu serves this to logged-in sessions only".into(),
      ..e
    })
  }

  pub fn request(&self, method: Method, url: &str) -> Req<'_> {
    let req = self
      .ctx
      .http
      .request(method, url)
      .header("x-requested-with", "fetch");
    match self.ctx.http.cookie("_xsrf") {
      Some(xsrf) => req.header("x-xsrftoken", xsrf),
      None => req,
    }
  }

  pub fn get(&self, url: &str) -> Req<'_> {
    self.request(Method::GET, url)
  }

  /// A GET to the mobile API host (`path` or a full URL) as the Android app
  /// sends it; it takes the web session cookies.
  pub fn mobile(&self, path: &str) -> Req<'_> {
    let url = if path.starts_with("http") {
      path.to_owned()
    } else {
      format!("{MOBILE}{path}")
    };
    self
      .get(&url)
      .header("x-api-version", "3.0.93")
      .header("x-app-version", "11.4.0")
      .header("x-app-za", "OS=Android&Release=13&VersionName=11.4.0&VersionCode=21000&Product=com.zhihu.android&DeviceType=AndroidPhone")
  }

  pub fn post(&self, url: &str) -> Req<'_> {
    self.request(Method::POST, url)
  }

  pub fn delete(&self, url: &str) -> Req<'_> {
    self.request(Method::DELETE, url)
  }

  /// Send with the logged-in session; see [`Api::call_public`].
  pub async fn call(&self, req: Req<'_>) -> Result<Value> {
    self.need_login()?;
    self.call_public(req).await
  }

  /// Send and return the JSON body (`Null` for empty 2xx bodies), or the mapped error.
  pub async fn call_public(&self, req: Req<'_>) -> Result<Value> {
    let resp = req.send().await?;
    let body: Option<Value> = serde_json::from_slice(&resp.body).ok();
    if resp.status.is_success() && body.as_ref().is_none_or(|b| !b["error"].is_object()) {
      return match body {
        Some(v) => Ok(v),
        None if resp.body.iter().all(u8::is_ascii_whitespace) => Ok(Value::Null),
        None => Err(Error::upstream(format!(
          "unexpected response: {}",
          truncate(&one_line(&resp.text()), 200)
        ))),
      };
    }
    let body = body.unwrap_or(Value::Null);
    Err(self.error(resp.status.as_u16(), &body, &resp.text()))
  }

  /// Map a failed answer (`{"error": {"code", "message"}}`) to a shared error.
  pub fn error(&self, status: u16, body: &Value, text: &str) -> Error {
    let code = body.i64("error.code");
    let message = body
      .first_str(&["error.message", "message"])
      .unwrap_or_else(|| format!("HTTP {status}: {}", truncate(&one_line(text), 200)));
    let logged_in = self.ctx.http.has_cookie("z_c0");
    let login_hint = self.ctx.login_hint();
    let need_login = body.bool("error.need_login") == Some(true);
    match (status, code) {
      (_, Some(40352 | 40362)) => {
        let hint = match body.str("error.redirect") {
          Some(url) if logged_in => format!("pass the check in a browser: {url}"),
          Some(url) => format!("{login_hint}, or pass the check in a browser: {url}"),
          None => login_hint,
        };
        Error::new(ErrorCode::VerificationRequired, message).with_hint(hint)
      }
      (_, Some(10003)) if !logged_in => Error::auth(format!(
        "{message} (Zhihu only answers signed x-zse-96 requests anonymously)"
      ))
      .with_hint(login_hint.clone()),
      (_, Some(10003)) => Error::new(ErrorCode::SignatureError, message),
      (401, _) | (_, Some(100 | 101)) => Error::auth(message).with_hint(login_hint.clone()),
      _ if need_login => Error::auth(message).with_hint(login_hint.clone()),
      (403, _) => Error::new(ErrorCode::PermissionDenied, message),
      (404, _) => Error::not_found(message),
      (429, _) => Error::new(ErrorCode::RateLimited, message),
      _ => Error::upstream(message),
    }
  }

  /// Fetch `_xsrf` and `d_c0` when the session lacks them (write endpoints need `_xsrf`).
  pub async fn helper_cookies(&self) -> Result<()> {
    let http = &self.ctx.http;
    if !http.has_cookie("_xsrf") {
      // The home page sets `_xsrf`; its status does not matter.
      let _ = http
        .get(format!("{WWW}/"))
        .header("accept", "text/html,application/xhtml+xml")
        .send()
        .await?;
    }
    if !http.has_cookie("d_c0") {
      let _ = self
        .post(&format!("{WWW}/udid"))
        .json(&serde_json::json!({}))
        .send()
        .await?;
    }
    Ok(())
  }

  /// Writes need a login and the CSRF cookie; pause a little like a person would.
  pub async fn prepare_write(&self) -> Result<()> {
    self.need_login()?;
    self.helper_cookies().await?;
    self
      .ctx
      .http
      .pause(Duration::from_millis(400), Duration::from_millis(1200))
      .await;
    Ok(())
  }

  /// One page of a v4 listing at `path` (offset paging), with `include` fields.
  pub async fn list(&self, path: &str, include: Option<&str>, page: &PageReq) -> Result<Value> {
    let mut req = self.get(&format!("{V4}/{path}")).queries([
      ("offset", page.number_or(0)),
      ("limit", page.size_within(20) as u64),
    ]);
    if let Some(include) = include {
      req = req.query("include", include);
    }
    self.call(req).await
  }

  /// `initialState` of a server-rendered page (`<script id="js-initialData">`).
  /// Some objects (questions, topics) are only served to unsigned clients this way:
  /// their v4 API endpoints demand the `x-zse-96` signature.
  pub async fn page_state(&self, url: &str) -> Result<Value> {
    self.need_login()?;
    let resp = self
      .ctx
      .http
      .get(url)
      .header("accept", "text/html,application/xhtml+xml")
      .send()
      .await?;
    let html = resp.text();
    if resp.url.contains("/account/unhuman") {
      return Err(
        Error::new(
          ErrorCode::VerificationRequired,
          "Zhihu wants a verification",
        )
        .with_hint(format!("pass the check in a browser: {}", resp.url)),
      );
    }
    if !resp.status.is_success() {
      return Err(self.error(resp.status.as_u16(), &Value::Null, &html));
    }
    let data = html
      .split_once(r#"<script id="js-initialData" type="text/json">"#)
      .and_then(|(_, rest)| rest.split_once("</script>"))
      .map(|(json, _)| json)
      .ok_or_else(|| Error::upstream(format!("no page data in {url}")))?;
    let v: Value = serde_json::from_str(data)?;
    Ok(v.at("initialState").clone())
  }
}

/// The posts of a v4 listing, mapped by `map`, with the next offset.
pub fn posts(v: &Value, map: fn(&Value) -> Post) -> Page<Post> {
  let items = v.list("data").iter().map(map).collect();
  Page::new(items, next_param(v, "offset"))
}

/// The value of `key` in `paging.next`, unless the listing has ended.
pub fn next_param(v: &Value, key: &str) -> Option<String> {
  let url = next_url(v)?;
  url::Url::parse(&url)
    .ok()?
    .query_pairs()
    .find(|(k, _)| k == key)
    .map(|(_, v)| v.into_owned())
    .filter(|v| !v.is_empty())
}

/// The query string of `paging.next`, unless the listing has ended.
pub fn next_query(v: &Value) -> Option<String> {
  let url = next_url(v)?;
  url.split_once('?').map(|(_, q)| q.to_owned())
}

fn next_url(v: &Value) -> Option<String> {
  if v.bool("paging.is_end").unwrap_or(true) || v.list("data").is_empty() {
    return None;
  }
  let next = v.str("paging.next")?;
  // Some listings return a path relative to the v4 API.
  Some(if next.starts_with('/') {
    format!("{V4}{next}")
  } else {
    next
  })
}
