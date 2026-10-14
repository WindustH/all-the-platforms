//! Your library: liked songs (我喜欢的音乐), collected playlists and
//! albums, recent plays and the listening ranking (听歌排行).

use atp_core::paging::slice;
use atp_core::text::from_millis;
use atp_core::{Error, Page, PageReq, Post, Result, ValueExt, json};

use crate::api::Api;
use crate::{parse, songs, users};

/// `favorites --folder` values; songs without one.
pub const FOLDERS: &[&str] = &["songs", "playlists", "albums"];
/// Songs one page of liked songs fetches at most.
const PAGE_SONGS: usize = 100;

/// Liked songs, playlists or albums someone collected (`folder`).
pub async fn favorites(
  api: &Api,
  user: Option<&str>,
  folder: Option<&str>,
  page: &PageReq,
) -> Result<Page<Post>> {
  let uid = users::id_or_me(api, user).await?;
  match folder.unwrap_or(FOLDERS[0]) {
    "songs" | "liked" => liked(api, &uid, page).await,
    "playlists" => users::playlists(api, &uid, true, page).await,
    "albums" if user.is_none() => albums(api, page).await,
    "albums" => Err(Error::unsupported("favorites --folder albums of others")),
    other => Err(Error::input(format!(
      "no folder {other}: use {}",
      FOLDERS.join(", ")
    ))),
  }
}

/// Liked songs (红心), newest first; the cursor is an offset.
pub async fn liked(api: &Api, uid: &str, page: &PageReq) -> Result<Page<Post>> {
  let v = api.web("song/like/get").arg("uid", uid).send().await?;
  let ids: Vec<String> = v.list("ids").iter().filter_map(|id| id.str("")).collect();
  let start = (page.number_or(0) as usize).min(ids.len());
  let end = (start + page.size_within(PAGE_SONGS)).min(ids.len());
  let posts = songs::details(api, &ids[start..end]).await?;
  Ok(Page::new(posts, (end < ids.len()).then(|| end.to_string())))
}

/// Albums you collected.
async fn albums(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  let offset = page.number_or(0);
  let v = api
    .web("album/sublist")
    .arg("limit", page.size_within(100) as u64)
    .arg("offset", offset)
    .arg("total", true)
    .send()
    .await?;
  let list = v.list("data");
  let posts = list.iter().filter_map(parse::album).collect();
  let end = offset + list.len() as u64;
  let more = v.bool("hasMore") == Some(true) && !list.is_empty();
  Ok(Page::new(posts, more.then(|| end.to_string())))
}

/// Songs you played lately, latest first, with when (`extra.played_at`).
pub async fn history(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let v = api
    .web("play-record/song/list")
    .arg("limit", 100)
    .send()
    .await?;
  let posts = v
    .list("data.list")
    .iter()
    .filter_map(|item| {
      let mut song = parse::song(item.at("data"))?;
      if let Some(at) = item.i64("playTime").and_then(from_millis) {
        song.extra.insert("played_at".into(), json!(at.to_string()));
      }
      Some(song)
    })
    .collect();
  Ok(slice(posts, page))
}

/// The listening ranking of a user (you without one): the last week, or
/// all time.
pub async fn ranking(
  api: &Api,
  user: Option<&str>,
  all_time: bool,
  page: &PageReq,
) -> Result<Page<Post>> {
  let uid = users::id_or_me(api, user).await?;
  let v = api
    .web("v1/play/record")
    .arg("uid", uid.as_str())
    .arg("type", if all_time { 0 } else { 1 })
    .send()
    .await?;
  let key = if all_time { "allData" } else { "weekData" };
  let posts = v
    .list(key)
    .iter()
    .filter_map(|item| {
      let mut song = parse::song(item.at("song"))?;
      if let Some(n) = item.u64("playCount") {
        song.metrics.views = Some(n);
      }
      if let Some(score) = item.u64("score") {
        song.extra.insert("score".into(), json!(score));
      }
      Some(song)
    })
    .collect();
  Ok(slice(posts, page))
}
