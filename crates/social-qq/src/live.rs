//! `listen`: the websocket gateway (`GET /gateway`). Hello → Identify with
//! intents → READY; heartbeats carry the last sequence number; op 7 asks for
//! a reconnect. Intents a bot lacks make QQ close the connection, so a
//! shorter set is tried before giving up (`QQ_BOT_INTENTS` sets them
//! exactly).

use std::collections::HashSet;
use std::time::Duration;

use social_core::output::{Stream, note};
use social_core::text::parse_time;
use social_core::{Error, Post, Result, User, Value, ValueExt, json};
use tokio::time::Instant;

use crate::api::Api;
use crate::parse;
use crate::refs::{self, Chat};

const GUILDS: u64 = 1 << 0;
const GUILD_MEMBERS: u64 = 1 << 1;
const GUILD_MESSAGE_REACTIONS: u64 = 1 << 10;
const DIRECT_MESSAGE: u64 = 1 << 12;
const GROUP_AND_C2C_EVENT: u64 = 1 << 25;
const PUBLIC_GUILD_MESSAGES: u64 = 1 << 30;
const BASIC: u64 = GUILDS | GUILD_MEMBERS | PUBLIC_GUILD_MESSAGES;

/// How a connection ended.
enum Ended {
  /// Closed after READY: reconnect.
  Lost,
  /// Refused before READY (intents, credentials).
  Refused,
}

