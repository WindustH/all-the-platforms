//! Slack payloads → the shared model. Messages are posts with
//! `CHANNEL/TS` ids and permalinks; files become media fetched with the
//! token; mrkdwn (`<@U…>`, `<#C…|name>`, `<url|label>`) is made readable.

use std::collections::HashMap;
use std::sync::LazyLock;

use jiff::Timestamp;
use regex::Regex;
use social_core::chat;
use social_core::{
  Collection, Comment, Media, MediaKind, Metrics, Post, User, Value, ValueExt, json,
};

use crate::chats::ChatInfo;
use crate::users::Brief;

static ANGLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<([^<>]+)>").expect("valid"));
static MENTION: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"<@([UW][A-Z0-9]+)").expect("valid"));

/// What turning payloads into posts needs to know.
pub struct View<'a> {
  pub team_url: Option<&'a str>,
  pub token: &'a str,
  pub users: &'a HashMap<String, Brief>,
  pub chats: &'a HashMap<String, ChatInfo>,
}

/// `1700000000.000100` → a timestamp.
pub fn ts_time(ts: &str) -> Option<Timestamp> {
  let (secs, frac) = ts.split_once('.').unwrap_or((ts, "0"));
  let micros: i64 = format!("{frac:0<6}")[..6].parse().ok()?;
  Timestamp::new(secs.parse().ok()?, (micros * 1000) as i32).ok()
}

/// `https://acme.slack.com/archives/C1/p1700000000000100[?thread_ts=…&cid=C1]`.
pub fn permalink(team_url: &str, channel: &str, ts: &str, thread: Option<&str>) -> String {
  let base = team_url.trim_end_matches('/');
  let p = ts.replace('.', "");
  match thread.filter(|t| *t != ts) {
    Some(t) => format!("{base}/archives/{channel}/p{p}?thread_ts={t}&cid={channel}"),
    None => format!("{base}/archives/{channel}/p{p}"),
  }
}

/// User ids a list of messages mentions or is written by.
pub fn user_ids(messages: &[Value]) -> Vec<String> {
  let mut ids = Vec::new();
  for m in messages {
    ids.extend(m.str("user"));
    ids.extend(m.str("message.user"));
    let text = m.str("text").unwrap_or_default();
    ids.extend(MENTION.captures_iter(&text).map(|c| c[1].to_owned()));
  }
  ids.sort();
  ids.dedup();
  ids
}

/// mrkdwn as plain text.
pub fn readable(text: &str, view: &View) -> String {
  let out = ANGLE.replace_all(text, |c: &regex::Captures| {
    let inner = &c[1];
    let (target, label) = match inner.split_once('|') {
      Some((t, l)) => (t, Some(l)),
      None => (inner, None),
    };
    if let Some(id) = target.strip_prefix('@') {
      let name = label
        .map(str::to_owned)
        .or_else(|| view.users.get(id).map(|b| b.name.clone()))
        .unwrap_or_else(|| id.to_owned());
      return format!("@{}", name.trim_start_matches('@'));
    }
    if let Some(id) = target.strip_prefix('#') {
      let name = label
        .map(str::to_owned)
        .or_else(|| view.chats.get(id).map(|c| c.name.clone()))
        .unwrap_or_else(|| id.to_owned());
      return format!("#{name}");
    }
    if let Some(special) = target.strip_prefix('!') {
      return match label {
        Some(l) => l.to_owned(),
        None => format!("@{}", special.split('^').next().unwrap_or(special)),
      };
    }
    let url = target.strip_prefix("mailto:").unwrap_or(target);
    match label {
      Some(l) if l != url && l != target => format!("{l} ({url})"),
      _ => url.to_owned(),
    }
  });
  out
    .replace("&lt;", "<")
    .replace("&gt;", ">")
    .replace("&amp;", "&")
}

fn author(v: &Value, view: &View) -> Option<User> {
  if let Some(id) = v.str("user") {
    if let Some(b) = view.users.get(&id) {
      return Some(b.user(&id));
    }
    let p = v.at("user_profile");
    return Some(User {
      name: p
        .first_str(&["display_name", "real_name", "name"])
        .unwrap_or_else(|| id.clone()),
      handle: p.str("name"),
      avatar: p.str("image_72"),
      id,
      ..User::default()
    });
  }
  let name = v.first_str(&["bot_profile.name", "username"])?;
  let mut u = User {
    id: v
      .first_str(&["bot_id", "bot_profile.id"])
      .unwrap_or_default(),
    name,
    avatar: v.str("bot_profile.icons.image_72"),
    ..User::default()
  };
  u.extra.insert("bot".into(), json!(true));
  Some(u)
}

/// A file as media, downloaded with the token.
fn file(f: &Value, token: &str) -> Option<Media> {
  let url = f.first_str(&["url_private_download", "url_private"])?;
  let mime = f.str("mimetype").unwrap_or_default();
  let mut m = Media::new(media_kind(&mime), url);
  m.width = f.u64("original_w").map(|w| w as u32);
  m.height = f.u64("original_h").map(|h| h as u32);
  m.size = f.u64("size");
  m.alt = f.first_str(&["title", "name"]);
  m.headers = vec![("authorization".into(), format!("Bearer {token}"))];
  Some(m)
}

