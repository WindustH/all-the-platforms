//! `listen`: DingTalk Stream mode, so no public callback URL is needed.
//!
//! `POST /v1.0/gateway/connections/open` with the app credentials and the
//! subscriptions (all events, and robot messages) hands out an endpoint and
//! a ticket. Frames are JSON `{type, headers, data}`: `SYSTEM` pings are
//! echoed and `disconnect` reconnects; every `EVENT` and `CALLBACK` is
//! acknowledged. Robot messages (`/v1.0/im/bot/messages/get`) become posts
//! whose `chat` is the conversation to reply to (`cid…`) and whose
//! `session_webhook` replies directly for a while.

use std::collections::HashSet;

use atp_core::live::Backoff;
use atp_core::output::{Stream, note};
use atp_core::socket::{Frame, Socket};
use atp_core::{Error, ErrorCode, Result, Value, ValueExt, json};

use crate::api::{API, Api, Creds, check_api};
use crate::parse;

const BOT_TOPIC: &str = "/v1.0/im/bot/messages/get";
const MAX_FAILURES: u32 = 5;

pub async fn listen(api: &Api, chats: &[String], out: &Stream) -> Result<()> {
  let creds = api.require()?;
  let wanted: HashSet<String> = chats.iter().map(|c| c.trim().to_owned()).collect();
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
        "the DingTalk stream keeps failing; check that the robot uses Stream mode",
      ));
    }
    backoff.wait().await;
  }
}

/// One connection; `Ok(true)` when frames arrived.
async fn session(api: &Api, creds: &Creds, wanted: &HashSet<String>, out: &Stream) -> Result<bool> {
  let mut socket = connect(api, creds).await?;
  note("connected; waiting for messages (Ctrl-C to stop)");
  let mut received = false;
  loop {
    let text = match socket.next().await? {
      None => return Ok(received),
      Some(Frame::Text(t)) => t,
      Some(Frame::Binary(b)) => String::from_utf8_lossy(&b).into_owned(),
    };
    received = true;
    let Ok(frame) = serde_json::from_str::<Value>(&text) else {
      continue;
    };
    if handle(&mut socket, &frame, wanted, out).await? {
      return Ok(true);
    }
  }
}

/// Open a stream connection: an endpoint and a ticket for it.
async fn connect(api: &Api, creds: &Creds) -> Result<Socket> {
  let resp = api
    .ctx
    .http
    .post(format!("{API}/v1.0/gateway/connections/open"))
    .no_cookies()
    .json(&json!({
      "clientId": creds.app_key,
      "clientSecret": creds.secret,
      "subscriptions": [
        { "type": "EVENT", "topic": "*" },
        { "type": "CALLBACK", "topic": BOT_TOPIC },
      ],
      "ua": concat!("all-the-platforms/", env!("CARGO_PKG_VERSION")),
      "localIp": "127.0.0.1",
    }))
    .send()
    .await?;
  let v = check_api(
    resp.status.as_u16(),
    resp.value().unwrap_or_default(),
    "/v1.0/gateway/connections/open",
  )?;
  let (Some(endpoint), Some(ticket)) = (v.str("endpoint"), v.str("ticket")) else {
    return Err(Error::upstream("the stream gateway returned no endpoint"));
  };
  let ticket: String = url::form_urlencoded::byte_serialize(ticket.as_bytes()).collect();
  let url = format!("{endpoint}?ticket={ticket}");
  api.ctx.http.websocket(&url, &[], false).await
}

/// Answer one frame and print what it carries; `true` when the server asks
/// to reconnect.
async fn handle(
  socket: &mut Socket,
  frame: &Value,
  wanted: &HashSet<String>,
  out: &Stream,
) -> Result<bool> {
  let kind = frame.str("type").unwrap_or_default();
  let topic = frame.str("headers.topic").unwrap_or_default();
  match kind.as_str() {
    "SYSTEM" => {
      ack(socket, frame, frame.str("data").unwrap_or_default()).await?;
      return Ok(topic == "disconnect");
    }
    "EVENT" | "CALLBACK" => {}
    _ => return Ok(false),
  }
  let data: Value = frame
    .str("data")
    .and_then(|d| serde_json::from_str(&d).ok())
    .unwrap_or_default();
  let post = if topic == BOT_TOPIC {
    parse::bot_message(&data)
  } else {
    parse::event(frame, &data)
  };
  let reply = if kind == "CALLBACK" {
    json!({ "response": null })
  } else {
    json!({ "status": "SUCCESS", "message": "success" })
  };
  ack(socket, frame, reply.to_string()).await?;
  let chat = post
    .extra
    .get("chat")
    .and_then(|c| c.as_str())
    .unwrap_or_default();
  if wanted.is_empty() || wanted.contains(chat) {
    out.post(&post);
  }
  Ok(false)
}

async fn ack(socket: &mut Socket, frame: &Value, data: String) -> Result<()> {
  socket
    .send_json(&json!({
      "code": 200,
      "headers": {
        "contentType": "application/json",
        "messageId": frame.str("headers.messageId"),
      },
      "message": "OK",
      "data": data,
    }))
    .await
}
