//! Libraries: liked songs (我喜欢), collected playlists and collected
//! albums, of anyone (by encrypted id) or of the session's account.

use atp_core::{Error, Page, PageReq, Post, Result, Value, ValueExt};

use crate::api::Api;
use crate::{parse, songs, users};

/// `favorites --folder` values; songs without one.
pub const FOLDERS: &[&str] = &["songs", "playlists", "albums"];
/// Items one page fetches at most.
const PAGE_SIZE: usize = 100;

/// Liked songs, or playlists or albums someone collected (`folder`).
pub async fn favorites(
  api: &Api,
  user: Option<&str>,
  folder: Option<&str>,
  page: &PageReq,
) -> Result<Page<Post>> {
  let euin = users::euin_or_me(api, user).await?;
  match folder.unwrap_or(FOLDERS[0]) {
    "songs" | "liked" => liked(api, &euin, page).await,
    "playlists" => collected(api, &euin, false, page).await,
    "albums" => collected(api, &euin, true, page).await,
    other => Err(Error::input(format!(
      "no folder {other}: use {}",
      FOLDERS.join(", ")
    ))),
  }
}

/// Liked songs (我喜欢), newest first; the cursor is an offset.
pub async fn liked(api: &Api, euin: &str, page: &PageReq) -> Result<Page<Post>> {
  let offset = page.number_or(0);
  let data = api
    .call("music.srfDissInfo.DissInfo", "CgiGetDiss")
    .arg("disstid", 0)
    .arg("dirid", songs::LIKED_DIR)
    .arg("tag", 1)
    .arg("song_begin", offset)
    .arg("song_num", page.size_within(PAGE_SIZE))
    .arg("userinfo", 1)
    .arg("orderlist", 1)
    .arg("enc_host_uin", euin)
    .send()
    .await?;
  let posts: Vec<Post> = data
    .list("songlist")
    .iter()
    .filter_map(parse::song)
    .collect();
  let total = data
    .u64("total_song_num")
    .or_else(|| data.u64("dirinfo.songnum"))
    .unwrap_or(0);
  let end = offset + posts.len() as u64;
  let next = (!posts.is_empty() && end < total).then(|| end.to_string());
  Ok(Page::new(posts, next))
}

/// Collected albums (`albums`) or playlists; the cursor is an offset.
async fn collected(api: &Api, euin: &str, albums: bool, page: &PageReq) -> Result<Page<Post>> {
  let offset = page.number_or(0);
  let call = if albums {
    api
      .call("music.musicasset.AlbumFavRead", "CgiGetAlbumFavInfo")
      .arg("euin", euin)
  } else {
    api
      .call("music.musicasset.PlaylistFavRead", "CgiGetPlaylistFavInfo")
      .arg("uin", euin)
  };
  let data = call
    .arg("offset", offset)
    .arg("size", page.size_within(PAGE_SIZE))
    .send()
    .await?;
  let list = data.list("v_list");
  let parse: fn(&Value) -> Option<Post> = if albums {
    parse::album
  } else {
    parse::playlist
  };
  let posts = list.iter().filter_map(parse).collect();
  let end = offset + list.len() as u64;
  let more = data.bool("hasmore").unwrap_or(false) && !list.is_empty();
  Ok(Page::new(posts, more.then(|| end.to_string())))
}
