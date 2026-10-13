//! Live rooms (直播): room info, the rooms you follow, recent danmaku,
//! sending danmaku, and the live danmaku stream for `listen` ([`live`]).

use jiff::Timestamp;
use social_core::chat;
use social_core::http::Part;
use social_core::text::from_secs;
use social_core::{Action, Error, Page, PageReq, Post, Result, User, Value, ValueExt, json};

use crate::api::Api;
use crate::refs::{self, room_url};
use crate::{parse, users};

const INFO: &str = "https://api.live.bilibili.com/xlive/web-room/v1/index/getInfoByRoom";
const OF_USER: &str = "https://api.live.bilibili.com/room/v1/Room/getRoomInfoOld";
const FOLLOWING: &str = "https://api.live.bilibili.com/xlive/web-ucenter/user/following";
const HISTORY: &str = "https://api.live.bilibili.com/xlive/web-room/v1/dM/gethistory";
const SEND: &str = "https://api.live.bilibili.com/msg/send";

/// A room id from a room link, `live:ID` or a bare number, or the room of a
/// user (`uid:MID`, `@name`, space link).
pub async fn room_id(api: &Api, input: &str) -> Result<u64> {
  if let Some(id) = refs::room(input).or_else(|| input.trim().parse().ok()) {
    return Ok(id);
  }
  let mid = users::mid(api, input).await?;
  let data = api.get(OF_USER).arg("mid", &mid).send().await?;
  data
    .u64("roomid")
    .filter(|id| *id > 0)
    .ok_or_else(|| Error::not_found(format!("user {mid} has no live room")))
}

/// Raw `getInfoByRoom` data (room_info, anchor_info, watched_show ...).
pub async fn info(api: &Api, room: u64) -> Result<Value> {
  api.get(INFO).arg("room_id", room).send().await
}

/// A room as a post: title, streamer, live state, audience.
pub async fn read(api: &Api, input: &str) -> Result<Post> {
  let room = room_id(api, input).await?;
  let data = info(api, room).await?;
  let r = data.at("room_info");
  let mut post = room_post(r);
  post.author = Some(User {
    id: r.str("uid").unwrap_or_default(),
    name: data.str("anchor_info.base_info.uname").unwrap_or_default(),
    avatar: data.str("anchor_info.base_info.face").map(parse::https),
    url: r.str("uid").map(refs::space_url),
    ..User::default()
  });
  post.text = r
    .str("description")
    .map(|d| social_core::text::html_to_text(&d));
  if let Some(n) = data.count("watched_show.num") {
    post.metrics.other.insert("watched".into(), n);
  }
  if let Some(n) = data.count("anchor_info.relation_info.attention") {
    post.extra.insert("followers".into(), json!(n));
  }
  post.raw = Some(data);
  Ok(post)
}

/// A room from any listing (info, search, following, history ...).
pub fn room_post(v: &Value) -> Post {
  let id = v.first_str(&["room_id", "roomid"]).unwrap_or_default();
  let status = v.u64("live_status");
  let mut post = Post {
    url: Some(room_url(&id)),
    id,
    kind: "live".into(),
    title: parse::plain(v, &["title"]),
    author: parse::author(v, &["uid"], &["uname"], &["uface", "face"]),
    created_at: v
      .i64("live_start_time")
      .and_then(from_secs)
      .or_else(|| v.str("live_time").and_then(|t| parse::beijing(&t))),
    raw: Some(v.clone()),
    ..Post::default()
  };
  post.metrics.views = v.count("online").filter(|n| *n > 0);
  if let Some(s) = status {
    post.extra.insert("live".into(), json!(s == 1));
  }
  for (key, paths) in [
    ("area", &["area_name", "area_name_v2", "cate_name"][..]),
    ("parent_area", &["parent_area_name"][..]),
    ("cover", &["user_cover", "cover", "room_cover"][..]),
    ("short_id", &["short_id"][..]),
  ] {
    if let Some(x) = v.first_str(paths).filter(|x| !x.is_empty() && x != "0") {
      let x = if key == "cover" {
        parse::https(x)
      } else {
        social_core::text::html_to_text(&x)
      };
      post.extra.insert(key.into(), json!(x));
    }
  }
  post
}

/// Rooms of the people you follow, live ones first.
pub async fn following(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let pn = page.number_or(1);
  let data = api
    .get(FOLLOWING)
    .arg("page", pn)
    .arg("page_size", page.size_within(10))
    .send()
    .await?;
  let posts: Vec<Post> = data.list("list").iter().map(room_post).collect();
  let more = pn < data.u64("totalPage").unwrap_or(0);
  let next = (more && !posts.is_empty()).then(|| (pn + 1).to_string());
  Ok(Page::new(posts, next))
}

/// The last danmaku of a room (about ten).
pub async fn recent(api: &Api, input: &str) -> Result<Page<Post>> {
  let room = room_id(api, input).await?;
  let data = api
    .get(HISTORY)
    .arg("roomid", room)
    .arg("room_type", 0)
    .send()
    .await?;
  let posts = data
    .list("room")
    .iter()
    .map(|d| {
      let at = d
        .i64("check_info.ts")
        .and_then(from_secs)
        .or_else(|| d.str("timeline").and_then(|t| parse::beijing(&t)));
      danmaku_post(
        room,
        d.str("id_str").unwrap_or_default(),
        d.str("text").unwrap_or_default(),
        d.str("uid").unwrap_or_default(),
        d.str("nickname").unwrap_or_default(),
        at,
        d.clone(),
      )
    })
    .collect();
  Ok(Page::last(posts))
}

/// One live danmaku as a post with id `ROOM/DANMAKU`.
pub fn danmaku_post(
  room: u64,
  id: String,
  text: String,
  uid: String,
  name: String,
  at: Option<Timestamp>,
  raw: Value,
) -> Post {
  let mut post = Post {
    id: chat::message_id(&room.to_string(), &id),
    kind: "message".into(),
    text: Some(text),
    author: Some(User {
      url: (!uid.is_empty() && uid != "0").then(|| refs::space_url(&uid)),
      id: uid,
      name,
      ..User::default()
    }),
    created_at: at,
    raw: Some(raw),
    ..Post::default()
  };
  post
    .extra
    .insert("chat".into(), json!(format!("live:{room}")));
  post
}

/// Send danmaku to a live room.
pub async fn send(api: &Api, input: &str, text: &str) -> Result<Action> {
  let room = room_id(api, input).await?;
  let now = Timestamp::now().as_second().to_string();
  let parts = vec![
    Part::text("bubble", "0"),
    Part::text("msg", text),
    Part::text("color", "16777215"),
    Part::text("mode", "1"),
    Part::text("room_type", "0"),
    Part::text("jumpfrom", "0"),
    Part::text("reply_mid", "0"),
    Part::text("reply_attr", "0"),
    Part::text("replay_dmid", ""),
    Part::text("statistics", r#"{"appId":100,"platform":5}"#),
    Part::text("reply_type", "0"),
    Part::text("reply_uname", ""),
    Part::text("fontsize", "25"),
    Part::text("rnd", now),
    Part::text("roomid", room.to_string()),
  ];
  api
    .post(SEND)
    .multipart(parts)
    .query("web_location", "444.8")
    .wbi()
    .send()
    .await?;
  Ok(Action::done("send", format!("live:{room}")).with_url(room_url(room)))
}
