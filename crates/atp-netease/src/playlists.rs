//! Playlists (歌单): one with its tracks, its tracks page by page,
//! collecting someone's, and your own: create, add, remove, delete.

use atp_core::cli::confirm;
use atp_core::{Action, Error, Page, PageReq, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::refs::{self, PostRef};
use crate::{parse, songs};

/// Tracks `read` shows.
const READ_TRACKS: usize = 100;
/// Tracks one page of `playlist tracks` fetches at most.
const PAGE_TRACKS: usize = 100;

/// The playlist and the ids of all its tracks; `n` tracks come inline.
async fn detail(api: &Api, id: u64, n: usize) -> Result<(Value, Vec<String>)> {
  let v = api
    .web("v6/playlist/detail")
    .arg("id", id)
    .arg("n", n)
    .arg("s", 8)
    .send()
    .await?;
  let ids = v
    .list("playlist.trackIds")
    .iter()
    .filter_map(|t| t.str("id"))
    .collect();
  Ok((v, ids))
}

/// A playlist with its first tracks (in the text and in `extra.tracks`).
pub async fn read(api: &Api, id: u64) -> Result<Post> {
  let (v, ids) = detail(api, id, READ_TRACKS).await?;
  let mut playlist = parse::playlist(v.at("playlist"))
    .ok_or_else(|| Error::not_found(format!("no playlist {id}")))?;
  let want = ids.len().min(READ_TRACKS);
  let mut tracks: Vec<Post> = v
    .list("playlist.tracks")
    .iter()
    .filter_map(parse::song)
    .take(want)
    .collect();
  // Logged-out answers inline only a few tracks; the rest by id.
  if tracks.len() < want {
    tracks = songs::details(api, &ids[..want]).await?;
  }
  let heading = if ids.len() > want {
    format!("Tracks (first {want} of {})", ids.len())
  } else {
    "Tracks".to_owned()
  };
  parse::with_tracks(&mut playlist, &heading, &tracks);
  playlist.raw = Some(v);
  Ok(playlist)
}

/// The tracks of a playlist, a page at a time (the cursor is an offset).
pub async fn tracks(api: &Api, id: u64, page: &PageReq) -> Result<Page<Post>> {
  let (_, ids) = detail(api, id, 1).await?;
  let start = (page.number_or(0) as usize).min(ids.len());
  let end = (start + page.size_within(PAGE_TRACKS)).min(ids.len());
  let posts = songs::details(api, &ids[start..end]).await?;
  let next = (end < ids.len()).then(|| end.to_string());
  Ok(Page::new(posts, next))
}

/// Collect a playlist (收藏歌单), or drop it.
pub async fn favorite(api: &Api, id: u64, undo: bool) -> Result<Action> {
  let path = if undo {
    "playlist/unsubscribe"
  } else {
    "playlist/subscribe"
  };
  api.web(path).arg("id", id).write().send().await?;
  let action = if undo { "unfavorite" } else { "favorite" };
  Ok(Action::done(action, format!("playlist:{id}")))
}

/// A new playlist of yours.
pub async fn create(api: &Api, name: &str, private: bool) -> Result<Action> {
  let v = api
    .web("playlist/create")
    .arg("name", name)
    .arg("privacy", if private { "10" } else { "0" })
    .arg("type", "NORMAL")
    .write()
    .send()
    .await?;
  let id = v
    .first_str(&["id", "playlist.id"])
    .ok_or_else(|| Error::upstream("playlist/create answered without an id"))?;
  Ok(
    Action::done("create", name)
      .with_url(refs::playlist_url(&id))
      .with_id(id),
  )
}

/// Add songs to one of your playlists, or remove them.
pub async fn change(api: &Api, playlist: u64, songs: &[PostRef], remove: bool) -> Result<Action> {
  let mut tracks = Vec::new();
  for song in songs {
    match song {
      PostRef::Song(id) => tracks.push(json!({"type": 3, "id": id})),
      other => return Err(Error::input(format!("not a song: {other:?}"))),
    }
  }
  let (path, action) = if remove {
    ("playlist/track/delete", "remove")
  } else {
    ("playlist/track/add", "add")
  };
  api
    .web(path)
    .arg("id", playlist)
    .arg("tracks", Value::Array(tracks).to_string())
    .write()
    .send()
    .await?;
  Ok(
    Action::done(action, format!("playlist:{playlist}"))
      .with_message(format!("{} song(s)", songs.len())),
  )
}

/// Delete one of your playlists, after a confirmation.
pub async fn delete(api: &Api, id: u64, yes: bool) -> Result<Action> {
  api.require_login()?;
  confirm(yes, &format!("delete playlist {id}"))?;
  api
    .web("playlist/remove")
    .arg("ids", format!("[{id}]"))
    .write()
    .send()
    .await?;
  Ok(Action::done("delete", format!("playlist:{id}")))
}
