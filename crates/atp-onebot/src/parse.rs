//! OneBot payloads → the shared model. Messages arrive as segment arrays
//! (`[{type, data}]`) or as CQ-code strings; both are read.

use atp_core::text::from_secs;
use atp_core::{Collection, Media, MediaKind, Post, User, Value, ValueExt, json};

use crate::refs::Chat;

/// Segments of a message, from either form.
pub fn segments(message: &Value) -> Vec<Value> {
  match message {
    Value::Array(a) => a.clone(),
    Value::String(s) => cq(s),
    _ => Vec::new(),
  }
}

/// `text[CQ:type,k=v,...]text` → segments.
fn cq(s: &str) -> Vec<Value> {
  let mut out = Vec::new();
  let mut rest = s;
  while let Some(start) = rest.find("[CQ:") {
    if start > 0 {
      out.push(json!({"type": "text", "data": {"text": unescape(&rest[..start])}}));
    }
    let Some(end) = rest[start..].find(']') else {
      break;
    };
    let body = &rest[start + 4..start + end];
    let mut parts = body.split(',');
    let kind = parts.next().unwrap_or_default();
    let data: serde_json::Map<String, Value> = parts
      .filter_map(|p| p.split_once('='))
      .map(|(k, v)| (k.to_owned(), Value::from(unescape(v))))
      .collect();
    out.push(json!({"type": kind, "data": data}));
    rest = &rest[start + end + 1..];
  }
  if !rest.is_empty() {
    out.push(json!({"type": "text", "data": {"text": unescape(rest)}}));
  }
  out
}

fn unescape(s: &str) -> String {
  s.replace("&#91;", "[")
    .replace("&#93;", "]")
    .replace("&#44;", ",")
    .replace("&amp;", "&")
}

/// What a message says, which media it carries and which message it answers.
pub struct Content {
  pub text: String,
  pub media: Vec<Media>,
  pub reply_to: Option<String>,
  pub mentions: Vec<String>,
}

pub fn content(message: &Value) -> Content {
  let mut c = Content {
    text: String::new(),
    media: Vec::new(),
    reply_to: None,
    mentions: Vec::new(),
  };
  for seg in segments(message) {
    let d = seg.at("data");
    let url = d
      .first_str(&["url", "file"])
      .filter(|u| u.starts_with("http"));
    match seg.str("type").as_deref().unwrap_or_default() {
      "text" => c.text.push_str(&d.str("text").unwrap_or_default()),
      "at" => {
        let qq = d.str("qq").unwrap_or_default();
        let name = d.str("name").filter(|n| !n.is_empty());
        c.text.push_str(&match (&name, qq.as_str()) {
          (_, "all") => "@全体成员".to_owned(),
          (Some(n), _) => format!("@{n}"),
          (None, q) => format!("@{q}"),
        });
        c.mentions.push(qq);
      }
      "face" => c
        .text
        .push_str(&format!("[face:{}]", d.str("id").unwrap_or_default())),
      "mface" | "marketface" => c
        .text
        .push_str(&d.str("summary").unwrap_or_else(|| "[sticker]".into())),
      "reply" => c.reply_to = d.str("id"),
      "image" => {
        if let Some(u) = url {
          let gif = d.str("summary").is_some_and(|s| s.contains("动画"));
          c.media.push(Media::new(
            if gif {
              MediaKind::Gif
            } else {
              MediaKind::Image
            },
            u,
          ));
        } else {
          c.text.push_str("[image]");
        }
      }
      "record" => match url {
        Some(u) => c.media.push(Media::new(MediaKind::Audio, u)),
        None => c.text.push_str("[voice]"),
      },
      "video" => match url {
        Some(u) => c.media.push(Media::video(u)),
        None => c.text.push_str("[video]"),
      },
      "file" => {
        let name = d.first_str(&["name", "file"]).unwrap_or_default();
        match url {
          Some(u) => {
            let mut m = Media::new(MediaKind::File, u);
            m.alt = Some(name);
            m.size = d.u64("file_size");
            c.media.push(m);
          }
          None => c.text.push_str(&format!("[file: {name}]")),
        }
      }
      "json" | "xml" | "ark" => c.text.push_str("[card]"),
      "forward" => c.text.push_str("[forwarded messages]"),
      "markdown" => c.text.push_str(&d.str("content").unwrap_or_default()),
      "poke" => c.text.push_str("[poke]"),
      "dice" | "rps" => c
        .text
        .push_str(&format!("[{}]", seg.str("type").unwrap_or_default())),
      other => c.text.push_str(&format!("[{other}]")),
    }
  }
  c
}

