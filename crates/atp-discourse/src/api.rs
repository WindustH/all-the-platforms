//! Discourse transport: the JSON API of one forum (`<forum>/<path>.json`),
//! reads as `GET`, writes as `POST` / `PUT` / `DELETE` with JSON bodies.
//! Requests go out as the browser-like client; a forum whose browser
//! check stops it is asked again under ATP's own declared User-Agent,
//! which some checks let through (and then first, for a day).
//!
//! The forum comes from `DISCOURSE_URL` when set, else from the session,
//! else meta.discourse.org. Credentials are a User API Key
//! (`User-Api-Key` + `User-Api-Client-Id`) from the forum's own consent
//! page, or an API key made by an admin (`Api-Key` + `Api-Username`);
//! `DISCOURSE_API_KEY` / `DISCOURSE_API_USERNAME` supply the latter for one
//! run. Errors arrive as `{"errors": […], "error_type": …}`; Cloudflare's
//! browser check is `verification_required`.

use std::path::Path;
use std::time::Duration;

use atp_core::file::Upload;
use atp_core::http::{Method, Part, Req, Resp, TOOL_USER_AGENT};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, credentials};

pub const DEFAULT_FORUM: &str = "https://meta.discourse.org";
pub const ENV_URL: &str = "DISCOURSE_URL";
pub const ENV_KEY: &str = "DISCOURSE_API_KEY";
pub const ENV_USERNAME: &str = "DISCOURSE_API_USERNAME";
pub const FORUM_KEY: &str = "forum";
pub const KEY_KEY: &str = "api_key";
pub const USERNAME_KEY: &str = "username";
/// Set for a User API Key: the client id it was made for.
pub const CLIENT_KEY: &str = "client_id";

/// How long a forum is asked under ATP's own User-Agent first.
const TOOL_TTL: Duration = Duration::from_secs(24 * 3600);

