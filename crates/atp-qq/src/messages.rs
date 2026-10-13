//! Messages: one sub-channel message, sending, recalling, reactions and pins.
//!
//! Bots read only what events bring and what single lookups return: there
//! is no message history. Replies are *passive* when they name a message
//! the bot received (`--reply-to CHAT/MSG`, or `event:<id>` for other
//! events): QQ allows a few per message within minutes. Anything else is an
//! *active* message, which QQ rations or refuses.

use atp_core::chat::message_id;
use atp_core::file::Upload;
use atp_core::http::Part;
use atp_core::{Action, Error, MediaKind, Message, Post, Result, Value, ValueExt, json};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::api::Api;
use crate::parse;
use crate::refs::{self, Chat};

/// Sub-channel messages can be looked up one by one; others cannot.
pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let message = refs::message(arg)?;
  let Chat::Channel(c) = &message.chat else {
    return Err(Error::unsupported("read").with_hint(
      "QQ bots can look up sub-channel messages only; group, C2C and 私信 messages arrive through `listen`",
    ));
  };
  let v = api
    .get(&format!("/channels/{c}/messages/{}", message.id), &[])
    .await?;
  let item = if v.at("message").is_object() {
    v.at("message").clone()
  } else {
    v
  };
  Ok(parse::message(&item, &message.chat))
}

/// What a reply names: a message to answer, or an event.
enum Passive {
  None,
  Message(String),
  Event(String),
}

impl Passive {
  fn of(reply_to: Option<&str>) -> Result<Self> {
    Ok(match reply_to {
      None => Passive::None,
      Some(r) => match r.strip_prefix("event:") {
        Some(e) => Passive::Event(e.to_owned()),
        None => Passive::Message(refs::message(r)?.id),
      },
    })
  }

  /// `msg_id` / `event_id` fields of a JSON body.
  fn mark(&self, body: &mut Value) {
    match self {
      Passive::Message(m) => body["msg_id"] = json!(m),
      Passive::Event(e) => body["event_id"] = json!(e),
      Passive::None => {}
    }
  }

  /// The same fields as multipart form parts.
  fn parts(&self) -> Option<Part> {
    match self {
      Passive::Message(m) => Some(Part::text("msg_id", m.clone())),
      Passive::Event(e) => Some(Part::text("event_id", e.clone())),
      Passive::None => None,
    }
  }
}

pub async fn send(api: &Api, chat: &str, message: &Message) -> Result<Action> {
  let chat = refs::chat(chat)?;
  let reply = Passive::of(message.reply_to.as_deref())?;
  let id = match &chat {
    Chat::Channel(c) => {
      send_guild(api, &format!("/channels/{c}/messages"), message, &reply).await?
    }
    Chat::Dm(g) => send_guild(api, &format!("/dms/{g}/messages"), message, &reply).await?,
    Chat::Group(g) => send_v2(api, &format!("/v2/groups/{g}"), message, &reply).await?,
    Chat::User(u) => send_v2(api, &format!("/v2/users/{u}"), message, &reply).await?,
    Chat::Guild(_) => {
      return Err(Error::input(
        "send to a sub-channel (see `chats guild:<id>`)",
      ));
    }
  };
  let mut action = Action::done("send", chat.id());
  if let Some(id) = id {
    action = action.with_id(message_id(&chat.id(), &id));
  }
  if matches!(reply, Passive::None) {
    action = action.with_message("sent as an active message (QQ rations these)");
  }
  Ok(action)
}

/// Sub-channels and 私信: JSON, or a multipart form with one image each.
async fn send_guild(
  api: &Api,
  path: &str,
  message: &Message,
  reply: &Passive,
) -> Result<Option<String>> {
  let mut first = None;
  let mut text = Some(message.text.clone()).filter(|t| !t.is_empty());
  for path_on_disk in &message.files {
    let file = Upload::read(path_on_disk).await?;
    if !file.is_image() {
      return Err(
        Error::unsupported("send --attach").with_hint("QQ sub-channels and 私信 take images only"),
      );
    }
    let mut parts = vec![Part::file("file_image", file.data, &file.name, file.mime)];
    if let Some(t) = text.take() {
      parts.push(Part::text("content", t));
    }
    parts.extend(reply.parts());
    let v = api.post_multipart(path, parts).await?;
    first = first.or(v.str("id"));
  }
  if let Some(t) = text {
    let mut body = json!({"content": t});
    reply.mark(&mut body);
    if let Passive::Message(m) = reply {
      body["message_reference"] = json!({"message_id": m, "ignore_get_message_error": true});
    }
    let v = api.post(path, &body).await?;
    first = first.or(v.str("id"));
  }
  Ok(first)
}

