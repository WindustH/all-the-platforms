//! Matrix transport: the client-server API of a homeserver
//! (`/_matrix/client/v3/...`) with an access token.
//!
//! The homeserver and token come from `MATRIX_HOMESERVER` /
//! `MATRIX_ACCESS_TOKEN` when set, else from the session (`login --token`).
//! A server name (`matrix.org`) is turned into its client URL through
//! `/.well-known/matrix/client`. Without a token only the public room
//! directory answers. Errors (`{errcode, error}`) map onto
//! `social_core::Error`; `M_LIMIT_EXCEEDED` waits and retries.

use std::cell::RefCell;
use std::time::Duration;

use social_core::credentials;
use social_core::http::{Method, Req};
use social_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};

pub const CLIENT: &str = "/_matrix/client/v3";
pub const DEFAULT_SERVER: &str = "matrix.org";
const BASE_KEY: &str = "homeserver";
const TOKEN_KEY: &str = "token";
const USER_KEY: &str = "user_id";
const MAX_TRIES: u32 = 3;

pub struct Api {
  pub ctx: Ctx,
  env_base: Option<String>,
  env_token: Option<String>,
  base: RefCell<Option<String>>,
}

/// A path segment, percent-encoded (`!room:server`, `$event`, `#alias:server`).
pub fn seg(segment: &str) -> String {
  url::form_urlencoded::byte_serialize(segment.as_bytes()).collect()
}

/// A random transaction id for sends.
pub fn txn() -> String {
  format!("sc{:016x}", rand::random::<u64>())
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self {
      ctx,
      env_base: credentials::env("MATRIX_HOMESERVER").map(|b| normalize(&b)),
      env_token: credentials::env("MATRIX_ACCESS_TOKEN"),
      base: RefCell::new(None),
    }
  }

  /// The homeserver's client URL (`https://matrix-client.matrix.org`).
  pub async fn base(&self) -> String {
    if let Some(b) = self.base.borrow().clone() {
      return b;
    }
    let server = self
      .env_base
      .clone()
      .or_else(|| self.ctx.extra(BASE_KEY))
      .unwrap_or_else(|| DEFAULT_SERVER.to_owned());
    let found = discover(self, &server).await;
    *self.base.borrow_mut() = Some(found.clone());
    found
  }

  /// Forget the resolved homeserver (after the session's server changed).
  pub fn forget_base(&self) {
    *self.base.borrow_mut() = None;
  }

  pub fn token(&self) -> Option<String> {
    self
      .env_token
      .clone()
      .or_else(|| self.ctx.extra(TOKEN_KEY))
      .filter(|t| !t.is_empty())
  }

  pub fn logged_in(&self) -> bool {
    self.token().is_some()
  }

  pub fn user_id(&self) -> Option<String> {
    self.ctx.extra(USER_KEY)
  }

  pub fn require(&self) -> Result<String> {
    self.token().ok_or_else(|| {
      Error::auth("not logged in to Matrix").with_hint(
        "run `social matrix login --token 'HOMESERVER|ACCESS_TOKEN'` (or `HOMESERVER|@you:server|PASSWORD` through `--token -`)",
      )
    })
  }

  pub fn save(&self, base: &str, token: &str, user_id: &str, device: Option<&str>) {
    self.ctx.set_extra(BASE_KEY, base);
    self.ctx.set_extra(TOKEN_KEY, token);
    self.ctx.set_extra(USER_KEY, user_id);
    if let Some(d) = device {
      self.ctx.set_extra("device_id", d);
    }
  }

  async fn request(&self, method: Method, path: &str, token: Option<&str>) -> Req<'_> {
    let url = if path.starts_with("https://") {
      path.to_owned()
    } else if path.starts_with("/_matrix/") {
      format!("{}{path}", self.base().await)
    } else {
      format!("{}{CLIENT}{path}", self.base().await)
    };
    let req = self.ctx.http.request(method, url).no_cookies().retries(0);
    match token {
      Some(t) => req.header("authorization", format!("Bearer {t}")),
      None => req,
    }
  }

  /// One call; `body` goes as JSON for writes. Logged in when a token exists.
  pub async fn call(
    &self,
    method: Method,
    path: &str,
    query: &[(&str, String)],
    body: Option<&Value>,
  ) -> Result<Value> {
    let token = self.token();
    self
      .call_with(token.as_deref(), method, path, query, body, None)
      .await
  }

  /// A call that needs the account.
  pub async fn auth(
    &self,
    method: Method,
    path: &str,
    query: &[(&str, String)],
    body: Option<&Value>,
  ) -> Result<Value> {
    let token = self.require()?;
    self
      .call_with(Some(&token), method, path, query, body, None)
      .await
  }

  pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    self.auth(Method::GET, path, query, None).await
  }

  /// A write: logged in, after a short pause.
  pub async fn write(&self, method: Method, path: &str, body: &Value) -> Result<Value> {
    self.require()?;
    self
      .ctx
      .http
      .pause(Duration::from_millis(200), Duration::from_millis(700))
      .await;
    self.auth(method, path, &[], Some(body)).await
  }

  pub async fn call_with(
    &self,
    token: Option<&str>,
    method: Method,
    path: &str,
    query: &[(&str, String)],
    body: Option<&Value>,
    timeout: Option<Duration>,
  ) -> Result<Value> {
    let mut tries = 0;
    loop {
      tries += 1;
      let mut req = self
        .request(method.clone(), path, token)
        .await
        .queries(query.iter().map(|(k, v)| (*k, v)));
      if let Some(b) = body {
        req = req.json(b);
      }
      if let Some(t) = timeout {
        req = req.timeout(t);
      }
      let resp = req.send().await?;
      let status = resp.status.as_u16();
      let v: Value = if resp.body.iter().all(u8::is_ascii_whitespace) {
        Value::Null
      } else {
        serde_json::from_slice(&resp.body).unwrap_or_default()
      };
      if resp.status.is_success() {
        return Ok(v);
      }
      let code = v.str("errcode").unwrap_or_default();
      if (status == 429 || code == "M_LIMIT_EXCEEDED") && tries < MAX_TRIES {
        let ms = v.u64("retry_after_ms").unwrap_or(2000).min(30_000);
        tokio::time::sleep(Duration::from_millis(ms + 300)).await;
        continue;
      }
      return Err(error(
        status,
        &code,
        v.str("error"),
        self.env_token.is_some(),
      ));
    }
  }

  /// Upload bytes to the media repository; the `mxc://` URI comes back.
  pub async fn upload(&self, name: &str, data: Vec<u8>, mime: &str) -> Result<String> {
    let token = self.require()?;
    let url = format!("{}/_matrix/media/v3/upload", self.base().await);
    let resp = self
      .ctx
      .http
      .post(url)
      .no_cookies()
      .query("filename", name)
      .header("authorization", format!("Bearer {token}"))
      .bytes(data, mime)
      .timeout(Duration::from_secs(300))
      .send()
      .await?;
    let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
    if !resp.status.is_success() {
      return Err(error(
        resp.status.as_u16(),
        &v.str("errcode").unwrap_or_default(),
        v.str("error"),
        false,
      ));
    }
    v.str("content_uri")
      .ok_or_else(|| Error::upstream("the media upload returned no content_uri"))
  }

  /// An `mxc://server/id` as an authenticated download URL.
  pub async fn media_url(&self, mxc: &str) -> Option<String> {
    let rest = mxc.strip_prefix("mxc://")?;
    Some(format!(
      "{}/_matrix/client/v1/media/download/{rest}",
      self.base().await
    ))
  }

  /// Headers a media download needs (authenticated media).
  pub fn media_headers(&self) -> Vec<(String, String)> {
    self
      .token()
      .map(|t| vec![("authorization".to_owned(), format!("Bearer {t}"))])
      .unwrap_or_default()
  }
}