fn media_kind(mime: &str) -> MediaKind {
  match mime.split('/').next() {
    _ if mime == "image/gif" => MediaKind::Gif,
    Some("image") => MediaKind::Image,
    Some("video") => MediaKind::Video,
    Some("audio") => MediaKind::Audio,
    _ => MediaKind::File,
  }
}

/// A message of `channel`.
pub fn message(v: &Value, channel: &str, view: &View) -> Post {
  let ts = v.str("ts").unwrap_or_default();
  let thread = v.str("thread_ts");
  let reactions = reactions(v);
  let total: u64 = v
    .list("reactions")
    .iter()
    .filter_map(|r| r.u64("count"))
    .sum();
  let subtype = v.str("subtype");
  let kind = match subtype.as_deref() {
    None | Some("bot_message" | "thread_broadcast" | "file_share" | "me_message") => "message",
    Some(other) => other,
  };
  let mut post = Post {
    id: chat::message_id(channel, &ts),
    kind: kind.into(),
    text: v
      .str("text")
      .map(|t| readable(&t, view))
      .filter(|t| !t.is_empty()),
    url: view
      .team_url
      .map(|base| permalink(base, channel, &ts, thread.as_deref())),
    author: author(v, view),
    created_at: ts_time(&ts),
    updated_at: v.str("edited.ts").and_then(|t| ts_time(&t)),
    metrics: Metrics {
      likes: (!reactions.is_empty()).then_some(total),
      comments: v.u64("reply_count"),
      ..Metrics::default()
    },
    media: v
      .list("files")
      .iter()
      .filter_map(|f| file(f, view.token))
      .collect(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  let reply = thread.as_deref().filter(|t| *t != ts);
  message_extra(&mut post, v, channel, &ts, reply, reactions, view);
  post
}

fn reactions(v: &Value) -> Vec<Value> {
  v.list("reactions")
    .iter()
    .map(|r| json!({"emoji": r.str("name"), "count": r.u64("count")}))
    .collect()
}

/// What a message carries beyond the shared fields: chat, thread,
/// reactions, files, attachments, pins.
fn message_extra(
  post: &mut Post,
  v: &Value,
  channel: &str,
  ts: &str,
  reply: Option<&str>,
  reactions: Vec<Value>,
  view: &View,
) {
  let e = &mut post.extra;
  e.insert("chat".into(), json!(channel));
  if let Some(c) = view.chats.get(channel) {
    e.insert("chat_name".into(), json!(c.name));
  }
  e.insert("message_id".into(), json!(ts));
  if let Some(t) = reply {
    e.insert("thread_ts".into(), json!(t));
    e.insert("reply_to".into(), json!(chat::message_id(channel, t)));
  }
  if !reactions.is_empty() {
    e.insert("reactions".into(), json!(reactions));
  }
  let files: Vec<Value> = v
    .list("files")
    .iter()
    .map(
      |f| json!({"name": f.str("name"), "title": f.str("title"), "permalink": f.str("permalink")}),
    )
    .collect();
  if !files.is_empty() {
    e.insert("files".into(), json!(files));
  }
  let attachments: Vec<Value> = v
    .list("attachments")
    .iter()
    .filter_map(|a| a.first_str(&["fallback", "text", "title"]))
    .map(|t| json!(readable(&t, view)))
    .collect();
  if !attachments.is_empty() {
    e.insert("attachments".into(), json!(attachments));
  }
  if let Some(n) = v.u64("reply_users_count") {
    e.insert("repliers".into(), json!(n));
  }
  if !v.list("pinned_to").is_empty() {
    e.insert("pinned".into(), json!(true));
  }
  if let Some(s) = v.str("subtype") {
    e.insert("subtype".into(), json!(s));
  }
}

/// A thread reply as a comment.
pub fn comment(v: &Value, channel: &str, view: &View) -> Comment {
  let post = message(v, channel, view);
  Comment {
    id: post.id,
    author: post.author,
    text: post.text.unwrap_or_default(),
    created_at: post.created_at,
    likes: post.metrics.likes,
    extra: post.extra,
    raw: post.raw,
    ..Comment::default()
  }
}

/// A conversation.
pub fn chat(v: &Value, view: &View) -> Collection {
  let id = v.str("id").unwrap_or_default();
  let info = crate::chats::info_of(v, view.users);
  let mut c = Collection {
    url: view
      .team_url
      .map(|base| format!("{}/archives/{id}", base.trim_end_matches('/'))),
    kind: info.kind.clone(),
    name: info.name,
    description: v
      .first_str(&["topic.value", "purpose.value"])
      .filter(|d| !d.is_empty()),
    followers: v.u64("num_members"),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  for (key, path) in [
    ("member", "is_member"),
    ("archived", "is_archived"),
    ("general", "is_general"),
    ("shared", "is_shared"),
  ] {
    if v.bool(path) == Some(true) {
      c.extra.insert(key.into(), json!(true));
    }
  }
  if let Some(n) = v.u64("unread_count_display").filter(|n| *n > 0) {
    c.extra.insert("unread".into(), json!(n));
  }
  if let Some(u) = v.str("user") {
    c.extra.insert("user".into(), json!(u));
  }
  if let Some(t) = v.i64("created").and_then(social_core::text::from_secs) {
    c.extra.insert("created_at".into(), json!(t.to_string()));
  }
  c.id = id;
  c
}