fn sender(v: &Value) -> Option<User> {
  let s = v.at("sender");
  let id = s.str("user_id").or_else(|| v.str("user_id"))?;
  let nickname = s.str("nickname").unwrap_or_default();
  let card = s.str("card").filter(|c| !c.is_empty());
  let mut u = User {
    name: card.clone().unwrap_or_else(|| nickname.clone()),
    handle: card.is_some().then_some(nickname).filter(|n| !n.is_empty()),
    avatar: Some(avatar(&id)),
    id,
    ..User::default()
  };
  if let Some(role) = s.str("role") {
    u.extra.insert("role".into(), json!(role));
  }
  Some(u)
}

pub fn avatar(qq: &str) -> String {
  format!("https://q1.qlogo.cn/g?b=qq&nk={qq}&s=640")
}

/// The chat a message or event belongs to.
pub fn chat_of(v: &Value) -> Option<Chat> {
  if let Some(g) = v.str("group_id") {
    return Some(Chat::Group(g));
  }
  match v.str("message_type").as_deref() {
    Some("private") => {
      // In messages you sent, `user_id` is you; the other side is `target_id`.
      v.first_str(&["target_id", "user_id"]).map(Chat::Private)
    }
    _ => v.str("user_id").map(Chat::Private),
  }
}

/// A message (`get_msg`, history items, `message` / `message_sent` events).
pub fn message(v: &Value, chat: Option<&Chat>) -> Post {
  let chat = chat.cloned().or_else(|| chat_of(v));
  let id = v.str("message_id").unwrap_or_default();
  let c = content(v.at("message"));
  let text = if c.text.trim().is_empty() {
    v.str("raw_message").filter(|_| c.media.is_empty())
  } else {
    Some(c.text.trim().to_owned())
  };
  let chat_id = chat.as_ref().map(Chat::id);
  let mut p = Post {
    id: match &chat_id {
      Some(ch) => format!("{ch}/{id}"),
      None => id.clone(),
    },
    kind: "message".into(),
    text,
    author: sender(v),
    created_at: v.i64("time").and_then(from_secs),
    media: c.media,
    raw: Some(v.clone()),
    ..Post::default()
  };
  let e = &mut p.extra;
  if let Some(ch) = &chat_id {
    e.insert("chat".into(), json!(ch));
  }
  e.insert("message_id".into(), json!(id));
  if let Some(seq) = v.first_str(&["message_seq", "real_seq"]) {
    e.insert("message_seq".into(), json!(seq));
  }
  if let (Some(r), Some(ch)) = (c.reply_to, &chat_id) {
    e.insert("reply_to".into(), json!(format!("{ch}/{r}")));
  }
  if !c.mentions.is_empty() {
    e.insert("mentions".into(), json!(c.mentions));
  }
  p
}

pub fn group(v: &Value) -> Collection {
  let id = v.str("group_id").unwrap_or_default();
  let mut c = Collection {
    kind: "group".into(),
    name: v.str("group_name").unwrap_or_else(|| id.clone()),
    description: v.str("group_memo").filter(|m| !m.is_empty()),
    followers: v.u64("member_count"),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if let Some(n) = v.u64("max_member_count") {
    c.extra.insert("max_members".into(), json!(n));
  }
  c.extra.insert("group_id".into(), json!(id));
  c.id = format!("group:{id}");
  c
}

pub fn friend(v: &Value) -> Collection {
  let id = v.str("user_id").unwrap_or_default();
  let nickname = v.str("nickname").unwrap_or_default();
  let remark = v.str("remark").filter(|r| !r.is_empty() && *r != nickname);
  let mut c = Collection {
    kind: "dm".into(),
    name: remark.clone().unwrap_or_else(|| nickname.clone()),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if remark.is_some() {
    c.extra.insert("nickname".into(), json!(nickname));
  }
  c.extra.insert("user_id".into(), json!(id));
  c.id = format!("user:{id}");
  c
}

/// Group members, strangers and the logged-in account.
pub fn user(v: &Value) -> User {
  let id = v.str("user_id").unwrap_or_default();
  let nickname = v.str("nickname").unwrap_or_default();
  let card = v.str("card").filter(|c| !c.is_empty());
  let mut u = User {
    name: card.clone().unwrap_or_else(|| nickname.clone()),
    handle: card.is_some().then_some(nickname).filter(|n| !n.is_empty()),
    avatar: Some(avatar(&id)),
    bio: v.first_str(&["long_nick", "sign"]),
    location: v.str("area").filter(|a| !a.is_empty()),
    id,
    raw: Some(v.clone()),
    ..User::default()
  };
  for key in [
    "role",
    "title",
    "level",
    "sex",
    "age",
    "qid",
    "join_time",
    "last_sent_time",
  ] {
    if let Some(val) = v
      .str(key)
      .filter(|s| !s.is_empty() && s != "0" && s != "unknown")
    {
      u.extra.insert(key.into(), json!(val));
    }
  }
  u
}