pub async fn listen(api: &Api, chats: &[String], out: &Stream) -> Result<()> {
  let mut wanted = HashSet::new();
  for c in chats {
    wanted.insert(refs::chat(c)?.id());
  }
  let fixed = std::env::var("QQ_BOT_INTENTS")
    .ok()
    .and_then(|v| v.trim().parse::<u64>().ok());
  let mut tries: Vec<u64> = match fixed {
    Some(i) => vec![i],
    None => vec![
      BASIC,
      BASIC | GROUP_AND_C2C_EVENT,
      BASIC | GROUP_AND_C2C_EVENT | DIRECT_MESSAGE | GUILD_MESSAGE_REACTIONS,
    ],
  };
  let mut intents = tries.pop().unwrap_or(BASIC);
  let mut lost = 0;
  loop {
    match session(api, intents, &wanted, out).await? {
      Ended::Lost => {
        lost += 1;
        if lost > 20 {
          return Err(Error::network(
            "the QQ gateway keeps dropping the connection",
          ));
        }
        note("the QQ gateway closed the connection; reconnecting");
      }
      Ended::Refused => match tries.pop() {
        Some(fewer) => {
          note("QQ refused some event groups for this bot; listening to fewer");
          intents = fewer;
        }
        None => {
          return Err(Error::auth(
            "the QQ gateway refused the connection; check the bot's event permissions (QQ_BOT_INTENTS)",
          ));
        }
      },
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
  }
}

async fn session(api: &Api, intents: u64, wanted: &HashSet<String>, out: &Stream) -> Result<Ended> {
  let creds = api.require()?;
  let token = api.access_token(&creds).await?;
  let gate = api.get("/gateway", &[]).await?;
  let url = gate
    .str("url")
    .ok_or_else(|| Error::upstream("QQ returned no gateway URL"))?;
  let mut ws = api.ctx.http.websocket(&url, &[], false).await?;
  let Some(hello) = ws.next_json().await? else {
    return Ok(Ended::Refused);
  };
  let every = Duration::from_millis(hello.u64("d.heartbeat_interval").unwrap_or(41_250));
  ws.send_json(&json!({
    "op": 2,
    "d": {"token": format!("QQBot {token}"), "intents": intents, "shard": [0, 1], "properties": {}},
  }))
  .await?;
  let mut ready = false;
  let mut seq: Option<u64> = None;
  let mut beat = Instant::now() + every;
  loop {
    tokio::select! {
      frame = ws.next_json() => {
        let Some(v) = frame? else {
          return Ok(if ready { Ended::Lost } else { Ended::Refused });
        };
        if let Some(s) = v.u64("s") {
          seq = Some(s);
        }
        match v.u64("op") {
          Some(0) => {
            let t = v.str("t").unwrap_or_default();
            if t == "READY" {
              ready = true;
              continue;
            }
            if let Some(post) = event(&t, v.at("d"), v.str("id"))
              && (wanted.is_empty() || post.extra.get("chat").and_then(Value::as_str).is_some_and(|c| wanted.contains(c)))
            {
              out.post(&post);
            }
          }
          Some(7) => return Ok(Ended::Lost),
          Some(9) => return Ok(if ready { Ended::Lost } else { Ended::Refused }),
          _ => {}
        }
      }
      _ = tokio::time::sleep_until(beat) => {
        ws.send_json(&json!({"op": 1, "d": seq})).await?;
        beat = Instant::now() + every;
      }
    }
  }
}

fn event(t: &str, d: &Value, event_id: Option<String>) -> Option<Post> {
  let chat = |d: &Value| -> Option<Chat> {
    match t {
      "GROUP_AT_MESSAGE_CREATE"
      | "GROUP_MESSAGE_CREATE"
      | "GROUP_ADD_ROBOT"
      | "GROUP_DEL_ROBOT" => d.str("group_openid").map(Chat::Group),
      "C2C_MESSAGE_CREATE" => d.str("author.user_openid").map(Chat::User),
      "FRIEND_ADD" | "FRIEND_DEL" => d.str("openid").map(Chat::User),
      "DIRECT_MESSAGE_CREATE" | "DIRECT_MESSAGE_DELETE" => d
        .str("guild_id")
        .or_else(|| d.str("message.guild_id"))
        .map(Chat::Dm),
      _ => d
        .str("channel_id")
        .or_else(|| d.str("message.channel_id"))
        .map(Chat::Channel),
    }
  };
  let mut post = match t {
    "AT_MESSAGE_CREATE"
    | "MESSAGE_CREATE"
    | "DIRECT_MESSAGE_CREATE"
    | "GROUP_AT_MESSAGE_CREATE"
    | "GROUP_MESSAGE_CREATE"
    | "C2C_MESSAGE_CREATE" => parse::message(d, &chat(d)?),
    "MESSAGE_DELETE" | "PUBLIC_MESSAGE_DELETE" | "DIRECT_MESSAGE_DELETE" => {
      let mut p = parse::message(d.at("message"), &chat(d)?);
      p.kind = "delete".into();
      if let Some(op) = d.str("op_user.id") {
        p.extra.insert("operator".into(), json!(op));
      }
      p
    }
    "MESSAGE_REACTION_ADD" | "MESSAGE_REACTION_REMOVE" => {
      let c = chat(d)?;
      let msg = d.str("target.id").unwrap_or_default();
      let mut p = Post {
        id: format!("{}/{msg}", c.id()),
        kind: if t.ends_with("ADD") {
          "reaction"
        } else {
          "unreaction"
        }
        .into(),
        author: d.str("user_id").map(|u| User {
          name: u.clone(),
          id: u,
          ..User::default()
        }),
        raw: Some(d.clone()),
        ..Post::default()
      };
      p.extra.insert("chat".into(), json!(c.id()));
      p.extra.insert("message_id".into(), json!(msg));
      if let Some(e) = d.str("emoji.id") {
        p.extra.insert("emoji".into(), json!(e));
      }
      p
    }
    "GROUP_ADD_ROBOT"
    | "GROUP_DEL_ROBOT"
    | "FRIEND_ADD"
    | "FRIEND_DEL"
    | "GUILD_MEMBER_ADD"
    | "GUILD_MEMBER_REMOVE" => {
      let c = match t {
        "GUILD_MEMBER_ADD" | "GUILD_MEMBER_REMOVE" => Chat::Guild(d.str("guild_id")?),
        _ => chat(d)?,
      };
      let joined = t.ends_with("ADD");
      let who = d
        .first_str(&["op_member_openid", "openid", "user.id"])
        .unwrap_or_default();
      let mut p = Post {
        id: format!("{}/{}", c.id(), event_id.clone().unwrap_or_default()),
        kind: if joined { "join" } else { "leave" }.into(),
        author: Some(User {
          name: d.str("user.username").unwrap_or_else(|| who.clone()),
          id: who,
          ..User::default()
        }),
        created_at: d
          .str("timestamp")
          .and_then(|s| s.parse::<i64>().ok())
          .and_then(social_core::text::from_secs)
          .or_else(|| d.str("joined_at").and_then(|s| parse_time(&s))),
        raw: Some(d.clone()),
        ..Post::default()
      };
      p.extra.insert("chat".into(), json!(c.id()));
      p
    }
    _ => return None,
  };
  if let Some(e) = event_id {
    post.extra.insert("event_id".into(), json!(e));
  }
  post.extra.insert("event".into(), json!(t));
  Some(post)
}
