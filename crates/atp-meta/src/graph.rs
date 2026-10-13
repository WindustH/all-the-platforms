//! The Graph API: `https://graph.facebook.com/<version>/<path>` (and
//! `graph.instagram.com` for Instagram-login tokens), authorized by an access
//! token, paged by `paging.cursors.after`.
//!
//! Errors arrive as `{"error": {"message", "type", "code", "error_subcode",
//! "error_user_msg", "fbtrace_id"}}`; usage against the rate limits comes in
//! `X-App-Usage` / `X-Business-Use-Case-Usage` / `X-Ad-Account-Usage` headers
//! (percentages), logged and warned about near the limit.

use atp_core::http::Resp;
use atp_core::{Ctx, Error, ErrorCode, PageReq, Result, Value, ValueExt};

/// Graph API version used unless `META_GRAPH_VERSION` says otherwise.
pub const VERSION: &str = "v24.0";
pub const FACEBOOK: &str = "https://graph.facebook.com";
pub const INSTAGRAM: &str = "https://graph.instagram.com";

/// Session extra holding a token given to `login --token`.
pub const TOKEN_KEY: &str = "graph_token";

/// Query parameters or form fields.
pub type Params = Vec<(String, String)>;

/// `key=value` pairs from string slices.
pub fn params(pairs: &[(&str, &str)]) -> Params {
  pairs
    .iter()
    .filter(|(_, v)| !v.is_empty())
    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
    .collect()
}

/// The first non-empty environment variable among `names`, else the token
/// saved with the session.
pub fn token(ctx: &Ctx, names: &[&str]) -> Option<String> {
  names
    .iter()
    .find_map(|n| std::env::var(n).ok().filter(|v| !v.trim().is_empty()))
    .or_else(|| ctx.extra(TOKEN_KEY))
    .map(|t| t.trim().to_owned())
}

pub struct Graph<'a> {
  ctx: &'a Ctx,
  token: String,
  base: String,
}

impl<'a> Graph<'a> {
  /// A client for `host` ([`FACEBOOK`] or [`INSTAGRAM`]).
  pub fn new(ctx: &'a Ctx, host: &str, token: impl Into<String>) -> Self {
    let version = std::env::var("META_GRAPH_VERSION")
      .ok()
      .filter(|v| !v.trim().is_empty())
      .unwrap_or_else(|| VERSION.to_owned());
    Self {
      ctx,
      token: token.into(),
      base: format!("{host}/{version}"),
    }
  }

  /// The same API with another token (a Page's own token, for instance).
  pub fn with_token(&self, token: impl Into<String>) -> Graph<'a> {
    Graph {
      ctx: self.ctx,
      token: token.into(),
      base: self.base.clone(),
    }
  }

  pub fn token(&self) -> &str {
    &self.token
  }

  fn url(&self, path: &str) -> String {
    if path.starts_with("https://") {
      path.to_owned()
    } else {
      format!("{}/{}", self.base, path.trim_start_matches('/'))
    }
  }

  pub async fn get(&self, path: &str, query: &[(String, String)]) -> Result<Value> {
    let resp = self
      .ctx
      .http
      .get(self.url(path))
      .no_cookies()
      .queries(query.iter().map(|(k, v)| (k.as_str(), v)))
      .query("access_token", &self.token)
      .send()
      .await?;
    check(resp)
  }

  /// POST form fields.
  pub async fn post(&self, path: &str, form: &[(String, String)]) -> Result<Value> {
    let mut fields: Vec<(String, String)> = form.to_vec();
    fields.push(("access_token".into(), self.token.clone()));
    let resp = self
      .ctx
      .http
      .post(self.url(path))
      .no_cookies()
      .form(fields)
      .send()
      .await?;
    check(resp)
  }

  /// POST a multipart form (file uploads such as Page photos).
  pub async fn post_multipart(
    &self,
    path: &str,
    mut parts: Vec<atp_core::http::Part>,
  ) -> Result<Value> {
    parts.push(atp_core::http::Part::text(
      "access_token",
      self.token.clone(),
    ));
    let resp = self
      .ctx
      .http
      .post(self.url(path))
      .no_cookies()
      .multipart(parts)
      .send()
      .await?;
    check(resp)
  }

  pub async fn delete(&self, path: &str) -> Result<Value> {
    let resp = self
      .ctx
      .http
      .request(atp_core::http::Method::DELETE, self.url(path))
      .no_cookies()
      .query("access_token", &self.token)
      .send()
      .await?;
    check(resp)
  }

  /// One page of an edge: its `data` and the cursor of the next page.
  pub async fn page(
    &self,
    path: &str,
    query: &[(String, String)],
    req: &PageReq,
    max: usize,
  ) -> Result<(Vec<Value>, Option<String>)> {
    let mut q: Params = query.to_vec();
    q.push(("limit".into(), req.size_within(max).to_string()));
    if let Some(after) = &req.cursor {
      q.push(("after".into(), after.clone()));
    }
    let v = self.get(path, &q).await?;
    let next = v
      .str("paging.next")
      .and_then(|_| v.str("paging.cursors.after"));
    Ok((v.list("data").to_vec(), next))
  }

  /// Every item of an edge, up to `limit` (0 = all), following cursors.
  pub async fn all(
    &self,
    path: &str,
    query: &[(String, String)],
    limit: usize,
  ) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    let mut req = PageReq {
      cursor: None,
      size: if limit == 0 { 100 } else { limit.min(100) },
    };
    loop {
      let (items, next) = self.page(path, query, &req, 100).await?;
      let empty = items.is_empty();
      out.extend(items);
      match next {
        Some(c) if !empty && (limit == 0 || out.len() < limit) => req.cursor = Some(c),
        _ => break,
      }
    }
    if limit > 0 {
      out.truncate(limit);
    }
    Ok(out)
  }
}

