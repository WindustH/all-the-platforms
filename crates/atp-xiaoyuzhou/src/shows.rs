//! Shows: one show's page, and the episodes it lists (the newest ones;
//! the website shows no more, and ATP does not ask the app API for them).

use atp_core::paging::slice;
use atp_core::{Error, Page, PageReq, Post, Result, Value, ValueExt};

use crate::api::Api;
use crate::parse;

/// The show object of `/podcast/<id>`.
pub async fn data(api: &Api, id: &str) -> Result<Value> {
  let props = api.page(&format!("/podcast/{id}")).await?;
  let show = props.at("podcast");
  if show.str("pid").is_none() {
    return Err(Error::not_found(format!("Xiaoyuzhou has no show {id}")));
  }
  Ok(show.clone())
}

pub async fn read(api: &Api, id: &str) -> Result<Post> {
  let mut show = data(api, id).await?;
  if let Some(map) = show.as_object_mut() {
    map.remove("episodes");
  }
  Ok(parse::show(&show))
}

/// The episodes the show's page lists, the newest first.
pub async fn episodes(api: &Api, id: &str, page: &PageReq) -> Result<Page<Post>> {
  let show = data(api, id).await?;
  let mut items: Vec<Post> = show.list("episodes").iter().map(parse::episode).collect();
  items.sort_by_key(|post| std::cmp::Reverse(post.created_at));
  Ok(slice(items, page))
}
