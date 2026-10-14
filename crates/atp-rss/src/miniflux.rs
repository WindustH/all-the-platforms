//! A Miniflux account (the user's own instance): its REST API under
//! `<URL>/v1`, with the API key as `X-Auth-Token`. Errors arrive as
//! `{"error_message": …}`.

use atp_core::http::{Method, TOOL_USER_AGENT, status_error};
use atp_core::{
  Action, Ctx, Error, ErrorCode, Page, PageReq, Post, Result, User, Value, ValueExt, credentials,
  json,
};

use crate::parse;

pub const URL: &str = "miniflux_url";
pub const KEY: &str = "miniflux_key";
pub const ENV_URL: &str = "MINIFLUX_URL";
pub const ENV_KEY: &str = "MINIFLUX_API_KEY";
/// `feed --type` values Miniflux answers, with the entry filter each sets.
pub const FEEDS: &[(&str, &str, &str)] = &[
  ("unread", "status", "unread"),
  ("all", "", ""),
  ("starred", "starred", "true"),
];
/// The most entries one request lists.
const PAGE_MAX: usize = 100;

pub struct Miniflux<'a> {
  ctx: &'a Ctx,
  url: String,
  key: String,
}

/// `miniflux.example.com/` → `https://miniflux.example.com`.
fn base(url: &str) -> String {
  let url = url.trim().trim_end_matches('/');
  if url.starts_with("http://") || url.starts_with("https://") {
    url.to_owned()
  } else {
    format!("https://{url}")
  }
}

impl<'a> Miniflux<'a> {
  pub fn from_session(ctx: &'a Ctx) -> Option<Self> {
    let (url, key) = match credentials::env(ENV_URL).zip(credentials::env(ENV_KEY)) {
      Some(pair) => pair,
      None => {
        let saved = credentials::saved(ctx, &[URL, KEY])?;
        (saved[0].clone(), saved[1].clone())
      }
    };
    Some(Self {
      ctx,
      url: base(&url),
      key,
    })
  }

  /// Keep `URL|API_KEY`; `whoami` checks them right after.
  pub fn login(ctx: &Ctx, token: &str) -> Result<()> {
    let fields = credentials::expect(token, 2, 2, "URL|API_KEY")?;
    credentials::save(ctx, &[(URL, &base(&fields[0])), (KEY, &fields[1])]);
    Ok(())
  }

  async fn call(
    &self,
    method: Method,
    path: &str,
    params: Vec<(&str, String)>,
    body: Option<Value>,
  ) -> Result<Value> {
    let mut req = self
      .ctx
      .http
      .request(method, format!("{}/v1/{path}", self.url))
      .no_cookies()
      .header("user-agent", TOOL_USER_AGENT)
      .header("x-auth-token", &self.key)
      .queries(params);
    if let Some(body) = &body {
      req = req.json(body);
    }
    let resp = req.send().await?;
    let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
    let message = v.str("error_message").unwrap_or_default();
    let text = format!("Miniflux: {message}");
    match resp.status.as_u16() {
      200..=299 => Ok(v),
      _ if message.is_empty() => Err(status_error(resp.status, &resp.text())),
      401 => Err(Error::auth(text).with_hint("check the API key: `atp guide login rss`")),
      403 => Err(Error::new(ErrorCode::PermissionDenied, text)),
      404 => Err(Error::not_found(text)),
      400 => Err(Error::input(text)),
      _ => Err(Error::upstream(text)),
    }
  }

  pub async fn me(&self) -> Result<User> {
    let v = self.call(Method::GET, "me", Vec::new(), None).await?;
    let name = v.str("username").unwrap_or_default();
    Ok(User {
      id: name.clone(),
      name,
      url: Some(self.url.clone()),
      raw: Some(v),
      ..User::default()
    })
  }

