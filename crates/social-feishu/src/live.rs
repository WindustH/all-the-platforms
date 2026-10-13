//! `listen`: Feishu's long connection (长连接), the event mode the official
//! SDKs use, so no public callback URL is needed.
//!
//! `POST /callback/ws/endpoint` with the app credentials hands out a
//! websocket URL; frames are protobuf ([`crate::pb`]). Control frames are
//! pings and pongs (a pong may carry new client settings); data frames carry
//! events as JSON, possibly split in parts, and each is acknowledged with
//! `{"code":200}`. The app must have 长连接 chosen as its event subscription
//! mode and subscribe to the events wanted (im.message.receive_v1 …).

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use social_core::output::{Stream, note};
use social_core::socket::Frame as WsFrame;
use social_core::text::from_millis;
use social_core::{Error, Post, Result, User, Value, ValueExt, json};
use tokio::time::{Instant, interval_at};

use crate::api::{Api, check};
use crate::pb::Frame;
use crate::{chats, parse, refs};

/// Give up after this many failed connections in a row.
const MAX_FAILURES: u32 = 5;

pub async fn listen(api: &Api, chats: &[String], out: &Stream) -> Result<()> {
  let creds = api.require()?;
  let wanted: HashSet<String> = chats.iter().map(|c| refs::chat(c)).collect::<Result<_>>()?;
  let mut failures = 0;
  loop {
    match session(api, &creds, &wanted, out).await {
      Ok(true) => failures = 0,
      Ok(false) => failures += 1,
      Err(e) if failures == 0 && e.code != social_core::ErrorCode::NetworkError => return Err(e),
      Err(e) => {
        failures += 1;
        note(&format!("connection lost: {}", e.message));
      }
    }
    if failures >= MAX_FAILURES {
      return Err(Error::network(
        "the Feishu long connection keeps failing; check the app's event subscription mode",
      ));
    }
    let wait = 2u64.pow(failures.min(5));
    tokio::time::sleep(Duration::from_secs(wait)).await;
  }
}

/// One connection; `Ok(true)` when it got as far as receiving frames.
async fn session(
  api: &Api,
  creds: &crate::api::Creds,
  wanted: &HashSet<String>,
  out: &Stream,
) -> Result<bool> {
  let v: Value = api
    .ctx
    .http
    .post(format!("{}/callback/ws/endpoint", creds.base))
    .no_cookies()
    .header("locale", "zh")
    .json(&json!({ "AppID": creds.app_id, "AppSecret": creds.secret }))
    .value()
    .await?;
  let v = check(v, "/callback/ws/endpoint")?;
  let url = v
    .str("data.URL")
    .ok_or_else(|| Error::upstream("the endpoint call returned no URL"))?;
  let service: i32 = query_param(&url, "service_id")
    .and_then(|s| s.parse().ok())
    .unwrap_or(0);
  let mut ping_secs = v
    .u64("data.ClientConfig.PingInterval")
    .unwrap_or(120)
    .max(10);
  let mut socket = api.ctx.http.websocket(&url, &[], false).await?;
  note("connected; waiting for events (Ctrl-C to stop)");
  let mut ping = interval_at(
    Instant::now() + Duration::from_secs(1),
    Duration::from_secs(ping_secs),
  );
  let mut parts: HashMap<String, Vec<Option<Vec<u8>>>> = HashMap::new();
  let mut received = false;
  loop {
    tokio::select! {
      _ = ping.tick() => {
        let frame = Frame {
          service,
          method: 0,
          headers: vec![("type".into(), "ping".into())],
          ..Frame::default()
        };
        socket.send_binary(frame.encode()).await?;
      }
      next = socket.next() => {
        let bytes = match next? {
          None => return Ok(received),
          Some(WsFrame::Binary(b)) => b,
          Some(WsFrame::Text(t)) => t.into_bytes(),
        };
        received = true;
        let frame = Frame::decode(&bytes)?;
        if frame.method == 0 {
          // A pong may carry new client settings.
          if frame.header("type") == Some("pong") && !frame.payload.is_empty()
            && let Ok(conf) = serde_json::from_slice::<Value>(&frame.payload)
            && let Some(secs) = conf.u64("PingInterval")
            && secs >= 10 && secs != ping_secs
          {
            ping_secs = secs;
            ping = interval_at(Instant::now() + Duration::from_secs(secs), Duration::from_secs(secs));
          }
          continue;
        }
        if let Some(payload) = assemble(&frame, &mut parts) {
          if frame.header("type") == Some("event")
            && let Ok(event) = serde_json::from_slice::<Value>(&payload)
          {
            emit(api, &event, wanted, out).await;
          }
          let mut ack = frame.clone();
          ack.headers.push(("biz_rt".into(), "0".into()));
          ack.payload = br#"{"code":200}"#.to_vec();
          socket.send_binary(ack.encode()).await?;
        }
      }
    }
  }
}

