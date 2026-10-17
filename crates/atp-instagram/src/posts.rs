//! Posts: one post, a user's posts, liking, saving, editing and deleting.

use atp_core::cli::Change;
use atp_core::{Action, Edit, Error, ErrorCode, Page, PageReq, Post, Result, Value, ValueExt};

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

/// `coauthor_invites` when the collaborators stay as they are.
const NO_INVITES: &str = r#"{"added":[],"in":[],"removed":[]}"#;

/// A new caption, as the web app's post editor sends it
/// (`PolarisPostActionEditPost`): `edit_media` with only what changes, so
/// the location, tagged people, collaborators and alt texts stay; long
/// videos (IGTV) send their title along.
pub async fn edit(api: &Api, post: &str, edit: &Edit) -> Result<Action> {
  api.require_session("editing posts")?;
  let Some(text) = edit.text.as_deref() else {
    return Err(Error::input("give the new caption"));
  };
  let pk = refs::media_id(post)?;
  let v = api.get(&format!("media/{pk}/info/"), &[]).await?;
  let item = v
    .list("items")
    .first()
    .ok_or_else(|| Error::not_found("no such post"))?;
  if item.str("user.pk").or_else(|| item.str("owner.pk")) != api.my_id() {
    return Err(Error::new(
      ErrorCode::PermissionDenied,
      format!("post {pk} is not yours; Instagram edits only your own"),
    ));
  }
  let mut change = Change::new(format!("Instagram post {pk}:"));
  let caption = item.str("caption.text").unwrap_or_default();
  change.field("caption", &caption, text);
  kept(item, &mut change);
  let mut form = vec![
    ("caption_text", text.to_owned()),
    ("coauthor_invites", NO_INVITES.to_owned()),
  ];
  if item.str("product_type").as_deref() == Some("igtv")
    && let Some(title) = item.str("title")
  {
    form.push(("title", title));
  }
  change.confirm(edit.yes, "change this post on Instagram")?;
  let id = item.str("id").unwrap_or_else(|| pk.clone());
  api
    .post(&format!("media/{id}/edit_media/"), &form)
    .await
    .map_err(promoted)?;
  let mut done = Action::done("edit", &pk).with_id(&pk);
  if let Some(code) = item.str("code") {
    done = done.with_url(refs::post_url(&code));
  }
  Ok(done)
}

/// The rows of what an edit leaves as it is.
fn kept(item: &Value, change: &mut Change) {
  let slides = item.list("carousel_media");
  if !slides.is_empty() {
    change.row("media", &format!("{} (kept)", slides.len()));
  }
  if let Some(place) = item.str("location.name") {
    change.row("location", &format!("{place} (kept)"));
  }
  let tagged = item.list("usertags.in").len()
    + slides
      .iter()
      .map(|m| m.list("usertags.in").len())
      .sum::<usize>();
  if tagged > 0 {
    change.row("tagged", &format!("{tagged} (kept)"));
  }
}

/// Instagram's refusal to edit a post with a running promotion, said
/// plainly.
fn promoted(e: Error) -> Error {
  if e.message.contains("Promotion with ongoing ads") {
    return Error::new(
      ErrorCode::PermissionDenied,
      "the post is promoted with ads running; Instagram lets it change once they end",
    );
  }
  e
}
