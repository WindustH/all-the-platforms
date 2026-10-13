//! Interactions as weibo.com sends them: comments and replies, likes,
//! favorites and follows.

use atp_core::{Action, Error, Reply, Result, ValueExt, json};

use crate::api::Api;
use crate::refs::{self, post_url};
use crate::users;

pub async fn comment(api: &Api, arg: &str, reply: &Reply) -> Result<Action> {
  api.require_login()?;
  let mid = refs::mid(arg)?;
  if reply.text.trim().is_empty() {
    return Err(Error::input("empty comment"));
  }
  api.write_pause().await;
  let mut form = vec![
    ("id", mid.as_str()),
    ("comment", reply.text.as_str()),
    ("pic_id", ""),
    ("is_repost", "0"),
    ("comment_ori", "0"),
    ("is_comment", "0"),
  ];
  let (path, name) = match &reply.reply_to {
    Some(cid) => {
      form.push(("cid", cid.as_str()));
      ("/ajax/comments/reply", "reply")
    }
    None => ("/ajax/comments/create", "comment"),
  };
  let v = api.post_form(path, &form).await?;
  if let Some(msg) = v.str("error") {
    return Err(Error::upstream(msg));
  }
  let data = if v.at("data").is_object() {
    v.at("data")
  } else {
    &v
  };
  let mut action = Action::done(name, &mid).with_url(post_url(None, &mid, None));
  if let Some(id) = data.first_str(&["idstr", "id"]) {
    action = action.with_id(id);
  }
  Ok(action)
}

pub async fn delete_comment(api: &Api, comment: &str) -> Result<Action> {
  api.require_login()?;
  let cid = comment.trim();
  api.write_pause().await;
  api
    .post_json(
      "/ajax/statuses/destroyComment",
      &json!({"cid": cid, "filter": {}}),
    )
    .await?;
  Ok(Action::done("delete-comment", cid))
}

pub async fn like(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  api.require_login()?;
  let mid = refs::mid(arg)?;
  let path = if undo {
    "/ajax/statuses/cancelLike"
  } else {
    "/ajax/statuses/setLike"
  };
  api.write_pause().await;
  api.post_json(path, &json!({"id": mid})).await?;
  Ok(Action::done(if undo { "unlike" } else { "like" }, &mid))
}

pub async fn favorite(api: &Api, arg: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
  api.require_login()?;
  if folder.is_some() {
    return Err(Error::unsupported("favorite --folder").with_hint(
      "Weibo files favorites into folders (tags) on weibo.com; `favorites --folder` reads them",
    ));
  }
  let mid = refs::mid(arg)?;
  let path = if undo {
    "/ajax/statuses/destoryFavorites"
  } else {
    "/ajax/statuses/createFavorites"
  };
  api.write_pause().await;
  api.post_json(path, &json!({"id": mid})).await?;
  Ok(Action::done(
    if undo { "unfavorite" } else { "favorite" },
    &mid,
  ))
}

pub async fn follow(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  api.require_login()?;
  let uid = users::uid(api, arg).await?;
  api.write_pause().await;
  if undo {
    api
      .post_json("/ajax/friendships/destory", &json!({"uid": uid}))
      .await?;
  } else {
    api
      .post_json("/ajax/friendships/create", &json!({"friend_uid": uid}))
      .await?;
  }
  Ok(Action::done(if undo { "unfollow" } else { "follow" }, &uid).with_url(refs::user_url(&uid)))
}
