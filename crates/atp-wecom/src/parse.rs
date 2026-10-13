//! WeCom JSON → the shared model.

use atp_core::chat::message_id;
use atp_core::text::from_secs;
use atp_core::{Collection, Post, User, Value, ValueExt};

/// A 微信客服 account (`kf/account/list`) as a `kf:OPEN_KFID` chat.
pub fn kf_account(a: &Value) -> Collection {
  Collection {
    id: format!("kf:{}", a.str("open_kfid").unwrap_or_default()),
    kind: "kf".into(),
    name: a.str("name").unwrap_or_default(),
    extra: a
      .str("avatar")
      .map(|x| ("avatar".to_owned(), Value::from(x)))
      .into_iter()
      .collect(),
    raw: Some(a.clone()),
    ..Collection::default()
  }
}

/// One `kf/sync_msg` message of a 微信客服 account.
pub fn kf_message(account: &str, m: &Value) -> Post {
  let kind = m.str("msgtype").unwrap_or_default();
  let customer = m.str("external_userid").unwrap_or_default();
  let author = match m.i64("origin") {
    Some(5) => m.str("servicer_userid"),
    Some(4) => Some("system".into()),
    _ => Some(customer.clone()),
  };
  let mut post = Post {
    id: message_id(
      &format!("kf:{account}"),
      &m.str("msgid").unwrap_or_default(),
    ),
    kind: if kind == "event" { "event" } else { "message" }.into(),
    text: kf_text(&kind, m),
    author: author.map(|id| User {
      id,
      ..User::default()
    }),
    created_at: m.i64("send_time").and_then(from_secs),
    raw: Some(m.clone()),
    ..Post::default()
  };
  post
    .extra
    .insert("chat".into(), format!("kf:{account}/{customer}").into());
  post.extra.insert("msg_type".into(), kind.clone().into());
  if let Some(media) = m.str(&format!("{kind}.media_id")) {
    post.extra.insert("media_id".into(), media.into());
  }
  post
}

fn kf_text(kind: &str, m: &Value) -> Option<String> {
  match kind {
    "text" => m.str("text.content"),
    "event" => Some(format!(
      "[{}]",
      m.str("event.event_type").unwrap_or_default()
    )),
    "link" => m.str("link.title"),
    "location" => m.str("location.name"),
    other => Some(format!("[{other}]")),
  }
}

/// An app group chat (`appchat/get`'s `chat_info`).
pub fn group_chat(id: &str, c: &Value) -> Collection {
  Collection {
    id: format!("chat:{id}"),
    kind: "group".into(),
    name: c.str("name").unwrap_or_default(),
    items: Some(c.list("userlist").len() as u64),
    owner: c.str("owner").map(|id| User {
      id,
      ..User::default()
    }),
    raw: Some(c.clone()),
    ..Collection::default()
  }
}

/// A department (`department/list`) as a `party:ID` chat.
pub fn department(d: &Value) -> Collection {
  Collection {
    id: format!("party:{}", d.str("id").unwrap_or_default()),
    kind: "department".into(),
    name: d.str("name").unwrap_or_default(),
    extra: [("parent".to_owned(), d.at("parentid").clone())]
      .into_iter()
      .collect(),
    raw: Some(d.clone()),
    ..Collection::default()
  }
}

/// A department member (`user/simplelist`).
pub fn member(u: &Value) -> User {
  User {
    id: u.str("userid").unwrap_or_default(),
    name: u.str("name").unwrap_or_default(),
    extra: [("department".to_owned(), u.at("department").clone())]
      .into_iter()
      .collect(),
    ..User::default()
  }
}

/// A user's details (`user/get`); `id` is what was asked for.
pub fn user(id: &str, v: &Value) -> User {
  const EXTRA: [&str; 6] = [
    "department",
    "main_department",
    "open_userid",
    "status",
    "email",
    "biz_mail",
  ];
  User {
    id: v.str("userid").unwrap_or_else(|| id.to_owned()),
    name: v.str("name").unwrap_or_default(),
    handle: v.str("alias").filter(|a| !a.is_empty()),
    avatar: v.str("avatar").filter(|a| !a.is_empty()),
    bio: v.str("position").filter(|p| !p.is_empty()),
    extra: EXTRA
      .into_iter()
      .filter_map(|k| {
        let x = v.at(k);
        (!x.is_null()).then(|| (k.to_owned(), x.clone()))
      })
      .collect(),
    raw: Some(v.clone()),
    ..User::default()
  }
}

/// A message to the AI bot (`aibot_msg_callback`'s body).
pub fn bot_message(b: &Value) -> Post {
  let user = b.str("from.userid").unwrap_or_default();
  let chat = match b.str("chattype").as_deref() {
    Some("group") => b.str("chatid").unwrap_or_default(),
    _ => format!("user:{user}"),
  };
  let mut post = Post {
    id: b.str("msgid").unwrap_or_default(),
    kind: "message".into(),
    text: bot_text(b),
    author: Some(User {
      id: user,
      ..User::default()
    }),
    created_at: Some(jiff::Timestamp::now()),
    raw: Some(b.clone()),
    ..Post::default()
  };
  post.extra.insert("chat".into(), chat.into());
  post.extra.insert(
    "chat_type".into(),
    b.str("chattype").unwrap_or_else(|| "single".into()).into(),
  );
  // Images in bot messages are encrypted with the bot's AES key.
  if let Some(url) = b.str("image.url") {
    post.extra.insert("image_url".into(), url.into());
  }
  post
}

fn bot_text(b: &Value) -> Option<String> {
  match b.str("msgtype").unwrap_or_default().as_str() {
    "text" => b.str("text.content"),
    "mixed" => Some(
      b.list("mixed.msg_item")
        .iter()
        .map(|item| match item.str("msgtype").as_deref() {
          Some("text") => item.str("text.content").unwrap_or_default(),
          Some(other) => format!("[{other}]"),
          None => String::new(),
        })
        .collect::<Vec<_>>()
        .join(" "),
    ),
    "voice" => b.str("voice.content").or(Some("[voice]".into())),
    other => Some(format!("[{other}]")),
  }
}

/// An event for the AI bot (`aibot_event_callback`'s body).
pub fn bot_event(b: &Value) -> Post {
  Post {
    id: b.str("msgid").unwrap_or_default(),
    kind: b.str("event.eventtype").unwrap_or_else(|| "event".into()),
    author: b.str("from.userid").map(|id| User {
      id,
      ..User::default()
    }),
    raw: Some(b.clone()),
    ..Post::default()
  }
}
