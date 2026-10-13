//! Searches: notes, users and hashtags, notes with a hashtag, trending
//! hashtags.

use social_core::{Collection, Page, PageReq, Post, Query, Result, User, json};

use crate::api::Api;
use crate::parse;

pub const FILTERS: &[&str] = &["local"];

/// Notes with the keyword; `#tag` alone lists the notes with that hashtag.
pub async fn posts(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  if let Some(tag) = query.keyword.strip_prefix('#').filter(|t| !t.contains(' ')) {
    let body = json!({"tag": tag});
    return api
      .list("notes/search-by-tag", body, page, parse::post)
      .await;
  }
  let mut body = json!({"query": query.keyword});
  if query.filter.as_deref() == Some("local") {
    body["host"] = json!(".");
  }
  api.list("notes/search", body, page, parse::post).await
}

pub async fn users(api: &Api, query: &Query, page: &PageReq) -> Result<Page<User>> {
  let body = json!({"query": query.keyword.trim_start_matches('@')});
  api
    .list_by_offset("users/search", body, page, parse::user)
    .await
}

pub async fn topics(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Collection>> {
  let body = json!({"query": query.keyword.trim_start_matches('#')});
  api
    .list_by_offset("hashtags/search", body, page, |row, _| parse::hashtag(row))
    .await
}

/// Notes with a hashtag.
pub async fn tagged(api: &Api, tag: &str, page: &PageReq) -> Result<Page<Post>> {
  let tag = tag.trim_start_matches('#');
  let body = json!({"tag": tag});
  api
    .list("notes/search-by-tag", body, page, parse::post)
    .await
}

/// Hashtags in use now (`hashtags/trend`).
pub async fn trends(api: &Api) -> Result<Page<Collection>> {
  let v = api.call("hashtags/trend", json!({})).await?;
  let items = v
    .as_array()
    .map(|rows| rows.iter().map(parse::hashtag).collect());
  Ok(Page::last(items.unwrap_or_default()))
}