/// Join an event split over several frames (`sum` > 1, `seq` = index).
fn assemble(frame: &Frame, parts: &mut HashMap<String, Vec<Option<Vec<u8>>>>) -> Option<Vec<u8>> {
  let sum: usize = frame
    .header("sum")
    .and_then(|s| s.parse().ok())
    .unwrap_or(1);
  if sum <= 1 {
    return Some(frame.payload.clone());
  }
  let seq: usize = frame
    .header("seq")
    .and_then(|s| s.parse().ok())
    .unwrap_or(0);
  let id = frame.header("message_id").unwrap_or_default().to_owned();
  let slots = parts.entry(id.clone()).or_insert_with(|| vec![None; sum]);
  if seq < slots.len() {
    slots[seq] = Some(frame.payload.clone());
  }
  if slots.iter().all(Option::is_some) {
    let joined = slots.iter().flatten().flatten().copied().collect();
    parts.remove(&id);
    return Some(joined);
  }
  None
}

/// One event as a post on the stream.
async fn emit(api: &Api, event: &Value, wanted: &HashSet<String>, out: &Stream) {
  let kind = event.str("header.event_type").unwrap_or_default();
  let e = event.at("event");
  let mut post = match kind.as_str() {
    "im.message.receive_v1" => parse::message(&parse::event_message(e), &api.base()),
    "im.message.recalled_v1" => Post {
      id: e.str("message_id").unwrap_or_default(),
      kind: "delete".into(),
      created_at: e.i64("recall_time").and_then(from_millis),
      ..Post::default()
    },
    "im.message.reaction.created_v1" | "im.message.reaction.deleted_v1" => Post {
      id: e.str("message_id").unwrap_or_default(),
      kind: if kind.ends_with("created_v1") {
        "reaction".into()
      } else {
        "unreaction".into()
      },
      text: e.str("reaction_type.emoji_type"),
      author: e.str("user_id.open_id").map(|id| User {
        id,
        ..User::default()
      }),
      created_at: e.i64("action_time").and_then(from_millis),
      ..Post::default()
    },
    "im.message.message_read_v1" => Post {
      id: e
        .list("message_id_list")
        .first()
        .and_then(|m| m.as_str())
        .unwrap_or_default()
        .to_owned(),
      kind: "read".into(),
      author: e.str("reader.reader_id.open_id").map(|id| User {
        id,
        ..User::default()
      }),
      created_at: e.i64("reader.read_time").and_then(from_millis),
      ..Post::default()
    },
    other => Post {
      id: event.str("header.event_id").unwrap_or_default(),
      kind: other.to_owned(),
      created_at: event.i64("header.create_time").and_then(from_millis),
      ..Post::default()
    },
  };
  let chat = e.str("message.chat_id").or_else(|| e.str("chat_id"));
  if !wanted.is_empty() && !chat.as_ref().is_some_and(|c| wanted.contains(c)) {
    return;
  }
  if let Some(chat) = &chat {
    post.extra.insert("chat".into(), chat.clone().into());
    if let Some(name) = chats::name(api, chat).await {
      post.extra.insert("chat_name".into(), name.into());
    }
  }
  if post.raw.is_none() {
    post.raw = Some(event.clone());
  }
  let mut one = [post];
  crate::users::names(api, &mut one).await;
  out.post(&one[0]);
}

fn query_param(url: &str, name: &str) -> Option<String> {
  let query = url.split_once('?')?.1;
  query.split('&').find_map(|pair| {
    let (k, v) = pair.split_once('=')?;
    (k == name).then(|| v.to_owned())
  })
}
