//! The web's push connection (RWP, `apppush-rws.xiaohongshu.com/rwp`): it
//! carries private messages and live rooms. Frames are JSON envelopes
//! `{v, t, m, b}`: `t` 0 heartbeats, 2 requests (handshake, register a
//! business, join a room), 3 actions (send a message), 4 pushes. Private
//! messages travel as base64 protobuf (`ChatOneMessage`), room events as
//! base64 JSON. The connection logs in with the token of `celestial/lt`.
//! Frame shapes as in `cv-cat/catbus` (`src/platforms/xhs/web/push.ts`,
//! MIT).

use std::collections::VecDeque;
use std::time::Duration;

use atp_core::http::USER_AGENT;
use atp_core::proto::{Msg, put_bytes, put_int};
use atp_core::socket::Socket;
use atp_core::{Error, Result, Value, ValueExt, json};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use tokio::time::{Instant, timeout_at};

use crate::api::{Api, HOME};
use crate::sign::now_ms;

const URL: &str = "wss://apppush-rws.xiaohongshu.com/rwp";
const LOGIN_TOKEN: &str = "/api/sns/web/v1/celestial/lt";
const HANDSHAKE_WAIT: Duration = Duration::from_secs(10);
/// How long a sent message waits for its receipt.
pub const ACK_WAIT: Duration = Duration::from_secs(10);
/// How often the connection says it is alive, and asks for missed messages.
pub const HEARTBEAT: Duration = Duration::from_secs(30);
pub const SYNC: Duration = Duration::from_secs(60);

/// An open push connection; frames that arrived during the handshake come
/// first.
pub struct Push {
  sock: Socket,
  pending: VecDeque<Value>,
}

/// Log in as `me`, register for private messages and join `rooms`.
pub async fn connect(api: &Api, me: &str, rooms: &[String]) -> Result<Push> {
  let device = uuid();
  let token = login_token(api, &device).await?;
  let mut sock = api
    .ctx
    .http
    .websocket(URL, &[("Origin", HOME)], true)
    .await?;
  let pending = handshake(&mut sock, me, &token, &device).await?;
  sock.send_json(&register("im", "protobuf")).await?;
  if !rooms.is_empty() {
    sock.send_json(&register("room", "json")).await?;
  }
  for room in rooms {
    sock.send_json(&join(room)).await?;
  }
  Ok(Push { sock, pending })
}

impl Push {
  /// The next frame; `None` once the server closed.
  pub async fn next(&mut self) -> Result<Option<Value>> {
    if let Some(v) = self.pending.pop_front() {
      return Ok(Some(v));
    }
    self.sock.next_json_lenient().await
  }

  pub async fn send(&mut self, frame: &Value) -> Result<()> {
    self.sock.send_json(frame).await
  }

  /// Send one private message and wait for its receipt (`None` when none
  /// came in time).
  pub async fn send_message(&mut self, mid: &str, payload: &[u8]) -> Result<Option<Ack>> {
    let frame = envelope(
      3,
      &frame_id(),
      json!({ "d": { "a": 2, "c": "sendMessage", "biz": "im", "b": STANDARD.encode(payload), "e": {}, "s": "rrmp.b.i" } }),
    );
    self.sock.send_json(&frame).await?;
    let deadline = Instant::now() + ACK_WAIT;
    while let Ok(next) = timeout_at(deadline, self.sock.next_json_lenient()).await {
      let Some(frame) = next? else {
        return Ok(None);
      };
      if let Some(ack) = ims(&frame)
        .into_iter()
        .find_map(|im| im.ack.filter(|a| a.mid == mid))
      {
        return Ok(Some(ack));
      }
      self.pending.push_back(frame);
    }
    Ok(None)
  }
}

/// The connection's login token (`aLt` of `celestial/lt`).
async fn login_token(api: &Api, device: &str) -> Result<String> {
  let data = api
    .get_with(LOGIN_TOKEN, &[], &[("c_device_id", device)])
    .await?;
  data
    .first_str(&["aLt", "a_lt"])
    .filter(|t| !t.is_empty())
    .ok_or_else(|| {
      Error::auth("Xiaohongshu gave no token for its message connection")
        .with_hint("run `atp xhs login --browser`")
    })
}

