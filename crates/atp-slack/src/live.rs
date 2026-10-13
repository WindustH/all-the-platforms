//! `listen`: Socket Mode when an app-level token (`xapp-…`, with
//! `connections:write`) is set — each event envelope is acknowledged and
//! messages, edits, deletions and reactions become posts — otherwise
//! polling the named conversations every few seconds.

use std::collections::HashSet;
use std::time::Duration;

use atp_core::live::{self, Backoff};
use atp_core::output::{Stream, note};
use atp_core::{Error, Post, Result, User, Value, ValueExt, chat};

use crate::api::Api;
use crate::{chats, messages, parse, users};

const POLL: Duration = Duration::from_secs(10);

pub async fn listen(api: &Api, args: &[String], out: &Stream) -> Result<()> {
  api.require()?;
  let mut ids = HashSet::new();
  for arg in args {
    ids.insert(chats::id(api, arg).await?);
  }
  match api.app_token() {
    Some(app) => socket_mode(api, &app, &ids, out).await,
    None if ids.is_empty() => Err(
      Error::input("name the conversations to watch, or set up Socket Mode").with_hint(
        "without an app-level token (`login --token 'xoxb-… xapp-…'` or SLACK_APP_TOKEN) `listen` polls the conversations given",
      ),
    ),
    None => poll(api, &ids, out).await,
  }
}

async fn socket_mode(api: &Api, app: &str, ids: &HashSet<String>, out: &Stream) -> Result<()> {
  let mut failures = 0;
  let mut backoff = Backoff::default();
  loop {
    let open = api
      .call_with(app, "apps.connections.open", Vec::new())
      .await
      .map_err(|e| {
        e.with_hint(
          "the app-level token needs `connections:write` and Socket Mode enabled for the app",
        )
      })?;
    let url = open
      .str("url")
      .ok_or_else(|| Error::upstream("apps.connections.open returned no URL"))?;
    let mut ws = api.ctx.http.websocket(&url, &[], false).await?;
    let mut hello = false;
    while let Some(frame) = ws.next_json().await? {
      if let Some(id) = frame.str("envelope_id") {
        ws.send_json(&serde_json::json!({ "envelope_id": id }))
          .await?;
      }
      match frame.str("type").as_deref() {
        Some("hello") => {
          hello = true;
          failures = 0;
          backoff.reset();
        }
        Some("disconnect") => break,
        Some("events_api") => event(api, frame.at("payload.event"), ids, out).await,
        _ => {}
      }
    }
    if !hello {
      failures += 1;
      if failures >= 3 {
        return Err(Error::upstream(
          "Slack keeps closing the Socket Mode connection",
        ));
      }
    }
    backoff.wait().await;
  }
}

async fn event(api: &Api, e: &Value, ids: &HashSet<String>, out: &Stream) {
  let channel = e
    .first_str(&["channel", "item.channel"])
    .unwrap_or_default();
  if !ids.is_empty() && !ids.contains(&channel) {
    return;
  }
  let kind = e.str("type").unwrap_or_default();
  let post = match (kind.as_str(), e.str("subtype").as_deref()) {
    ("message", Some("message_changed")) => convert(api, e.at("message"), &channel, "edit").await,
    ("message", Some("message_deleted")) => {
      let mut old = e.at("previous_message").clone();
      if old.is_null() {
        old = serde_json::json!({ "ts": e.str("deleted_ts") });
      }
      convert(api, &old, &channel, "delete").await
    }
    ("message", _) => convert(api, e, &channel, "message").await,
    ("reaction_added" | "reaction_removed", _) => reaction(api, e, &channel, &kind).await,
    _ => None,
  };
  if let Some(p) = post {
    out.post(&p);
  }
}

async fn convert(api: &Api, m: &Value, channel: &str, kind: &str) -> Option<Post> {
  let mut p = messages::posts(api, std::slice::from_ref(m), channel)
    .await
    .ok()?
    .pop()?;
  if kind != "message" {
    p.kind = kind.into();
  }
  Some(p)
}

async fn reaction(api: &Api, e: &Value, channel: &str, kind: &str) -> Option<Post> {
  let user = e.str("user").unwrap_or_default();
  users::resolve(api, [user.clone()]).await;
  let author = api
    .users
    .borrow()
    .get(&user)
    .map(|b| b.user(&user))
    .unwrap_or(User {
      name: user.clone(),
      id: user,
      ..User::default()
    });
  let ts = e.str("item.ts")?;
  let mut p = Post {
    id: chat::message_id(channel, &ts),
    kind: if kind == "reaction_added" {
      "reaction"
    } else {
      "unreaction"
    }
    .into(),
    text: e.str("reaction").map(|r| format!(":{r}:")),
    author: Some(author),
    created_at: e.str("event_ts").and_then(|t| parse::ts_time(&t)),
    raw: Some(e.clone()),
    ..Post::default()
  };
  p.extra.insert("chat".into(), channel.into());
  if let Some(c) = api.chats.borrow().get(channel) {
    p.extra.insert("chat_name".into(), c.name.clone().into());
  }
  Some(p)
}

/// Without Socket Mode: the newest messages of each conversation, every
/// [`POLL`]; what existed when listening started is not printed.
async fn poll(api: &Api, ids: &HashSet<String>, out: &Stream) -> Result<()> {
  note(&format!(
    "Polling {} conversation(s) every {}s; an app-level token gives live events (see `atp guide slack`).",
    ids.len(),
    POLL.as_secs()
  ));
  live::poll(out, POLL, async || {
    let mut posts = Vec::new();
    for channel in ids {
      let v = api
        .call(
          "conversations.history",
          vec![("channel", channel.clone()), ("limit", "100".into())],
        )
        .await?;
      posts.extend(messages::posts(api, v.list("messages"), channel).await?);
    }
    Ok(posts)
  })
  .await
}
