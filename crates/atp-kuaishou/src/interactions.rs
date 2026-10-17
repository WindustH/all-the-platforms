//! Likes, favorites, comments and follows: the mutations and REST calls
//! of the short-video and profile pages. They want the post's author (and
//! `expTag`), so a write reads the post first. Not tried against a live
//! account yet.

use std::time::Duration;

use atp_core::{Action, Error, Reply, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::{posts, refs, users};

/// Verbatim from the web (the server checks the text).
const VIDEO_LIKE: &str = "mutation visionVideoLike($photoId: String, $photoAuthorId: String, $cancel: Int, $expTag: String) {\n  visionVideoLike(photoId: $photoId, photoAuthorId: $photoAuthorId, cancel: $cancel, expTag: $expTag) {\n    result\n    __typename\n  }\n}\n";

const ADD_COMMENT: &str = "mutation visionAddComment($photoId: String, $photoAuthorId: String, $content: String, $replyToCommentId: ID, $replyTo: ID, $expTag: String) {\n  visionAddComment(photoId: $photoId, photoAuthorId: $photoAuthorId, content: $content, replyToCommentId: $replyToCommentId, replyTo: $replyTo, expTag: $expTag) {\n    result\n    commentId\n    content\n    timestamp\n    status\n    __typename\n  }\n}\n";

const FOLLOW: &str = "mutation visionFollow($touid: String, $ftype: Int, $followSource: Int, $expTag: String) {\n  visionFollow(touid: $touid, ftype: $ftype, followSource: $followSource, expTag: $expTag) {\n    result\n    followStatus\n    hostName\n    error_msg\n    __typename\n  }\n}\n";

/// The pause before a write, like a person reading first.
const PAUSE: (Duration, Duration) = (Duration::from_millis(800), Duration::from_millis(2000));

/// The photo id, its author's eid and its `expTag`.
async fn target(api: &Api, arg: &str) -> Result<(String, String, String)> {
  api.require_login()?;
  let post = posts::read(api, arg).await?;
  let author = post
    .author
    .as_ref()
    .map(|a| a.id.clone())
    .ok_or_else(|| Error::upstream("Kuaishou: the post has no author"))?;
  let exp_tag = post
    .extra
    .get("exp_tag")
    .and_then(Value::as_str)
    .unwrap_or_default()
    .to_owned();
  api.ctx.http.pause(PAUSE.0, PAUSE.1).await;
  Ok((post.id, author, exp_tag))
}

/// The mutation's answer object, failing unless its `result` is 1.
fn result(data: &Value, field: &str) -> Result<Value> {
  crate::api::check(data.at(field).clone())
}

pub async fn like(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let (id, author, exp_tag) = target(api, arg).await?;
  let data = api
    .graphql(
      "visionVideoLike",
      json!({
        "photoId": id,
        "photoAuthorId": author,
        "cancel": i32::from(undo),
        "expTag": exp_tag,
      }),
      VIDEO_LIKE,
      &refs::photo_url(&id),
    )
    .await?;
  result(&data, "visionVideoLike")?;
  let action = if undo { "unlike" } else { "like" };
  Ok(Action::done(action, refs::photo_url(&id)))
}

/// Collect (收藏) a video, or with `undo` drop it, as the player's collect
/// button does: `/rest/v/photo/collect` with `collect` 1 or 2.
pub async fn favorite(api: &Api, arg: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
  if folder.is_some() {
    return Err(Error::unsupported(
      "favorite --folder (Kuaishou has no folders)",
    ));
  }
  let (id, author, _) = target(api, arg).await?;
  api
    .www("/rest/v/photo/collect")
    .json(json!({
      "photoId": id,
      "userId": author,
      "collect": if undo { 2 } else { 1 },
    }))
    .referer(&refs::photo_url(&id))
    .send()
    .await?;
  let action = if undo { "unfavorite" } else { "favorite" };
  Ok(Action::done(action, refs::photo_url(&id)))
}

pub async fn comment(api: &Api, arg: &str, reply: &Reply) -> Result<Action> {
  if !reply.images.is_empty() {
    return Err(Error::unsupported("comment --image"));
  }
  let (id, author, exp_tag) = target(api, arg).await?;
  let mut variables = json!({
    "photoId": id,
    "photoAuthorId": author,
    "content": reply.text,
    "expTag": exp_tag,
  });
  if let Some(to) = &reply.reply_to {
    variables["replyToCommentId"] = json!(to);
  }
  let data = api
    .graphql(
      "visionAddComment",
      variables,
      ADD_COMMENT,
      &refs::photo_url(&id),
    )
    .await?;
  let added = result(&data, "visionAddComment")?;
  let mut action = Action::done("comment", refs::photo_url(&id));
  if let Some(cid) = added.str("commentId") {
    action = action.with_id(cid);
  }
  Ok(action)
}

pub async fn follow(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  api.require_login()?;
  let eid = users::eid(api, arg).await?;
  api.ctx.http.pause(PAUSE.0, PAUSE.1).await;
  let data = api
    .graphql(
      "visionFollow",
      json!({
        "touid": eid,
        "ftype": if undo { 2 } else { 1 },
        "followSource": 3,
        "expTag": "",
      }),
      FOLLOW,
      &refs::profile_url(&eid),
    )
    .await?;
  result(&data, "visionFollow")?;
  let action = if undo { "unfollow" } else { "follow" };
  Ok(Action::done(action, refs::profile_url(&eid)))
}