/// Groups and C2C: text, and files uploaded first as rich media.
async fn send_v2(
  api: &Api,
  base: &str,
  message: &Message,
  reply: &Passive,
) -> Result<Option<String>> {
  let with_reply = |mut body: Value| {
    reply.mark(&mut body);
    body["msg_seq"] = json!(rand::random_range(1..=1_000_000u32));
    body
  };
  let mut first = None;
  let mut text = Some(message.text.clone()).filter(|t| !t.is_empty());
  for path in &message.files {
    let info = upload_media(api, base, &Upload::read(path).await?).await?;
    let body = with_reply(json!({
      "msg_type": 7,
      "content": text.take().unwrap_or_else(|| " ".into()),
      "media": {"file_info": info},
    }));
    let v = api.post(&format!("{base}/messages"), &body).await?;
    first = first.or(v.str("id"));
  }
  if let Some(t) = text {
    let body = with_reply(json!({"msg_type": 0, "content": t}));
    let v = api.post(&format!("{base}/messages"), &body).await?;
    first = first.or(v.str("id"));
  }
  Ok(first)
}

/// Upload a file as rich media for a group or C2C message; its `file_info`.
async fn upload_media(api: &Api, base: &str, file: &Upload) -> Result<String> {
  let body = json!({
    "file_type": file_type(file),
    "file_data": STANDARD.encode(&file.data),
    "srv_send_msg": false,
  });
  let v = api.post(&format!("{base}/files"), &body).await?;
  v.str("file_info")
    .ok_or_else(|| Error::upstream("QQ returned no file_info for the upload"))
}

/// QQ's rich-media `file_type`: 1 image, 2 video, 3 voice, 4 file. Voice
/// notes are often `.silk`, which has no MIME type of its own.
fn file_type(file: &Upload) -> u8 {
  let silk = file.name.to_ascii_lowercase().ends_with(".silk");
  match file.kind() {
    MediaKind::Image | MediaKind::Gif => 1,
    MediaKind::Video => 2,
    MediaKind::Audio => 3,
    MediaKind::File if silk => 3,
    MediaKind::File => 4,
  }
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let message = refs::message(arg)?;
  let id = &message.id;
  let path = match &message.chat {
    Chat::Channel(c) => format!("/channels/{c}/messages/{id}"),
    Chat::Dm(g) => format!("/dms/{g}/messages/{id}"),
    Chat::Group(g) => format!("/v2/groups/{g}/messages/{id}"),
    Chat::User(u) => format!("/v2/users/{u}/messages/{id}"),
    Chat::Guild(_) => return Err(Error::input("give CHAT/MESSAGE")),
  };
  api.delete(&path, &[("hidetip", "false".into())]).await?;
  Ok(Action::done("delete", arg))
}

/// Sub-channel reactions: a system face by number, or an emoji.
pub async fn react(api: &Api, arg: &str, emoji: &str, undo: bool) -> Result<Action> {
  let message = refs::message(arg)?;
  let Chat::Channel(c) = &message.chat else {
    return Err(Error::unsupported("react").with_hint("QQ bots react in sub-channels only"));
  };
  let (kind, id) = if emoji.bytes().all(|b| b.is_ascii_digit()) {
    (1, emoji.to_owned())
  } else {
    let ch = emoji
      .chars()
      .next()
      .ok_or_else(|| Error::input("give an emoji or a system face id"))?;
    (2, u32::from(ch).to_string())
  };
  let path = format!(
    "/channels/{c}/messages/{}/reactions/{kind}/{id}",
    message.id
  );
  if undo {
    api.delete(&path, &[]).await?;
  } else {
    api.put(&path).await?;
  }
  Ok(Action::done(if undo { "unreact" } else { "react" }, arg))
}

/// Pinned messages of a sub-channel, as `CHANNEL/MESSAGE` ids.
pub async fn pins(api: &Api, channel: &str) -> Result<Value> {
  let Chat::Channel(c) = refs::chat(channel)? else {
    return Err(Error::input("give a sub-channel id"));
  };
  let v = api.get(&format!("/channels/{c}/pins"), &[]).await?;
  let ids: Vec<Value> = v
    .list("message_ids")
    .iter()
    .map(|m| json!(message_id(&c, m.as_str().unwrap_or_default())))
    .collect();
  Ok(Value::from(ids))
}

/// Pin a sub-channel message, or unpin it.
pub async fn pin(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let message = refs::message(arg)?;
  let Chat::Channel(c) = &message.chat else {
    return Err(Error::input("only sub-channel messages can be pinned"));
  };
  let path = format!("/channels/{c}/pins/{}", message.id);
  if undo {
    api.delete(&path, &[]).await?;
  } else {
    api.put(&path).await?;
  }
  Ok(Action::done(if undo { "unpin" } else { "pin" }, arg))
}
