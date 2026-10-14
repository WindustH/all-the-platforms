//! Searches: posts (for accounts only: the public AppView refuses them)
//! and accounts; trending topics.

use atp_core::{Collection, Error, Page, PageReq, Post, Query, Result, User};

use crate::api::Api;
use crate::parse;

pub const SORTS: &[&str] = &["top", "latest"];

pub async fn posts(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  if !api.logged_in() {
    return Err(
      Error::auth("Bluesky searches posts for logged-in accounts only")
        .with_hint("log in with an app password: `atp guide login bluesky`"),
    );
  }
  let mut params = vec![("q", query.keyword.trim().to_owned())];
  if let Some(sort) = &query.sort {
    params.push(("sort", sort.clone()));
  }
  api
    .list("app.bsky.feed.searchPosts", params, "posts", page, |v| {
      Some(parse::post(v))
    })
    .await
}

pub async fn users(api: &Api, query: &Query, page: &PageReq) -> Result<Page<User>> {
  let params = vec![("q", query.keyword.trim().trim_start_matches('@').to_owned())];
  api
    .list("app.bsky.actor.searchActors", params, "actors", page, |v| {
      Some(parse::user(v))
    })
    .await
}

/// Topics trending now; each links to a feed of its posts.
pub async fn trends(api: &Api) -> Result<Page<Collection>> {
  let v = api
    .get("app.bsky.unspecced.getTrendingTopics", Vec::new())
    .await?;
  let topics = atp_core::ValueExt::list(&v, "topics");
  Ok(Page::last(topics.iter().map(parse::trend).collect()))
}
