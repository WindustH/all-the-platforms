//! Managing the account's playlists, the favorites folders of the shared
//! `folder-*` commands, and Watch later (`WL`) of `later-add` /
//! `later-remove`, with the endpoints YouTube.js `PlaylistManager`
//! calls: `playlist/create`, `playlist/delete`, `browse/edit_playlist`
//! (videos, their order, title, description, privacy) and `like/like` /
//! `like/removelike` on a playlist (saving it to the library).

use atp_core::bulk;
use atp_core::{Action, Error, FolderSpec, PageReq, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::{parse, refs};

pub const PRIVACY: &[&str] = &["private", "unlisted", "public"];

/// Videos sent in one `browse/edit_playlist` call by `folder-add` and
/// `folder-remove`; how many YouTube takes at once is not known.
const BATCH: usize = 50;
/// Pages of a playlist `playlist-move` reads at most (a playlist holds
/// 5,000 videos, about 100 a page).
const PAGES: usize = 60;

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

/// A new playlist, private as before (`--private` changes nothing): the
/// result carries its id and link.
pub async fn create(api: &Api, folder: &FolderSpec) -> Result<Action> {
  let title = folder.name.as_deref().unwrap_or_default();
  let mut body = json!({
    "title": title,
    "privacyStatus": "PRIVATE",
    "videoIds": [],
  });
  if let Some(d) = &folder.description {
    body["description"] = d.as_str().into();
  }
  let v = api.write("playlist/create", body).await?;
  let id = v
    .str("playlistId")
    .ok_or_else(|| Error::upstream("YouTube created no playlist"))?;
  Ok(
    Action::done("folder-create", title)
      .with_url(refs::playlist_url(&id))
      .with_id(id),
  )
}

/// Delete playlists, one call each.
pub async fn delete(api: &Api, folders: &[String]) -> Result<Vec<Action>> {
  bulk::each("folder-delete", folders, async |folder| {
    let id = refs::playlist(folder)?;
    api
      .write("playlist/delete", json!({ "playlistId": id }))
      .await?;
    Ok(Action::done("folder-delete", &id))
  })
  .await
}

/// Apply `browse/edit_playlist` actions and check the overall status.
async fn apply(api: &Api, id: &str, actions: Vec<Value>) -> Result<()> {
  let body = json!({ "playlistId": id, "actions": actions });
  let v = api.write("browse/edit_playlist", body).await?;
  if v.str("status").is_some_and(|s| s != "STATUS_SUCCEEDED") {
    return Err(Error::upstream(format!(
      "YouTube could not change playlist {id} ({})",
      v.str("status").unwrap_or_default()
    )));
  }
  Ok(())
}

/// Add videos to a playlist (`undo`: take them out by video id), 50 per
/// call; an Action per video.
pub async fn file(api: &Api, folder: &str, posts: &[String], undo: bool) -> Result<Vec<Action>> {
  let name = if undo { "folder-remove" } else { "folder-add" };
  edit_videos(api, &refs::playlist(folder)?, posts, undo, name).await
}

/// Add videos to Watch later (`WL`), or with `undo` take them out, as
/// [`file`] does with a playlist.
pub async fn later(api: &Api, posts: &[String], undo: bool) -> Result<Vec<Action>> {
  let name = if undo { "later-remove" } else { "later-add" };
  edit_videos(api, "WL", posts, undo, name).await
}

/// Add videos to the playlist `id` (`undo`: take them out by video id),
/// 50 per call; an Action named `name` per video.
async fn edit_videos(
  api: &Api,
  id: &str,
  posts: &[String],
  undo: bool,
  name: &str,
) -> Result<Vec<Action>> {
  let ids = videos(posts)?;
  bulk::batches(name, &ids, BATCH, String::clone, async |chunk| {
    let actions = chunk
      .iter()
      .map(|v| {
        if undo {
          json!({ "action": "ACTION_REMOVE_VIDEO_BY_VIDEO_ID", "removedVideoId": v })
        } else {
          json!({ "action": "ACTION_ADD_VIDEO", "addedVideoId": v })
        }
      })
      .collect();
    apply(api, id, actions).await?;
    Ok(
      chunk
        .iter()
        .map(|v| {
          Action::done(name, v)
            .with_url(refs::video_url(v))
            .with_message(format!("playlist {id}"))
        })
        .collect(),
    )
  })
  .await
}

/// `folder-edit`: the title, description and privacy, whichever are given.
pub async fn edit(api: &Api, folder: &str, change: &FolderSpec) -> Result<Action> {
  let id = refs::playlist(folder)?;
  let mut actions = Vec::new();
  if let Some(t) = &change.name {
    actions.push(json!({ "action": "ACTION_SET_PLAYLIST_NAME", "playlistName": t }));
  }
  if let Some(d) = &change.description {
    actions.push(json!({ "action": "ACTION_SET_PLAYLIST_DESCRIPTION", "playlistDescription": d }));
  }
  if let Some(private) = change.private {
    let level = if private { "PRIVATE" } else { "PUBLIC" };
    actions.push(json!({ "action": "ACTION_SET_PLAYLIST_PRIVACY", "playlistPrivacy": level }));
  }
  if actions.is_empty() {
    return Err(Error::input("nothing to change"));
  }
  apply(api, &id, actions).await?;
  Ok(Action::done("folder-edit", &id).with_url(refs::playlist_url(&id)))
}

/// Who can see a playlist: private, unlisted or public.
pub async fn set_privacy(api: &Api, folder: &str, level: &str) -> Result<Action> {
  let id = refs::playlist(folder)?;
  let action =
    json!({ "action": "ACTION_SET_PLAYLIST_PRIVACY", "playlistPrivacy": privacy(level)? });
  apply(api, &id, vec![action]).await?;
  Ok(
    Action::done("playlist-privacy", &id)
      .with_url(refs::playlist_url(&id))
      .with_message(level.to_owned()),
  )
}

/// Move a video of a playlist right after another one, or with `after`
/// `None` to the top, as dragging it in the playlist page does
/// (`ACTION_MOVE_VIDEO_AFTER` with the entries' `setVideoId`s, read from the
/// playlist first).
pub async fn reorder(api: &Api, folder: &str, post: &str, after: Option<&str>) -> Result<Action> {
  let id = refs::playlist(folder)?;
  let video = refs::video(post)?;
  let after = after.map(refs::video).transpose()?;
  if after.as_deref() == Some(video.as_str()) {
    return Err(Error::input("a video cannot move after itself"));
  }
  let mut wanted = vec![video.clone()];
  wanted.extend(after.clone());
  let entries = entries(api, &id, &wanted).await?;
  let entry = |v: &str| {
    entries
      .iter()
      .find(|(video, _)| video == v)
      .map(|(_, set)| set.clone())
      .ok_or_else(|| Error::not_found(format!("{v} is not in playlist {id}")))
  };
  let mut action = json!({ "action": "ACTION_MOVE_VIDEO_AFTER", "setVideoId": entry(&video)? });
  if let Some(a) = &after {
    action["movedSetVideoIdPredecessor"] = entry(a)?.into();
  }
  apply(api, &id, vec![action]).await?;
  let place = match &after {
    Some(a) => format!("after {a}"),
    None => "to the top".to_owned(),
  };
  Ok(
    Action::done("playlist-move", &video)
      .with_url(refs::playlist_url(&id))
      .with_message(place),
  )
}

/// `(video id, setVideoId)` of the playlist's entries, read page by page
/// until every video of `wanted` turned up (the first entry of each), the
/// playlist ends or [`PAGES`] pages were read.
async fn entries(api: &Api, id: &str, wanted: &[String]) -> Result<Vec<(String, String)>> {
  api.require_login()?;
  let mut found: Vec<(String, String)> = Vec::new();
  let mut req = PageReq::first(100);
  for _ in 0..PAGES {
    let l = parse::listing(&api.browse(&format!("VL{id}"), None, &req).await?);
    for post in &l.posts {
      let set = post.extra.get("set_video_id").and_then(Value::as_str);
      if let Some(set) = set
        && wanted.contains(&post.id)
        && !found.iter().any(|(v, _)| *v == post.id)
      {
        found.push((post.id.clone(), set.to_owned()));
      }
    }
    if found.len() == wanted.len() {
      return Ok(found);
    }
    match l.next {
      Some(next) => req.cursor = Some(next),
      None => break,
    }
  }
  Ok(found)
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
