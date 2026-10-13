//! Feishu payloads → the shared model. Message `content` is a JSON string
//! whose shape depends on `msg_type` (text, post, image, file, audio, media,
//! sticker, interactive cards, shared chats ...).

use atp_core::text::from_millis;
use atp_core::{Collection, Media, MediaKind, Post, User, Value, ValueExt, json};

/// A message from a list or a get call (or an event normalized to that
/// shape by [`event_message`]); `base` is the API host for media links.
pub fn message(v: &Value, base: &str) -> Post {
  let id = v.str("message_id").unwrap_or_default();
  let msg_type = v.str("msg_type").unwrap_or_default();
  let content: Value = v
    .str("body.content")
    .and_then(|c| serde_json::from_str(&c).ok())
    .unwrap_or_default();
  let (text, media) = body(base, &id, &msg_type, &content);
  let sender = v.at("sender");
  let author = sender.str("id").map(|sid| User {
    id: sid,
    name: String::new(),
    extra: [(
      "sender_type".to_owned(),
      Value::from(sender.str("sender_type").unwrap_or_default()),
    )]
    .into_iter()
    .collect(),
    ..User::default()
  });
  let mut post = Post {
    id,
    kind: if v.bool("deleted") == Some(true) {
      "deleted".into()
    } else {
      "message".into()
    },
    text: Some(text).filter(|t| !t.is_empty()),
    author,
    created_at: v.i64("create_time").and_then(from_millis),
    updated_at: (v.bool("updated") == Some(true))
      .then(|| v.i64("update_time").and_then(from_millis))
      .flatten(),
    media,
    raw: Some(v.clone()),
    ..Post::default()
  };
  for (key, path) in [
    ("chat", "chat_id"),
    ("msg_type", "msg_type"),
    ("reply_to", "parent_id"),
    ("root", "root_id"),
    ("thread", "thread_id"),
  ] {
    if let Some(value) = v.str(path) {
      post.extra.insert(key.into(), value.into());
    }
  }
  let mentions: Vec<Value> = v
    .list("mentions")
    .iter()
    .map(|m| json!({ "key": m.str("key"), "id": m.str("id"), "name": m.str("name") }))
    .collect();
  if !mentions.is_empty() {
    post.extra.insert("mentions".into(), mentions.into());
  }
  post
}

/// Text and media of a message body. Media URLs point at the message
/// resource endpoint; downloads add the app's token.
fn body(base: &str, id: &str, msg_type: &str, c: &Value) -> (String, Vec<Media>) {
  let resource = |key: &str, kind: &str| {
    format!("{base}/open-apis/im/v1/messages/{id}/resources/{key}?type={kind}")
  };
  match msg_type {
    "text" => (c.str("text").unwrap_or_default(), Vec::new()),
    "post" => post_text(c),
    "image" => (
      String::new(),
      c.str("image_key")
        .map(|k| vec![Media::image(resource(&k, "image"))])
        .unwrap_or_default(),
    ),
    "file" | "audio" | "media" => {
      let kind = match msg_type {
        "audio" => MediaKind::Audio,
        "media" => MediaKind::Video,
        _ => MediaKind::File,
      };
      let mut m = Media::new(
        kind,
        c.str("file_key")
          .map(|k| resource(&k, "file"))
          .unwrap_or_default(),
      );
      m.alt = c.str("file_name");
      m.duration = c.f64("duration").map(|ms| ms / 1000.0);
      (c.str("file_name").unwrap_or_default(), vec![m])
    }
    "sticker" => ("[sticker]".into(), Vec::new()),
    "share_chat" => (
      format!("[shared chat {}]", c.str("chat_id").unwrap_or_default()),
      Vec::new(),
    ),
    "share_user" => (
      format!("[shared user {}]", c.str("user_id").unwrap_or_default()),
      Vec::new(),
    ),
    "interactive" => (card_text(c), Vec::new()),
    "system" => (c.str("template").unwrap_or_default(), Vec::new()),
    "merge_forward" => ("[forwarded messages]".into(), Vec::new()),
    other => (format!("[{other}]"), Vec::new()),
  }
}

/// A rich-text (`post`) body: title and paragraphs; images inline as media.
fn post_text(c: &Value) -> (String, Vec<Media>) {
  // Sent posts carry one locale (`zh_cn` …); received ones are flat.
  let root = if c.at("content").is_array() {
    c.clone()
  } else {
    c.as_object()
      .and_then(|m| m.values().find(|v| v.at("content").is_array()))
      .cloned()
      .unwrap_or_default()
  };
  let mut lines = Vec::new();
  if let Some(title) = root.str("title") {
    lines.push(title);
  }
  for para in root.list("content") {
    let mut line = String::new();
    for el in para.as_array().map(Vec::as_slice).unwrap_or_default() {
      match el.str("tag").as_deref() {
        Some("text" | "md" | "code_block") => line += &el.str("text").unwrap_or_default(),
        Some("a") => {
          line += &el
            .str("text")
            .or_else(|| el.str("href"))
            .unwrap_or_default()
        }
        Some("at") => line += &format!("@{}", el.str("user_name").unwrap_or_default()),
        Some("img") => line += "[image]",
        Some("emotion") => line += &format!("[{}]", el.str("emoji_type").unwrap_or_default()),
        _ => {}
      }
    }
    lines.push(line);
  }
  (lines.join("\n"), Vec::new())
}