fn error(status: u16, code: &str, message: Option<String>, from_env: bool) -> Error {
  let text = match message {
    Some(m) if !code.is_empty() => format!("{m} ({code})"),
    Some(m) => m,
    None if !code.is_empty() => code.to_owned(),
    None => format!("HTTP {status}"),
  };
  match code {
    "M_UNKNOWN_TOKEN" | "M_MISSING_TOKEN" => Error::auth(text).with_hint(if from_env {
      "check MATRIX_ACCESS_TOKEN"
    } else {
      "the access token is no longer valid; log in again"
    }),
    "M_FORBIDDEN" => Error::new(ErrorCode::PermissionDenied, text),
    "M_NOT_FOUND" => Error::not_found(text),
    "M_LIMIT_EXCEEDED" => Error::new(ErrorCode::RateLimited, text),
    "M_BAD_JSON" | "M_NOT_JSON" | "M_INVALID_PARAM" | "M_UNRECOGNIZED" | "M_TOO_LARGE" => {
      Error::input(text)
    }
    _ => match status {
      401 => Error::auth(text),
      403 => Error::new(ErrorCode::PermissionDenied, text),
      404 => Error::not_found(text),
      _ => Error::upstream(format!("Matrix: {text}")),
    },
  }
}

/// `matrix.org` / `https://matrix.org/` → `https://matrix.org`.
pub fn normalize(server: &str) -> String {
  let server = server.trim().trim_end_matches('/');
  if server.starts_with("https://") || server.starts_with("http://") {
    server.to_owned()
  } else {
    format!("https://{server}")
  }
}

/// The client URL a server name delegates to (`.well-known`), cached a day.
pub async fn discover(api: &Api, server: &str) -> String {
  let base = normalize(server);
  let key = format!(
    "wellknown-{}",
    base
      .trim_start_matches("https://")
      .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
  );
  if let Some(found) = api
    .ctx
    .store
    .cache_get::<String>(&key, Duration::from_secs(24 * 3600))
  {
    return found;
  }
  let v = api
    .ctx
    .http
    .get(format!("{base}/.well-known/matrix/client"))
    .no_cookies()
    .send()
    .await
    .ok()
    .filter(|r| r.status.is_success())
    .and_then(|r| r.value().ok())
    .unwrap_or_else(|| json!({}));
  // The key has a dot in it, so no `ValueExt` path.
  let found = v
    .get("m.homeserver")
    .and_then(|h| h.get("base_url"))
    .and_then(Value::as_str)
    .map(normalize)
    .unwrap_or(base);
  api.ctx.store.cache_put(&key, &found);
  found
}
