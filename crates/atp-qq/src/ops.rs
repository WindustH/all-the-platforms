//! QQ bot calls behind the shared commands.
//!
//! Bots read only what events bring and what single lookups return: there
//! is no message history. Replies are *passive* when they name a message
//! the bot received (`--reply-to CHAT/MSG`, or `event:<id>` for other
//! events): QQ allows a few per message within minutes. Anything else is an
//! *active* message, which QQ rations or refuses.

use std::path::Path;

use atp_core::http::Part;
use atp_core::{
  Action, Collection, Error, Message, Page, PageReq, Post, Result, User, Value, ValueExt, json,
};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::api::Api;
use crate::parse;
use crate::refs::{self, Chat};

pub async fn whoami(api: &Api) -> Result<User> {
  Ok(parse::user(&api.get("/users/@me", &[]).await?))
}

/// Without a parent: the bot's servers (频道). With one: its sub-channels.
pub async fn chats(api: &Api, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
  if let Some(parent) = parent {
    let guild = guild_of(api, &refs::chat(parent)?).await?;
    let v = api.get(&format!("/guilds/{guild}/channels"), &[]).await?;
    let rows = v.as_array().cloned().unwrap_or_default();
    return Ok(Page::last(rows.iter().map(parse::channel).collect()));
  }
  let limit = page.size_within(100);
  let mut query = vec![("limit", limit.to_string())];
  if let Some(after) = &page.cursor {
    query.push(("after", after.clone()));
  }
  let v = api.get("/users/@me/guilds", &query).await?;
  let rows = v.as_array().cloned().unwrap_or_default();
  let next = (rows.len() >= limit)
    .then(|| rows.last().and_then(|g| g.str("id")))
    .flatten();
  Ok(Page::new(rows.iter().map(parse::guild).collect(), next))
}

async fn guild_of(api: &Api, chat: &Chat) -> Result<String> {
  match chat {
    Chat::Guild(g) => Ok(g.clone()),
    Chat::Channel(c) => api
      .get(&format!("/channels/{c}"), &[])
      .await?
      .str("guild_id")
      .ok_or_else(|| Error::not_found(format!("channel {c} has no server"))),
    _ => Err(Error::input(
      "give a server (guild:<id>) or one of its channels",
    )),
  }
}

pub async fn members(api: &Api, chat: &str, page: &PageReq) -> Result<Page<User>> {
  let guild = guild_of(api, &refs::chat(chat)?).await?;
  let limit = page.size_within(400);
  let after = page.cursor.clone().unwrap_or_else(|| "0".into());
  let v = api
    .get(
      &format!("/guilds/{guild}/members"),
      &[("after", after), ("limit", limit.to_string())],
    )
    .await?;
  let rows = v.as_array().cloned().unwrap_or_default();
  let next = (rows.len() >= limit)
    .then(|| rows.last().and_then(|m| m.str("user.id")))
    .flatten();
  Ok(Page::new(rows.iter().map(parse::member).collect(), next))
}

/// `GUILD/USER`: a member of a server (bots look users up by server).
pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let (guild, user) = arg
    .trim()
    .trim_start_matches("guild:")
    .split_once('/')
    .ok_or_else(|| Error::input("give SERVER/USER (bots look users up within a server)"))?;
  let v = api
    .get(&format!("/guilds/{guild}/members/{user}"), &[])
    .await?;
  Ok(parse::member(&v))
}

/// Sub-channel messages can be looked up one by one; others cannot.
pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let r = refs::message(arg)?;
  let Chat::Channel(c) = &r.chat else {
    return Err(Error::unsupported("read").with_hint(
      "QQ bots can look up sub-channel messages only; group, C2C and 私信 messages arrive through `listen`",
    ));
  };
  let v = api
    .get(&format!("/channels/{c}/messages/{}", r.id), &[])
    .await?;
  let m = if v.at("message").is_object() {
    v.at("message").clone()
  } else {
    v
  };
  Ok(parse::message(&m, &r.chat))
}

/// What a reply names: a message to answer, or an event.
enum Passive {
  None,
  Message(String),
  Event(String),
}

fn passive(reply_to: Option<&str>) -> Result<Passive> {
  Ok(match reply_to {
    None => Passive::None,
    Some(r) => match r.strip_prefix("event:") {
      Some(e) => Passive::Event(e.to_owned()),
      None => Passive::Message(refs::message(r)?.id),
    },
  })
}

