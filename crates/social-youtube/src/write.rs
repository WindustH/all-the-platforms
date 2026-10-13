//! Likes, Watch later / playlist saves and subscriptions, with the request
//! bodies of YouTube.js `InteractionManager` / `PlaylistManager` and the web
//! app's own `playlistEditEndpoint`s.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use social_core::{Action, Error, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::proto::Msg;
use crate::{channel, refs};

pub async fn like(api: &Api, post: &str, undo: bool) -> Result<Action> {
  let id = refs::video(post)?;
  let (path, name) = if undo {
    ("like/removelike", "unlike")
  } else {
    ("like/like", "like")
  };
  api
    .write(path, json!({ "target": { "videoId": id } }))
    .await?;
  Ok(Action::done(name, &id).with_url(refs::video_url(&id)))
}

fn failed(v: &Value) -> bool {
  v.str("status").is_some_and(|s| s != "STATUS_SUCCEEDED")
}

/// Add to (or remove from) Watch later or the playlist named by `folder`.
pub async fn favorite(api: &Api, post: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
  let id = refs::video(post)?;
  let list = match folder {
    Some(f) => refs::playlist(f)?,
    None => "WL".to_owned(),
  };
  let action = if undo {
    json!({ "action": "ACTION_REMOVE_VIDEO_BY_VIDEO_ID", "removedVideoId": id })
  } else {
    json!({ "action": "ACTION_ADD_VIDEO", "addedVideoId": id })
  };
  let body = json!({ "playlistId": list, "actions": [action] });
  let v = api.write("browse/edit_playlist", body).await?;
  if failed(&v) {
    return Err(Error::upstream(format!(
      "YouTube could not change playlist {list} ({})",
      v.str("status").unwrap_or_default()
    )));
  }
  let name = if undo { "unfavorite" } else { "favorite" };
  Ok(Action::done(name, &id).with_url(refs::playlist_url(&list)))
}

/// Subscribe to (or unsubscribe from) a channel.
pub async fn follow(api: &Api, user: &str, undo: bool) -> Result<Action> {
  api.require_login()?;
  let id = channel::id(api, user).await?;
  let (path, params, name) = if undo {
    ("subscription/unsubscribe", "CgIIAhgA", "unfollow")
  } else {
    ("subscription/subscribe", "EgIIAhgA", "follow")
  };
  api
    .write(path, json!({ "channelIds": [id], "params": params }))
    .await?;
  Ok(Action::done(name, &id).with_url(refs::channel_url(&id)))
}

pub const BELL: &[&str] = &["all", "personalized", "none"];

/// The bell of a subscription: all, personalized or no notifications.
/// Params as YouTube.js `setNotificationPreferences` writes them:
/// `{1: channel, 2: {1: level}, 3: 0, 4: 4}`, base64 then URI-escaped.
pub async fn bell(api: &Api, user: &str, level: &str) -> Result<Action> {
  api.require_login()?;
  let id = channel::id(api, user).await?;
  let index = match level {
    "personalized" => 1,
    "all" => 2,
    "none" => 3,
    other => return Err(Error::input(format!("unknown bell setting `{other}`"))),
  };
  let raw = Msg::new()
    .str(1, &id)
    .msg(2, Msg::new().int(1, index))
    .int(3, 0)
    .int(4, 4)
    .into_bytes();
  let b64 = STANDARD.encode(raw);
  let params: String = url::form_urlencoded::byte_serialize(b64.as_bytes()).collect();
  api
    .write(
      "notification/modify_channel_preference",
      json!({ "params": params }),
    )
    .await?;
  Ok(
    Action::done("bell", &id)
      .with_url(refs::channel_url(&id))
      .with_message(format!("notifications: {level}")),
  )
}
