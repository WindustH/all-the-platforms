//! Singers (歌手): one with their top songs and fans, and following
//! (关注) a singer or a user.

use atp_core::music::with_tracks;
use atp_core::{Action, Error, Post, Result, ValueExt, json};

use crate::api::Api;
use crate::parse;

/// Top songs a singer page lists in its text.
const TOP_SONGS: usize = 20;
/// `usertype` of the follow calls.
pub const USER: u64 = 0;
pub const SINGER: u64 = 1;

/// A singer with their top songs and how many follow them.
pub async fn read(api: &Api, mid: &str) -> Result<Post> {
  let detail = api
    .call("music.musichallSinger.SingerInfoInter", "GetSingerDetail")
    .arg("singer_mids", json!([mid]))
    .arg("ex_singer", 1)
    .arg("wiki_singer", 0)
    .arg("group_singer", 0)
    .arg("pic", 1)
    .arg("photos", 0);
  let songs = api
    .call("music.musichallSong.SongListInter", "GetSingerSongList")
    .arg("singerMid", mid)
    .arg("begin", 0)
    .arg("num", TOP_SONGS)
    .arg("order", 1);
  let fans = api
    .call("Concern.ConcernSystemServer", "cgi_qry_concern_num")
    .arg("vec_userinfo", json!([{"userid": mid, "usertype": SINGER}]));
  let data = api.batch(vec![detail, songs, fans]).await?;
  let mut singer = parse::artist(data[0].at("singer_list.0"))
    .ok_or_else(|| Error::not_found(format!("no singer {mid}")))?;
  let top: Vec<Post> = data[1]
    .list("songList")
    .iter()
    .filter_map(|s| parse::song(s.at("songInfo")))
    .collect();
  if let Some(n) = data[1].u64("totalNum") {
    singer.extra.insert("songs".into(), json!(n));
  }
  if let Some(fans) = data[2].u64(&format!("map_singer_num.{mid}.user_fansnum")) {
    singer.metrics.other.insert("fans".into(), fans);
  }
  with_tracks(&mut singer, "Top songs", &top);
  Ok(singer)
}

/// Follow (关注) a singer (`SINGER`, by mid) or a user (`USER`, by
/// encrypted id), or stop.
pub async fn follow(api: &Api, kind: u64, id: &str, undo: bool) -> Result<Action> {
  let data = api
    .call("Concern.ConcernSystemServer", "cgi_concern_user_v2")
    .arg("opertype", if undo { 1 } else { 0 })
    .arg("source", 0)
    .arg("userinfo", json!({"usertype": kind, "userid": id}))
    .write()
    .send()
    .await?;
  if let Some(code) = data.i64("code").filter(|c| *c != 0) {
    return Err(api.error(code, &data));
  }
  let target = if kind == SINGER {
    format!("singer:{id}")
  } else {
    id.to_owned()
  };
  Ok(Action::done(
    if undo { "unfollow" } else { "follow" },
    target,
  ))
}
