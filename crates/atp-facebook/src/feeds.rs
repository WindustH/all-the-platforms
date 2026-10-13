//! Logged-in story lists: the home feed and post search.

use atp_core::{Error, Page, PageReq, Post, Result};
use url::form_urlencoded::byte_serialize;

use crate::api::Api;
use crate::parse;
use crate::refs::HOME;
use crate::web::advance;

pub async fn feed(api: &Api, req: &PageReq) -> Result<Page<Post>> {
  api.require_session("the home feed")?;
  stories(api, &format!("{HOME}/"), &["NewsFeed", "Feed"], req).await
}

pub async fn search(api: &Api, query: &str, req: &PageReq) -> Result<Page<Post>> {
  api.require_session("search")?;
  let query: String = byte_serialize(query.as_bytes()).collect();
  stories(
    api,
    &format!("{HOME}/search/posts/?q={query}"),
    &["Search"],
    req,
  )
  .await
}

/// A logged-in page's stories, then its paged operation named by `ops`.
async fn stories(api: &Api, url: &str, ops: &[&str], req: &PageReq) -> Result<Page<Post>> {
  let page = api.page(url).await?;
  page.check("this page", &[])?;
  let Some(cursor) = &req.cursor else {
    let mut posts = Vec::new();
    let mut next = None;
    for doc in page.docs.values() {
      let (found, cursor) = parse::stories(doc);
      if !found.is_empty() {
        posts.extend(found);
        next = next.or(cursor);
      }
    }
    return Ok(Page::new(posts, next));
  };
  let operation = page
    .paged(ops)
    .ok_or_else(|| Error::unsupported("paging this list (no replayable operation on the page)"))?;
  let data = advance(&api.ctx, &page, operation, cursor, req.size_within(10)).await?;
  let (posts, next) = parse::stories(&data);
  Ok(Page::new(posts, next))
}
