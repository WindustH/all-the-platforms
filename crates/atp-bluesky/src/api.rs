//! Bluesky transport: AT Protocol XRPC (`<host>/xrpc/<nsid>`), queries as
//! `GET` with parameters, procedures as `POST` with JSON bodies, under
//! ATP's own User-Agent.
//!
//! Without an account, queries go to the public AppView
//! (`public.api.bsky.app`). With one, everything goes to the account's
//! PDS with its access JWT; the PDS answers `app.bsky.*` queries through
//! its AppView, with your view of each post (liked, reposted, saved). The
//! access JWT lasts a couple of hours and is renewed with the refresh JWT
//! (`com.atproto.server.refreshSession`) before it runs out.

use std::time::Duration;

use atp_core::http::{Method, Resp, TOOL_USER_AGENT};
use atp_core::oauth::{self, Tokens};
use atp_core::{Ctx, Error, ErrorCode, Page, PageReq, Result, Value, ValueExt, json};

use crate::login;
use crate::refs::AtUri;

pub const PUBLIC: &str = "https://public.api.bsky.app";
pub const PDS_KEY: &str = "pds";
pub const DID_KEY: &str = "did";
pub const HANDLE_KEY: &str = "handle";
/// The most rows a listing answers at once.
const PAGE_MAX: usize = 100;

/// Query parameters.
pub type Query = Vec<(&'static str, String)>;

/// Now, as records date themselves.
pub fn now() -> String {
  jiff::Timestamp::now()
    .strftime("%Y-%m-%dT%H:%M:%S%.3fZ")
    .to_string()
}

pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self { ctx }
  }

  pub fn logged_in(&self) -> bool {
    Tokens::saved(&self.ctx).is_some() && self.pds().is_some()
  }

  pub fn pds(&self) -> Option<String> {
    self.ctx.extra(PDS_KEY).filter(|pds| !pds.is_empty())
  }

  /// Your DID; fails without an account.
  pub fn did(&self) -> Result<String> {
    self
      .ctx
      .extra(DID_KEY)
      .filter(|did| !did.is_empty() && self.logged_in())
      .ok_or_else(|| {
        Error::auth("not logged in to Bluesky").with_hint(
          "run `atp bluesky login --token 'HANDLE|APP_PASSWORD'` (see `atp guide login bluesky`)",
        )
      })
  }

  /// A usable access JWT, renewed first when it is about to run out.
  async fn access(&self) -> Result<String> {
    self.did()?;
    oauth::access_token(&self.ctx, async |refresh| {
      login::refresh(self, refresh).await
    })
    .await
  }

  /// A query: through your PDS when logged in, else the public AppView.
  pub async fn get(&self, nsid: &str, query: Query) -> Result<Value> {
    if self.logged_in() {
      return self.get_auth(nsid, query).await;
    }
    let resp = self
      .send(Method::GET, PUBLIC, nsid, query, None, None)
      .await?;
    check(&resp)
  }

  /// A query that needs the account.
  pub async fn get_auth(&self, nsid: &str, query: Query) -> Result<Value> {
    let token = self.access().await?;
    let pds = self.pds().unwrap_or_default();
    let resp = self
      .send(Method::GET, &pds, nsid, query, None, Some(&token))
      .await?;
    check(&resp)
  }

  /// A procedure (a change): logged in, after a short pause.
  pub async fn call(&self, nsid: &str, body: Value) -> Result<Value> {
    let token = self.access().await?;
    self
      .ctx
      .http
      .pause(Duration::from_millis(300), Duration::from_millis(900))
      .await;
    let pds = self.pds().unwrap_or_default();
    let resp = self
      .send(
        Method::POST,
        &pds,
        nsid,
        Vec::new(),
        Some(body),
        Some(&token),
      )
      .await?;
    check(&resp)
  }

  /// Create a record in your repository; its `uri` and `cid`.
  pub async fn create_record(&self, collection: &str, record: Value) -> Result<Value> {
    let body = json!({"repo": self.did()?, "collection": collection, "record": record});
    self.call("com.atproto.repo.createRecord", body).await
  }

  /// Delete one of your records by its URI.
  pub async fn delete_record(&self, uri: &str) -> Result<()> {
    let did = self.did()?;
    let at = AtUri::parse(uri).ok_or_else(|| Error::input(format!("not a record: {uri}")))?;
    if at.repo != did {
      return Err(Error::new(
        ErrorCode::PermissionDenied,
        format!("{uri} is not yours"),
      ));
    }
    let body = json!({"repo": did, "collection": at.collection, "rkey": at.rkey});
    self.call("com.atproto.repo.deleteRecord", body).await?;
    Ok(())
  }

  /// One page of a cursor listing: rows under `key`, each through `item`.
  pub async fn list<T>(
    &self,
    nsid: &str,
    mut query: Query,
    key: &str,
    page: &PageReq,
    item: impl Fn(&Value) -> Option<T>,
  ) -> Result<Page<T>> {
    query.push(("limit", page.size_within(PAGE_MAX).to_string()));
    if let Some(cursor) = &page.cursor {
      query.push(("cursor", cursor.clone()));
    }
    let v = self.get(nsid, query).await?;
    let items: Vec<T> = v.list(key).iter().filter_map(item).collect();
    let next = v.str("cursor").filter(|_| !items.is_empty());
    Ok(Page::new(items, next))
  }

  /// Store a file in your repository (`com.atproto.repo.uploadBlob`); the
  /// blob it answers goes into a record.
  pub async fn upload_blob(&self, data: Vec<u8>, mime: &str) -> Result<Value> {
    let token = self.access().await?;
    let pds = self.pds().unwrap_or_default();
    let resp = self
      .ctx
      .http
      .post(format!("{pds}/xrpc/com.atproto.repo.uploadBlob"))
      .no_cookies()
      .header("user-agent", TOOL_USER_AGENT)
      .header("authorization", format!("Bearer {token}"))
      .bytes(data, mime)
      .timeout(Duration::from_secs(300))
      .send()
      .await?;
    let v = check(&resp)?;
    Ok(v.at("blob").clone())
  }

  /// One XRPC request to `host`.
  pub async fn send(
    &self,
    method: Method,
    host: &str,
    nsid: &str,
    query: Query,
    body: Option<Value>,
    token: Option<&str>,
  ) -> Result<Resp> {
    let mut req = self
      .ctx
      .http
      .request(method, format!("{host}/xrpc/{nsid}"))
      .no_cookies()
      .header("user-agent", TOOL_USER_AGENT)
      .queries(query);
    if let Some(token) = token {
      req = req.header("authorization", format!("Bearer {token}"));
    }
    if let Some(body) = &body {
      req = req.json(body);
    }
    req.send().await
  }
}

