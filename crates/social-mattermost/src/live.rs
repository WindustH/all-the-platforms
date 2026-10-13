//! `listen`: the server's websocket (`/api/v4/websocket`), authenticated by
//! the bearer header and an `authentication_challenge`. New, edited and
//! deleted posts and reactions become posts on the stream; the connection
//! is reopened when it drops.

use std::collections::HashSet;
use std::time::Duration;

use social_core::output::Stream;
use social_core::{Error, Post, Result, User, Value, ValueExt, json};

use crate::api::Api;
use crate::{chats, posts, users};

pub async fn listen(api: &Api, args: &[String], out: &Stream) -> Result<()> {
  let creds = api.require()?;
  api.me().await?;
  let mut ids = HashSet::new();
  for arg in args {
    ids.insert(chats::channel_id(api, arg).await?);
  }
  let url = format!(
    "{}/api/v4/websocket",
    creds
      .server
      .replacen("https://", "wss://", 1)
      .replacen("http://", "ws://", 1)
  );
  let auth = format!("Bearer {}", creds.token);
  let mut failures = 0;
  loop {
    let mut ws = api
      .ctx
      .http
      .websocket(&url, &[("Authorization", auth.as_str())], false)
      .await?;
    ws.send_json(&json!({
      "seq": 1,
      "action": "authentication_challenge",
      "data": {"token": creds.token},
    }))
    .await?;
    let mut hello = false;
    while let Some(frame) = ws.next_json().await? {
      match frame.str("event").as_deref() {
        Some("hello") => {
          hello = true;
          failures = 0;
        }
        Some(event) => {
          if let Some(p) = event_post(api, event, &frame, &ids).await {
            out.post(&p);
          }
        }
        None => {}
      }
    }
    if !hello {
      failures += 1;
      if failures >= 3 {
        return Err(Error::upstream(
          "the Mattermost websocket keeps closing before it says hello",
        ));
      }
    }
    tokio::time::sleep(Duration::from_secs_f64(rand::random_range(1.0..4.0))).await;
  }
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
    let r: Value = serde_json::from_str(&data.str("reaction")?).ok()?;
    if !ids.is_empty() && !ids.contains(&channel) {
      return None;
    }
    let user = r.str("user_id").unwrap_or_default();
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
    let mut p = Post {
      id: r.str("post_id")?,
      kind: kind.into(),
      text: r.str("emoji_name").map(|e| format!(":{e}:")),
      author: Some(author),
      created_at: r.i64("create_at").and_then(social_core::text::from_millis),
      raw: Some(r),
      ..Post::default()
    };
    p.extra.insert("chat".into(), json!(channel));
    return Some(p);
  }
  let post: Value = serde_json::from_str(&data.str("post")?).ok()?;
  let channel = post.str("channel_id").unwrap_or(channel);
  if !ids.is_empty() && !ids.contains(&channel) {
    return None;
  }
  let mut p = posts::convert(api, &[post]).await.ok()?.pop()?;
  if kind != "message" {
    p.kind = kind.into();
  }
  Some(p)
}
