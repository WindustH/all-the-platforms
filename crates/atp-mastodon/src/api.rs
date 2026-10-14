//! Mastodon transport: the REST API of one instance
//! (`https://<instance>/api/v1/…`, `/api/v2/…`), reads as `GET` with query
//! parameters, writes as `POST` / `DELETE` with form fields, the access
//! token as `Authorization: Bearer`, under ATP's own User-Agent.
//!
//! The instance and token come from `MASTODON_URL` / `MASTODON_TOKEN` when
//! set, else from the session (`login --token`), else mastodon.social
//! without a token. Listings page through the `Link: <…>; rel="next"`
//! header; its paging parameters (`max_id`, `offset`) are the cursor.
//! Errors arrive as `{"error": "…"}` and map onto `atp_core::Error`.

use std::path::Path;
use std::time::Duration;

use atp_core::file::Upload;
use atp_core::http::{Method, Part, Resp, TOOL_USER_AGENT};
use atp_core::{Ctx, Error, ErrorCode, Page, PageReq, Result, Value, ValueExt, credentials};

pub const DEFAULT_INSTANCE: &str = "mastodon.social";
pub const ENV_INSTANCE: &str = "MASTODON_URL";
pub const ENV_TOKEN: &str = "MASTODON_TOKEN";
pub const INSTANCE_KEY: &str = "instance";
pub const TOKEN_KEY: &str = "token";
/// The most rows Mastodon lists at once.
const PAGE_MAX: usize = 40;
/// The paging parameters a `Link` header carries.
const PAGING_KEYS: [&str; 2] = ["max_id", "offset"];

/// Query parameters.
pub type Query = Vec<(&'static str, String)>;

pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self { ctx }
  }

  /// The instance's host (`mastodon.social`).
  pub fn instance(&self) -> String {
    credentials::env(ENV_INSTANCE)
      .map(|url| credentials::host(&url))
      .or_else(|| self.ctx.extra(INSTANCE_KEY))
      .filter(|host| !host.is_empty())
      .unwrap_or_else(|| DEFAULT_INSTANCE.to_owned())
  }

  pub fn base(&self) -> String {
    format!("https://{}", self.instance())
  }

  pub fn token(&self) -> Option<String> {
    credentials::env(ENV_TOKEN)
      .or_else(|| self.ctx.extra(TOKEN_KEY))
      .filter(|token| !token.is_empty())
  }

  pub fn logged_in(&self) -> bool {
    self.token().is_some()
  }

  pub fn require(&self) -> Result<String> {
    self.token().ok_or_else(|| {
      Error::auth(format!("not logged in to {}", self.instance()))
        .with_hint("run `atp mastodon login --token INSTANCE` (see `atp guide login mastodon`)")
    })
  }

  pub fn save(&self, instance: &str, token: &str) {
    credentials::save(&self.ctx, &[(INSTANCE_KEY, instance), (TOKEN_KEY, token)]);
  }

  /// `GET /api/<path>` (`v1/statuses/1`), with the token when there is one.
  pub async fn get(&self, path: &str, query: Query) -> Result<Value> {
    let resp = self.send(Method::GET, path, query, None).await?;
    check(&resp)
  }

  /// One page of a listing: `limit` and the cursor added to `query`, each
  /// row through `item` (which gets the instance too), the next page from
  /// the `Link` header.
  pub async fn list<T>(
    &self,
    path: &str,
    mut query: Query,
    page: &PageReq,
    item: impl Fn(&Value, &str) -> T,
  ) -> Result<Page<T>> {
    query.push(("limit", page.size_within(PAGE_MAX).to_string()));
    if let Some(cursor) = &page.cursor {
      query.extend(cursor_pairs(cursor));
    }
    let resp = self.send(Method::GET, path, query, None).await?;
    let v = check(&resp)?;
    let instance = self.instance();
    let rows = v.as_array().map(Vec::as_slice).unwrap_or_default();
    let items: Vec<T> = rows.iter().map(|row| item(row, &instance)).collect();
    let next = next_cursor(&resp).filter(|_| !items.is_empty());
    Ok(Page::new(items, next))
  }

  /// A change: logged in, after a short pause, with form fields.
  pub async fn write(&self, method: Method, path: &str, form: Query) -> Result<Value> {
    self.require()?;
    self
      .ctx
      .http
      .pause(Duration::from_millis(300), Duration::from_millis(900))
      .await;
    let resp = self.send(method, path, Vec::new(), Some(form)).await?;
    check(&resp)
  }

  async fn send(
    &self,
    method: Method,
    path: &str,
    query: Query,
    form: Option<Query>,
  ) -> Result<Resp> {
    let mut req = self
      .ctx
      .http
      .request(method, format!("{}/api/{path}", self.base()))
      .no_cookies()
      .header("user-agent", TOOL_USER_AGENT)
      .queries(query);
    if let Some(token) = self.token() {
      req = req.header("authorization", format!("Bearer {token}"));
    }
    if let Some(form) = form {
      req = req.form(form);
    }
    req.send().await
  }

  /// Upload a file (`POST /api/v2/media`) and wait until the instance has
  /// processed it; its id goes into `media_ids[]`.
  pub async fn upload(&self, path: &Path, description: Option<&str>) -> Result<String> {
    let token = self.require()?;
    let file = Upload::read(path).await?;
    let mut parts = vec![Part::file("file", file.data, &file.name, file.mime)];
    if let Some(text) = description {
      parts.push(Part::text("description", text));
    }
    let resp = self
      .ctx
      .http
      .post(format!("{}/api/v2/media", self.base()))
      .no_cookies()
      .header("user-agent", TOOL_USER_AGENT)
      .header("authorization", format!("Bearer {token}"))
      .multipart(parts)
      .timeout(Duration::from_secs(300))
      .send()
      .await?;
    let v = check(&resp)?;
    let id = v
      .str("id")
      .ok_or_else(|| Error::upstream("the media upload returned no id"))?;
    if v.str("url").is_none() {
      self.processed(&id).await?;
    }
    Ok(id)
  }

  /// Poll `GET /api/v1/media/:id` until the file has a URL (a minute at most).
  async fn processed(&self, id: &str) -> Result<()> {
    for _ in 0..30 {
      tokio::time::sleep(Duration::from_secs(2)).await;
      let v = self.get(&format!("v1/media/{id}"), Vec::new()).await?;
      if v.str("url").is_some() {
        return Ok(());
      }
    }
    Err(Error::upstream(format!(
      "media {id} was still processing after a minute; try again"
    )))
  }
}

