//! Playlists (歌单): one with its tracks, its tracks page by page,
//! collecting someone's, and the account's own: create, add, remove,
//! delete. The account's playlists are changed by their `dirId`, which
//! [`dir`] finds for a playlist id.

use atp_core::cli::confirm;
use atp_core::music::with_tracks;
use atp_core::{Action, Error, Page, PageReq, Post, Result, Value, ValueExt};

use crate::api::{Api, Call};
use crate::comments::{self, Thread};
use crate::refs::{self, Key};
use crate::{albums, parse, songs};

/// Tracks `read` shows.
const READ_TRACKS: usize = 100;
/// Tracks one page of `playlist tracks` fetches at most.
const PAGE_TRACKS: usize = 100;
/// Longest name the web player accepts, counting wide characters twice.
const NAME_WIDTH: usize = 40;

/// The playlist page's call: details and `num` tracks from `begin`.
fn info_call(api: &Api, id: u64, begin: u64, num: usize) -> Call<'_> {
  api
    .call("music.srfDissInfo.aiDissInfo", "uniform_get_Dissinfo")
    .arg("disstid", id)
    .arg("userinfo", 1)
    .arg("tag", 1)
    .arg("song_begin", begin)
    .arg("song_num", num)
    .arg("onlysonglist", 0)
    .arg("orderlist", 1)
    .arg("enc_host_uin", "")
}

fn total(data: &Value) -> u64 {
  data
    .u64("total_song_num")
    .or_else(|| data.u64("dirinfo.songnum"))
    .unwrap_or(0)
}

/// A playlist with its first tracks (in the text and in `extra.tracks`)
/// and its number of comments.
pub async fn read(api: &Api, id: u64) -> Result<Post> {
  let thread = Thread::playlist(id);
  let mut data = api
    .batch(vec![
      info_call(api, id, 0, READ_TRACKS),
      comments::count_call(api, &thread),
    ])
    .await?;
  let count = data.pop().unwrap_or_default();
  let info = data.pop().unwrap_or_default();
  let mut playlist = parse::playlist(info.at("dirinfo"))
    .ok_or_else(|| Error::not_found(format!("no playlist {id}")))?;
  let tracks: Vec<Post> = info
    .list("songlist")
    .iter()
    .filter_map(parse::song)
    .collect();
  let total = total(&info).max(tracks.len() as u64);
  let heading = if total > tracks.len() as u64 {
    format!("Tracks (first {} of {total})", tracks.len())
  } else {
    "Tracks".to_owned()
  };
  with_tracks(&mut playlist, &heading, &tracks);
  playlist.metrics.comments = count.u64("response.count");
  playlist.raw = Some(info);
  Ok(playlist)
}

/// The tracks of a playlist, a page at a time (the cursor is an offset).
pub async fn tracks(api: &Api, id: u64, page: &PageReq) -> Result<Page<Post>> {
  let offset = page.number_or(0);
  let data = info_call(api, id, offset, page.size_within(PAGE_TRACKS))
    .send()
    .await?;
  let posts: Vec<Post> = data
    .list("songlist")
    .iter()
    .filter_map(parse::song)
    .collect();
  let end = offset + posts.len() as u64;
  let next = (!posts.is_empty() && end < total(&data)).then(|| end.to_string());
  Ok(Page::new(posts, next))
}

/// Collect a playlist (收藏歌单), or drop it.
pub async fn favorite(api: &Api, id: u64, undo: bool) -> Result<Action> {
  api.require_login()?;
  let method = if undo {
    "CancelFavPlaylist"
  } else {
    "FavPlaylist"
  };
  api
    .call("music.musicasset.PlaylistFavWrite", method)
    .arg("uin", api.uin().unwrap_or(0).to_string())
    .arg("v_playlistId", vec![id])
    .write()
    .send()
    .await?;
  Ok(albums::favorited(undo, format!("playlist:{id}")))
}

/// The account's own playlists, as the gateway lists them (`tid` is the
/// playlist id, `dirId` the account's number for it).
pub async fn mine(api: &Api) -> Result<Vec<Value>> {
  api.require_login()?;
  let uin = api.uin().ok_or_else(|| {
    Error::auth("the session has no account number").with_hint(api.ctx.login_hint())
  })?;
  let data = api
    .call("music.musicasset.PlaylistBaseRead", "GetPlaylistByUin")
    .arg("uin", uin.to_string())
    .arg("bWithoutStatus", false)
    .send()
    .await?;
  Ok(data.list("v_playlist").to_vec())
}

/// The `dirId` of one of the account's playlists by its id.
pub async fn dir(api: &Api, id: u64) -> Result<u64> {
  mine(api)
    .await?
    .iter()
    .find(|p| p.u64("tid") == Some(id))
    .and_then(|p| p.u64("dirId"))
    .ok_or_else(|| Error::input(format!("playlist {id} is not one of yours")))
}

/// A new playlist of the account's.
pub async fn create(api: &Api, name: &str) -> Result<Action> {
  let width: usize = name.chars().map(|c| if c.is_ascii() { 1 } else { 2 }).sum();
  if name.trim().is_empty() || width > NAME_WIDTH {
    return Err(Error::input(
      "a playlist name takes 1 to 40 letters (20 Chinese characters)",
    ));
  }
  let data = api
    .call("music.musicasset.PlaylistBaseWrite", "AddPlaylist")
    .arg("dirName", name)
    .write()
    .send()
    .await?;
  let result = data.at("result");
  let mut action = Action::done("create", name);
  if let Some(tid) = result.str("tid").filter(|t| t != "0") {
    action = action.with_url(refs::playlist_url(&tid)).with_id(tid);
  } else if let Some(dir) = result.str("dirId") {
    action = action.with_message(format!("dirId {dir}"));
  }
  Ok(action)
}

/// Add songs to one of the account's playlists, or take them out.
pub async fn change(api: &Api, id: u64, keys: &[Key], remove: bool) -> Result<Action> {
  let dir = dir(api, id).await?;
  let found = songs::details(api, keys).await?;
  if found.len() < keys.len() {
    return Err(Error::not_found(format!(
      "QQ Music knows {} of the {} songs",
      found.len(),
      keys.len()
    )));
  }
  songs::change(api, dir, &found, remove).await?;
  let action = if remove { "remove" } else { "add" };
  Ok(
    Action::done(action, format!("playlist:{id}")).with_message(format!("{} song(s)", found.len())),
  )
}

/// Delete one of the account's playlists, after a confirmation.
pub async fn delete(api: &Api, id: u64, yes: bool) -> Result<Action> {
  api.require_login()?;
  confirm(yes, &format!("delete playlist {id}"))?;
  let dir = dir(api, id).await?;
  if dir == songs::LIKED_DIR {
    return Err(Error::input("我喜欢 cannot be deleted"));
  }
  api
    .call("music.musicasset.PlaylistBaseWrite", "DelPlaylist")
    .arg("dirId", dir)
    .write()
    .send()
    .await?;
  Ok(Action::done("delete", format!("playlist:{id}")))
}
