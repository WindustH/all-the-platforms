//! `listen`: the AI bot's long connection (智能机器人 长连接,
//! `wss://openws.work.weixin.qq.com`), so no public callback URL is needed.
//!
//! After `aibot_subscribe` with the bot id and secret, messages to the bot
//! arrive as `aibot_msg_callback` and events as `aibot_event_callback`; a
//! `ping` command every 30 seconds keeps the connection. Ordinary app
//! callbacks need a public HTTPS URL and are not supported.

use std::collections::HashSet;
use std::time::Duration;

use atp_core::live::Backoff;
use atp_core::output::{Stream, note};
use atp_core::socket::{Frame, Socket};
use atp_core::{Error, ErrorCode, Result, Value, ValueExt, json};
use tokio::time::{Instant, interval_at};

use crate::api::{Api, Bot, LOGIN_HINT};
use crate::parse;

const WS: &str = "wss://openws.work.weixin.qq.com";
const MAX_FAILURES: u32 = 5;

pub async fn listen(api: &Api, chats: &[String], out: &Stream) -> Result<()> {
  let bot = api.bot().ok_or_else(|| {
    Error::auth("listening needs an AI bot (智能机器人) in long-connection mode")
      .with_hint(LOGIN_HINT)
  })?;
  let wanted: HashSet<String> = chats.iter().map(|c| c.trim().to_owned()).collect();
  let mut failures = 0;
  let mut backoff = Backoff::default();
  loop {
    match session(api, &bot, &wanted, out).await {
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
      return Err(Error::network("the WeCom long connection keeps failing"));
    }
    backoff.wait().await;
  }
}

fn req_id() -> String {
  format!("{:032x}", rand::random::<u128>())
}

async fn session(api: &Api, bot: &Bot, wanted: &HashSet<String>, out: &Stream) -> Result<bool> {
  let mut socket: Socket = api.ctx.http.websocket(WS, &[], false).await?;
  subscribe(&mut socket, bot).await?;
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
        emit(&text, wanted, out);
      }
    }
  }
}

/// `aibot_subscribe` with the bot's id and secret, and its answer.
async fn subscribe(socket: &mut Socket, bot: &Bot) -> Result<()> {
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
    Some(0) | None => Ok(()),
    Some(code) => {
      let message = reply.str("errmsg").unwrap_or_default();
      let message = message.split(", hint:").next().unwrap_or_default();
      Err(Error::auth(format!("WeCom refused the AI bot ({code} {message})")).with_hint(LOGIN_HINT))
    }
  }
}

/// Print a message or event frame when its chat is wanted.
fn emit(text: &str, wanted: &HashSet<String>, out: &Stream) {
  let Ok(frame) = serde_json::from_str::<Value>(text) else {
    return;
  };
  let body = frame.at("body");
  let post = match frame.str("cmd").as_deref() {
    Some("aibot_msg_callback") => parse::bot_message(body),
    Some("aibot_event_callback") => parse::bot_event(body),
    _ => return,
  };
  let chat = post
    .extra
    .get("chat")
    .and_then(|c| c.as_str())
    .unwrap_or_default();
  if wanted.is_empty() || wanted.contains(chat) {
    out.post(&post);
  }
}
