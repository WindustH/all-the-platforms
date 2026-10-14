//! The hot board (热榜) of the home page and the recommended feed.

use atp_core::{Error, Page, PageReq, Post, Result, ValueExt, json, paging};

use crate::api::{Api, WWW};
use crate::parse;

pub const FEED_KINDS: &[&str] = &["recommend"];

/// The home page's hot board: its Apollo state holds the ranked words.
pub async fn hot(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  let html = api.page(&format!("{WWW}/?isHome=1"), false).await?;
  let state = parse::page_state(&html, "window.__APOLLO_STATE__")
    .ok_or_else(|| Error::upstream("Kuaishou home page: no Apollo state"))?;
  let cache = state.at("defaultClient");
  let board = cache
    .at("ROOT_QUERY")
    .get(r#"visionHotRank({"page":"home"})"#)
    .and_then(|r| r.str("id"))
    .and_then(|id| cache.get(&id))
    .ok_or_else(|| Error::upstream("Kuaishou home page: no hot board"))?;
  let posts = board
    .list("items")
    .iter()
    .filter_map(|item| item.str("id").and_then(|id| cache.get(&id)))
    .filter_map(parse::hot)
    .collect();
  Ok(paging::slice(posts, page))
}

/// The recommended feed of `/new-reco`. Its `pcursor` is always `"1"`, so
/// the cursor counts pages; the first page posts no body, later ones `{}`.
pub async fn feed(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  let number = page.number_or(0);
  let mut call = api.www("/rest/v/feed/hot");
  if number > 0 {
    call = call.json(json!({}));
  }
  let v = call.send().await?;
  let posts: Vec<Post> = v.list("feeds").iter().filter_map(parse::post).collect();
  let more = !posts.is_empty() && parse::more(v.str("pcursor")).is_some();
  Ok(Page::new(posts, more.then(|| (number + 1).to_string())))
}
