//! `listen ROOM`: what happens in a live room, over the webcast websocket
//! (`webcast5-ws-web-hl.douyin.com`). The connection names the room and a
//! visitor id, signed with [`frontier`]; it starts at the newest message
//! (no `/webcast/im/fetch/` cursor). Chat, gifts, likes, arrivals,
//! follows, shares and audience counts come as posts, as Bilibili's
//! danmaku do.

use std::time::Duration;

use atp_bytedance::query::query_string;
use atp_bytedance::webcast::{self, Event, Sender};
use atp_core::http::USER_AGENT;
use atp_core::output::Stream;
use atp_core::socket::{Frame, Socket};
use atp_core::text::from_millis;
use atp_core::{Error, Post, Result, User, ValueExt, chat, json};
use rand::Rng;

use crate::api::Api;
use crate::refs;
use crate::rooms::{self, Room};
use crate::sign::frontier;

const PUSH: &str = "wss://webcast5-ws-web-hl.douyin.com/webcast/im/push/v2/";
const LIVE: &str = "https://live.douyin.com";
const FETCH: &str = "/webcast/im/fetch/";
const QUERY_USER: &str = "/aweme/v1/web/query/user/";
const HEARTBEAT: Duration = Duration::from_secs(10);

/// Follow every room in `rooms` until their streams end.
pub async fn listen(api: &Api, rooms: &[String], out: &Stream) -> Result<()> {
  if rooms.is_empty() {
    return Err(Error::input(
      "give the live rooms to follow: `listen ROOM` (a web_rid or a live.douyin.com link)",
    ));
  }
  let tasks = rooms.iter().map(|r| room_loop(api, r, out));
  futures_util::future::try_join_all(tasks).await?;
  Ok(())
}

async fn room_loop(api: &Api, arg: &str, out: &Stream) -> Result<()> {
  let room = rooms::find(api, arg).await?;
  if !room.live {
    return Err(Error::not_found(format!(
      "the Douyin room {} is not live now",
      room.web_rid
    )));
  }
  let mut sock = connect(api, &room).await?;
  let mut beat = tokio::time::interval(HEARTBEAT);
  loop {
    tokio::select! {
      _ = beat.tick() => sock.send_binary(webcast::frame(0, "", "hb", &[])).await?,
      frame = sock.next() => {
        let Some(frame) = frame? else {
          return Ok(());
        };
        let Frame::Binary(data) = frame else {
          continue;
        };
        let Some(batch) = webcast::decode(&data, "") else {
          continue;
        };
        if let Some(ack) = batch.ack {
          sock.send_binary(ack).await?;
        }
        if emit(&room, &batch.messages, out) {
          return Ok(());
        }
      }
    }
  }
}

/// Print the events of a batch; whether the live ended.
fn emit(room: &Room, messages: &[webcast::Message], out: &Stream) -> bool {
  let mut ended = false;
  for post in messages.iter().filter_map(|m| event(room, m)) {
    ended |= post.kind == "offline";
    out.post(&post);
  }
  ended
}

async fn connect(api: &Api, room: &Room) -> Result<Socket> {
  let user = visitor_id(api).await;
  let mut pairs: Vec<(String, String)> = [
    ("app_name", "douyin_web"),
    ("version_code", "180800"),
    ("device_platform", "web"),
    ("cookie_enabled", "true"),
    ("screen_width", "1536"),
    ("screen_height", "864"),
    ("browser_language", "zh-CN"),
    ("browser_platform", "MacIntel"),
    ("browser_name", "Mozilla"),
    ("browser_version", USER_AGENT.trim_start_matches("Mozilla/")),
    ("browser_online", "true"),
    ("tz_name", "Asia/Shanghai"),
    ("host", LIVE),
    ("aid", "6383"),
    ("live_id", "1"),
    ("did_rule", "3"),
    ("endpoint", "live_pc"),
    ("support_wrds", "1"),
    ("identity", "audience"),
    ("need_persist_msg_count", "15"),
    ("insert_task_id", ""),
    ("live_reason", ""),
    ("webcast_sdk_version", frontier::SDK_VERSION),
    ("update_version_code", frontier::SDK_VERSION),
    ("compress", "gzip"),
    ("im_path", FETCH),
    ("heartbeatDuration", "0"),
  ]
  .iter()
  .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
  .collect();
  for (k, v) in [
    ("room_id", room.id.clone()),
    ("user_unique_id", user.clone()),
    ("cursor", String::new()),
    ("internal_ext", String::new()),
    ("signature", frontier::signature(&room.id, &user)),
  ] {
    pairs.push((k.to_owned(), v));
  }
  let url = format!("{PUSH}?{}", query_string(&pairs));
  api
    .ctx
    .http
    .websocket(&url, &[("Origin", LIVE)], true)
    .await
}

/// The visitor's `user_unique_id` (`/query/user/`), or a made-up one.
async fn visitor_id(api: &Api) -> String {
  let id = match api.get(QUERY_USER).send().await {
    Ok(v) => v
      .str("id")
      .filter(|id| id.bytes().all(|b| b.is_ascii_digit())),
    Err(e) => {
      tracing::debug!("query/user: {e}");
      None
    }
  };
  id.unwrap_or_else(|| {
    let mut rng = rand::rng();
    (0..19)
      .map(|i| char::from(b'0' + rng.random_range(u8::from(i == 0)..10)))
      .collect()
  })
}

/// One message as a post: its kind, text and sender.
fn event(room: &Room, m: &webcast::Message) -> Option<Post> {
  let Event {
    kind,
    text,
    sender,
    at_ms,
    counts,
  } = webcast::event(m)?;
  let mut post = Post {
    id: chat::message_id(&room.web_rid, &m.id.to_string()),
    kind: kind.into(),
    text,
    author: sender.and_then(user),
    created_at: at_ms
      .and_then(|ms| from_millis(ms as i64))
      .or_else(|| Some(jiff::Timestamp::now())),
    raw: Some(json!({ "method": m.method, "msg_id": m.id.to_string() })),
    ..Post::default()
  };
  for (name, n) in counts {
    post.extra.insert(name.into(), json!(n));
  }
  post
    .extra
    .insert("chat".into(), json!(format!("live:{}", room.web_rid)));
  post.extra.insert("chat_name".into(), json!(room.title));
  Some(post)
}

/// A sender as a Douyin user: known by `sec_uid` where it has one.
fn user(s: Sender) -> Option<User> {
  Some(User {
    id: s.sec_uid.clone().or(s.id)?,
    name: s.nickname,
    handle: s.display_id,
    url: s.sec_uid.map(|sec| refs::user_url(&sec)),
    ..User::default()
  })
}
