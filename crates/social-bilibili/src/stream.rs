//! The live danmaku stream (`listen live:ROOM`): Bilibili's broadcast
//! websocket. Packets have a 16-byte header (length, header length, protocol
//! version, operation, sequence); operation 7 authenticates with the token
//! of `getDanmuInfo`, 2 is the 30-second heartbeat, 5 carries commands as
//! JSON, zlib-compressed batches under protocol version 2.

use std::io::Read;
use std::time::Duration;

use flate2::read::ZlibDecoder;
use social_core::output::Stream;
use social_core::socket::Frame;
use social_core::text::from_secs;
use social_core::{Ctx, Error, Post, Result, Value, ValueExt, json};

use crate::live::{self, danmaku_post};
use crate::{account, api};

const DANMU_INFO: &str = "https://api.live.bilibili.com/xlive/web-room/v1/index/getDanmuInfo";
const HEARTBEAT: Duration = Duration::from_secs(30);

const OP_HEARTBEAT: u32 = 2;
const OP_MESSAGE: u32 = 5;
const OP_AUTH: u32 = 7;

/// Follow every room in `rooms` until the connections end.
pub async fn listen(ctx: &Ctx, rooms: &[String], out: &Stream) -> Result<()> {
  if rooms.is_empty() {
    return Err(Error::input(
      "give the live rooms to follow: `listen live:ROOM` or a live.bilibili.com link",
    ));
  }
  let mut tasks = Vec::new();
  for r in rooms {
    let room = live::room_id(ctx, r).await?;
    tasks.push(room_loop(ctx, room, out));
  }
  futures_util::future::try_join_all(tasks).await?;
  Ok(())
}

async fn room_loop(ctx: &Ctx, room: u64, out: &Stream) -> Result<()> {
  let info = live::info(ctx, room).await?;
  let real = info.u64("room_info.room_id").unwrap_or(room);
  let title = info.str("room_info.title").unwrap_or_default();
  let auth = api::get(ctx, DANMU_INFO)
    .arg("id", real)
    .arg("type", 0)
    .arg("web_location", "444.8")
    .wbi()
    .send()
    .await?;
  let token = auth.str("token").unwrap_or_default();
  let host = auth
    .list("host_list")
    .first()
    .and_then(|h| {
      Some(format!(
        "wss://{}:{}/sub",
        h.str("host")?,
        h.str("wss_port")?
      ))
    })
    .unwrap_or_else(|| "wss://broadcastlv.chat.bilibili.com/sub".into());
  // Logged out, Bilibili masks the senders' names.
  let uid: u64 = match account::my_mid(ctx).await {
    Ok(mid) => mid.parse().unwrap_or(0),
    Err(_) => 0,
  };
  let mut sock = ctx
    .http
    .websocket(&host, &[("Origin", "https://live.bilibili.com")], true)
    .await?;
  let body = json!({
    "uid": uid,
    "roomid": real,
    "protover": 2,
    "buvid": ctx.http.cookie("buvid3").unwrap_or_default(),
    "platform": "web",
    "type": 2,
    "key": token,
  });
  sock
    .send_binary(packet(OP_AUTH, body.to_string().as_bytes()))
    .await?;
  let chat = Chat { room: real, title };
  let mut beat = tokio::time::interval(HEARTBEAT);
  loop {
    tokio::select! {
      _ = beat.tick() => sock.send_binary(packet(OP_HEARTBEAT, b"[object Object]")).await?,
      frame = sock.next() => match frame? {
        Some(Frame::Binary(data)) => {
          for v in commands(&data) {
            if let Some(post) = chat.event(&v) {
              out.post(&post);
            }
          }
        }
        Some(Frame::Text(_)) => {}
        None => return Ok(()),
      },
    }
  }
}

fn packet(op: u32, body: &[u8]) -> Vec<u8> {
  let mut p = Vec::with_capacity(16 + body.len());
  p.extend_from_slice(&((16 + body.len()) as u32).to_be_bytes());
  p.extend_from_slice(&16u16.to_be_bytes());
  p.extend_from_slice(&1u16.to_be_bytes());
  p.extend_from_slice(&op.to_be_bytes());
  p.extend_from_slice(&1u32.to_be_bytes());
  p.extend_from_slice(body);
  p
}

