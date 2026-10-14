//! Albums: one with its tracks, and collecting it (收藏专辑).

use atp_core::music::with_tracks;
use atp_core::{Action, Error, Post, Result, ValueExt};

use crate::api::{Api, Call};
use crate::parse;
use crate::refs::Key;
use crate::songs;

/// Tracks `read` shows.
const READ_TRACKS: usize = 100;

/// A call of the album page's modules for an album by mid or number.
fn album_call<'a>(api: &'a Api, method: &str, key: &Key) -> Call<'a> {
  let module = if method == "GetAlbumDetail" {
    "music.musichallAlbum.AlbumInfoServer"
  } else {
    "music.musichallAlbum.AlbumSongList"
  };
  let call = api.call(module, method);
  match key {
    Key::Mid(mid) => call.arg("albumMid", mid.as_str()),
    Key::Id(id) => call.arg("albumId", *id),
  }
}

/// An album's page: its details, or `not_found`.
pub async fn detail(api: &Api, key: &Key) -> Result<Post> {
  let data = album_call(api, "GetAlbumDetail", key).send().await?;
  parse::album(&data).ok_or_else(|| Error::not_found(format!("no album {}", songs::show(key))))
}

/// An album with its tracks (in the text and in `extra.tracks`).
pub async fn read(api: &Api, key: &Key) -> Result<Post> {
  let tracks = album_call(api, "GetAlbumSongList", key)
    .arg("begin", 0)
    .arg("num", READ_TRACKS)
    .arg("order", 2);
  let mut data = api
    .batch(vec![album_call(api, "GetAlbumDetail", key), tracks])
    .await?;
  let list = data.pop().unwrap_or_default();
  let detail = data.pop().unwrap_or_default();
  let mut album = parse::album(&detail)
    .ok_or_else(|| Error::not_found(format!("no album {}", songs::show(key))))?;
  let tracks: Vec<Post> = list
    .list("songList")
    .iter()
    .filter_map(|s| parse::song(s.at("songInfo")))
    .collect();
  let total = list.u64("totalNum").unwrap_or(tracks.len() as u64);
  let heading = if total > tracks.len() as u64 {
    format!("Tracks (first {} of {total})", tracks.len())
  } else {
    "Tracks".to_owned()
  };
  with_tracks(&mut album, &heading, &tracks);
  album.raw = Some(detail);
  Ok(album)
}

/// Collect an album (收藏专辑), or drop it.
pub async fn favorite(api: &Api, key: &Key, undo: bool) -> Result<Action> {
  api.require_login()?;
  let mid = match key {
    Key::Mid(mid) => mid.clone(),
    Key::Id(_) => detail(api, key).await?.id,
  };
  let uin = api.uin().unwrap_or(0).to_string();
  let method = if undo { "CancelFavAlbum" } else { "FavAlbum" };
  let data = api
    .call("music.musicasset.AlbumFavWrite", method)
    .arg("uin", uin)
    .arg("v_albumMid", vec![mid.clone()])
    .write()
    .send()
    .await?;
  if let Some(result) = data.i64("result").filter(|r| *r != 0) {
    return Err(Error::upstream(format!(
      "QQ Music did not change the collection ({result})"
    )));
  }
  Ok(favorited(undo, format!("album:{mid}")))
}

/// `favorite` / `unfavorite` done.
pub fn favorited(undo: bool, target: String) -> Action {
  Action::done(if undo { "unfavorite" } else { "favorite" }, target)
}
