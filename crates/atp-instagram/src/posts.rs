//! Posts: one post, a user's posts, liking, saving and deleting.

use atp_core::{Action, Error, Page, PageReq, Post, Result, ValueExt};

use crate::api::{Api, with_cursor};
use crate::parse;
use crate::refs;

/// One post: v1 media info when logged in, the public GraphQL query otherwise.
pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  if !api.has_session() {
    return Ok(parse::post(&api.public_post(arg).await?));
  }
  let id = refs::media_id(arg)?;
  let v = api.get(&format!("media/{id}/info/"), &[]).await?;
  let item = v
    .list("items")
    .first()
    .ok_or_else(|| Error::not_found("no such post"))?;
  Ok(parse::post(item))
}

/// A user's posts: the v1 feed when logged in, the first twelve otherwise.
pub async fn user_posts(api: &Api, arg: &str, req: &PageReq) -> Result<Page<Post>> {
  if !api.has_session() {
    if req.cursor.is_some() {
      return Err(
        Error::auth("more than the first posts needs a logged-in session")
          .with_hint("run `atp instagram login --browser`"),
      );
    }
    let profile = api.profile(arg).await?;
    let posts = profile
      .list("edge_owner_to_timeline_media.edges")
      .iter()
      .map(|edge| parse::post(edge.at("node")))
      .collect();
    return Ok(Page::new(posts, None));
  }
  let id = api.user_id(arg).await?;
  let query = with_cursor(
    vec![("count", req.size_within(33).to_string())],
    "max_id",
    req.cursor.as_ref(),
  );
  let v = api.get(&format!("feed/user/{id}/"), &query).await?;
  Ok(parse::items(&v))
}

pub async fn like(api: &Api, post: &str, undo: bool) -> Result<Action> {
  api.require_session("liking")?;
  let id = refs::media_id(post)?;
  let verb = if undo { "unlike" } else { "like" };
  api.post(&format!("/web/likes/{id}/{verb}/"), &[]).await?;
  Ok(Action::done(verb, &id))
}

/// Save a post (into collection `folder`), or unsave it.
pub async fn save(api: &Api, post: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
  api.require_session("saving")?;
  let id = refs::media_id(post)?;
  let verb = if undo { "unsave" } else { "save" };
  let form: Vec<_> = folder
    .map(|folder| ("added_collection_ids", format!("[\"{folder}\"]")))
    .into_iter()
    .collect();
  api.post(&format!("/web/save/{id}/{verb}/"), &form).await?;
  Ok(Action::done(verb, &id))
}

pub async fn delete(api: &Api, post: &str) -> Result<Action> {
  api.require_session("deleting posts")?;
  let id = refs::media_id(post)?;
  api
    .post(&format!("media/{id}/delete/"), &[("media_id", id.clone())])
    .await?;
  Ok(Action::done("delete", &id))
}
