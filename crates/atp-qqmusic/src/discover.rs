//! Discovery: the charts (榜单) and the songs QQ Music picks for you
//! (猜你喜欢, the personal radio).

use atp_core::music::with_tracks;
use atp_core::paging::slice;
use atp_core::{Error, Page, PageReq, Post, Result, Value, ValueExt, json};

use crate::api::{Api, Call};
use crate::parse;

/// `hot --category` values; the first is the default. `atp qqmusic
/// toplists` lists every chart.
pub const CHARTS: &[&str] = &[
  "hot",
  "soaring",
  "new",
  "popular",
  "mainland",
  "hongkong",
  "taiwan",
  "western",
  "korean",
  "japanese",
  "rap",
  "electronic",
  "acg",
  "games",
  "film",
  "guofeng",
  "ktv",
  "douyin",
  "internet",
  "billboard",
  "uk",
  "oricon",
  "melon",
];

/// The `topId` of a chart in [`CHARTS`].
fn chart_id(name: &str) -> u64 {
  match name {
    "soaring" => 62,    // 飙升榜
    "new" => 27,        // 新歌榜
    "popular" => 4,     // 流行指数榜
    "mainland" => 5,    // 内地榜
    "hongkong" => 59,   // 香港地区榜
    "taiwan" => 61,     // 台湾地区榜
    "western" => 3,     // 欧美榜
    "korean" => 16,     // 韩国榜
    "japanese" => 17,   // 日本榜
    "rap" => 58,        // 说唱榜
    "electronic" => 57, // 电音榜
    "acg" => 72,        // 动漫音乐榜
    "games" => 73,      // 游戏音乐榜
    "film" => 29,       // 影视金曲榜
    "guofeng" => 65,    // 国风热歌榜
    "ktv" => 36,        // K歌金曲榜
    "douyin" => 60,     // 抖音热歌榜
    "internet" => 28,   // 网络歌曲榜
    "billboard" => 108, // 美国公告牌榜
    "uk" => 107,        // 英国UK榜
    "oricon" => 105,    // 日本公信榜
    "melon" => 129,     // 韩国Melon榜
    _ => 26,            // 热歌榜
  }
}

/// Tracks `read` shows.
const READ_TRACKS: usize = 100;
/// Tracks one chart page fetches at most.
const PAGE_TRACKS: usize = 100;
/// The personal radio of 猜你喜欢.
const GUESS_RADIO: u64 = 99;
/// Songs one batch of the radio holds at most.
const GUESS_SONGS: usize = 20;

/// The chart call: its details and `num` songs from `offset`.
fn detail_call(api: &Api, id: u64, offset: u64, num: usize) -> Call<'_> {
  api
    .call("music.musicToplist.Toplist", "GetDetail")
    .arg("topId", id)
    .arg("offset", offset)
    .arg("num", num)
    .arg("period", "")
}

/// The songs of a chart page, numbered by their place.
fn ranked(data: &Value, offset: u64) -> Vec<Post> {
  data
    .list("songInfoList")
    .iter()
    .filter_map(parse::song)
    .zip(offset + 1..)
    .map(|(mut song, rank)| {
      song.extra.insert("rank".into(), json!(rank));
      song
    })
    .collect()
}

/// The songs of a chart, in chart order.
pub async fn hot(api: &Api, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let id = chart_id(category.unwrap_or(CHARTS[0]));
  tracks(api, id, page).await
}

/// The songs of a chart by its `topId`, a page at a time (the cursor is an
/// offset).
pub async fn tracks(api: &Api, id: u64, page: &PageReq) -> Result<Page<Post>> {
  let offset = page.number_or(0);
  let data = detail_call(api, id, offset, page.size_within(PAGE_TRACKS))
    .send()
    .await?;
  let songs = ranked(&data, offset);
  let total = data.u64("data.totalNum").unwrap_or(0);
  let end = offset + songs.len() as u64;
  let next = (!songs.is_empty() && end < total).then(|| end.to_string());
  Ok(Page::new(songs, next))
}

/// A chart with its first songs.
pub async fn read(api: &Api, id: u64) -> Result<Post> {
  let data = detail_call(api, id, 0, READ_TRACKS).send().await?;
  let mut chart =
    parse::toplist(data.at("data")).ok_or_else(|| Error::not_found(format!("no chart {id}")))?;
  let songs = ranked(&data, 0);
  let total = data.u64("data.totalNum").unwrap_or(0);
  let heading = if total > songs.len() as u64 {
    format!("Songs (first {} of {total})", songs.len())
  } else {
    "Songs".to_owned()
  };
  with_tracks(&mut chart, &heading, &songs);
  chart.raw = Some(data);
  Ok(chart)
}

/// Every chart, with the group it is shown in.
pub async fn toplists(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  let data = api
    .call("music.musicToplist.Toplist", "GetAll")
    .send()
    .await?;
  let mut charts = Vec::new();
  for group in data.list("group") {
    for item in group.list("toplist") {
      let Some(mut chart) = parse::toplist(item) else {
        continue;
      };
      if let Some(name) = group.str("groupName") {
        chart.extra.insert("group".into(), json!(name));
      }
      charts.push(chart);
    }
  }
  Ok(slice(charts, page))
}

/// Songs picked for you (猜你喜欢; needs a login): one batch, a new one
/// each time.
pub async fn guess(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let data = api
    .call("music.radioProxy.MbTrackRadioSvr", "get_radio_track")
    .arg("id", GUESS_RADIO)
    .arg("num", page.size_within(GUESS_SONGS))
    .arg("from", 0)
    .arg("scene", 0)
    .arg("song_ids", json!([]))
    .send()
    .await?;
  Ok(Page::last(
    data.list("tracks").iter().filter_map(parse::song).collect(),
  ))
}