/// The body as JSON, or the XRPC error (`{"error", "message"}`) it carries.
pub fn check(resp: &Resp) -> Result<Value> {
  let status = resp.status.as_u16();
  let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
  if (200..300).contains(&status) {
    return Ok(v);
  }
  let code = v.str("error").unwrap_or_default();
  let message = v.str("message").unwrap_or_else(|| format!("HTTP {status}"));
  let text = if code.is_empty() {
    format!("Bluesky: {message}")
  } else {
    format!("Bluesky: {message} ({code})")
  };
  Err(match (status, code.as_str()) {
    (403, "") => Error::auth(format!("{text}: the public AppView refused this"))
      .with_hint("Bluesky serves this to logged-in accounts only: `atp guide login bluesky`"),
    (401, _) | (_, "AuthRequired" | "ExpiredToken" | "InvalidToken" | "AuthenticationRequired") => {
      Error::auth(text).with_hint("log in again: `atp guide login bluesky`")
    }
    (429, _) | (_, "RateLimitExceeded") => Error::new(ErrorCode::RateLimited, text)
      .with_hint("Bluesky limits requests per account and per IP; wait and retry"),
    (_, "BlockedActor" | "BlockedByActor") | (403, _) => {
      Error::new(ErrorCode::PermissionDenied, text)
    }
    (_, code) if code.contains("NotFound") || message.to_lowercase().contains("not found") => {
      Error::not_found(text)
    }
    (400, _) => Error::input(text),
    _ => Error::upstream(text),
  })
}
