//! `listen`: events from the forward websocket. Messages (yours too) are
//! posts; recalls, emoji reactions, joins, leaves and pokes are posts with
//! their kind. A lost connection is re-opened.

use std::collections::HashSet;
use std::time::Duration;

use atp_core::output::{Stream, note};
use atp_core::text::from_secs;
use atp_core::{Error, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, Chat};

pub async fn listen(api: &Api, chats: &[String], out: &Stream) -> Result<()> {
  let e = api.require()?;
  let Some(url) = e.events().map(str::to_owned) else {
    return Err(
      Error::input("no OneBot websocket configured for events").with_hint(
        "enable the implementation's forward websocket and log in with \
           `--token 'http://…|TOKEN|ws://host:port'` (or set ONEBOT_WS_URL)",
      ),
    );
  };
  let wanted: HashSet<String> = chats
    .iter()
    .map(|c| refs::chat(c).map(|c| c.id()))
    .collect::<Result<_>>()?;
  let mut connected_once = false;
  loop {
    match api.connect(&url, e.token.as_deref()).await {
      Ok(mut ws) => {
        connected_once = true;
        while let Some(v) = ws.next_json().await? {
          if let Some(post) = event(&v)
            && (wanted.is_empty()
              || post
                .extra
                .get("chat")
                .and_then(Value::as_str)
                .is_some_and(|c| wanted.contains(c)))
          {
            out.post(&post);
          }
        }
        note("the OneBot websocket closed; reconnecting");
      }
      Err(err) if !connected_once => return Err(err),
      Err(err) => tracing::debug!("reconnect failed: {}", err.message),
    }
    tokio::time::sleep(Duration::from_secs(3)).await;
  }
}

fn event(v: &Value) -> Option<Post> {
  match v.str("post_type").as_deref()? {
    "message" | "message_sent" => Some(parse::message(v, None)),
    "notice" => notice(v),
    _ => None,
  }
}

fn notice(v: &Value) -> Option<Post> {
  let chat = parse::chat_of(v)?;
  let kind = match (
    v.str("notice_type").as_deref()?,
    v.str("sub_type").as_deref(),
  ) {
    ("group_recall" | "friend_recall", _) => "delete",
    ("group_msg_emoji_like", _) => "reaction",
    ("group_increase", _) => "join",
    ("group_decrease", _) => "leave",
    ("notify", Some("poke")) => "poke",
    ("group_upload", _) => "file",
    _ => return None,
  };
  let id = v.str("message_id").unwrap_or_default();
  let mut p = Post {
    id: format!("{}/{id}", chat.id()),
    kind: kind.into(),
    created_at: v.i64("time").and_then(from_secs),
    author: v.str("user_id").map(|u| atp_core::User {
      avatar: Some(parse::avatar(&u)),
      name: u.clone(),
      id: u,
      ..atp_core::User::default()
    }),
    raw: Some(v.clone()),
    ..Post::default()
  };
  p.extra.insert("chat".into(), json!(chat.id()));
  if !id.is_empty() {
    p.extra.insert("message_id".into(), json!(id));
  }
  if let Some(likes) = v.as_object().and_then(|o| o.get("likes")) {
    p.extra.insert("emoji".into(), likes.clone());
  }
  if let Some(op) = v.str("operator_id") {
    p.extra.insert("operator".into(), json!(op));
  }
  if let Chat::Group(_) = chat
    && kind == "file"
  {
    p.text = v.str("file.name");
  }
  Some(p)
}
