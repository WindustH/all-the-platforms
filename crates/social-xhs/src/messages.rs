//! Private messages (私信), read-only: the conversation list and a
//! conversation's history, as the web message page loads them.
//!
//! Sending goes over the message websocket as RSA-encrypted protobuf frames,
//! so it is not offered. A conversation is the other account's user id; a
//! message's id is `<chat>/<message id>`.

use std::collections::BTreeMap;
use std::time::Duration;

use social_core::chat;
use social_core::text::from_millis;
use social_core::{Collection, Error, Page, PageReq, Post, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::refs::user_url;

const CHATS: &str = "/api/im/web/v3/chats";
const HISTORY: &str = "/api/im/web/messages/history";
/// Nicknames of the conversations `chats` listed, for naming messages.
const NAMES: &str = "chat-names";
const NAMES_TTL: Duration = Duration::from_secs(30 * 86400);

/// Conversations, most recent first (page-numbered).
pub async fn chats(api: &Api, req: &PageReq) -> Result<Page<Collection>> {
  api.require_login()?;
  let page = req.number_or(0);
  let limit = req.size_within(100);
  let (page_s, limit_s) = (page.to_string(), limit.to_string());
  let params = [
    ("limit", limit_s.as_str()),
    ("complete", "true"),
    ("page", page_s.as_str()),
    ("source", "pc"),
  ];
  let data = api.get(CHATS, &params).await?;
  let items: Vec<Collection> = data.list("chats").iter().filter_map(chat).collect();
  remember_names(api, &items);
  let seen = (page + 1) * limit as u64;
  let next = (!items.is_empty() && data.u64("total").is_some_and(|t| seen < t))
    .then(|| (page + 1).to_string());
  Ok(Page::new(items, next))
}

fn chat(v: &Value) -> Option<Collection> {
  let id = v.str("chat_user_id")?;
  let info = v.at("info");
  let mut collection = Collection {
    kind: "dm".into(),
    name: info
      .first_str(&["nickname", "user_name"])
      .unwrap_or_else(|| id.clone()),
    description: v.str("last_msg_content"),
    url: Some(user_url(&id)),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  for (key, value) in [
    (
      "last_message_at",
      v.i64("last_msg_time")
        .and_then(from_millis)
        .map(|t| Value::from(t.to_string())),
    ),
    ("friend", info.get("is_friend").cloned()),
    ("follow_status", info.get("follow_status").cloned()),
    ("official", info.get("is_official").cloned()),
    ("avatar", info.get("avatar").cloned()),
  ] {
    if let Some(value) = value.filter(|v| !v.is_null()) {
      collection.extra.insert(key.into(), value);
    }
  }
  collection.id = id;
  Some(collection)
}

/// One page of a conversation, newest first; the cursor is the store id of
/// the oldest message seen.
pub async fn messages(api: &Api, arg: &str, req: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let peer = chat_id(arg)?;
  let limit = req.size_within(50);
  let last = req.cursor.clone().unwrap_or_else(|| "0".into());
  let limit_s = limit.to_string();
  let params = [
    ("chat_user_id", peer.as_str()),
    ("last_id", last.as_str()),
    ("start_id", "0"),
    ("limit", limit_s.as_str()),
  ];
  let data = api.get(HISTORY, &params).await?;
  let name = names(api).remove(&peer);
  let rows = data.list("out_message_list");
  let items: Vec<Post> = rows
    .iter()
    .filter_map(|m| message(m, &peer, name.as_deref()))
    .collect();
  let oldest = rows.iter().filter_map(|m| m.u64("store_id")).min();
  let next = oldest
    .filter(|&o| o > 1 && rows.len() >= limit)
    .map(|o| o.to_string());
  Ok(Page::new(items, next))
}

fn names(api: &Api) -> BTreeMap<String, String> {
  api
    .ctx
    .store
    .cache_get(NAMES, NAMES_TTL)
    .unwrap_or_default()
}

fn remember_names(api: &Api, chats: &[Collection]) {
  let mut map = names(api);
  for chat in chats {
    map.insert(chat.id.clone(), chat.name.clone());
  }
  api.ctx.store.cache_put(NAMES, &map);
}

fn message(item: &Value, peer: &str, peer_name: Option<&str>) -> Option<Post> {
  let id = item.str("id")?;
  let sender = item.str("sender_id").unwrap_or_default();
  // `content` is itself JSON: `{content, content_type, …}`.
  let body: Value = item
    .str("content")
    .and_then(|s| serde_json::from_str(&s).ok())
    .unwrap_or(Value::Null);
  let content_type = body.i64("content_type");
  let text = match content_type {
    Some(1) | None => body.str("content").or_else(|| item.str("content")),
    Some(t) => Some(
      body
        .str("content")
        .unwrap_or_else(|| format!("[message type {t}]")),
    ),
  };
  let revoked = item.bool("revoked") == Some(true);
  let mut post = Post {
    id: chat::message_id(peer, &id),
    kind: if revoked { "revoked" } else { "message" }.into(),
    text,
    author: Some(User {
      name: if sender == peer {
        peer_name.unwrap_or(peer).to_owned()
      } else {
        "you".to_owned()
      },
      url: Some(user_url(&sender)),
      id: sender,
      ..User::default()
    }),
    created_at: item.i64("created_at").and_then(from_millis),
    raw: Some(item.clone()),
    ..Post::default()
  };
  post.extra.insert("chat".into(), Value::from(peer));
  if let Some(t) = content_type {
    post.extra.insert("content_type".into(), Value::from(t));
  }
  Some(post)
}

/// A conversation: the other account's user id, a profile link, or a
/// message id (`<chat>/<id>`).
fn chat_id(arg: &str) -> Result<String> {
  let arg = arg.trim();
  let hex24 = |s: &&str| s.len() == 24 && s.bytes().all(|b| b.is_ascii_hexdigit());
  let id = if arg.starts_with("http") {
    let path = arg.split(['?', '#']).next().unwrap_or(arg);
    path.rsplit('/').find(hex24)
  } else {
    arg.split('/').next().filter(hex24)
  };
  id.map(str::to_owned).ok_or_else(|| {
    Error::input(format!(
      "not a conversation: {arg} (expected a user id from `social xhs chats`)"
    ))
  })
}
