//! `listen @HANDLE`: what happens in a live room, over the webcast
//! websocket (`webcast-ws.tiktok.com`), the protocol Douyin speaks too
//! ([`atp_bytedance::webcast`]). The connection names the room and carries
//! a `frontierSign` ([`sign::webcast_signature`]); the first frame enters
//! the room, heartbeats name it every ten seconds. Chat, gifts, likes,
//! arrivals, follows, shares and viewer counts come as posts; a dropped
//! connection is opened again until the live ends.

use std::time::Duration;

use atp_bytedance::query::query_string;
use atp_bytedance::webcast::{self, Event, Sender};
use atp_core::http::USER_AGENT;
use atp_core::live::{Backoff, Seen};
use atp_core::output::Stream;
use atp_core::proto::{Msg, put_bytes, put_int};
use atp_core::socket::{Frame, Socket};
use atp_core::text::from_millis;
use atp_core::{Error, ErrorCode, Post, Result, User, chat, json};
use tokio::time::{Instant, interval_at};

use crate::api::{Api, WWW};
use crate::refs;
use crate::rooms::{self, Room};
use crate::sign;

const PUSH: &str = "wss://webcast-ws.tiktok.com/webcast/im/ws_proxy/ws_reuse_supplement/";
/// TikTok names the payload encoding of its frames.
const ENCODING: &str = "pb";
/// The web's `live_id`.
const LIVE_ID: u64 = 12;
const HEARTBEAT: Duration = Duration::from_secs(10);
/// A connection that lasted this long resets the reconnect delay.
const STABLE: Duration = Duration::from_secs(60);
/// Viewer counts come every few seconds; one in this long is printed.
const STATS_EVERY: Duration = Duration::from_secs(30);

/// What a room printed: message ids, and when viewer counts last.
#[derive(Default)]
struct Printed {
  seen: Seen,
  stats: Option<Instant>,
}

impl Printed {
  /// Whether a message is worth printing: new, and for viewer counts the
  /// first in [`STATS_EVERY`].
  fn admit(&mut self, id: u64, kind: &str) -> bool {
    if id != 0 && !self.seen.insert(&id.to_string()) {
      return false;
    }
    if kind != "stats" {
      return true;
    }
    if self.stats.is_some_and(|at| at.elapsed() < STATS_EVERY) {
      return false;
    }
    self.stats = Some(Instant::now());
    true
  }
}

/// Follow every room in `rooms` until their lives end.
pub async fn listen(api: &Api, rooms: &[String], out: &Stream) -> Result<()> {
  if rooms.is_empty() {
    return Err(Error::input(
      "give the live rooms to follow: `listen @HANDLE` (a tiktok.com/@handle/live link or a room id work too)",
    ));
  }
  // The socket wants the visitor cookie (`ttwid`) a page sets.
  if !api.ctx.http.has_cookie("ttwid") {
    api.html(&format!("{WWW}/live")).await?;
  }
  let mut tasks = Vec::new();
  for arg in rooms {
    let room = rooms::find(api, arg).await?;
    tasks.push(room_loop(api, room, out));
  }
  futures_util::future::try_join_all(tasks).await?;
  Ok(())
}

/// Connect, and reconnect with growing delays, until the live ends or an
/// error that is not the network's.
async fn room_loop(api: &Api, room: Room, out: &Stream) -> Result<()> {
  let id: u64 = room
    .id
    .parse()
    .map_err(|_| Error::input(format!("not a room id: {}", room.id)))?;
  let mut printed = Printed::default();
  let mut backoff = Backoff::default();
  loop {
    let started = Instant::now();
    match session(api, &room, id, &mut printed, out).await {
      Ok(true) => return Ok(()),
      Ok(false) => tracing::debug!("live socket of {} closed", room.key),
      Err(e) if e.code == ErrorCode::NetworkError => tracing::debug!("live socket: {e}"),
      Err(e) => return Err(e),
    }
    if started.elapsed() > STABLE {
      backoff.reset();
    }
    backoff.wait().await;
  }
}

/// One connection: enter the room, heartbeat and print what it pushes;
/// whether the live ended.
async fn session(
  api: &Api,
  room: &Room,
  id: u64,
  printed: &mut Printed,
  out: &Stream,
) -> Result<bool> {
  let mut sock = connect(api, room).await?;
  sock.send_binary(heartbeat(id)).await?;
  sock.send_binary(enter(id)).await?;
  let mut beat = interval_at(Instant::now() + HEARTBEAT, HEARTBEAT);
  loop {
    tokio::select! {
      _ = beat.tick() => sock.send_binary(heartbeat(id)).await?,
      frame = sock.next() => {
        let Some(frame) = frame? else {
          return Ok(false);
        };
        let Frame::Binary(data) = frame else {
          continue;
        };
        let Some(batch) = webcast::decode(&data, ENCODING) else {
          continue;
        };
        if let Some(ack) = batch.ack {
          sock.send_binary(ack).await?;
        }
        if emit(room, &batch.messages, printed, out) {
          return Ok(true);
        }
      }
    }
  }
}

