//! `listen`: Misskey's streaming websocket (`/streaming?i=<token>`).
//!
//! Logged in, it follows the home timeline and the main channel (mentions,
//! replies, chat messages, notifications); anonymously, the local timeline;
//! with chats named, those direct chats and rooms. Lost connections
//! reconnect.

use std::time::Duration;

use atp_core::live::Backoff;
use atp_core::output::Stream;
use atp_core::{Error, Post, Result, Value, ValueExt, chat, json};
use tokio::time::Instant;

use crate::api::Api;
use crate::chats::{self, Chat};
use crate::parse;
use crate::users;

/// Misskey closes idle sockets; a heartbeat now and then keeps it open.
const HEARTBEAT: Duration = Duration::from_secs(60);
const MAX_FAILURES: u32 = 5;

struct Channel {
  id: String,
  kind: String,
  params: Value,
  /// `user:<id>` / `room:<id>` for chat channels.
  chat: Option<String>,
}

impl Channel {
  fn new(kind: &str, params: Value, chat: Option<String>) -> Self {
    Self {
      id: format!("{:016x}", rand::random::<u64>()),
      kind: kind.into(),
      params,
      chat,
    }
  }
}

pub async fn listen(api: &Api, chats: &[String], out: &Stream) -> Result<()> {
  let channels = channels(api, chats).await?;
  if api.logged_in() {
    // A bad token fails here instead of in a reconnect loop.
    users::my_id(api).await?;
  }
  let mut backoff = Backoff::default();
  let mut failures = 0;
  loop {
    match session(api, &channels, out).await {
      Ok(true) => {
        failures = 0;
        backoff.reset();
      }
      Ok(false) => failures += 1,
      Err(e) => {
        tracing::debug!("stream error: {e}");
        failures += 1;
      }
    }
    if failures >= MAX_FAILURES {
      return Err(Error::upstream(
        "the Misskey stream keeps closing; check the instance and the token",
      ));
    }
    backoff.wait().await;
  }
}

/// The channels to follow: the named chats, else the timelines.
async fn channels(api: &Api, chats: &[String]) -> Result<Vec<Channel>> {
  if chats.is_empty() {
    let kinds: &[&str] = if api.logged_in() {
      &["main", "homeTimeline"]
    } else {
      &["localTimeline"]
    };
    let channels = kinds.iter().map(|kind| Channel::new(kind, json!({}), None));
    return Ok(channels.collect());
  }
  api.require()?;
  let mut channels = Vec::new();
  for arg in chats {
    let chat = chats::chat(api, arg).await?;
    let (kind, params) = match &chat {
      Chat::Room(id) => ("chatRoom", json!({"roomId": id})),
      Chat::User(id) => ("chatUser", json!({"otherId": id})),
    };
    channels.push(Channel::new(kind, params, Some(chat.key())));
  }
  Ok(channels)
}

/// One connection; `true` when it delivered something before it ended.
async fn session(api: &Api, channels: &[Channel], out: &Stream) -> Result<bool> {
  let mut url = format!("wss://{}/streaming", api.instance());
  if let Some(token) = api.token() {
    url.push_str(&format!("?i={token}"));
  }
  let origin = api.base();
  let headers = [("Origin", origin.as_str())];
  let mut socket = api.ctx.http.websocket(&url, &headers, false).await?;
  for channel in channels {
    socket
      .send_json(&json!({
        "type": "connect",
        "body": {"channel": channel.kind, "id": channel.id, "params": channel.params},
      }))
      .await?;
  }
  let instance = api.instance();
  let mut delivered = false;
  let mut beat = Instant::now() + HEARTBEAT;
  loop {
    tokio::select! {
      frame = socket.next_json() => {
        let Some(v) = frame? else { return Ok(delivered) };
        if let Some(post) = frame_post(channels, &v, &instance) {
          out.post(&post);
          delivered = true;
        }
      }
      _ = tokio::time::sleep_until(beat) => {
        socket.send_text("h").await?;
        beat = Instant::now() + HEARTBEAT;
      }
    }
  }
}

/// A `channel` frame of one of `channels` → a post.
fn frame_post(channels: &[Channel], v: &Value, instance: &str) -> Option<Post> {
  if v.str("type").as_deref() != Some("channel") {
    return None;
  }
  let body = v.at("body");
  let id = body.str("id")?;
  let channel = channels.iter().find(|channel| channel.id == id)?;
  let kind = body.str("type").unwrap_or_default();
  event(channel, &kind, body.at("body"), instance)
}

fn event(channel: &Channel, kind: &str, body: &Value, instance: &str) -> Option<Post> {
  match kind {
    "note" => Some(parse::post(body, instance)),
    "mention" | "reply" | "newChatMessage" | "notification" if channel.kind == "main" => {
      main_event(kind, body, instance)
    }
    _ => chat_event(
      channel.chat.as_deref().unwrap_or_default(),
      kind,
      body,
      instance,
    ),
  }
}

/// Events of the main channel: mentions, replies, chat messages, notifications.
fn main_event(kind: &str, body: &Value, instance: &str) -> Option<Post> {
  match kind {
    "mention" | "reply" => {
      let mut post = parse::post(body, instance);
      post.kind = kind.into();
      Some(post)
    }
    "newChatMessage" => {
      let chat = match body.str("toRoomId") {
        Some(room) => format!("room:{room}"),
        None => format!("user:{}", body.str("fromUserId").unwrap_or_default()),
      };
      Some(parse::message(body, &chat, "", instance))
    }
    _ => {
      let notification = parse::notification(body, instance);
      // Mentions and replies already arrive as notes.
      if matches!(notification.kind.as_str(), "mention" | "reply") {
        return None;
      }
      Some(Post {
        id: notification.id,
        kind: notification.kind,
        text: Some(notification.text).filter(|t| !t.is_empty()),
        author: notification.actor,
        created_at: notification.created_at,
        url: notification.url,
        ..Post::default()
      })
    }
  }
}

/// Events of a chat channel: messages, deletions and reactions.
fn chat_event(chat: &str, kind: &str, body: &Value, instance: &str) -> Option<Post> {
  match kind {
    "message" => Some(parse::message(body, chat, "", instance)),
    "deleted" => {
      let id = body
        .as_str()
        .map(str::to_owned)
        .or_else(|| body.str("id"))?;
      Some(Post {
        id: chat::message_id(chat, &id),
        kind: "delete".into(),
        ..Post::default()
      })
    }
    "react" | "unreact" => {
      let message = body.str("messageId").unwrap_or_default();
      let mut post = Post {
        id: chat::message_id(chat, &message),
        kind: if kind == "react" {
          "reaction"
        } else {
          "unreaction"
        }
        .into(),
        text: body.str("reaction"),
        author: (!body.at("user").is_null()).then(|| parse::user(body.at("user"), instance)),
        ..Post::default()
      };
      post.extra.insert("chat".into(), json!(chat));
      Some(post)
    }
    _ => None,
  }
}
