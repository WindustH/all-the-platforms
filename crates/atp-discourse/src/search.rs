//! Search (`/search.json`, pages from 1) and users (`/u/search/users`).

use std::collections::HashMap;

use atp_core::{Page, PageReq, Post, Query, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::parse::Forum;

pub const SORTS: &[&str] = &["relevance", "latest", "likes", "views"];

/// Posts with the words, each pointing at its place in its topic. The
/// forum's own search syntax works in the words (`in:title`,
/// `#category`, `@user`, `status:solved` …).
pub async fn posts(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let mut q = query.keyword.trim().to_owned();
  match query.sort.as_deref() {
    Some("latest") => q.push_str(" order:latest"),
    Some("likes") => q.push_str(" order:likes"),
    Some("views") => q.push_str(" order:views"),
    _ => {}
  }
  let number = page.number_or(1).max(1);
  let params = vec![("q", q), ("page", number.to_string())];
  let v = api.get("/search.json", params).await?;
  let titles: HashMap<String, Value> = v
    .list("topics")
    .iter()
    .filter_map(|t| Some((t.str("id")?, t.clone())))
    .collect();
  let base = api.forum();
  let forum = Forum { base: &base };
  let posts: Vec<Post> = v
    .list("posts")
    .iter()
    .map(|p| {
      let topic = p.str("topic_id").and_then(|id| titles.get(&id));
      let mut row = p.clone();
      if let Some(slug) = topic.and_then(|t| t.str("slug")) {
        row["topic_slug"] = Value::from(slug);
      }
      forum.post(&row, topic.and_then(|t| t.str("title")))
    })
    .collect();
  let more = v.bool("grouped_search_result.more_full_page_results") == Some(true);
  Ok(Page::new(posts, more.then(|| (number + 1).to_string())))
}

pub async fn users(api: &Api, query: &Query, _page: &PageReq) -> Result<Page<User>> {
  let term = query.keyword.trim().trim_start_matches('@').to_owned();
  let v = api
    .get("/u/search/users.json", vec![("term", term)])
    .await?;
  let base = api.forum();
  let forum = Forum { base: &base };
  Ok(Page::last(
    v.list("users")
      .iter()
      .filter_map(|u| forum.user_brief(u))
      .collect(),
  ))
}