/// Query parameters.
pub type Query = Vec<(&'static str, String)>;

/// How requests prove who they are.
#[derive(Debug, Clone)]
pub enum Auth {
  /// A key an admin made, acting as `username`.
  Key { key: String, username: String },
  /// A User API Key and the client id it belongs to.
  User { key: String, client: String },
}

pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self { ctx }
  }

  /// The forum's base URL (`https://meta.discourse.org`, or with the
  /// subfolder it lives in).
  pub fn forum(&self) -> String {
    credentials::env(ENV_URL)
      .map(|url| forum_url(&url))
      .or_else(|| self.ctx.extra(FORUM_KEY))
      .filter(|url| !url.is_empty())
      .unwrap_or_else(|| DEFAULT_FORUM.to_owned())
  }

  pub fn auth(&self) -> Option<Auth> {
    if let (Some(key), Some(username)) = (credentials::env(ENV_KEY), credentials::env(ENV_USERNAME))
    {
      return Some(Auth::Key { key, username });
    }
    let key = self.ctx.extra(KEY_KEY).filter(|k| !k.is_empty())?;
    match self.ctx.extra(CLIENT_KEY).filter(|c| !c.is_empty()) {
      Some(client) => Some(Auth::User { key, client }),
      None => Some(Auth::Key {
        key,
        username: self.ctx.extra(USERNAME_KEY).unwrap_or_default(),
      }),
    }
  }

  pub fn logged_in(&self) -> bool {
    self.auth().is_some()
  }

  pub fn require(&self) -> Result<Auth> {
    self.auth().ok_or_else(|| {
      Error::auth(format!("not logged in to {}", self.forum()))
        .with_hint("run `atp discourse login --token FORUM_URL` (see `atp guide login discourse`)")
    })
  }

  /// Your username, as saved at login (or `DISCOURSE_API_USERNAME`).
  pub fn username(&self) -> Option<String> {
    credentials::env(ENV_USERNAME)
      .or_else(|| self.ctx.extra(USERNAME_KEY))
      .filter(|name| !name.is_empty())
  }

  /// `GET <forum><path>` (`/latest.json`).
  pub async fn get(&self, path: &str, query: Query) -> Result<Value> {
    let resp = self
      .exchange(Method::GET, path, |req| req.queries(query.clone()))
      .await?;
    self.check(resp)
  }

  /// A change, logged in, after a short pause; `body` as JSON.
  pub async fn write(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
    self.require()?;
    self
      .ctx
      .http
      .pause(Duration::from_millis(500), Duration::from_millis(1500))
      .await;
    let resp = self
      .exchange(method, path, |req| match &body {
        Some(body) => req.json(body),
        None => req,
      })
      .await?;
    self.check(resp)
  }

  /// Upload a file for a post (`/uploads.json`); its `short_url` goes
  /// into the text.
  pub async fn upload(&self, path: &Path) -> Result<Value> {
    self.require()?;
    let file = Upload::read(path).await?;
    let parts = vec![
      Part::text("type", "composer"),
      Part::text("synchronous", "true"),
      Part::file("file", file.data, &file.name, file.mime),
    ];
    let req = self
      .request(Method::POST, "/uploads.json", self.tool_first())
      .multipart(parts)
      .timeout(Duration::from_secs(300));
    self.check(req.send().await?)
  }

  /// Send a request `build` completes, as the browser-like client; when a
  /// browser check answers instead of the forum, once more under ATP's
  /// own User-Agent (remembered for a day when that gets through).
  async fn exchange(
    &self,
    method: Method,
    path: &str,
    build: impl Fn(Req<'_>) -> Req<'_>,
  ) -> Result<Resp> {
    let tool = self.tool_first();
    let resp = build(self.request(method.clone(), path, tool))
      .send()
      .await?;
    if tool || !is_challenge(&resp) {
      return Ok(resp);
    }
    let retry = build(self.request(method, path, true)).send().await?;
    if !is_challenge(&retry) {
      self.ctx.store.cache_put(&self.tool_key(), &true);
    }
    Ok(retry)
  }

  fn tool_first(&self) -> bool {
    self
      .ctx
      .store
      .cache_get::<bool>(&self.tool_key(), TOOL_TTL)
      .unwrap_or(false)
  }

  fn tool_key(&self) -> String {
    format!("tool-ua-{}", credentials::host(&self.forum()))
  }

  fn request(&self, method: Method, path: &str, tool: bool) -> Req<'_> {
    let mut req = self
      .ctx
      .http
      .request(method, format!("{}{path}", self.forum()))
      .no_cookies()
      .header("accept", "application/json");
    if tool {
      req = req.header("user-agent", TOOL_USER_AGENT);
    }
    match self.auth() {
      Some(Auth::Key { key, username }) => {
        req.header("api-key", key).header("api-username", username)
      }
      Some(Auth::User { key, client }) => req
        .header("user-api-key", key)
        .header("user-api-client-id", client),
      None => req,
    }
  }

  /// The body as JSON, or what went wrong.
  fn check(&self, resp: Resp) -> Result<Value> {
    if is_challenge(&resp) {
      let forum = self.forum();
      // Requests go without cookies and the challenge is bound to the
      // browser that passes it, so a browser pass cannot clear it.
      return Err(
        Error::verification(format!("{forum} answered with a browser check (Cloudflare)"))
          .with_url(&forum)
          .with_hint("the forum checks clients before its API; it may let logged-in requests through, or try again later or from another network")
          .unclearable(),
      );
    }
    let status = resp.status.as_u16();
    let v: Option<Value> = serde_json::from_slice(&resp.body).ok();
    match v {
      Some(v) if (200..300).contains(&status) => Ok(v),
      Some(v) => Err(self.refused(status, &v)),
      None if resp.url.contains("/login") => Err(self.login_wall()),
      None if (200..300).contains(&status) => Err(Error::upstream(format!(
        "{} did not answer with Discourse JSON",
        self.forum()
      ))),
      None => Err(atp_core::http::status_error(resp.status, &resp.text())),
    }
  }

  fn login_wall(&self) -> Error {
    Error::auth(format!(
      "{} shows this to logged-in users only",
      self.forum()
    ))
    .with_hint("log in: `atp guide login discourse`")
  }

  /// An `{"errors", "error_type"}` answer as an error.
  fn refused(&self, status: u16, v: &Value) -> Error {
    let kind = v.str("error_type").unwrap_or_default();
    let messages: Vec<String> = v
      .list("errors")
      .iter()
      .filter_map(|e| e.as_str().map(str::to_owned))
      .collect();
    let text = if messages.is_empty() {
      format!("Discourse: HTTP {status}")
    } else {
      format!("Discourse: {}", messages.join("; "))
    };
    match (status, kind.as_str()) {
      (_, "not_logged_in") => self.login_wall(),
      (403, _) | (_, "invalid_access") if !self.logged_in() => self.login_wall(),
      (403, _) | (_, "invalid_access") => Error::new(ErrorCode::PermissionDenied, text),
      (429, _) | (_, "rate_limit") => {
        let wait = v.u64("extras.wait_seconds").unwrap_or(60);
        Error::new(ErrorCode::RateLimited, text).with_hint(format!("wait {wait} s and retry"))
      }
      (404, _) | (_, "not_found") => Error::not_found(text),
      (409, _) => Error::upstream(text)
        .with_hint("it changed since ATP loaded it; look at it again and repeat the edit"),
      (400 | 422, _) | (_, "invalid_parameters") => Error::input(text),
      _ => Error::upstream(text),
    }
  }

  /// An absolute link on the forum.
  pub fn link(&self, path: &str) -> String {
    format!("{}{path}", self.forum())
  }
}

/// Cloudflare's "Just a moment…" page instead of an answer.
fn is_challenge(resp: &Resp) -> bool {
  if !matches!(resp.status.as_u16(), 403 | 429 | 503) {
    return false;
  }
  if resp.header("cf-mitigated") == Some("challenge") {
    return true;
  }
  let text = resp.text();
  text.contains("Just a moment") || text.contains("cf-chl") || text.contains("challenge-platform")
}

/// `linux.do`, `https://linux.do/` → `https://linux.do`; a subfolder stays.
pub fn forum_url(raw: &str) -> String {
  let raw = raw.trim().trim_end_matches('/');
  if raw.starts_with("https://") || raw.starts_with("http://") {
    raw.to_owned()
  } else {
    format!("https://{raw}")
  }
}
