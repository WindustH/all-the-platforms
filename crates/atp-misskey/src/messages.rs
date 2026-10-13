//! Chat messages: listing, sending, reading, deleting and reactions.
//! Messages carry `<chat>/<message>` ids (`user:<id>/<message>`,
//! `room:<id>/<message>`).

use atp_core::{Action, Error, Message, Page, PageReq, Post, Result, ValueExt, chat, json};

use crate::api::Api;
use crate::chats::{self, Chat};
use crate::parse;
use crate::refs::MessageRef;

pub async fn list(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let (chat, name) = chats::named(api, arg).await?;
  let (endpoint, body) = match &chat {
    Chat::Room(id) => ("chat/messages/room-timeline", json!({"roomId": id})),
    Chat::User(id) => ("chat/messages/user-timeline", json!({"userId": id})),
  };
  let key = chat.key();
  api.require()?;
  api
    .list(endpoint, body, page, |row, instance| {
      parse::message(row, &key, &name, instance)
    })
    .await
}

/// Send text, then each further file as its own message (one file each).
pub async fn send(api: &Api, arg: &str, message: &Message) -> Result<Action> {
  if message.reply_to.is_some() {
    return Err(Error::unsupported("send --reply-to").with_hint("Misskey chat has no replies"));
  }
  let (chat, name) = chats::named(api, arg).await?;
  let mut files = Vec::new();
  for path in &message.files {
    files.push(api.upload(path).await?);
  }
  let mut first = None;
  let mut texts: Vec<Option<&str>> = vec![Some(message.text.as_str()).filter(|t| !t.is_empty())];
  texts.resize(files.len().max(1), None);
  for (index, text) in texts.into_iter().enumerate() {
    let (endpoint, mut body) = match &chat {
      Chat::Room(id) => ("chat/messages/create-to-room", json!({"toRoomId": id})),
      Chat::User(id) => ("chat/messages/create-to-user", json!({"toUserId": id})),
    };
    if let Some(text) = text {
      body["text"] = json!(text);
    }
    if let Some(file) = files.get(index) {
      body["fileId"] = json!(file);
    }
    let v = api.write(endpoint, body).await?;
    first.get_or_insert(v.str("id").unwrap_or_default());
  }
  let id = chat::message_id(&chat.key(), &first.unwrap_or_default());
  Ok(Action::done("send", &name).with_id(id))
}

pub async fn read(api: &Api, message: &MessageRef) -> Result<Post> {
  let v = api
    .call_auth("chat/messages/show", json!({"messageId": message.id}))
    .await?;
  Ok(parse::message(&v, &message.chat, "", &api.instance()))
}

pub async fn delete(api: &Api, message: &MessageRef) -> Result<Action> {
  api
    .write("chat/messages/delete", json!({"messageId": message.id}))
    .await?;
  Ok(Action::done(
    "delete",
    chat::message_id(&message.chat, &message.id),
  ))
}

pub async fn react(api: &Api, message: &MessageRef, reaction: &str, undo: bool) -> Result<Action> {
  let endpoint = if undo {
    "chat/messages/unreact"
  } else {
    "chat/messages/react"
  };
  let body = json!({"messageId": message.id, "reaction": reaction.trim()});
  api.write(endpoint, body).await?;
  Ok(
    Action::done(
      if undo { "unreact" } else { "react" },
      chat::message_id(&message.chat, &message.id),
    )
    .with_message(reaction.trim()),
  )
}
