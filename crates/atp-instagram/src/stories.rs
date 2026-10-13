//! Stories: a user's current ones, their highlights, and the stories of
//! one highlight.

use atp_core::{Collection, Page, Post, Result, ValueExt};

use crate::api::Api;
use crate::parse;

/// Current stories of a user.
pub async fn stories(api: &Api, user: &str) -> Result<Page<Post>> {
  api.require_session("stories")?;
  let id = api.user_id(user).await?;
  reel(api, &id).await
}

pub async fn highlights(api: &Api, user: &str) -> Result<Page<Collection>> {
  api.require_session("highlights")?;
  let id = api.user_id(user).await?;
  let v = api
    .get(&format!("highlights/{id}/highlights_tray/"), &[])
    .await?;
  Ok(Page::last(
    v.list("tray").iter().map(parse::highlight).collect(),
  ))
}

/// The stories inside one highlight (`ID` or `highlight:ID`).
pub async fn highlight(api: &Api, highlight: &str) -> Result<Page<Post>> {
  let id = if highlight.starts_with("highlight:") {
    highlight.to_owned()
  } else {
    format!("highlight:{highlight}")
  };
  api.require_session("highlights")?;
  reel(api, &id).await
}

/// One reel tray: a user's stories or a highlight (`highlight:ID`).
async fn reel(api: &Api, reel_id: &str) -> Result<Page<Post>> {
  let v = api
    .get("feed/reels_media/", &[("reel_ids", reel_id.to_owned())])
    .await?;
  let items = v
    .at("reels")
    .get(reel_id)
    .or_else(|| v.list("reels_media").first())
    .map(|reel| reel.list("items").to_vec())
    .unwrap_or_default();
  Ok(Page::last(
    items
      .iter()
      .map(|item| {
        let mut post = parse::post(item);
        post.kind = "story".into();
        post
      })
      .collect(),
  ))
}
