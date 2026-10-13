//! `listen`: KOOK's websocket gateway (`/gateway/index?compress=0`, so
//! frames are plain JSON). Signals: 0 event, 1 hello, 2 ping (sent every
//! 30 s with the last `sn`), 3 pong, 5 reconnect. Messages, edits,
//! deletions and reactions become posts.

use std::collections::HashSet;
use std::time::Duration;

use social_core::output::{Stream, note};
use social_core::text::from_millis;
use social_core::{Error, Post, Result, User, Value, ValueExt, json};
use tokio::time::Instant;

use crate::api::Api;
use crate::parse;
use crate::refs::{self, Chat};

const PING: Duration = Duration::from_secs(30);

pub async fn listen(api: &Api, chats: &[String], out: &Stream) -> Result<()> {
  let me = api
    .get("/user/me", &[])
    .await?
    .str("id")
    .unwrap_or_default();
  let mut wanted = HashSet::new();
  for c in chats {
    wanted.insert(refs::chat(c)?.id());
  }
  let mut failures = 0;
  loop {
    match session(api, &me, &wanted, out).await {
      Ok(()) => failures = 0,
      Err(e) => {
        failures += 1;
        if failures >= 3 {
          return Err(e);
        }
        tracing::debug!("gateway error: {}", e.message);
      }
    }
    note("the KOOK gateway closed; reconnecting");
    tokio::time::sleep(Duration::from_secs_f64(rand::random_range(1.0..5.0))).await;
  }
}

async fn session(api: &Api, me: &str, wanted: &HashSet<String>, out: &Stream) -> Result<()> {
  let gate = api
    .get("/gateway/index", &[("compress", "0".into())])
    .await?;
  let url = gate
    .str("url")
    .ok_or_else(|| Error::upstream("KOOK returned no gateway URL"))?;
  let mut ws = api.ctx.http.websocket(&url, &[], false).await?;
  let mut sn: u64 = 0;
  let mut ping = Instant::now() + PING;
  loop {
    tokio::select! {
      frame = ws.next_json() => {
        let Some(v) = frame? else { return Ok(()) };
        match v.u64("s") {
          Some(0) => {
            if let Some(n) = v.u64("sn") {
              sn = n;
            }
            if let Some(post) = event(v.at("d"), me)
              && (wanted.is_empty() || post.extra.get("chat").and_then(Value::as_str).is_some_and(|c| wanted.contains(c)))
            {
              out.post(&post);
            }
          }
          Some(1) => {
            if v.i64("d.code").unwrap_or(0) != 0 {
              return Err(Error::auth(format!("KOOK refused the connection (code {})", v.i64("d.code").unwrap_or(-1))));
            }
          }
          Some(5) => return Ok(()),
          _ => {}
        }
      }
      _ = tokio::time::sleep_until(ping) => {
        ws.send_json(&json!({"s": 2, "sn": sn})).await?;
        ping = Instant::now() + PING;
      }
    }
  }
}

/// A gateway event as a post, or `None` for events not about messages.
fn event(d: &Value, me: &str) -> Option<Post> {
  let extra = d.at("extra");
  let direct = d.str("channel_type").as_deref() == Some("PERSON");
  let author = d.str("author_id").unwrap_or_default();
  let chat = if direct {
    // In direct chats the target is the bot; the chat is the other person.
    let other = if author == me {
      d.str("target_id").unwrap_or_default()
    } else {
      author.clone()
    };
    Chat::Direct(other)
  } else {
    Chat::Channel(d.str("target_id").unwrap_or_default())
  };
  if d.u64("type") != Some(255) {
    let mut v = json!({
      "id": d.str("msg_id"),
      "type": d.u64("type"),
      "content": extra.at("kmarkdown.raw_content").as_str().map(str::to_owned).or_else(|| d.str("content")),
      "create_at": d.i64("msg_timestamp"),
      "author": extra.at("author"),
      "mention": extra.at("mention"),
      "quote": extra.at("quote"),
    });
    if extra.at("attachments").is_object() {
      v["attachments"] = extra.at("attachments").clone();
    }
    let name = extra.str("channel_name").map(|n| format!("#{n}"));
    let mut p = parse::message(&v, &chat, name.as_deref());
    if let Some(g) = extra.str("guild_id") {
      p.extra.insert("guild".into(), json!(g));
    }
    p.raw = Some(d.clone());
    return Some(p);
  }
  // System events: what happened to which message.
  let body = extra.at("body");
  let kind = match extra.str("type")?.as_str() {
    "updated_message" | "updated_private_message" => "edit",
    "deleted_message" | "deleted_private_message" => "delete",
    "added_reaction" | "private_added_reaction" => "reaction",
    "deleted_reaction" | "private_deleted_reaction" => "unreaction",
    "joined_guild" => "join",
    "exited_guild" => "leave",
    _ => return None,
  };
  let msg = body.str("msg_id").unwrap_or_default();
  let chat = match body.str("channel_id") {
    Some(c) if !direct => Chat::Channel(c),
    _ => chat,
  };
  let mut p = Post {
    id: format!("{}/{msg}", chat.id()),
    kind: kind.into(),
    text: body.str("content"),
    author: body.str("user_id").map(|u| User {
      name: u.clone(),
      id: u,
      ..User::default()
    }),
    created_at: d.i64("msg_timestamp").and_then(from_millis),
    raw: Some(d.clone()),
    ..Post::default()
  };
  p.extra.insert("chat".into(), json!(chat.id()));
  p.extra.insert("message_id".into(), json!(msg));
  if let Some(e) = body.str("emoji.name").or_else(|| body.str("emoji.id")) {
    p.extra.insert("emoji".into(), json!(e));
  }
  Some(p)
}