/// The body as JSON, or the Graph error it carries mapped to an error code.
pub fn check(resp: Resp) -> Result<Value> {
  usage(&resp);
  let v: Value = resp.value().unwrap_or_else(|_| Value::String(resp.text()));
  let Some(err) = v.get("error").filter(|e| e.is_object()) else {
    if !resp.status.is_success() {
      return Err(atp_core::http::status_error(resp.status, &resp.text()));
    }
    return Ok(v);
  };
  Err(error(err))
}

/// A Graph error object as an [`Error`].
pub fn error(err: &Value) -> Error {
  let code = err.i64("code").unwrap_or(0);
  let sub = err.i64("error_subcode").unwrap_or(0);
  let msg = err
    .str("error_user_msg")
    .or_else(|| err.str("message"))
    .unwrap_or_else(|| "unknown error".into());
  let mut text = format!("Graph API error {code}");
  if sub != 0 {
    text += &format!("/{sub}");
  }
  text += &format!(": {msg}");
  if let Some(trace) = err.str("fbtrace_id") {
    text += &format!(" (fbtrace_id {trace})");
  }
  match code {
    104 | 190 | 102 | 2500 => Error::auth(text).with_hint(
      "the access token is invalid or expired: run `login --token` with a new one or update the environment variable",
    ),
    4 | 17 | 32 | 613 | 80000..=80014 => Error::new(ErrorCode::RateLimited, text)
      .with_hint("wait (usually an hour at most) and slow down; usage is in the X-App-Usage headers"),
    368 => Error::new(ErrorCode::PermissionDenied, text)
      .with_hint("Meta temporarily blocked this action for policy reasons"),
    10 | 200..=299 => Error::new(ErrorCode::PermissionDenied, text)
      .with_hint("the token lacks a permission for this call (check its scopes, and the app's review status)"),
    803 => Error::not_found(text),
    100 if sub == 33 => Error::not_found(text),
    100 => Error::input(text),
    _ => Error::upstream(text),
  }
}

/// Log the rate-limit usage headers; warn above 90 %.
fn usage(resp: &Resp) {
  for name in [
    "x-app-usage",
    "x-business-use-case-usage",
    "x-ad-account-usage",
  ] {
    let Some(raw) = resp.header(name) else {
      continue;
    };
    tracing::debug!("{name}: {raw}");
    let high = serde_json::from_str::<Value>(raw)
      .ok()
      .map(|v| max_percent(&v))
      .unwrap_or(0.0);
    if high >= 90.0 {
      tracing::warn!("{name} at {high:.0}% of Meta's rate limit; slow down");
    }
  }
}

fn max_percent(v: &Value) -> f64 {
  match v {
    Value::Number(n) => n.as_f64().unwrap_or(0.0),
    Value::Object(m) => m.values().map(max_percent).fold(0.0, f64::max),
    Value::Array(a) => a.iter().map(max_percent).fold(0.0, f64::max),
    _ => 0.0,
  }
}
