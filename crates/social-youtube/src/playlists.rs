//! Managing the account's playlists, with the endpoints YouTube.js
//! `PlaylistManager` calls: `playlist/create`, `playlist/delete`,
//! `browse/edit_playlist` (videos, title, description, privacy) and
//! `like/like` / `like/removelike` on a playlist (saving it to the library).

use social_core::{Action, Error, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::refs;

pub const PRIVACY: &[&str] = &["private", "unlisted", "public"];

fn privacy(name: &str) -> Result<&'static str> {
  match name {
    "private" => Ok("PRIVATE"),
    "unlisted" => Ok("UNLISTED"),
    "public" => Ok("PUBLIC"),
    other => Err(Error::input(format!("unknown privacy `{other}`"))),
  }
}

fn videos(args: &[String]) -> Result<Vec<String>> {
  args.iter().map(|v| refs::video(v)).collect()
}

pub async fn create(
  api: &Api,
  title: &str,
  description: Option<&str>,
  privacy_name: &str,
  video_args: &[String],
) -> Result<Action> {
  let mut body = json!({
    "title": title,
    "privacyStatus": privacy(privacy_name)?,
    "videoIds": videos(video_args)?,
  });
  if let Some(d) = description {
    body["description"] = d.into();
  }
  let v = api.write("playlist/create", body).await?;
  let id = v
    .str("playlistId")
    .ok_or_else(|| Error::upstream("YouTube created no playlist"))?;
  Ok(
    Action::done("playlist_create", title)
      .with_url(refs::playlist_url(&id))
      .with_id(id),
  )
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let id = refs::playlist(arg)?;
  api
    .write("playlist/delete", json!({ "playlistId": id }))
    .await?;
  Ok(Action::done("playlist_delete", &id))
}

/// Apply `browse/edit_playlist` actions and check the overall status.
async fn edit(api: &Api, id: &str, actions: Vec<Value>, name: &str) -> Result<Action> {
  if actions.is_empty() {
    return Err(Error::input("nothing to change"));
  }
  let body = json!({ "playlistId": id, "actions": actions });
  let v = api.write("browse/edit_playlist", body).await?;
  if v.str("status").is_some_and(|s| s != "STATUS_SUCCEEDED") {
    return Err(Error::upstream(format!(
      "YouTube could not change playlist {id} ({})",
      v.str("status").unwrap_or_default()
    )));
  }
  Ok(Action::done(name, id).with_url(refs::playlist_url(id)))
}

pub async fn add(api: &Api, arg: &str, video_args: &[String]) -> Result<Action> {
  let id = refs::playlist(arg)?;
  let actions = videos(video_args)?
    .into_iter()
    .map(|v| json!({ "action": "ACTION_ADD_VIDEO", "addedVideoId": v }))
    .collect();
  edit(api, &id, actions, "playlist_add").await
}

pub async fn remove(api: &Api, arg: &str, video_args: &[String]) -> Result<Action> {
  let id = refs::playlist(arg)?;
  let actions = videos(video_args)?
    .into_iter()
    .map(|v| json!({ "action": "ACTION_REMOVE_VIDEO_BY_VIDEO_ID", "removedVideoId": v }))
    .collect();
  edit(api, &id, actions, "playlist_remove").await
}

/// Title, description and privacy, whichever are given.
pub async fn update(
  api: &Api,
  arg: &str,
  title: Option<&str>,
  description: Option<&str>,
  privacy_name: Option<&str>,
) -> Result<Action> {
  let id = refs::playlist(arg)?;
  let mut actions = Vec::new();
  if let Some(t) = title {
    actions.push(json!({ "action": "ACTION_SET_PLAYLIST_NAME", "playlistName": t }));
  }
  if let Some(d) = description {
    actions.push(json!({ "action": "ACTION_SET_PLAYLIST_DESCRIPTION", "playlistDescription": d }));
  }
  if let Some(p) = privacy_name {
    actions
      .push(json!({ "action": "ACTION_SET_PLAYLIST_PRIVACY", "playlistPrivacy": privacy(p)? }));
  }
  edit(api, &id, actions, "playlist_edit").await
}

/// Save someone's playlist to the library (or remove it with `undo`).
pub async fn save(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let id = refs::playlist(arg)?;
  let (path, name) = if undo {
    ("like/removelike", "playlist_unsave")
  } else {
    ("like/like", "playlist_save")
  };
  api
    .write(path, json!({ "target": { "playlistId": id } }))
    .await?;
  Ok(Action::done(name, &id).with_url(refs::playlist_url(&id)))
}
