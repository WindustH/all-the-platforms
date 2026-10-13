//! Messages of channels and direct chats: history, one message, sending,
//! editing, deleting and reactions.

use atp_core::chat::message_id;
use atp_core::{Action, Error, Message, Page, PageReq, Post, Result, ValueExt, json};

use crate::api::Api;
use crate::refs::{self, Chat, MessageRef};
use crate::{chats, parse, uploads};

/// Newest first, paging back from the oldest message of each page.
pub async fn list(api: &Api, chat: &str, page: &PageReq) -> Result<Page<Post>> {
  let chat = refs::chat(chat)?;
  let size = page.size_within(100);
  let (path, target) = match &chat {
    Chat::Channel(c) => ("/message/list", c.clone()),
    Chat::Direct(u) => ("/direct-message/list", u.clone()),
    Chat::Guild(_) => {
      return Err(Error::input(
        "a server has channels, not messages; list them with `chats guild:<id>`",
      ));
    }
  };
  let mut query = vec![("target_id", target), ("page_size", size.to_string())];
  if let Some(before) = &page.cursor {
    query.push(("msg_id", before.clone()));
    query.push(("flag", "before".into()));
  }
  let v = api.get(path, &query).await?;
  let name = chats::name(api, &chat).await;
  let mut rows = v.list("items").to_vec();
  rows.sort_by_key(|m| std::cmp::Reverse(m.i64("create_at").unwrap_or_default()));
  let next = (rows.len() >= size)
    .then(|| rows.last().and_then(|m| m.str("id")))
    .flatten();
  Ok(Page::new(
    rows
      .iter()
      .map(|m| parse::message(m, &chat, name.as_deref()))
      .collect(),
    next,
  ))
}

/// A direct chat's code, which some direct-message calls take.
async fn chat_code(api: &Api, user: &str) -> Result<String> {
  let v = api
    .post("/user-chat/create", json!({"target_id": user}))
    .await?;
  v.str("code")
    .ok_or_else(|| Error::upstream("KOOK returned no chat code"))
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let message = refs::message(arg)?;
  if let Some(Chat::Direct(user)) = message.chat {
    let code = chat_code(api, &user).await?;
    let query = [("chat_code", code), ("msg_id", message.id.clone())];
    let v = api.get("/direct-message/view", &query).await?;
    let item = v.list("items").first().cloned().unwrap_or(v);
    return Ok(parse::message(&item, &Chat::Direct(user), None));
  }
  let v = api
    .get("/message/view", &[("msg_id", message.id.clone())])
    .await?;
  let chat = Chat::Channel(v.str("channel_id").unwrap_or_default());
  let name = chats::name(api, &chat).await;
  Ok(parse::message(&v, &chat, name.as_deref()))
}

/// Text goes as KMarkdown (type 9, the kind KOOK lets you edit later);
/// each file is uploaded and sent as its own message.
pub async fn send(api: &Api, chat: &str, message: &Message) -> Result<Action> {
  let chat = refs::chat(chat)?;
  let (path, target) = match &chat {
    Chat::Channel(c) => ("/message/create", c.clone()),
    Chat::Direct(u) => ("/direct-message/create", u.clone()),
    Chat::Guild(_) => {
      return Err(Error::input(
        "send to a channel of the server (see `chats guild:<id>`)",
      ));
    }
  };
  let quote = message
    .reply_to
    .as_deref()
    .map(refs::message)
    .transpose()?
    .map(|r| r.id);
  let body = |kind: u64, content: String| {
    let mut body = json!({"type": kind, "target_id": target, "content": content});
    if let Some(q) = &quote {
      body["quote"] = json!(q);
    }
    body
  };
  let mut sent = Vec::new();
  if !message.text.is_empty() {
    let v = api.post(path, body(9, message.text.clone())).await?;
    sent.extend(v.str("msg_id"));
  }
  for file in &message.files {
    let (kind, url) = uploads::upload(api, file).await?;
    let v = api.post(path, body(kind, url)).await?;
    sent.extend(v.str("msg_id"));
  }
  let id = sent
    .first()
    .map(|m| message_id(&chat.id(), m))
    .unwrap_or_default();
  Ok(Action::done("send", chat.id()).with_id(id))
}

/// The channel or the direct-message variant of a write.
fn route(message: &MessageRef, channel: &'static str, direct: &'static str) -> &'static str {
  if matches!(message.chat, Some(Chat::Direct(_))) {
    direct
  } else {
    channel
  }
}

pub async fn edit(api: &Api, arg: &str, text: &str) -> Result<Action> {
  let message = refs::message(arg)?;
  let path = route(&message, "/message/update", "/direct-message/update");
  api
    .post(path, json!({"msg_id": message.id, "content": text}))
    .await?;
  Ok(Action::done("edit", arg))
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let message = refs::message(arg)?;
  let path = route(&message, "/message/delete", "/direct-message/delete");
  api.post(path, json!({"msg_id": message.id})).await?;
  Ok(Action::done("delete", arg))
}

pub async fn react(api: &Api, arg: &str, emoji: &str, undo: bool) -> Result<Action> {
  let message = refs::message(arg)?;
  let path = if undo {
    route(
      &message,
      "/message/delete-reaction",
      "/direct-message/delete-reaction",
    )
  } else {
    route(
      &message,
      "/message/add-reaction",
      "/direct-message/add-reaction",
    )
  };
  api
    .post(path, json!({"msg_id": message.id, "emoji": emoji}))
    .await?;
  Ok(Action::done(if undo { "unreact" } else { "react" }, arg))
}
