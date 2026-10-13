//! `listen`: the event websocket. PING every 10 s; on a dropped connection
//! IDENTIFY again with the last `sn`, so the server replays what was missed.
//! Message, reaction and member events become posts; events of other
//! accounts on the same server are left out when one account is chosen.

use std::collections::HashSet;
use std::time::Duration;

use social_core::live::Backoff;
use social_core::output::{Stream, note};
use social_core::text::from_millis;
use social_core::{Error, Post, Result, Value, ValueExt, json};
use tokio::time::Instant;

use crate::api::{Api, Server};
use crate::parse;
use crate::refs::{self, ChatRef};

const PING: Duration = Duration::from_secs(10);

pub async fn listen(api: &Api, chats: &[String], out: &Stream) -> Result<()> {
  let server = api.require()?;
  let mut wanted = HashSet::new();
  for c in chats {
    wanted.insert(match refs::chat(c)? {
      ChatRef::Guild(g) => g,
      ChatRef::Channel(c) => c,
      ChatRef::User(_) => {
        return Err(Error::input(
          "listen takes channels or servers; open the direct chat with `messages user:<id>` first",
        ));
      }
    });
  }
  let mut sn: Option<u64> = None;
  let mut failures = 0;
  let mut backoff = Backoff::default();
  loop {
    match session(api, &server, &wanted, &mut sn, out).await {
      Ok(()) => {
        failures = 0;
        backoff.reset();
      }
      Err(e) => {
        failures += 1;
        if failures >= 3 {
          return Err(e);
        }
        tracing::debug!("event socket error: {}", e.message);
      }
    }
    note("the Satori event connection closed; reconnecting");
    backoff.wait().await;
  }
}

async fn session(
  api: &Api,
  server: &Server,
  wanted: &HashSet<String>,
  sn: &mut Option<u64>,
  out: &Stream,
) -> Result<()> {
  let (mut ws, _) = api.events(server, *sn).await?;
  let mut ping = Instant::now() + PING;
  loop {
    tokio::select! {
      frame = ws.next_json() => {
        let Some(v) = frame? else { return Ok(()) };
        if v.u64("op") != Some(0) {
          continue;
        }
        let body = v.at("body");
        if let Some(n) = body.u64("sn") {
          *sn = Some(n);
        }
        if !mine(server, body) {
          continue;
        }
        if let Some(post) = event(body)
          && (wanted.is_empty() || [post.extra.get("chat"), post.extra.get("guild")]
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .any(|id| wanted.contains(id)))
        {
          out.post(&post);
        }
      }
      _ = tokio::time::sleep_until(ping) => {
        ws.send_json(&json!({"op": 1, "body": {}})).await?;
        ping = Instant::now() + PING;
      }
    }
  }
}

/// Whether an event belongs to the chosen account (all, when none is).
fn mine(server: &Server, body: &Value) -> bool {
  let (Some(p), Some(u)) = (&server.platform, &server.user_id) else {
    return true;
  };
  let platform = body.str("login.platform").or_else(|| body.str("platform"));
  let user = body.str("login.user.id").or_else(|| body.str("self_id"));
  platform.is_none_or(|x| x == *p) && user.is_none_or(|x| x == *u)
}

fn event(body: &Value) -> Option<Post> {
  let kind = match body.str("type")?.as_str() {
    "message-created" => "message",
    "message-updated" => "edit",
    "message-deleted" => "delete",
    "reaction-added" => "reaction",
    "reaction-removed" => "unreaction",
    "guild-member-added" => "join",
    "guild-member-removed" => "leave",
    _ => return None,
  };
  let channel = body.str("channel.id").unwrap_or_default();
  let mut msg = body.at("message").clone();
  if !msg.is_object() {
    msg = json!({});
  }
  // Events carry user / member / guild beside the message.
  for key in ["user", "member", "channel", "guild"] {
    if msg.at(key).is_null() && !body.at(key).is_null() {
      msg[key] = body.at(key).clone();
    }
  }
  let mut p = parse::message(&msg, &channel, body.str("channel.name").as_deref());
  p.kind = kind.into();
  if p.created_at.is_none() {
    p.created_at = body.i64("timestamp").and_then(from_millis);
  }
  if let Some(g) = body.str("guild.id") {
    p.extra.insert("guild".into(), json!(g));
  }
  if let Some(e) = body.str("emoji.name").or_else(|| body.str("emoji.id")) {
    p.extra.insert("emoji".into(), json!(e));
  }
  p.raw = Some(body.clone());
  Some(p)
}
