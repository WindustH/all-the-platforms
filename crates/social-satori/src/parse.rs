//! Satori resources → the shared model. Message content is a string of
//! message elements (`<at id/>`, `<img src/>`, `<quote>…</quote>` ...),
//! read here into text, media and the quoted message.

use std::sync::LazyLock;

use regex::Regex;
use social_core::text::from_millis;
use social_core::{Collection, Media, MediaKind, Post, User, Value, ValueExt, json};

static TAG: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r#"<(/?)([A-Za-z][\w:-]*)((?:\s+[\w:-]+(?:\s*=\s*"[^"]*")?)*)\s*(/?)>"#)
    .expect("valid regex")
});
static ATTR: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"([\w:-]+)\s*=\s*"([^"]*)""#).expect("valid regex"));

pub fn escape(s: &str) -> String {
  s.replace('&', "&amp;")
    .replace('<', "&lt;")
    .replace('>', "&gt;")
    .replace('"', "&quot;")
}

fn unescape(s: &str) -> String {
  s.replace("&lt;", "<")
    .replace("&gt;", ">")
    .replace("&quot;", "\"")
    .replace("&#39;", "'")
    .replace("&amp;", "&")
}

pub struct Content {
  pub text: String,
  pub media: Vec<Media>,
  pub quote: Option<String>,
  pub mentions: Vec<String>,
}

pub fn content(raw: &str) -> Content {
  let mut c = Content {
    text: String::new(),
    media: Vec::new(),
    quote: None,
    mentions: Vec::new(),
  };
  let mut in_quote = 0usize;
  let mut last = 0;
  for m in TAG.captures_iter(raw) {
    let whole = m.get(0).expect("match");
    if in_quote == 0 {
      c.text.push_str(&unescape(&raw[last..whole.start()]));
    }
    last = whole.end();
    let closing = !m[1].is_empty();
    let name = m[2].to_ascii_lowercase();
    let attrs: Vec<(String, String)> = ATTR
      .captures_iter(&m[3])
      .map(|a| (a[1].to_owned(), unescape(&a[2])))
      .collect();
    let attr = |k: &str| attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    let self_closing = !m[4].is_empty();
    if name == "quote" {
      if closing {
        in_quote = in_quote.saturating_sub(1);
      } else {
        if c.quote.is_none() {
          c.quote = attr("id");
        }
        if !self_closing {
          in_quote += 1;
        }
      }
      continue;
    }
    if in_quote > 0 {
      if name == "message" && c.quote.is_none() {
        c.quote = attr("id");
      }
      continue;
    }
    if closing {
      if name == "p" {
        c.text.push('\n');
      }
      continue;
    }
    match name.as_str() {
      "at" => {
        let who = match attr("type").as_deref() {
          Some("all") => "all".to_owned(),
          Some("here") => "here".to_owned(),
          _ => attr("name").or_else(|| attr("id")).unwrap_or_default(),
        };
        c.text.push_str(&format!("@{who}"));
        if let Some(id) = attr("id") {
          c.mentions.push(id);
        }
      }
      "sharp" => c.text.push_str(&format!(
        "#{}",
        attr("name").or_else(|| attr("id")).unwrap_or_default()
      )),
      "a" => {
        if let Some(h) = attr("href") {
          c.text.push_str(&h);
        }
      }
      "br" => c.text.push('\n'),
      "emoji" | "face" => c.text.push_str(&format!(
        ":{}:",
        attr("name").or_else(|| attr("id")).unwrap_or_default()
      )),
      "img" | "image" | "audio" | "video" | "file" => {
        let kind = match name.as_str() {
          "audio" => MediaKind::Audio,
          "video" => MediaKind::Video,
          "file" => MediaKind::File,
          _ => MediaKind::Image,
        };
        match attr("src")
          .or_else(|| attr("url"))
          .filter(|s| s.starts_with("http"))
        {
          Some(src) => {
            let mut media = Media::new(kind, src);
            media.alt = attr("title");
            media.width = attr("width").and_then(|w| w.parse().ok());
            media.height = attr("height").and_then(|h| h.parse().ok());
            c.media.push(media);
          }
          None => c.text.push_str(&format!("[{name}]")),
        }
      }
      _ => {}
    }
  }
  if in_quote == 0 {
    c.text.push_str(&unescape(&raw[last..]));
  }
  c
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
    id: format!("{channel}/{id}"),
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
    e.insert("reply_to".into(), json!(format!("{channel}/{q}")));
  }
  if !c.mentions.is_empty() {
    e.insert("mentions".into(), json!(c.mentions));
  }
  p
}
