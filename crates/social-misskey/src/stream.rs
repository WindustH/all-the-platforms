//! `listen`: Misskey's streaming websocket (`/streaming?i=<token>`).
//!
//! Logged in, it follows the home timeline and the main channel (mentions,
//! replies, chat messages, notifications); anonymously, the local timeline;
//! with chats named, those direct chats and rooms. Lost connections
//! reconnect.

use std::time::Duration;

use social_core::output::Stream;
use social_core::{Error, Post, Result, Value, ValueExt, json};
use tokio::time::Instant;

use crate::api::Api;
use crate::parse;
use crate::refs::{self, Chat, UserRef};
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

fn channel_id() -> String {
  format!("{:016x}", rand::random::<u64>())
}

pub async fn listen(api: &Api, chats: &[String], out: &Stream) -> Result<()> {
  let mut channels = Vec::new();
  if chats.is_empty() {
    if api.logged_in() {
      for kind in ["main", "homeTimeline"] {
        channels.push(Channel {
          id: channel_id(),
          kind: kind.into(),
          params: json!({}),
          chat: None,
        });
      }
    } else {
      channels.push(Channel {
        id: channel_id(),
        kind: "localTimeline".into(),
        params: json!({}),
        chat: None,
      });
    }
  } else {
    api.require()?;
    for arg in chats {
      let (kind, params, chat) = match refs::chat(arg, &api.instance())? {
        Chat::Room(id) => ("chatRoom", json!({"roomId": id}), format!("room:{id}")),
        Chat::User(r) => {
          let id = match r {
            UserRef::Id(id) => id,
            other => users::lookup(api, &other)
              .await?
              .str("id")
              .unwrap_or_default(),
          };
          ("chatUser", json!({"otherId": id}), format!("user:{id}"))
        }
      };
      channels.push(Channel {
        id: channel_id(),
        kind: kind.into(),
        params,
        chat: Some(chat),
      });
    }
  }
  if api.logged_in() {
    // A bad token fails here instead of in a reconnect loop.
    users::my_id(api).await?;
  }
  let mut failures = 0;
  loop {
    match session(api, &channels, out).await {
      Ok(true) => failures = 0,
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
    let wait: f64 = rand::random_range(1.0..5.0);
    tokio::time::sleep(Duration::from_secs_f64(wait * f64::from(failures.max(1)))).await;
  }
}

/// One connection; `true` when it delivered something before it ended.
async fn session(api: &Api, channels: &[Channel], out: &Stream) -> Result<bool> {
  let mut url = format!("wss://{}/streaming", api.instance());
  if let Some(t) = api.token() {
    url.push_str(&format!("?i={t}"));
  }
  let origin = api.base();
  let headers = [("Origin", origin.as_str())];
  let mut ws = api.ctx.http.websocket(&url, &headers, false).await?;
  for c in channels {
    ws.send_json(&json!({
      "type": "connect",
      "body": {"channel": c.kind, "id": c.id, "params": c.params},
    }))
    .await?;
  }
  let instance = api.instance();
  let mut delivered = false;
  let mut beat = Instant::now() + HEARTBEAT;
  loop {
    tokio::select! {
      frame = ws.next_json() => {
        let Some(v) = frame? else { return Ok(delivered) };
        if v.str("type").as_deref() != Some("channel") {
          continue;
        }
        let body = v.at("body");
        let Some(c) = channels.iter().find(|c| body.str("id").as_deref() == Some(c.id.as_str())) else {
          continue;
        };
        if let Some(post) = event(c, body.str("type").as_deref().unwrap_or_default(), body.at("body"), &instance) {
          out.post(&post);
          delivered = true;
        }
      }
      _ = tokio::time::sleep_until(beat) => {
        ws.send_text("h").await?;
        beat = Instant::now() + HEARTBEAT;
      }
    }
  }
}

fn event(c: &Channel, kind: &str, body: &Value, instance: &str) -> Option<Post> {
  match (c.kind.as_str(), kind) {
    (_, "note") => Some(parse::note(body, instance)),
    ("main", "mention" | "reply") => {
      let mut p = parse::note(body, instance);
      p.kind = kind.into();
      Some(p)
    }
    ("main", "newChatMessage") => {
      let chat = match body.str("toRoomId") {
        Some(r) => format!("room:{r}"),
        None => format!("user:{}", body.str("fromUserId").unwrap_or_default()),
      };
      Some(parse::chat_message(body, &chat, "", instance))
    }
    ("main", "notification") => {
      let n = parse::notification(body, instance);
      // Mentions and replies already arrive as notes.
      if matches!(n.kind.as_str(), "mention" | "reply") {
        return None;
      }
      Some(Post {
        id: n.id,
        kind: n.kind,
        text: Some(n.text).filter(|t| !t.is_empty()),
        author: n.actor,
        created_at: n.created_at,
        url: n.url,
        ..Post::default()
      })
    }
    (_, "message") => {
      let chat = c.chat.clone().unwrap_or_default();
      Some(parse::chat_message(body, &chat, "", instance))
    }
    (_, "deleted") => {
      let chat = c.chat.clone().unwrap_or_default();
      let id = body
        .as_str()
        .map(str::to_owned)
        .or_else(|| body.str("id"))?;
      Some(Post {
        id: format!("{chat}/{id}"),
        kind: "delete".into(),
        ..Post::default()
      })
    }
    (_, "react" | "unreact") => {
      let chat = c.chat.clone().unwrap_or_default();
      let mut p = Post {
        id: format!("{chat}/{}", body.str("messageId").unwrap_or_default()),
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
      p.extra.insert("chat".into(), json!(chat));
      Some(p)
    }
    _ => None,
  }
}