  /// One page of entries, the newest first; `filters` narrow them
  /// (`status`, `starred`, `search`, `feed_id` …).
  pub async fn entries(&self, filters: Vec<(&str, String)>, page: &PageReq) -> Result<Page<Post>> {
    let offset = page.number_or(0);
    let limit = page.size_within(PAGE_MAX);
    let mut params = vec![
      ("order", "published_at".to_owned()),
      ("direction", "desc".to_owned()),
      ("limit", limit.to_string()),
      ("offset", offset.to_string()),
    ];
    params.extend(filters);
    let v = self.call(Method::GET, "entries", params, None).await?;
    let rows = v.list("entries");
    let next = offset + rows.len() as u64;
    let more = !rows.is_empty() && v.u64("total").is_some_and(|total| next < total);
    Ok(Page::new(
      rows.iter().map(parse::miniflux_entry).collect(),
      more.then(|| next.to_string()),
    ))
  }

  /// `feed --type unread|all|starred`.
  pub async fn listing(&self, kind: &str, page: &PageReq) -> Result<Page<Post>> {
    let (_, key, value) = FEEDS
      .iter()
      .find(|(name, _, _)| *name == kind)
      .ok_or_else(|| Error::input(format!("unknown feed: {kind}")))?;
    let filters = if key.is_empty() {
      Vec::new()
    } else {
      vec![(*key, (*value).to_owned())]
    };
    self.entries(filters, page).await
  }

  pub async fn entry(&self, id: u64) -> Result<Post> {
    let v = self
      .call(Method::GET, &format!("entries/{id}"), Vec::new(), None)
      .await?;
    Ok(parse::miniflux_entry(&v))
  }

  /// Mark entries read (or unread).
  pub async fn mark(&self, ids: &[u64], read: bool) -> Result<()> {
    let status = if read { "read" } else { "unread" };
    let body = json!({"entry_ids": ids, "status": status});
    self
      .call(Method::PUT, "entries", Vec::new(), Some(body))
      .await?;
    Ok(())
  }

  /// Star an entry (or take the star away). Miniflux only toggles, so the
  /// entry is read first and left alone when it already is as asked.
  pub async fn star(&self, id: u64, undo: bool) -> Result<Action> {
    let entry = self.entry(id).await?;
    let starred = entry.extra.get("starred").and_then(Value::as_bool) == Some(true);
    let action = if undo { "unstar" } else { "star" };
    if starred == undo {
      let path = format!("entries/{id}/bookmark");
      self.call(Method::PUT, &path, Vec::new(), None).await?;
    }
    Ok(Action::done(action, format!("miniflux:{id}")))
  }

  /// The account's feeds with their unread counts.
  pub async fn feeds(&self) -> Result<Vec<User>> {
    let feeds = self.call(Method::GET, "feeds", Vec::new(), None).await?;
    let counters = self
      .call(Method::GET, "feeds/counters", Vec::new(), None)
      .await
      .unwrap_or_default();
    Ok(
      feeds
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|feed| {
          let unread = feed
            .str("id")
            .and_then(|id| counters.u64(&format!("unreads.{id}")));
          parse::miniflux_feed(feed, unread)
        })
        .collect(),
    )
  }

  /// Subscribe the account to a feed, in `category` or the first one.
  pub async fn subscribe(&self, url: &str, category: Option<u64>) -> Result<Action> {
    let category = match category {
      Some(id) => id,
      None => {
        let v = self
          .call(Method::GET, "categories", Vec::new(), None)
          .await?;
        v.list("")
          .first()
          .and_then(|c| c.u64("id"))
          .ok_or_else(|| {
            Error::not_found("the Miniflux account has no category to put the feed in")
          })?
      }
    };
    let body = json!({"feed_url": url, "category_id": category});
    let v = self
      .call(Method::POST, "feeds", Vec::new(), Some(body))
      .await?;
    let id = v.str("feed_id").unwrap_or_default();
    Ok(Action::done("subscribe", url).with_id(id))
  }

  /// Unsubscribe the account from a feed, by its Miniflux id or feed URL.
  pub async fn unsubscribe(&self, feed: &str) -> Result<Action> {
    let id = match feed.parse::<u64>() {
      Ok(id) => id,
      Err(_) => self
        .feeds()
        .await?
        .into_iter()
        .find(|f| f.url.as_deref() == Some(feed))
        .and_then(|f| f.id.parse().ok())
        .ok_or_else(|| Error::not_found(format!("the Miniflux account has no feed {feed}")))?,
    };
    self
      .call(Method::DELETE, &format!("feeds/{id}"), Vec::new(), None)
      .await?;
    Ok(Action::done("unsubscribe", feed))
  }
}