/// Say who we are and wait for the answer to it; the frames that came
/// before it.
async fn handshake(
  sock: &mut Socket,
  me: &str,
  token: &str,
  device: &str,
) -> Result<VecDeque<Value>> {
  let id = frame_id();
  let chrome = format!("{}.0.0.0", atp_core::http::CHROME_VERSION);
  let body = json!({
    "appId": "xhs-pc",
    "authInfo": { "authType": "generic", "sid": token, "uid": me, "domain": "red" },
    "deviceInfo": {
      "deviceId": device, "fingerprint": now_ms().to_string(), "platform": "browser",
      "os": "web", "osVersion": "10.15", "deviceName": "Chrome",
      "appVersion": chrome, "userAgent": USER_AGENT,
    },
    "serviceTag": "",
    "bizInfos": [
      { "bizName": "dqa_chatsearch", "serializeType": "protobuf" },
      { "bizName": "xhs_dots_pc", "serializeType": "protobuf" },
      { "bizName": "push", "serializeType": "json" },
    ],
    "roomInfo": [], "roomInfos": [], "tagInfo": [], "extInfo": {}, "state": 1,
  });
  sock
    .send_json(&envelope(
      2,
      &id,
      json!({ "d": { "a": 1, "s": 0, "b": body } }),
    ))
    .await?;
  let deadline = Instant::now() + HANDSHAKE_WAIT;
  let mut pending = VecDeque::new();
  loop {
    let next = timeout_at(deadline, sock.next_json_lenient())
      .await
      .map_err(|_| Error::network("the message connection did not answer its handshake"))?;
    let Some(frame) = next? else {
      return Err(Error::network(
        "the message connection closed during its handshake",
      ));
    };
    if frame.str("m").as_deref() != Some(id.as_str()) {
      pending.push_back(frame);
      continue;
    }
    return match frame.i64("b.a.c") {
      Some(0) => Ok(pending),
      code => Err(
        Error::auth(format!(
          "Xiaohongshu refused the message connection: {} ({})",
          frame.str("b.a.m").unwrap_or_default(),
          code.unwrap_or(-1)
        ))
        .with_hint("run `atp xhs login --browser`"),
      ),
    };
  }
}

fn envelope(t: u8, id: &str, body: Value) -> Value {
  json!({ "v": 1, "t": t, "m": id, "b": body })
}

pub fn heartbeat() -> Value {
  json!({ "v": 1, "t": 0 })
}

/// Ask for what the connection missed.
pub fn sync() -> Value {
  envelope(2, &frame_id(), json!({ "d": { "a": 1, "s": 6, "b": {} } }))
}

fn register(biz: &str, serialize: &str) -> Value {
  let info = json!({ "bizInfo": { "bizName": biz, "serializeType": serialize }, "register": true });
  envelope(
    2,
    &frame_id(),
    json!({ "d": { "a": 1, "s": 1, "b": info } }),
  )
}

fn join(room: &str) -> Value {
  let info = json!({ "info": { "bizName": "room", "roomId": room, "roomType": "LIVE" } });
  envelope(
    2,
    &frame_id(),
    json!({ "d": { "a": 1, "s": 8, "b": info } }),
  )
}

// ── private messages ─────────────────────────────────────────────────────

/// `ChatOneMessage {type = 1, send = 9}` holding a text
/// `ChatSendMessage`: id (1), time (2), token (3), sender (4), receiver (5),
/// content (6), content type (7), nickname (8), group chat (9), ref id
/// (11), command (12), empty fields written as the web writes them.
pub fn text_message(mid: &str, sender: &str, receiver: &str, text: &str) -> Vec<u8> {
  let mut send = Vec::new();
  put_bytes(&mut send, 1, mid.as_bytes());
  put_int(&mut send, 2, now_ms());
  put_bytes(&mut send, 3, b"");
  put_bytes(&mut send, 4, sender.as_bytes());
  put_bytes(&mut send, 5, receiver.as_bytes());
  put_bytes(&mut send, 6, text.as_bytes());
  put_int(&mut send, 7, 1);
  put_bytes(&mut send, 8, b"");
  put_int(&mut send, 9, 0);
  put_bytes(&mut send, 11, b"");
  put_int(&mut send, 12, 1);
  let mut out = Vec::new();
  put_int(&mut out, 1, 1);
  put_bytes(&mut out, 9, &send);
  out
}

