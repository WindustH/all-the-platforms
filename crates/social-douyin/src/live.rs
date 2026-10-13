//! A live room (`live.douyin.com/<web_rid>`): title, anchor, audience and
//! the pull stream addresses.

use social_core::text::from_secs;
use social_core::{Ctx, Error, Extra, Post, Result, Value, ValueExt};

use crate::{api, parse, refs};

const ENTER: &str = "/webcast/room/web/enter/";

pub async fn room(ctx: &Ctx, arg: &str) -> Result<Post> {
  let web_rid = refs::live(arg)?;
  let chrome = format!("{}.0.0.0", social_core::http::CHROME_VERSION);
  let v = api::get(ctx, ENTER)
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
    .arg("web_rid", &web_rid)
    .arg("enter_source", "")
    .arg("is_need_double_stream", "false")
    .send()
    .await?;
  let data = v.at("data");
  let room = data.at("data.0");
  if !room.is_object() {
    return Err(Error::not_found(format!("no Douyin live room {web_rid}")));
  }
  let anchor = data.at("user");
  let live = room.i64("status") == Some(2);
  let mut extra = Extra::new();
  extra.insert("live".into(), live.into());
  extra.insert("web_rid".into(), web_rid.clone().into());
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
  Ok(Post {
    id: room.str("id_str").unwrap_or_else(|| web_rid.clone()),
    kind: "live".into(),
    title: room.str("title"),
    url: Some(format!("https://live.douyin.com/{web_rid}")),
    author: anchor.as_object().and(parse::user(anchor)),
    created_at: room.i64("create_time").and_then(from_secs),
    extra,
    raw: Some(v.clone()),
    ..Post::default()
  })
}