async fn connect(api: &Api, room: &Room) -> Result<Socket> {
  let signature = sign::webcast_signature();
  let pairs: Vec<(String, String)> = [
    ("version_code", "180800"),
    ("device_platform", "web"),
    ("cookie_enabled", "true"),
    ("screen_width", "1920"),
    ("screen_height", "1080"),
    ("browser_language", "en-US"),
    ("browser_platform", "MacIntel"),
    ("browser_name", "Mozilla"),
    ("browser_version", USER_AGENT.trim_start_matches("Mozilla/")),
    ("browser_online", "true"),
    ("tz_name", "America/New_York"),
    ("app_name", "tiktok_web"),
    ("sup_ws_ds_opt", "1"),
    ("update_version_code", "2.0.0"),
    ("compress", "gzip"),
    ("webcast_language", "en"),
    ("ws_direct", "1"),
    ("aid", "1988"),
    ("live_id", "12"),
    ("version_code", "270000"),
    ("app_language", "en"),
    ("client_enter", "1"),
    ("room_id", room.id.as_str()),
    ("identity", "audience"),
    ("history_comment_count", "6"),
    ("last_rtt", "0"),
    ("heartbeat_duration", "10000"),
    ("resp_content_type", "protobuf"),
    ("did_rule", "3"),
    ("X-Bogus", signature.as_str()),
  ]
  .iter()
  .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
  .collect();
  let url = format!("{PUSH}?{}", query_string(&pairs));
  api.ctx.http.websocket(&url, &[("Origin", WWW)], true).await
}

/// A heartbeat names the room (`roomId` 1).
fn heartbeat(room: u64) -> Vec<u8> {
  let mut body = Vec::new();
  put_int(&mut body, 1, room);
  webcast::frame(0, ENCODING, "hb", &body)
}

/// `im_enter_room`: the room (1), `live_id` (4), the identity (5) and an
/// empty cursor (6): the push starts at the newest messages.
fn enter(room: u64) -> Vec<u8> {
  let mut body = Vec::new();
  put_int(&mut body, 1, room);
  put_int(&mut body, 4, LIVE_ID);
  put_bytes(&mut body, 5, b"audience");
  put_bytes(&mut body, 6, b"");
  put_int(&mut body, 7, 0);
  put_bytes(&mut body, 9, b"0");
  put_int(&mut body, 10, 0);
  webcast::frame(0, ENCODING, "im_enter_room", &body)
}

/// Print the events of a batch worth printing; whether the live ended.
fn emit(room: &Room, messages: &[webcast::Message], printed: &mut Printed, out: &Stream) -> bool {
  let mut ended = false;
  for m in messages {
    let Some(post) = event(room, m) else {
      continue;
    };
    if printed.admit(m.id, &post.kind) {
      ended |= post.kind == "offline";
      out.post(&post);
    }
  }
  ended
}

/// One message as a post: its kind, text and sender.
fn event(room: &Room, m: &webcast::Message) -> Option<Post> {
  let Event {
    kind,
    text,
    sender,
    at_ms,
    counts,
  } = webcast::event(m).or_else(|| viewers(m))?;
  let mut post = Post {
    id: chat::message_id(&room.key, &m.id.to_string()),
    kind: kind.into(),
    text,
    author: sender.map(user),
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
    .insert("chat".into(), json!(format!("live:{}", room.key)));
  if !room.title.is_empty() {
    post.extra.insert("chat_name".into(), json!(room.title));
  }
  Some(post)
}

/// `WebcastRoomUserSeqMessage`: viewers now (`total` 3) and in all
/// (`totalUser` 7).
fn viewers(m: &webcast::Message) -> Option<Event> {
  if m.method != "WebcastRoomUserSeqMessage" {
    return None;
  }
  let msg = Msg(&m.payload);
  let now = msg.varint(3)?;
  let mut counts = vec![("viewers", now)];
  counts.extend(msg.varint(7).map(|n| ("viewers_total", n)));
  Some(Event {
    kind: "stats",
    text: Some(format!("{now} watching")),
    sender: None,
    at_ms: msg.message(1).varint(4),
    counts,
  })
}

/// A sender as a TikTok user: the numeric id, the handle and its profile.
fn user(s: Sender) -> User {
  User {
    id: s.id.unwrap_or_default(),
    name: s.nickname,
    url: s.display_id.as_deref().map(refs::user_url),
    handle: s.display_id,
    ..User::default()
  }
}
