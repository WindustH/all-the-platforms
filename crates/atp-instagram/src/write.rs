//! Writes of the logged-in web session: likes, saves, comments, follows,
//! deleting your own posts and comments.

use atp_core::{Action, Error, Reply, Result, ValueExt};

use crate::client::Client;
use crate::read;
use crate::refs;

pub async fn like(c: &Client, post: &str, undo: bool) -> Result<Action> {
  c.require_session("liking")?;
  let id = refs::media_id(post)?;
  let verb = if undo { "unlike" } else { "like" };
  c.post(&format!("/web/likes/{id}/{verb}/"), &[]).await?;
  Ok(Action::done(verb, &id))
}

pub async fn save(c: &Client, post: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
  c.require_session("saving")?;
  let id = refs::media_id(post)?;
  let verb = if undo { "unsave" } else { "save" };
  let mut form = Vec::new();
  if let Some(f) = folder {
    form.push(("added_collection_ids", format!("[\"{f}\"]")));
  }
  c.post(&format!("/web/save/{id}/{verb}/"), &form).await?;
  Ok(Action::done(verb, &id))
}

pub async fn comment(c: &Client, post: &str, reply: &Reply) -> Result<Action> {
  c.require_session("commenting")?;
  if !reply.images.is_empty() {
    return Err(Error::unsupported("comment --image"));
  }
  let id = refs::media_id(post)?;
  let mut form = vec![("comment_text", reply.text.clone())];
  if let Some(r) = &reply.reply_to {
    form.push(("replied_to_comment_id", r.clone()));
  }
  let v = c.post(&format!("/web/comments/{id}/add/"), &form).await?;
  Ok(
    Action::done(
      if reply.reply_to.is_some() {
        "reply"
      } else {
        "comment"
      },
      &id,
    )
    .with_id(v.str("id").unwrap_or_default()),
  )
}

pub async fn delete_comment(c: &Client, post: &str, comment: &str) -> Result<Action> {
  c.require_session("deleting comments")?;
  let id = refs::media_id(post)?;
  c.post(&format!("/web/comments/{id}/delete/{comment}/"), &[])
    .await?;
  Ok(Action::done("delete-comment", comment))
}

pub async fn follow(c: &Client, user: &str, undo: bool) -> Result<Action> {
  c.require_session("following")?;
  let id = read::user_id(c, user).await?;
  let verb = if undo { "destroy" } else { "create" };
  c.post(
    &format!("friendships/{verb}/{id}/"),
    &[
      ("user_id", id.clone()),
      ("container_module", "profile".to_owned()),
    ],
  )
  .await?;
  Ok(Action::done(if undo { "unfollow" } else { "follow" }, user).with_id(id))
}

pub async fn delete(c: &Client, post: &str) -> Result<Action> {
  c.require_session("deleting posts")?;
  let id = refs::media_id(post)?;
  c.post(&format!("media/{id}/delete/"), &[("media_id", id.clone())])
    .await?;
  Ok(Action::done("delete", &id))
}
