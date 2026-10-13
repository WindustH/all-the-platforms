//! `listen`: Feishu's long connection (长连接), the event mode the official
//! SDKs use, so no public callback URL is needed.
//!
//! `POST /callback/ws/endpoint` with the app credentials hands out a
//! websocket URL; frames are protobuf ([`crate::frame`]). Control frames are
//! pings and pongs (a pong may carry new client settings); data frames carry
//! events as JSON, possibly split in parts, and each is acknowledged with
//! `{"code":200}`. The app must have 长连接 chosen as its event subscription
//! mode and subscribe to the events wanted (im.message.receive_v1 …).

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use atp_core::live::Backoff;
use atp_core::output::{Stream, note};
use atp_core::socket::Frame as WsFrame;
use atp_core::{Error, ErrorCode, Result, Value, ValueExt, json};
use tokio::time::{Instant, Interval, interval_at};

use crate::api::{Api, Creds, check};
use crate::frame::Frame;
use crate::{chats, parse, refs, users};

/// Give up after this many failed connections in a row.
const MAX_FAILURES: u32 = 5;

/// Event parts waiting for the rest, by message id.
type Parts = HashMap<String, Vec<Option<Vec<u8>>>>;

pub async fn listen(api: &Api, chats: &[String], out: &Stream) -> Result<()> {
  let creds = api.require()?;
  let wanted: HashSet<String> = chats.iter().map(|c| refs::chat(c)).collect::<Result<_>>()?;
  let mut failures = 0;
  let mut backoff = Backoff::default();
  loop {
    match session(api, &creds, &wanted, out).await {
      Ok(true) => {
        failures = 0;
        backoff.reset();
      }
      Ok(false) => failures += 1,
      Err(e) if failures == 0 && e.code != ErrorCode::NetworkError => return Err(e),
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
    backoff.wait().await;
  }
}

/// Where to connect: the websocket URL, its service id and ping interval.
struct Endpoint {
  url: String,
  service: i32,
  ping_secs: u64,
}

async fn endpoint(api: &Api, creds: &Creds) -> Result<Endpoint> {
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
  let service = query_param(&url, "service_id")
    .and_then(|s| s.parse().ok())
    .unwrap_or(0);
  let ping_secs = v
    .u64("data.ClientConfig.PingInterval")
    .unwrap_or(120)
    .max(10);
  Ok(Endpoint {
    url,
    service,
    ping_secs,
  })
}

/// One connection; `Ok(true)` when it got as far as receiving frames.
async fn session(api: &Api, creds: &Creds, wanted: &HashSet<String>, out: &Stream) -> Result<bool> {
  let Endpoint {
    url,
    service,
    mut ping_secs,
  } = endpoint(api, creds).await?;
  let mut socket = api.ctx.http.websocket(&url, &[], false).await?;
  note("connected; waiting for events (Ctrl-C to stop)");
  let mut ping = interval_at(
    Instant::now() + Duration::from_secs(1),
    Duration::from_secs(ping_secs),
  );
  let mut parts = Parts::new();
  let mut received = false;
  loop {
    tokio::select! {
      _ = ping.tick() => socket.send_binary(Frame::ping(service).encode()).await?,
      next = socket.next() => {
        let bytes = match next? {
          None => return Ok(received),
          Some(WsFrame::Binary(b)) => b,
          Some(WsFrame::Text(t)) => t.into_bytes(),
        };
        received = true;
        let frame = Frame::decode(&bytes)?;
        if frame.method == 0 {
          update_ping(&frame, &mut ping_secs, &mut ping);
          continue;
        }
        if let Some(payload) = assemble(&frame, &mut parts) {
          if frame.header("type") == Some("event")
            && let Ok(event) = serde_json::from_slice::<Value>(&payload)
          {
            emit(api, &event, wanted, out).await;
          }
          socket.send_binary(frame.ack().encode()).await?;
        }
      }
    }
  }
}

/// A pong may carry new client settings: ping at the new interval.
fn update_ping(frame: &Frame, ping_secs: &mut u64, ping: &mut Interval) {
  if frame.header("type") != Some("pong") || frame.payload.is_empty() {
    return;
  }
  let Ok(settings) = serde_json::from_slice::<Value>(&frame.payload) else {
    return;
  };
  if let Some(secs) = settings.u64("PingInterval")
    && secs >= 10
    && secs != *ping_secs
  {
    *ping_secs = secs;
    *ping = interval_at(
      Instant::now() + Duration::from_secs(secs),
      Duration::from_secs(secs),
    );
  }
}

/// Join an event split over several frames (`sum` > 1, `seq` = index).
fn assemble(frame: &Frame, parts: &mut Parts) -> Option<Vec<u8>> {
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

/// One event as a post on the stream, when its chat is wanted.
async fn emit(api: &Api, event: &Value, wanted: &HashSet<String>, out: &Stream) {
  let mut post = parse::event(event, &api.base());
  let e = event.at("event");
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
  users::names(api, &mut one).await;
  out.post(&one[0]);
}

fn query_param(url: &str, name: &str) -> Option<String> {
  let query = url.split_once('?')?.1;
  query.split('&').find_map(|pair| {
    let (k, v) = pair.split_once('=')?;
    (k == name).then(|| v.to_owned())
  })
}
