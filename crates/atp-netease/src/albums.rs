//! Albums and artists: one with its tracks, and collecting it (收藏).

use atp_core::{Action, Error, Post, Result, ValueExt};

use crate::api::Api;
use crate::parse;

/// Top songs an artist page lists in its text.
const ARTIST_TOP: usize = 20;

/// An album with its tracks (in the text and in `extra.tracks`).
pub async fn read(api: &Api, id: u64) -> Result<Post> {
  let v = api.web(&format!("v1/album/{id}")).send().await?;
  let mut album =
    parse::album(v.at("album")).ok_or_else(|| Error::not_found(format!("no album {id}")))?;
  let tracks: Vec<Post> = v.list("songs").iter().filter_map(parse::song).collect();
  parse::with_tracks(&mut album, "Tracks", &tracks);
  album.raw = Some(v);
  Ok(album)
}

/// An artist with their top songs.
pub async fn artist(api: &Api, id: u64) -> Result<Post> {
  let v = api.web(&format!("v1/artist/{id}")).send().await?;
  let mut artist =
    parse::artist(v.at("artist")).ok_or_else(|| Error::not_found(format!("no artist {id}")))?;
  let mut top: Vec<Post> = v.list("hotSongs").iter().filter_map(parse::song).collect();
  top.truncate(ARTIST_TOP);
  parse::with_tracks(&mut artist, "Top songs", &top);
  artist.raw = Some(v);
  Ok(artist)
}

/// Collect an album (收藏专辑), or drop it.
pub async fn favorite(api: &Api, id: u64, undo: bool) -> Result<Action> {
  let path = if undo { "album/unsub" } else { "album/sub" };
  api.web(path).arg("id", id).write().send().await?;
  Ok(favorited(undo, format!("album:{id}")))
}

/// Collect an artist (收藏歌手), or drop them.
pub async fn favorite_artist(api: &Api, id: u64, undo: bool) -> Result<Action> {
  let path = if undo { "artist/unsub" } else { "artist/sub" };
  api
    .web(path)
    .arg("artistId", id)
    .arg("artistIds", format!("[{id}]"))
    .write()
    .send()
    .await?;
  Ok(favorited(undo, format!("artist:{id}")))
}

/// `favorite` / `unfavorite` done.
pub fn favorited(undo: bool, target: String) -> Action {
  Action::done(if undo { "unfavorite" } else { "favorite" }, target)
}