fn seq() -> u32 {
  rand::random_range(1..=1_000_000)
}

pub async fn send(api: &Api, chat: &str, message: &Message) -> Result<Action> {
  let chat = refs::chat(chat)?;
  let reply = passive(message.reply_to.as_deref())?;
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
    action = action.with_id(format!("{}/{id}", chat.id()));
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
  let reply_fields = |b: &mut Value| match reply {
    Passive::Message(m) => {
      b["msg_id"] = json!(m);
      b["message_reference"] = json!({"message_id": m, "ignore_get_message_error": true});
    }
    Passive::Event(e) => b["event_id"] = json!(e),
    Passive::None => {}
  };
  for file in &message.files {
    let (data, name, mime) = read_file(file).await?;
    if !mime.starts_with("image/") {
      return Err(
        Error::unsupported("send --attach").with_hint("QQ sub-channels and 私信 take images only"),
      );
    }
    let mut parts = vec![Part::file("file_image", data, &name, &mime)];
    if let Some(t) = text.take() {
      parts.push(Part::text("content", t));
    }
    match reply {
      Passive::Message(m) => parts.push(Part::text("msg_id", m.clone())),
      Passive::Event(e) => parts.push(Part::text("event_id", e.clone())),
      Passive::None => {}
    }
    let v = api.post_multipart(path, parts).await?;
    first = first.or(v.str("id"));
  }
  if let Some(t) = text {
    let mut body = json!({"content": t});
    reply_fields(&mut body);
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
  let with_reply = |mut b: Value| {
    match reply {
      Passive::Message(m) => b["msg_id"] = json!(m),
      Passive::Event(e) => b["event_id"] = json!(e),
      Passive::None => {}
    }
    b["msg_seq"] = json!(seq());
    b
  };
  let mut first = None;
  let mut text = Some(message.text.clone()).filter(|t| !t.is_empty());
  for file in &message.files {
    let (data, _, mime) = read_file(file).await?;
    let file_type = match mime.split('/').next() {
      Some("image") => 1,
      Some("video") => 2,
      Some("audio") => 3,
      _ => 4,
    };
    let up = api
      .post(
        &format!("{base}/files"),
        &json!({"file_type": file_type, "file_data": STANDARD.encode(&data), "srv_send_msg": false}),
      )
      .await?;
    let info = up
      .str("file_info")
      .ok_or_else(|| Error::upstream("QQ returned no file_info for the upload"))?;
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

async fn read_file(path: &Path) -> Result<(Vec<u8>, String, String)> {
  let data = tokio::fs::read(path)
    .await
    .map_err(|e| Error::input(format!("cannot read {}: {e}", path.display())))?;
  let name = path
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or("file")
    .to_owned();
  let ext = path
    .extension()
    .and_then(|e| e.to_str())
    .unwrap_or_default()
    .to_ascii_lowercase();
  let mime = match atp_core::file::sniff(&data) {
    Some(img) => format!("image/{}", if img == "jpg" { "jpeg" } else { img }),
    None => match ext.as_str() {
      "mp4" | "mov" => "video/mp4".into(),
      "silk" | "amr" | "mp3" | "wav" => "audio/silk".into(),
      _ => "application/octet-stream".into(),
    },
  };
  Ok((data, name, mime))
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let r = refs::message(arg)?;
  let path = match &r.chat {
    Chat::Channel(c) => format!("/channels/{c}/messages/{}", r.id),
    Chat::Dm(g) => format!("/dms/{g}/messages/{}", r.id),
    Chat::Group(g) => format!("/v2/groups/{g}/messages/{}", r.id),
    Chat::User(u) => format!("/v2/users/{u}/messages/{}", r.id),
    Chat::Guild(_) => return Err(Error::input("give CHAT/MESSAGE")),
  };
  api.delete(&path, &[("hidetip", "false".into())]).await?;
  Ok(Action::done("delete", arg))
}

/// Sub-channel reactions: a system face by number, or an emoji.
pub async fn react(api: &Api, arg: &str, emoji: &str, undo: bool) -> Result<Action> {
  let r = refs::message(arg)?;
  let Chat::Channel(c) = &r.chat else {
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
  let path = format!("/channels/{c}/messages/{}/reactions/{kind}/{id}", r.id);
  if undo {
    api.delete(&path, &[]).await?;
  } else {
    api.put(&path).await?;
  }
  Ok(Action::done(if undo { "unreact" } else { "react" }, arg))
}
