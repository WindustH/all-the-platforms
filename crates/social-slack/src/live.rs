//! `listen`: Socket Mode when an app-level token (`xapp-…`, with
//! `connections:write`) is set — each event envelope is acknowledged and
//! messages, edits, deletions and reactions become posts — otherwise
//! polling the named conversations every few seconds.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use social_core::output::{Stream, note};
use social_core::{Error, Post, Result, User, Value, ValueExt};

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
    tokio::time::sleep(Duration::from_secs_f64(rand::random_range(1.0..3.0))).await;
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
    id: format!("{channel}/{ts}"),
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

/// Ask each conversation for messages newer than the last one seen.
async fn poll(api: &Api, ids: &HashSet<String>, out: &Stream) -> Result<()> {
  note(&format!(
    "Polling {} conversation(s) every {}s; an app-level token gives live events (see `social guide slack`).",
    ids.len(),
    POLL.as_secs()
  ));
  let now = jiff::Timestamp::now();
  let start = format!("{}.{:06}", now.as_second(), now.subsec_microsecond());
  let mut last: HashMap<String, String> = ids.iter().map(|c| (c.clone(), start.clone())).collect();
  loop {
    for (channel, since) in last.iter_mut() {
      let v = api
        .call(
          "conversations.history",
          vec![
            ("channel", channel.clone()),
            ("oldest", since.clone()),
            ("limit", "100".into()),
          ],
        )
        .await?;
      let mut rows = v.list("messages").to_vec();
      rows.reverse();
      if let Some(ts) = rows.last().and_then(|m| m.str("ts")) {
        *since = ts;
      }
      for p in messages::posts(api, &rows, channel).await? {
        out.post(&p);
      }
    }
    let jitter = rand::random_range(0.0..3.0);
    tokio::time::sleep(POLL + Duration::from_secs_f64(jitter)).await;
  }
}
