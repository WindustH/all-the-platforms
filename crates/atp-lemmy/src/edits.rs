//! Editing a post (`PUT /api/v3/post`): its title and text. Lemmy keeps
//! what the request leaves out (link, alt text, NSFW mark, language), but
//! every edit fetches the link's preview again and replaces the thumbnail
//! with the link's own image, so a post with a link sends its thumbnail
//! back as `custom_thumbnail`. Only the post's author can edit it, at any
//! time.

use atp_core::cli::Change;
use atp_core::{Action, Edit, Error, ErrorCode, Result, ValueExt, json};

use crate::api::Api;
use crate::posts;
use crate::users;

/// Change the title, the text or both of one of your posts.
pub async fn edit(api: &Api, arg: &str, edit: &Edit) -> Result<Action> {
  api.require()?;
  let id = posts::own_post(api, arg).await?;
  let v = api.get("post", vec![("id", id.to_string())]).await?;
  let post = v.at("post_view.post");
  if post.u64("creator_id") != Some(users::my_id(api).await?) {
    return Err(Error::new(
      ErrorCode::PermissionDenied,
      format!("post {id} is not yours; Lemmy lets only the author edit a post"),
    ));
  }
  let mut body = json!({ "post_id": id });
  let mut change = Change::new(format!("Lemmy post {id}:"));
  let title = post.str("name").unwrap_or_default();
  let new_title = match edit.title.as_deref().map(str::trim) {
    Some("") => return Err(Error::input("a Lemmy post needs a title")),
    Some(new) => {
      body["name"] = json!(new);
      new
    }
    None => &title,
  };
  change.field("title", &title, new_title);
  if let Some(text) = &edit.text {
    change.field("text", &post.str("body").unwrap_or_default(), text);
    body["body"] = json!(text);
  }
  if let Some(link) = post.str("url") {
    change.row("link", &format!("{link} (kept)"));
    if let Some(thumbnail) = post.str("thumbnail_url") {
      body["custom_thumbnail"] = json!(thumbnail);
    }
  }
  change.confirm(edit.yes, "change this post on Lemmy")?;
  api.put("post", body).await?;
  Ok(
    Action::done("edit", arg)
      .with_id(id.to_string())
      .with_url(format!("https://{}/post/{id}", api.instance())),
  )
}
