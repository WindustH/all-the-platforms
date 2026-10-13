//! Writes with the logged-in session, as the web client sends them: signed
//! POSTs with the parameters in the query and the `tt-csrf-token` header.

use atp_core::{Action, Error, Reply, Result, ValueExt};

use crate::api::Api;
use crate::{refs, users};

fn flag(undo: bool) -> String {
  if undo { "0" } else { "1" }.into()
}

pub async fn like(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let r = refs::post_ref(api, arg).await?;
  api.write_pause().await;
  api
    .post(
      "/api/commit/item/digg/",
      &[("aweme_id", r.id.clone()), ("type", flag(undo))],
    )
    .await?;
  Ok(Action::done(if undo { "unlike" } else { "like" }, &r.id).with_url(r.url()))
}

pub async fn favorite(api: &Api, arg: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
  if folder.is_some() {
    return Err(Error::unsupported("favorite --folder"));
  }
  let r = refs::post_ref(api, arg).await?;
  api.write_pause().await;
  api
    .post(
      "/api/item/collect/",
      &[("itemId", r.id.clone()), ("action", flag(undo))],
    )
    .await?;
  Ok(Action::done(if undo { "unfavorite" } else { "favorite" }, &r.id).with_url(r.url()))
}

pub async fn comment(api: &Api, arg: &str, reply: &Reply) -> Result<Action> {
  let r = refs::post_ref(api, arg).await?;
  api.write_pause().await;
  let mut params = vec![
    ("aweme_id", r.id.clone()),
    ("text", reply.text.clone()),
    ("text_extra", "[]".into()),
  ];
  if let Some(target) = &reply.reply_to {
    params.push(("reply_id", target.trim().to_owned()));
    params.push(("reply_to_reply_id", "0".into()));
  }
  let v = api.post("/api/comment/publish/", &params).await?;
  let name = if reply.reply_to.is_some() {
    "reply"
  } else {
    "comment"
  };
  let mut action = Action::done(name, &r.id).with_url(r.url());
  if let Some(id) = v.str("comment.cid") {
    action = action.with_id(id);
  }
  Ok(action)
}

pub async fn delete_comment(api: &Api, arg: &str, comment: &str) -> Result<Action> {
  let r = refs::post_ref(api, arg).await?;
  api.write_pause().await;
  api
    .post(
      "/api/comment/delete/",
      &[
        ("cid", comment.trim().to_owned()),
        ("aweme_id", r.id.clone()),
      ],
    )
    .await?;
  Ok(Action::done("delete-comment", comment.trim()).with_url(r.url()))
}

pub async fn follow(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let (_, id) = users::ids(api, Some(arg)).await?;
  if id.is_empty() {
    return Err(Error::upstream("the profile carries no user id"));
  }
  api.write_pause().await;
  let params = [
    ("user_id", id.clone()),
    ("type", flag(undo)),
    ("action_type", flag(undo)),
    ("from", "18".into()),
    ("from_pre", "0".into()),
    ("channel_id", "3".into()),
  ];
  api.post("/api/commit/follow/user/", &params).await?;
  Ok(Action::done(if undo { "unfollow" } else { "follow" }, arg.trim()).with_id(id))
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let r = refs::post_ref(api, arg).await?;
  api.write_pause().await;
  api
    .post("/api/aweme/delete/", &[("aweme_id", r.id.clone())])
    .await?;
  Ok(Action::done("delete", &r.id))
}
