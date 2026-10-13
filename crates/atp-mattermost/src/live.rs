//! `listen`: the server's websocket (`/api/v4/websocket`), authenticated by
//! the bearer header and an `authentication_challenge`. New, edited and
//! deleted posts and reactions become posts on the stream; the connection
//! is reopened when it drops.

use std::collections::HashSet;

use atp_core::live::Backoff;
use atp_core::output::Stream;
use atp_core::text::from_millis;
use atp_core::{Error, Post, Result, User, Value, ValueExt, json};

use crate::api::{Api, Creds};
use crate::{chats, messages, users};

/// Give up when this many connections close before the server says hello.
const MAX_FAILURES: u32 = 3;

pub async fn listen(api: &Api, args: &[String], out: &Stream) -> Result<()> {
  let creds = api.require()?;
  api.me().await?;
  let mut ids = HashSet::new();
  for arg in args {
    ids.insert(chats::channel_id(api, arg).await?);
  }
  let mut backoff = Backoff::default();
  let mut failures = 0;
  loop {
    if session(api, &creds, &ids, out).await? {
      failures = 0;
      backoff.reset();
    } else {
      failures += 1;
      if failures >= MAX_FAILURES {
        return Err(Error::upstream(
          "the Mattermost websocket keeps closing before it says hello",
        ));
      }
    }
    backoff.wait().await;
  }
}

/// One connection until it closes; whether the server said hello.
async fn session(api: &Api, creds: &Creds, ids: &HashSet<String>, out: &Stream) -> Result<bool> {
  let url = format!(
    "{}/api/v4/websocket",
    creds
      .server
      .replacen("https://", "wss://", 1)
      .replacen("http://", "ws://", 1)
  );
  let auth = format!("Bearer {}", creds.token);
  let mut socket = api
    .ctx
    .http
    .websocket(&url, &[("Authorization", auth.as_str())], false)
    .await?;
  socket
    .send_json(&json!({
      "seq": 1,
      "action": "authentication_challenge",
      "data": {"token": creds.token},
    }))
    .await?;
  let mut hello = false;
  while let Some(frame) = socket.next_json().await? {
    match frame.str("event").as_deref() {
      Some("hello") => hello = true,
      Some(event) => {
        if let Some(post) = event_post(api, event, &frame, ids).await {
          out.post(&post);
        }
      }
      None => {}
    }
  }
  Ok(hello)
}

async fn event_post(api: &Api, event: &str, frame: &Value, ids: &HashSet<String>) -> Option<Post> {
  let data = frame.at("data");
  let channel = frame.str("broadcast.channel_id").unwrap_or_default();
  let kind = match event {
    "posted" => "message",
    "post_edited" => "edit",
    "post_deleted" => "delete",
    "reaction_added" => "reaction",
    "reaction_removed" => "unreaction",
    _ => return None,
  };
  if kind.ends_with("reaction") {
    if !ids.is_empty() && !ids.contains(&channel) {
      return None;
    }
    let reaction: Value = serde_json::from_str(&data.str("reaction")?).ok()?;
    return reaction_post(api, kind, reaction, &channel).await;
  }
  let post: Value = serde_json::from_str(&data.str("post")?).ok()?;
  let channel = post.str("channel_id").unwrap_or(channel);
  if !ids.is_empty() && !ids.contains(&channel) {
    return None;
  }
  let mut post = messages::posts(api, &[post]).await.ok()?.pop()?;
  if kind != "message" {
    post.kind = kind.into();
  }
  Some(post)
}

/// A reaction added or removed, as a post on the reacted-to post's id.
async fn reaction_post(api: &Api, kind: &str, reaction: Value, channel: &str) -> Option<Post> {
  let user = reaction.str("user_id").unwrap_or_default();
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
  let mut post = Post {
    id: reaction.str("post_id")?,
    kind: kind.into(),
    text: reaction.str("emoji_name").map(|e| format!(":{e}:")),
    author: Some(author),
    created_at: reaction.i64("create_at").and_then(from_millis),
    raw: Some(reaction),
    ..Post::default()
  };
  post.extra.insert("chat".into(), json!(channel));
  Some(post)
}
