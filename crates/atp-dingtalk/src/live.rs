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
use std::time::Duration;

use atp_core::output::{Stream, note};
use atp_core::socket::{Frame, Socket};
use atp_core::text::from_millis;
use atp_core::{Error, ErrorCode, Post, Result, User, Value, ValueExt, json};

use crate::api::{API, Api, check_api};

const BOT_TOPIC: &str = "/v1.0/im/bot/messages/get";
const MAX_FAILURES: u32 = 5;

pub async fn listen(api: &Api, chats: &[String], out: &Stream) -> Result<()> {
  let creds = api.require()?;
  let wanted: HashSet<String> = chats.iter().map(|c| c.trim().to_owned()).collect();
  let mut failures = 0;
  loop {
    match session(api, &creds, &wanted, out).await {
      Ok(true) => failures = 0,
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
    tokio::time::sleep(Duration::from_secs(2u64.pow(failures.min(5)))).await;
  }
}

/// One connection; `Ok(true)` when frames arrived.
async fn session(
  api: &Api,
  creds: &crate::api::Creds,
  wanted: &HashSet<String>,
  out: &Stream,
) -> Result<bool> {
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
  let url = format!(
    "{endpoint}?ticket={}",
    url::form_urlencoded::byte_serialize(ticket.as_bytes()).collect::<String>()
  );
  let mut socket = api.ctx.http.websocket(&url, &[], false).await?;
  note("connected; waiting for messages (Ctrl-C to stop)");
  let mut received = false;
  loop {
    let text = match socket.next().await? {
      None => return Ok(received),
      Some(Frame::Text(t)) => t,
      Some(Frame::Binary(b)) => String::from_utf8_lossy(&b).into_owned(),
    };
    received = true;
    let Ok(msg) = serde_json::from_str::<Value>(&text) else {
      continue;
    };
    let kind = msg.str("type").unwrap_or_default();
    let topic = msg.str("headers.topic").unwrap_or_default();
    match kind.as_str() {
      "SYSTEM" => {
        ack(&mut socket, &msg, msg.str("data").unwrap_or_default()).await?;
        if topic == "disconnect" {
          return Ok(true);
        }
      }
      "EVENT" | "CALLBACK" => {
        let data: Value = msg
          .str("data")
          .and_then(|d| serde_json::from_str(&d).ok())
          .unwrap_or_default();
        let post = if topic == BOT_TOPIC {
          bot_message(&data)
        } else {
          event(&msg, &data)
        };
        let reply = if kind == "CALLBACK" {
          json!({ "response": null }).to_string()
        } else {
          json!({ "status": "SUCCESS", "message": "success" }).to_string()
        };
        ack(&mut socket, &msg, reply).await?;
        let chat = post
          .extra
          .get("chat")
          .and_then(|c| c.as_str())
          .unwrap_or_default();
        if wanted.is_empty() || wanted.contains(chat) {
          out.post(&post);
        }
      }
      _ => {}
    }
  }
}

async fn ack(socket: &mut Socket, msg: &Value, data: String) -> Result<()> {
  socket
    .send_json(&json!({
      "code": 200,
      "headers": {
        "contentType": "application/json",
        "messageId": msg.str("headers.messageId"),
      },
      "message": "OK",
      "data": data,
    }))
    .await
}

/// A message to the robot (a mention in a group, or a direct message).
fn bot_message(d: &Value) -> Post {
  let chat = d.str("conversationId").unwrap_or_default();
  let text = match d.str("msgtype").as_deref() {
    Some("text") => d.str("text.content"),
    Some("richText") => Some(
      d.list("content.richText")
        .iter()
        .filter_map(|p| p.str("text"))
        .collect::<Vec<_>>()
        .join(""),
    ),
    Some("picture") => Some("[image]".into()),
    Some("audio") => d.str("content.recognition").or(Some("[voice]".into())),
    Some("file") => Some(format!(
      "[file {}]",
      d.str("content.fileName").unwrap_or_default()
    )),
    Some(other) => Some(format!("[{other}]")),
    None => None,
  };
  let mut post = Post {
    id: format!("{chat}/{}", d.str("msgId").unwrap_or_default()),
    kind: "message".into(),
    text: text.map(|t| t.trim().to_owned()),
    author: Some(User {
      id: d
        .str("senderStaffId")
        .or_else(|| d.str("senderId"))
        .unwrap_or_default(),
      name: d.str("senderNick").unwrap_or_default(),
      ..User::default()
    }),
    created_at: d.i64("createAt").and_then(from_millis),
    raw: Some(d.clone()),
    ..Post::default()
  };
  let chat_type = match d.str("conversationType").as_deref() {
    Some("1") => "dm",
    _ => "group",
  };
  let reply_to = if chat_type == "dm" {
    d.str("senderStaffId").map(|s| format!("user:{s}"))
  } else {
    Some(chat.clone())
  };
  for (key, value) in [
    ("chat", Some(chat.clone())),
    ("chat_name", d.str("conversationTitle")),
    ("chat_type", Some(chat_type.to_owned())),
    ("reply_to_chat", reply_to),
    ("session_webhook", d.str("sessionWebhook")),
    ("robot_code", d.str("robotCode")),
  ] {
    if let Some(v) = value {
      post.extra.insert(key.into(), v.into());
    }
  }
  post
}

/// Any other event: its type as `kind`, the payload as `raw`.
fn event(msg: &Value, d: &Value) -> Post {
  let mut post = Post {
    id: msg
      .str("headers.eventId")
      .or_else(|| msg.str("headers.messageId"))
      .unwrap_or_default(),
    kind: msg
      .str("headers.eventType")
      .or_else(|| msg.str("headers.topic"))
      .unwrap_or_else(|| "event".into()),
    created_at: msg
      .str("headers.eventBornTime")
      .and_then(|t| t.parse().ok())
      .and_then(from_millis),
    raw: Some(d.clone()),
    ..Post::default()
  };
  if let Some(c) = d
    .str("openConversationId")
    .or_else(|| d.str("OpenConversationId"))
  {
    post.extra.insert("chat".into(), c.into());
  }
  post
}
