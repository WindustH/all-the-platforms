//! Keyword and topic-tag search (`/keyword_search`), which needs Meta's
//! `threads_keyword_search` permission.

use atp_core::{Error, ErrorCode, Page, PageReq, Post, Query, Result};
use atp_meta::graph::params;

use crate::api::Api;
use crate::posts;

pub const SORTS: &[&str] = &["top", "recent"];

/// Posts with the words; `#tag` alone searches a topic tag.
pub async fn posts(api: &Api, query: &Query, req: &PageReq) -> Result<Page<Post>> {
  let keyword = query.keyword.trim();
  let (mode, q) = match keyword.strip_prefix('#') {
    Some(tag) if !tag.contains(' ') => ("TAG", tag),
    _ => ("KEYWORD", keyword),
  };
  let sort = match query.sort.as_deref() {
    Some("recent") => "RECENT",
    _ => "TOP",
  };
  let form = params(&[("q", q), ("search_type", sort), ("search_mode", mode)]);
  posts::list(api, "keyword_search", form, req)
    .await
    .map_err(permission)
}

fn permission(e: Error) -> Error {
  if e.code != ErrorCode::PermissionDenied {
    return e;
  }
  e.with_hint(
    "search needs the threads_keyword_search permission on your app and token; without Meta's \
     approval (advanced access) the app cannot search others' posts",
  )
}
