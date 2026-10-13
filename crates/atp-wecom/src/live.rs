//! `listen`: the AI bot's long connection (智能机器人 长连接,
//! `wss://openws.work.weixin.qq.com`), so no public callback URL is needed.
//!
//! After `aibot_subscribe` with the bot id and secret, messages to the bot
//! arrive as `aibot_msg_callback` and events as `aibot_event_callback`; a
//! `ping` command every 30 seconds keeps the connection. Ordinary app
//! callbacks need a public HTTPS URL and are not supported.

use std::collections::HashSet;
use std::time::Duration;

use atp_core::output::{Stream, note};
use atp_core::socket::{Frame, Socket};
use atp_core::{Error, ErrorCode, Post, Result, User, Value, ValueExt, json};
use tokio::time::{Instant, interval_at};

use crate::api::{Api, Bot, LOGIN_HINT};

const WS: &str = "wss://openws.work.weixin.qq.com";
const MAX_FAILURES: u32 = 5;

pub async fn listen(api: &Api, chats: &[String], out: &Stream) -> Result<()> {
  let bot = api.bot().ok_or_else(|| {
    Error::auth("listening needs an AI bot (智能机器人) in long-connection mode")
      .with_hint(LOGIN_HINT)
  })?;
  let wanted: HashSet<String> = chats.iter().map(|c| c.trim().to_owned()).collect();
  let mut failures = 0;
  loop {
    match session(api, &bot, &wanted, out).await {
      Ok(true) => failures = 0,
      Ok(false) => failures += 1,
      Err(e) if failures == 0 && e.code != ErrorCode::NetworkError => return Err(e),
      Err(e) => {
        failures += 1;
        note(&format!("connection lost: {}", e.message));
      }
    }
    if failures >= MAX_FAILURES {
      return Err(Error::network("the WeCom long connection keeps failing"));
    }
    tokio::time::sleep(Duration::from_secs(2u64.pow(failures.min(5)))).await;
  }
}

fn req_id() -> String {
  format!("{:032x}", rand::random::<u128>())
}

async fn session(api: &Api, bot: &Bot, wanted: &HashSet<String>, out: &Stream) -> Result<bool> {
  let mut socket: Socket = api.ctx.http.websocket(WS, &[], false).await?;
  socket
    .send_json(&json!({
      "cmd": "aibot_subscribe",
      "headers": { "req_id": req_id() },
      "body": { "bot_id": bot.bot_id, "secret": bot.secret },
    }))
    .await?;
  let reply = tokio::time::timeout(Duration::from_secs(15), socket.next_json())
    .await
    .map_err(|_| Error::network("no answer to the subscription"))??
    .unwrap_or_default();
  match reply.i64("errcode") {
    Some(0) | None => {}
    Some(code) => {
      return Err(
        Error::auth(format!(
          "WeCom refused the AI bot ({code} {})",
          reply
            .str("errmsg")
            .unwrap_or_default()
            .split(", hint:")
            .next()
            .unwrap_or_default()
        ))
        .with_hint(LOGIN_HINT),
      );
    }
  }
  note("connected; waiting for messages (Ctrl-C to stop)");
  let mut ping = interval_at(
    Instant::now() + Duration::from_secs(30),
    Duration::from_secs(30),
  );
  let mut received = false;
  loop {
    tokio::select! {
      _ = ping.tick() => {
        socket.send_json(&json!({ "cmd": "ping", "headers": { "req_id": req_id() } })).await?;
      }
      next = socket.next() => {
        let text = match next? {
          None => return Ok(received),
          Some(Frame::Text(t)) => t,
          Some(Frame::Binary(b)) => String::from_utf8_lossy(&b).into_owned(),
        };
        received = true;
        let Ok(msg) = serde_json::from_str::<Value>(&text) else { continue };
        let body = msg.at("body");
        let post = match msg.str("cmd").as_deref() {
          Some("aibot_msg_callback") => message(body),
          Some("aibot_event_callback") => Post {
            id: body.str("msgid").unwrap_or_default(),
            kind: body.str("event.eventtype").unwrap_or_else(|| "event".into()),
            author: body.str("from.userid").map(|id| User { id, ..User::default() }),
            raw: Some(body.clone()),
            ..Post::default()
          },
          _ => continue,
        };
        let chat = post.extra.get("chat").and_then(|c| c.as_str()).unwrap_or_default();
        if wanted.is_empty() || wanted.contains(chat) {
          out.post(&post);
        }
      }
    }
  }
}

fn message(b: &Value) -> Post {
  let kind = b.str("msgtype").unwrap_or_default();
  let text = match kind.as_str() {
    "text" => b.str("text.content"),
    "mixed" => Some(
      b.list("mixed.msg_item")
        .iter()
        .map(|i| match i.str("msgtype").as_deref() {
          Some("text") => i.str("text.content").unwrap_or_default(),
          Some(other) => format!("[{other}]"),
          None => String::new(),
        })
        .collect::<Vec<_>>()
        .join(" "),
    ),
    "voice" => b.str("voice.content").or(Some("[voice]".into())),
    other => Some(format!("[{other}]")),
  };
  let user = b.str("from.userid").unwrap_or_default();
  let chat = match b.str("chattype").as_deref() {
    Some("group") => b.str("chatid").unwrap_or_default(),
    _ => format!("user:{user}"),
  };
  let mut post = Post {
    id: b.str("msgid").unwrap_or_default(),
    kind: "message".into(),
    text,
    author: Some(User {
      id: user,
      ..User::default()
    }),
    created_at: Some(jiff::Timestamp::now()),
    raw: Some(b.clone()),
    ..Post::default()
  };
  post.extra.insert("chat".into(), chat.into());
  post.extra.insert(
    "chat_type".into(),
    b.str("chattype").unwrap_or_else(|| "single".into()).into(),
  );
  // Images in bot messages are encrypted with the bot's AES key.
  if let Some(url) = b.str("image.url") {
    post.extra.insert("image_url".into(), url.into());
  }
  post
}