/// A pushed private message: ids, time (ms) and the message as JSON.
pub struct Incoming {
  pub mid: String,
  pub id: String,
  pub at_ms: Option<u64>,
  pub body: Value,
}

/// The receipt of a sent message; `code` 0 is success.
pub struct Ack {
  pub mid: String,
  pub id: String,
  pub code: u64,
  pub message: String,
}

/// `ChatOneMessage`: a received message (4) and / or a receipt (5).
#[derive(Default)]
pub struct Im {
  pub incoming: Option<Incoming>,
  pub ack: Option<Ack>,
}

/// The private-message payloads of a frame: one base64 string (`t` 3), a
/// batch of `{d}` (`t` 4), or a receipt in `b.a.b`.
pub fn ims(frame: &Value) -> Vec<Im> {
  let d = frame.at("b.d");
  let mut encoded: Vec<String> = Vec::new();
  if d.str("biz").as_deref() == Some("im") {
    match d.get("b") {
      Some(Value::String(s)) => encoded.push(s.clone()),
      Some(Value::Array(items)) => encoded.extend(items.iter().filter_map(|i| i.str("d"))),
      _ => {}
    }
  }
  if encoded.is_empty() {
    encoded.extend(frame.str("b.a.b"));
  }
  encoded
    .iter()
    .filter_map(|s| STANDARD.decode(s).ok())
    .map(|bytes| im(Msg(&bytes)))
    .collect()
}

fn im(outer: Msg) -> Im {
  let text = |m: Msg, n| m.string(n).unwrap_or_default();
  let incoming = outer.bytes(4).map(Msg).map(|m| Incoming {
    mid: text(m, 1),
    id: text(m, 2),
    at_ms: m.varint(3),
    body: m
      .string(5)
      .and_then(|s| serde_json::from_str(&s).ok())
      .unwrap_or(Value::Null),
  });
  let ack = outer.bytes(5).map(Msg).map(|m| Ack {
    mid: text(m, 1),
    id: text(m, 2),
    code: m.varint(5).unwrap_or(0),
    message: text(m, 6),
  });
  Im { incoming, ack }
}

// ── live rooms ───────────────────────────────────────────────────────────

/// The room events of a push (`t` 4, `biz` room): base64 JSON whose
/// `customData` is JSON again.
pub fn room_events(frame: &Value) -> Vec<Value> {
  let d = frame.at("b.d");
  if frame.i64("t") != Some(4) || d.str("biz").as_deref() != Some("room") {
    return Vec::new();
  }
  d.list("b")
    .iter()
    .filter_map(|item| STANDARD.decode(item.str("d")?).ok())
    .filter_map(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
    .map(|mut v| {
      let custom = v
        .str("customData")
        .and_then(|s| serde_json::from_str(&s).ok());
      if let (Some(custom), Some(obj)) = (custom, v.as_object_mut()) {
        obj.insert("customData".into(), custom);
      }
      v
    })
    .collect()
}

// ── ids ──────────────────────────────────────────────────────────────────

/// A random version-4 UUID.
pub fn uuid() -> String {
  let mut b: [u8; 16] = rand::random();
  b[6] = (b[6] & 0x0f) | 0x40;
  b[8] = (b[8] & 0x3f) | 0x80;
  let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
  format!(
    "{}-{}-{}-{}-{}",
    &hex[..8],
    &hex[8..12],
    &hex[12..16],
    &hex[16..20],
    &hex[20..]
  )
}

/// A frame's `m`: 14 hex digits of a UUID, `-`, the clock in hex.
fn frame_id() -> String {
  let id = uuid().replace('-', "");
  let now = format!("{:x}", now_ms());
  format!("{}-{}", &id[..14], &now[now.len().saturating_sub(11)..])
}
