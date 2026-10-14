//! A live room (`live.douyin.com/<web_rid>`): title, anchor, audience and
//! the pull stream addresses; its ids for `listen`.

use atp_core::text::from_secs;
use atp_core::{Error, Extra, Post, Result, Value, ValueExt};

use crate::api::Api;
use crate::{parse, refs};

const ENTER: &str = "/webcast/room/web/enter/";

/// A room as `listen` connects to it.
pub struct Room {
  pub web_rid: String,
  pub id: String,
  pub title: String,
  pub live: bool,
}

pub async fn find(api: &Api, arg: &str) -> Result<Room> {
  let web_rid = refs::live(arg)?;
  let v = enter(api, &web_rid).await?;
  let room = v.at("data.data.0");
  let Some(id) = room.str("id_str").filter(|id| id != "0") else {
    return Err(Error::not_found(format!("no Douyin live room {web_rid}")));
  };
  Ok(Room {
    id,
    title: room.str("title").unwrap_or_default(),
    live: room.i64("status") == Some(2),
    web_rid,
  })
}

pub async fn room(api: &Api, arg: &str) -> Result<Post> {
  let web_rid = refs::live(arg)?;
  let v = enter(api, &web_rid).await?;
  let data = v.at("data");
  let room = data.at("data.0");
  if !room.is_object() {
    return Err(Error::not_found(format!("no Douyin live room {web_rid}")));
  }
  let anchor = data.at("user");
  Ok(Post {
    id: room.str("id_str").unwrap_or_else(|| web_rid.clone()),
    kind: "live".into(),
    title: room.str("title"),
    url: Some(format!("https://live.douyin.com/{web_rid}")),
    author: anchor.as_object().and(parse::user(anchor)),
    created_at: room.i64("create_time").and_then(from_secs),
    extra: room_extra(room, &web_rid),
    raw: Some(v.clone()),
    ..Post::default()
  })
}

/// The room as the live site loads it (its own parameters, not the web app's).
async fn enter(api: &Api, web_rid: &str) -> Result<Value> {
  let chrome = format!("{}.0.0.0", atp_core::http::CHROME_VERSION);
  api
    .get(ENTER)
    .bare()
    .arg("aid", "6383")
    .arg("app_name", "douyin_web")
    .arg("live_id", "1")
    .arg("device_platform", "web")
    .arg("language", "zh-CN")
    .arg("enter_from", "web_live")
    .arg("cookie_enabled", "true")
    .arg("screen_width", "1536")
    .arg("screen_height", "864")
    .arg("browser_language", "zh-CN")
    .arg("browser_platform", "MacIntel")
    .arg("browser_name", "Chrome")
    .arg("browser_version", &chrome)
    .arg("web_rid", web_rid)
    .arg("enter_source", "")
    .arg("is_need_double_stream", "false")
    .send()
    .await
}

/// Live state, ids, audience counters and the pull stream addresses.
fn room_extra(room: &Value, web_rid: &str) -> Extra {
  let mut extra = Extra::new();
  extra.insert("live".into(), (room.i64("status") == Some(2)).into());
  extra.insert("web_rid".into(), web_rid.into());
  for (key, path) in [
    ("room_id", "id_str"),
    ("viewers", "room_view_stats.display_value"),
    ("audience", "user_count_str"),
    ("likes", "like_count"),
  ] {
    if let Some(value) = room.str(path) {
      extra.insert(key.into(), value.into());
    }
  }
  let streams: serde_json::Map<String, Value> = ["flv_pull_url", "hls_pull_url_map"]
    .iter()
    .filter_map(|k| {
      let map = room.at(&format!("stream_url.{k}"));
      map.is_object().then(|| ((*k).to_owned(), map.clone()))
    })
    .collect();
  if !streams.is_empty() {
    extra.insert("streams".into(), Value::Object(streams));
  }
  extra
}