/// The visible text of an interactive card, roughly.
fn card_text(c: &Value) -> String {
  let mut out = Vec::new();
  if let Some(t) = c.str("title").or_else(|| c.str("header.title.content")) {
    out.push(t);
  }
  fn walk(v: &Value, out: &mut Vec<String>) {
    match v {
      Value::Object(m) => {
        for (k, v) in m {
          if (k == "content" || k == "text") && v.is_string() {
            out.push(v.as_str().unwrap_or_default().to_owned());
          } else {
            walk(v, out);
          }
        }
      }
      Value::Array(a) => a.iter().for_each(|v| walk(v, out)),
      _ => {}
    }
  }
  walk(c.at("elements"), &mut out);
  out.retain(|s| !s.trim().is_empty());
  if out.is_empty() {
    "[card]".into()
  } else {
    out.join("\n")
  }
}

/// A `im.message.receive_v1` event's message in the list-call shape.
pub fn event_message(e: &Value) -> Value {
  let m = e.at("message");
  json!({
    "message_id": m.str("message_id"),
    "chat_id": m.str("chat_id"),
    "msg_type": m.str("message_type"),
    "body": { "content": m.str("content") },
    "create_time": m.str("create_time"),
    "update_time": m.str("update_time"),
    "parent_id": m.str("parent_id"),
    "root_id": m.str("root_id"),
    "thread_id": m.str("thread_id"),
    "mentions": m.at("mentions").as_array().map(|a| {
      a.iter().map(|x| json!({
        "key": x.str("key"),
        "id": x.str("id.open_id"),
        "name": x.str("name"),
      })).collect::<Vec<_>>()
    }),
    "sender": {
      "id": e.str("sender.sender_id.open_id"),
      "sender_type": e.str("sender.sender_type"),
    },
    "chat_type": m.str("chat_type"),
  })
}

pub fn chat(v: &Value) -> Collection {
  let mode = v.str("chat_mode").unwrap_or_else(|| "group".into());
  let mut c = Collection {
    id: v.str("chat_id").unwrap_or_default(),
    kind: match mode.as_str() {
      "p2p" => "dm".into(),
      "topic" => "topic-group".into(),
      _ => "group".into(),
    },
    name: v.str("name").unwrap_or_default(),
    description: v.str("description").filter(|d| !d.is_empty()),
    items: v.u64("user_count").or_else(|| v.u64("member_count")),
    owner: v.str("owner_id").map(|id| User {
      id,
      ..User::default()
    }),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  for (key, path) in [
    ("avatar", "avatar"),
    ("external", "external"),
    ("status", "chat_status"),
    ("tenant", "tenant_key"),
  ] {
    if let Some(value) = v.at(path).as_str().map(Value::from).or_else(|| {
      let x = v.at(path);
      (!x.is_null()).then(|| x.clone())
    }) {
      c.extra.insert(key.into(), value);
    }
  }
  c
}

pub fn member(v: &Value) -> User {
  User {
    id: v.str("member_id").unwrap_or_default(),
    name: v.str("name").unwrap_or_default(),
    extra: [(
      "id_type".to_owned(),
      Value::from(v.str("member_id_type").unwrap_or_default()),
    )]
    .into_iter()
    .collect(),
    ..User::default()
  }
}

pub fn user(v: &Value) -> User {
  User {
    id: v.str("open_id").unwrap_or_default(),
    name: v.str("name").unwrap_or_default(),
    handle: v.str("en_name").filter(|n| !n.is_empty()),
    avatar: v
      .str("avatar.avatar_origin")
      .or_else(|| v.str("avatar.avatar_240")),
    bio: v.str("description").filter(|d| !d.is_empty()),
    location: v.str("city").filter(|c| !c.is_empty()),
    extra: ["user_id", "union_id", "email", "job_title", "employee_no"]
      .into_iter()
      .filter_map(|k| v.str(k).map(|x| (k.to_owned(), Value::from(x))))
      .collect(),
    raw: Some(v.clone()),
    ..User::default()
  }
}

/// A long-connection event as a post: messages, recalls, reactions and
/// read receipts by their own shape, anything else by its type.
pub fn event(event: &Value, base: &str) -> Post {
  let kind = event.str("header.event_type").unwrap_or_default();
  let e = event.at("event");
  match kind.as_str() {
    "im.message.receive_v1" => message(&event_message(e), base),
    "im.message.recalled_v1" => Post {
      id: e.str("message_id").unwrap_or_default(),
      kind: "delete".into(),
      created_at: e.i64("recall_time").and_then(from_millis),
      ..Post::default()
    },
    "im.message.reaction.created_v1" | "im.message.reaction.deleted_v1" => Post {
      id: e.str("message_id").unwrap_or_default(),
      kind: if kind.ends_with("created_v1") {
        "reaction"
      } else {
        "unreaction"
      }
      .into(),
      text: e.str("reaction_type.emoji_type"),
      author: e.str("user_id.open_id").map(|id| User {
        id,
        ..User::default()
      }),
      created_at: e.i64("action_time").and_then(from_millis),
      ..Post::default()
    },
    "im.message.message_read_v1" => read_receipt(e),
    other => Post {
      id: event.str("header.event_id").unwrap_or_default(),
      kind: other.to_owned(),
      created_at: event.i64("header.create_time").and_then(from_millis),
      ..Post::default()
    },
  }
}

fn read_receipt(e: &Value) -> Post {
  Post {
    id: e
      .list("message_id_list")
      .first()
      .and_then(|m| m.as_str())
      .unwrap_or_default()
      .to_owned(),
    kind: "read".into(),
    author: e.str("reader.reader_id.open_id").map(|id| User {
      id,
      ..User::default()
    }),
    created_at: e.i64("reader.read_time").and_then(from_millis),
    ..Post::default()
  }
}