/// The paging parameters of a cursor (`max_id=…&offset=…`).
fn cursor_pairs(cursor: &str) -> Query {
  url::form_urlencoded::parse(cursor.as_bytes())
    .filter_map(|(key, value)| {
      let key = PAGING_KEYS.iter().find(|k| **k == key)?;
      Some((*key, value.into_owned()))
    })
    .collect()
}

/// The paging parameters of the `rel="next"` link, as the next cursor.
fn next_cursor(resp: &Resp) -> Option<String> {
  let link = resp.header("link")?;
  let next = link.split(',').find(|part| part.contains("rel=\"next\""))?;
  let url = next.split(['<', '>']).nth(1)?;
  let url = url::Url::parse(url).ok()?;
  let mut out = url::form_urlencoded::Serializer::new(String::new());
  let mut any = false;
  for (key, value) in url.query_pairs() {
    if PAGING_KEYS.contains(&key.as_ref()) {
      out.append_pair(&key, &value);
      any = true;
    }
  }
  any.then(|| out.finish())
}

/// The body as JSON, or the `{"error": …}` it carries as an error.
fn check(resp: &Resp) -> Result<Value> {
  let status = resp.status.as_u16();
  let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
  if (200..300).contains(&status) {
    return Ok(v);
  }
  let message = v.str("error").unwrap_or_else(|| format!("HTTP {status}"));
  let text = format!("Mastodon: {message}");
  Err(match status {
    401 => Error::auth(text)
      .with_hint("the token was refused; log in again (`atp guide login mastodon`)"),
    422 if message.contains("authenticated") => Error::auth(text)
      .with_hint("the instance shows this to logged-in accounts only: `atp guide login mastodon`"),
    403 => Error::new(ErrorCode::PermissionDenied, text)
      .with_hint("the token may lack this scope, or the instance closes this to you"),
    404 => Error::not_found(text),
    410 => Error::not_found(format!("{text} (gone)")),
    429 => Error::new(ErrorCode::RateLimited, text)
      .with_hint("the instance limits requests (300 per 5 minutes by default); wait and retry"),
    400 | 422 => Error::input(text),
    _ => Error::upstream(text),
  })
}
