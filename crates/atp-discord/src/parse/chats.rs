//! Guilds and channels as collections.

use atp_core::{Collection, Value, ValueExt, json};

use super::*;

pub fn guild(v: &Value) -> Collection {
  let id = v.str("id").unwrap_or_default();
  let mut c = Collection {
    kind: "server".into(),
    name: v.str("name").unwrap_or_default(),
    description: v.str("description"),
    url: Some(format!("{WEB}/channels/{id}")),
    followers: v.u64("approximate_member_count"),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if let Some(n) = v.u64("approximate_presence_count") {
    c.extra.insert("online".into(), json!(n));
  }
  if v.bool("owner") == Some(true) {
    c.extra.insert("owner".into(), json!(true));
  }
  if let Some(hash) = v.str("icon") {
    c.extra
      .insert("icon".into(), json!(format!("{CDN}/icons/{id}/{hash}.png")));
  }
  if let Some(t) = snowflake_time(&id) {
    c.extra.insert("created_at".into(), json!(t.to_string()));
  }
  c.id = id;
  c
}

/// `kind` of a channel type.
pub fn channel_kind(code: u64) -> &'static str {
  match code {
    0 => "channel",
    1 => "dm",
    2 => "voice",
    3 => "group",
    4 => "category",
    5 => "announcement",
    10 | 11 => "thread",
    12 => "private-thread",
    13 => "stage",
    14 => "directory",
    15 => "forum",
    16 => "media",
    _ => "channel",
  }
}

/// What a channel is called: `#name`, the other people of a direct chat, or
/// a group's name.
pub fn chat_name(v: &Value) -> String {
  let t = v.u64("type").unwrap_or_default();
  if let Some(name) = v.str("name").filter(|n| !n.is_empty()) {
    return match t {
      1 | 3 => name,
      _ => format!("#{name}"),
    };
  }
  let people: Vec<String> = v
    .list("recipients")
    .iter()
    .filter_map(|r| r.first_str(&["global_name", "username"]))
    .collect();
  if people.is_empty() {
    v.str("id").unwrap_or_default()
  } else {
    format!("@{}", people.join(", @"))
  }
}

pub fn channel(v: &Value) -> Collection {
  let id = v.str("id").unwrap_or_default();
  let t = v.u64("type").unwrap_or_default();
  let guild = v.str("guild_id");
  let mut c = Collection {
    kind: channel_kind(t).into(),
    name: chat_name(v),
    description: v.str("topic").filter(|t| !t.is_empty()),
    url: Some(permalink(guild.as_deref().unwrap_or("@me"), &id, None)),
    items: v.u64("message_count"),
    followers: v
      .u64("member_count")
      .or_else(|| (matches!(t, 1 | 3)).then(|| v.list("recipients").len() as u64 + 1)),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if let Some(g) = guild {
    c.extra.insert("guild".into(), json!(g));
  }
  if let Some(p) = v.str("parent_id") {
    c.extra.insert("parent".into(), json!(p));
  }
  if let Some(t) = v.str("last_message_id").and_then(|m| snowflake_time(&m)) {
    c.extra
      .insert("last_message_at".into(), json!(t.to_string()));
  }
  if let Some(s) = v.u64("rate_limit_per_user").filter(|s| *s > 0) {
    c.extra.insert("slowmode_seconds".into(), json!(s));
  }
  if v.bool("nsfw") == Some(true) {
    c.extra.insert("nsfw".into(), json!(true));
  }
  if v.bool("thread_metadata.archived") == Some(true) {
    c.extra.insert("archived".into(), json!(true));
  }
  c.id = id;
  c
}
