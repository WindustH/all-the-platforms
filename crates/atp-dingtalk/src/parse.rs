//! DingTalk JSON → the shared model.

use atp_core::chat::message_id;
use atp_core::text::from_millis;
use atp_core::{Collection, Post, User, Value, ValueExt};

/// A staff member (`topapi/v2/user/get`'s `result`); `id` is what was asked for.
pub fn staff(id: &str, r: &Value) -> User {
  const EXTRA: [&str; 7] = [
    "unionid",
    "email",
    "org_email",
    "dept_id_list",
    "hired_date",
    "admin",
    "boss",
  ];
  User {
    id: r.str("userid").unwrap_or_else(|| id.to_owned()),
    name: r.str("name").unwrap_or_default(),
    handle: r.str("job_number"),
    avatar: r.str("avatar").filter(|a| !a.is_empty()),
    bio: r.str("title").filter(|t| !t.is_empty()),
    location: r.str("work_place").filter(|w| !w.is_empty()),
    extra: EXTRA
      .into_iter()
      .filter_map(|k| {
        let v = r.at(k);
        (!v.is_null()).then(|| (k.to_owned(), v.clone()))
      })
      .collect(),
    raw: Some(r.clone()),
    ..User::default()
  }
}

/// A scene group (`scenegroup/get`'s `result`).
pub fn scene_group(id: &str, r: &Value) -> Collection {
  let mut group = Collection {
    id: id.to_owned(),
    kind: "group".into(),
    name: r.str("title").unwrap_or_default(),
    owner: r.str("owner_staff_id").map(|id| User {
      id,
      ..User::default()
    }),
    raw: Some(r.clone()),
    ..Collection::default()
  };
  for key in ["chatid", "template_id", "icon", "group_url"] {
    if let Some(v) = r.str(key) {
      group.extra.insert(key.into(), Value::from(v));
    }
  }
  group
}

/// A message to the robot (a mention in a group, or a direct message).
pub fn bot_message(d: &Value) -> Post {
  let chat = d.str("conversationId").unwrap_or_default();
  let mut post = Post {
    id: message_id(&chat, &d.str("msgId").unwrap_or_default()),
    kind: "message".into(),
    text: bot_text(d).map(|t| t.trim().to_owned()),
    author: Some(User {
      id: d
        .str("senderStaffId")
        .or_else(|| d.str("senderId"))
        .unwrap_or_default(),
      name: d.str("senderNick").unwrap_or_default(),
      ..User::default()
    }),
    created_at: d.i64("createAt").and_then(from_millis),
    raw: Some(d.clone()),
    ..Post::default()
  };
  let chat_type = match d.str("conversationType").as_deref() {
    Some("1") => "dm",
    _ => "group",
  };
  let reply_to = if chat_type == "dm" {
    d.str("senderStaffId").map(|s| format!("user:{s}"))
  } else {
    Some(chat.clone())
  };
  for (key, value) in [
    ("chat", Some(chat)),
    ("chat_name", d.str("conversationTitle")),
    ("chat_type", Some(chat_type.to_owned())),
    ("reply_to_chat", reply_to),
    ("session_webhook", d.str("sessionWebhook")),
    ("robot_code", d.str("robotCode")),
  ] {
    if let Some(v) = value {
      post.extra.insert(key.into(), v.into());
    }
  }
  post
}

fn bot_text(d: &Value) -> Option<String> {
  match d.str("msgtype").as_deref() {
    Some("text") => d.str("text.content"),
    Some("richText") => Some(
      d.list("content.richText")
        .iter()
        .filter_map(|p| p.str("text"))
        .collect::<Vec<_>>()
        .join(""),
    ),
    Some("picture") => Some("[image]".into()),
    Some("audio") => d.str("content.recognition").or(Some("[voice]".into())),
    Some("file") => Some(format!(
      "[file {}]",
      d.str("content.fileName").unwrap_or_default()
    )),
    Some(other) => Some(format!("[{other}]")),
    None => None,
  }
}

/// Any other event: its type as `kind`, the payload as `raw`.
pub fn event(frame: &Value, d: &Value) -> Post {
  let mut post = Post {
    id: frame
      .str("headers.eventId")
      .or_else(|| frame.str("headers.messageId"))
      .unwrap_or_default(),
    kind: frame
      .str("headers.eventType")
      .or_else(|| frame.str("headers.topic"))
      .unwrap_or_else(|| "event".into()),
    created_at: frame
      .str("headers.eventBornTime")
      .and_then(|t| t.parse().ok())
      .and_then(from_millis),
    raw: Some(d.clone()),
    ..Post::default()
  };
  if let Some(c) = d
    .str("openConversationId")
    .or_else(|| d.str("OpenConversationId"))
  {
    post.extra.insert("chat".into(), c.into());
  }
  post
}
