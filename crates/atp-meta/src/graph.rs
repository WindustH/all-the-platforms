//! The Graph API: `https://graph.facebook.com/<version>/<path>` (and
//! `graph.instagram.com` for Instagram-login tokens), authorized by an access
//! token, paged by `paging.cursors.after`. Answers are checked in
//! [`crate::errors`].

use atp_core::credentials;
use atp_core::http::{Method, Part};
use atp_core::{Ctx, PageReq, Result, User, Value, ValueExt};

use crate::errors::check;

/// Graph API version used unless `META_GRAPH_VERSION` says otherwise.
pub const VERSION: &str = "v24.0";
pub const FACEBOOK: &str = "https://graph.facebook.com";
pub const INSTAGRAM: &str = "https://graph.instagram.com";

/// Session extra holding a token given to `login --token`.
pub const TOKEN_KEY: &str = "graph_token";

/// Query parameters or form fields.
pub type Params = Vec<(String, String)>;

/// `key=value` pairs from string slices (empty values left out).
pub fn params(pairs: &[(&str, &str)]) -> Params {
  pairs
    .iter()
    .filter(|(_, v)| !v.is_empty())
    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
    .collect()
}

/// The `fields` parameter alone.
pub fn fields(names: &str) -> Params {
  params(&[("fields", names)])
}

/// The first environment variable among `names` that is set, else the token
/// saved with the session.
pub fn token(ctx: &Ctx, names: &[&str]) -> Option<String> {
  credentials::env_any(names)
    .or_else(|| ctx.extra(TOKEN_KEY))
    .map(|t| t.trim().to_owned())
}

/// The host a token belongs to: Instagram-login tokens start with `IG`.
pub fn host_for(token: &str) -> &'static str {
  if token.starts_with("IG") {
    INSTAGRAM
  } else {
    FACEBOOK
  }
}

/// `login --token`: check the token against `host` and keep it.
pub async fn login(ctx: &Ctx, host: &str, token: &str) -> Result<()> {
  Graph::new(ctx, host, token)
    .get("me", &fields("id"))
    .await?;
  ctx.set_extra(TOKEN_KEY, token);
  Ok(())
}

/// The person, Page or system user a token belongs to.
pub async fn me(graph: &Graph<'_>, names: &str) -> Result<User> {
  let v = graph.get("me", &fields(names)).await?;
  Ok(User {
    id: v.str("id").unwrap_or_default(),
    name: v.str("name").unwrap_or_default(),
    url: v.str("link"),
    extra: [("source".to_owned(), Value::from("graph-api"))].into(),
    raw: Some(v),
    ..User::default()
  })
}

pub struct Graph<'a> {
  ctx: &'a Ctx,
  token: String,
  base: String,
}

impl<'a> Graph<'a> {
  /// A client for `host` ([`FACEBOOK`] or [`INSTAGRAM`]).
  pub fn new(ctx: &'a Ctx, host: &str, token: impl Into<String>) -> Self {
    let version = credentials::env("META_GRAPH_VERSION").unwrap_or_else(|| VERSION.to_owned());
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
    let mut form: Vec<(String, String)> = form.to_vec();
    form.push(("access_token".into(), self.token.clone()));
    let resp = self
      .ctx
      .http
      .post(self.url(path))
      .no_cookies()
      .form(form)
      .send()
      .await?;
    check(resp)
  }

  /// POST a multipart form (file uploads such as Page photos).
  pub async fn post_multipart(&self, path: &str, mut parts: Vec<Part>) -> Result<Value> {
    parts.push(Part::text("access_token", self.token.clone()));
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
      .request(Method::DELETE, self.url(path))
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
    let mut query: Params = query.to_vec();
    query.push(("limit".into(), req.size_within(max).to_string()));
    if let Some(after) = &req.cursor {
      query.push(("after".into(), after.clone()));
    }
    let v = self.get(path, &query).await?;
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
        Some(cursor) if !empty && (limit == 0 || out.len() < limit) => req.cursor = Some(cursor),
        _ => break,
      }
    }
    if limit > 0 {
      out.truncate(limit);
    }
    Ok(out)
  }
}
