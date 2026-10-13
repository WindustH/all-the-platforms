//! The logged-in account's saved posts, collections and liked posts.

use atp_core::{Collection, Page, PageReq, Post, Result, ValueExt};

use crate::api::{Api, with_cursor};
use crate::parse;

/// Saved posts, or those of one collection.
pub async fn saved(
  api: &Api,
  user: Option<&str>,
  folder: Option<&str>,
  req: &PageReq,
) -> Result<Page<Post>> {
  api.require_own(user, "saved posts")?;
  api.require_session("saved posts")?;
  let path = match folder {
    Some(folder) => format!("feed/collection/{folder}/posts/"),
    None => "feed/saved/posts/".to_owned(),
  };
  let query = with_cursor(Vec::new(), "max_id", req.cursor.as_ref());
  Ok(parse::items(&api.get(&path, &query).await?))
}

pub async fn collections(api: &Api, user: Option<&str>) -> Result<Page<Collection>> {
  api.require_own(user, "collections")?;
  api.require_session("collections")?;
  let types = r#"["ALL_MEDIA_AUTO_COLLECTION","MEDIA","AUDIO_AUTO_COLLECTION"]"#;
  let v = api
    .get(
      "collections/list/",
      &[("collection_types", types.to_owned())],
    )
    .await?;
  Ok(Page::last(
    v.list("items").iter().map(parse::folder).collect(),
  ))
}

pub async fn liked(api: &Api, user: Option<&str>, req: &PageReq) -> Result<Page<Post>> {
  api.require_own(user, "liked posts")?;
  api.require_session("liked posts")?;
  let query = with_cursor(Vec::new(), "max_id", req.cursor.as_ref());
  Ok(parse::items(&api.get("feed/liked/", &query).await?))
}
