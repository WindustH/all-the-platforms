//! Satori resources → the shared model. Message content is a string of
//! message elements (`<at id/>`, `<img src/>`, `<quote>…</quote>` ...),
//! read here into text, media and the quoted message.

use std::sync::LazyLock;

use atp_core::chat;
use atp_core::text::from_millis;
use atp_core::{Collection, Media, MediaKind, Post, User, Value, ValueExt, json};
use regex::Regex;

static TAG: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r#"<(/?)([A-Za-z][\w:-]*)((?:\s+[\w:-]+(?:\s*=\s*"[^"]*")?)*)\s*(/?)>"#)
    .expect("valid regex")
});
static ATTR: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"([\w:-]+)\s*=\s*"([^"]*)""#).expect("valid regex"));

pub fn escape(text: &str) -> String {
  text
    .replace('&', "&amp;")
    .replace('<', "&lt;")
    .replace('>', "&gt;")
    .replace('"', "&quot;")
}

fn unescape(text: &str) -> String {
  text
    .replace("&lt;", "<")
    .replace("&gt;", ">")
    .replace("&quot;", "\"")
    .replace("&#39;", "'")
    .replace("&amp;", "&")
}

/// What a message's content holds.
#[derive(Default)]
pub struct Content {
  pub text: String,
  pub media: Vec<Media>,
  pub quote: Option<String>,
  pub mentions: Vec<String>,
}

/// One element tag of the content.
struct Tag {
  closing: bool,
  self_closing: bool,
  name: String,
  attrs: Vec<(String, String)>,
}

impl Tag {
  fn of(m: &regex::Captures) -> Self {
    Self {
      closing: !m[1].is_empty(),
      self_closing: !m[4].is_empty(),
      name: m[2].to_ascii_lowercase(),
      attrs: ATTR
        .captures_iter(&m[3])
        .map(|a| (a[1].to_owned(), unescape(&a[2])))
        .collect(),
    }
  }

  fn attr(&self, key: &str) -> Option<String> {
    self
      .attrs
      .iter()
      .find(|(n, _)| n == key)
      .map(|(_, v)| v.clone())
  }
}

/// Read message content: text with mentions and emoji written out, media
/// with web addresses, and the quoted message (whose own text is left out).
pub fn content(raw: &str) -> Content {
  let mut c = Content::default();
  let mut in_quote = 0usize;
  let mut last = 0;
  for m in TAG.captures_iter(raw) {
    let whole = m.get(0).expect("match");
    if in_quote == 0 {
      c.text.push_str(&unescape(&raw[last..whole.start()]));
    }
    last = whole.end();
    let tag = Tag::of(&m);
    if tag.name == "quote" {
      quote(&mut c, &tag, &mut in_quote);
    } else if in_quote > 0 {
      if tag.name == "message" && c.quote.is_none() {
        c.quote = tag.attr("id");
      }
    } else if tag.closing {
      if tag.name == "p" {
        c.text.push('\n');
      }
    } else {
      element(&mut c, &tag);
    }
  }
  if in_quote == 0 {
    c.text.push_str(&unescape(&raw[last..]));
  }
  c
}

/// `<quote>`: the quoted message's id, and how deep in quotes we are.
fn quote(c: &mut Content, tag: &Tag, depth: &mut usize) {
  if tag.closing {
    *depth = depth.saturating_sub(1);
    return;
  }
  if c.quote.is_none() {
    c.quote = tag.attr("id");
  }
  if !tag.self_closing {
    *depth += 1;
  }
}

/// An opening or self-closing element outside quotes.
fn element(c: &mut Content, tag: &Tag) {
  let name_or_id = || {
    tag
      .attr("name")
      .or_else(|| tag.attr("id"))
      .unwrap_or_default()
  };
  match tag.name.as_str() {
    "at" => {
      let who = match tag.attr("type").as_deref() {
        Some("all") => "all".to_owned(),
        Some("here") => "here".to_owned(),
        _ => name_or_id(),
      };
      c.text.push_str(&format!("@{who}"));
      if let Some(id) = tag.attr("id") {
        c.mentions.push(id);
      }
    }
    "sharp" => c.text.push_str(&format!("#{}", name_or_id())),
    "a" => {
      if let Some(h) = tag.attr("href") {
        c.text.push_str(&h);
      }
    }
    "br" => c.text.push('\n'),
    "emoji" | "face" => c.text.push_str(&format!(":{}:", name_or_id())),
    "img" | "image" | "audio" | "video" | "file" => media(c, tag),
    _ => {}
  }
}

/// A media element: a media item when it has a web address, else a marker.
fn media(c: &mut Content, tag: &Tag) {
  let kind = match tag.name.as_str() {
    "audio" => MediaKind::Audio,
    "video" => MediaKind::Video,
    "file" => MediaKind::File,
    _ => MediaKind::Image,
  };
  let src = tag
    .attr("src")
    .or_else(|| tag.attr("url"))
    .filter(|s| s.starts_with("http"));
  let Some(src) = src else {
    c.text.push_str(&format!("[{}]", tag.name));
    return;
  };
  let mut media = Media::new(kind, src);
  media.alt = tag.attr("title");
  media.width = tag.attr("width").and_then(|w| w.parse().ok());
  media.height = tag.attr("height").and_then(|h| h.parse().ok());
  c.media.push(media);
}

pub fn user(v: &Value) -> User {
  let id = v.str("id").unwrap_or_default();
  let mut u = User {
    name: v.first_str(&["nick", "name"]).unwrap_or_else(|| id.clone()),
    handle: v.str("name").filter(|n| Some(n) != v.str("nick").as_ref()),
    avatar: v.str("avatar"),
    id,
    raw: Some(v.clone()),
    ..User::default()
  };
  if v.bool("is_bot") == Some(true) {
    u.extra.insert("bot".into(), json!(true));
  }
  u
}

/// A guild member: its user with the guild's nickname and avatar.
pub fn member(v: &Value) -> User {
  let mut u = user(v.at("user"));
  if let Some(nick) = v.str("nick").or_else(|| v.str("name")) {
    u.extra.insert("user_name".into(), json!(u.name));
    u.name = nick;
  }
  if let Some(a) = v.str("avatar") {
    u.avatar = Some(a);
  }
  if let Some(t) = v.i64("joined_at").and_then(from_millis) {
    u.extra.insert("joined_at".into(), json!(t.to_string()));
  }
  u.raw = Some(v.clone());
  u
}

pub fn guild(v: &Value) -> Collection {
  let id = v.str("id").unwrap_or_default();
  let mut c = Collection {
    kind: "server".into(),
    name: v.str("name").unwrap_or_else(|| id.clone()),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if let Some(a) = v.str("avatar") {
    c.extra.insert("avatar".into(), json!(a));
  }
  c.id = format!("guild:{id}");
  c
}

pub fn channel(v: &Value) -> Collection {
  let kind = match v.u64("type") {
    Some(1) => "dm",
    Some(2) => "category",
    Some(3) => "voice",
    _ => "channel",
  };
  let mut c = Collection {
    id: v.str("id").unwrap_or_default(),
    kind: kind.into(),
    name: v.str("name").unwrap_or_default(),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if let Some(p) = v.str("parent_id") {
    c.extra.insert("parent".into(), json!(p));
  }
  c
}

pub fn message(v: &Value, channel: &str, chat_name: Option<&str>) -> Post {
  let channel = v.str("channel.id").unwrap_or_else(|| channel.to_owned());
  let id = v.str("id").unwrap_or_default();
  let c = content(&v.str("content").unwrap_or_default());
  let mut author = v.at("user").is_object().then(|| user(v.at("user")));
  if let (Some(a), Some(nick)) = (author.as_mut(), v.str("member.nick")) {
    a.name = nick;
  }
  let mut p = Post {
    id: chat::message_id(&channel, &id),
    kind: "message".into(),
    text: Some(c.text.trim().to_owned()).filter(|t| !t.is_empty()),
    author,
    created_at: v.i64("created_at").and_then(from_millis),
    updated_at: v.i64("updated_at").and_then(from_millis),
    media: c.media,
    raw: Some(v.clone()),
    ..Post::default()
  };
  let e = &mut p.extra;
  e.insert("chat".into(), json!(channel));
  if let Some(name) = chat_name
    .map(str::to_owned)
    .or_else(|| v.str("channel.name"))
  {
    e.insert("chat_name".into(), json!(name));
  }
  if let Some(g) = v.str("guild.id") {
    e.insert("guild".into(), json!(g));
  }
  e.insert("message_id".into(), json!(id));
  if let Some(q) = c.quote {
    e.insert("reply_to".into(), json!(chat::message_id(&channel, &q)));
  }
  if !c.mentions.is_empty() {
    e.insert("mentions".into(), json!(c.mentions));
  }
  p
}
