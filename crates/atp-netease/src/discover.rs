//! Discovery: the charts (榜单) and your daily recommendations (每日推荐).

use atp_core::paging::slice;
use atp_core::{Page, PageReq, Post, Result, ValueExt, json};

use crate::api::Api;
use crate::parse;
use crate::playlists;

/// `hot --category` values; the first is the default. `atp ncm toplists`
/// lists every chart.
pub const CHARTS: &[&str] = &[
  "hot",
  "soaring",
  "new",
  "original",
  "rap",
  "electronic",
  "acg",
  "classical",
  "rock",
  "folk",
  "guofeng",
  "western",
  "korean",
  "japanese",
  "vip",
  "billboard",
  "uk",
  "oricon",
];

/// The playlist id of a chart in [`CHARTS`].
fn chart_id(name: &str) -> u64 {
  match name {
    "soaring" => 19723756,      // 飙升榜
    "new" => 3779629,           // 新歌榜
    "original" => 2884035,      // 原创榜
    "rap" => 991319590,         // 中文说唱榜
    "electronic" => 1978921795, // 电音榜
    "acg" => 71385702,          // ACG 榜
    "classical" => 71384707,    // 古典榜
    "rock" => 5059633707,       // 摇滚榜
    "folk" => 5059661515,       // 民谣榜
    "guofeng" => 5059642708,    // 国风榜
    "western" => 2809513713,    // 欧美热歌榜
    "korean" => 745956260,      // 韩语榜
    "japanese" => 5059644681,   // 日语榜
    "vip" => 5453912201,        // 黑胶VIP爱听榜
    "billboard" => 60198,       // 美国 Billboard 榜
    "uk" => 180106,             // UK 排行榜周榜
    "oricon" => 60131,          // 日本 Oricon 榜
    _ => 3778678,               // 热歌榜
  }
}

/// The songs of a chart, in chart order.
pub async fn hot(api: &Api, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let id = chart_id(category.unwrap_or(CHARTS[0]));
  playlists::tracks(api, id, page).await
}

/// Every chart, as playlists to `read`.
pub async fn toplists(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  let v = api.web("toplist").send().await?;
  let charts = v.list("list").iter().filter_map(parse::playlist).collect();
  Ok(slice(charts, page))
}

/// Today's recommended songs (needs a login), with the reason when given.
pub async fn daily(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let v = api.web("v3/discovery/recommend/songs").send().await?;
  let reasons = v.list("data.recommendReasons");
  let songs = v
    .list("data.dailySongs")
    .iter()
    .filter_map(|s| {
      let mut song = parse::song(s)?;
      let reason = reasons
        .iter()
        .find(|r| r.str("songId").as_deref() == Some(song.id.as_str()))
        .and_then(|r| r.str("reason"))
        .or_else(|| s.str("reason"));
      if let Some(reason) = reason {
        song.extra.insert("reason".into(), json!(reason));
      }
      Some(song)
    })
    .collect();
  Ok(slice(songs, page))
}