/// The JSON commands in a frame, unpacking compressed batches.
fn commands(data: &[u8]) -> Vec<Value> {
  let mut out = Vec::new();
  let mut rest = data;
  while rest.len() >= 16 {
    let len = u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
    let header = u16::from_be_bytes([rest[4], rest[5]]) as usize;
    let version = u16::from_be_bytes([rest[6], rest[7]]);
    let op = u32::from_be_bytes([rest[8], rest[9], rest[10], rest[11]]);
    if len < header || len > rest.len() {
      break;
    }
    let body = &rest[header..len];
    if op == OP_MESSAGE {
      match version {
        2 => {
          let mut inflated = Vec::new();
          if ZlibDecoder::new(body).read_to_end(&mut inflated).is_ok() {
            out.extend(commands(&inflated));
          }
        }
        _ => {
          if let Ok(v) = serde_json::from_slice::<Value>(body) {
            out.push(v);
          }
        }
      }
    }
    rest = &rest[len..];
  }
  out
}

struct Chat {
  room: u64,
  title: String,
}

impl Chat {
  /// Danmaku, super chats, gifts, memberships and live state changes.
  fn event(&self, v: &Value) -> Option<Post> {
    let cmd = v.str("cmd")?;
    let cmd = cmd.split(':').next().unwrap_or_default();
    let d = v.at("data");
    let mut post = match cmd {
      "DANMU_MSG" => {
        let info = v.at("info");
        let at = info.i64("0.4").map(|ms| ms / 1000).and_then(from_secs);
        let id = info
          .str("0.15.extra")
          .and_then(|e| serde_json::from_str::<Value>(&e).ok())
          .and_then(|e| e.str("id_str"))
          .unwrap_or_else(|| info.str("0.4").unwrap_or_default());
        danmaku_post(
          self.room,
          id,
          info.str("1").unwrap_or_default(),
          info.str("2.0").unwrap_or_default(),
          info.str("2.1").unwrap_or_default(),
          at,
          v.clone(),
        )
      }
      "SUPER_CHAT_MESSAGE" => {
        let mut p = self.other(
          "superchat",
          d,
          d.str("message"),
          ("uid", "user_info.uname"),
          "start_time",
        );
        if let Some(price) = d.u64("price") {
          p.extra.insert("price_cny".into(), json!(price));
        }
        p
      }
      "SEND_GIFT" => {
        let text = format!(
          "{} × {}",
          d.str("giftName").unwrap_or_default(),
          d.u64("num").unwrap_or(1)
        );
        let mut p = self.other("gift", d, Some(text), ("uid", "uname"), "timestamp");
        if let Some(price) = d.u64("total_coin").or_else(|| d.u64("price")) {
          p.extra.insert("coins".into(), json!(price));
        }
        p
      }
      "GUARD_BUY" => {
        let text = format!(
          "{} × {}",
          d.str("gift_name").unwrap_or_default(),
          d.u64("num").unwrap_or(1)
        );
        self.other(
          "membership",
          d,
          Some(text),
          ("uid", "username"),
          "start_time",
        )
      }
      "LIVE" | "PREPARING" => {
        let state = if cmd == "LIVE" {
          "live started"
        } else {
          "live ended"
        };
        let mut p = Post {
          id: format!("{}/{}", self.room, cmd.to_lowercase()),
          kind: if cmd == "LIVE" { "live" } else { "offline" }.into(),
          text: Some(state.into()),
          created_at: Some(jiff::Timestamp::now()),
          raw: Some(v.clone()),
          ..Post::default()
        };
        p.extra
          .insert("chat".into(), json!(format!("live:{}", self.room)));
        p
      }
      _ => return None,
    };
    post
      .extra
      .insert("chat_name".into(), json!(self.title.clone()));
    Some(post)
  }

  fn other(
    &self,
    kind: &str,
    d: &Value,
    text: Option<String>,
    (uid, name): (&str, &str),
    time: &str,
  ) -> Post {
    let at = d.i64(time).and_then(from_secs);
    let id = d
      .first_str(&["id", "tid"])
      .unwrap_or_else(|| d.str(time).unwrap_or_default());
    let mut p = danmaku_post(
      self.room,
      id,
      text.unwrap_or_default(),
      d.str(uid).unwrap_or_default(),
      d.str(name).unwrap_or_default(),
      at,
      d.clone(),
    );
    p.kind = kind.into();
    p
  }
}
