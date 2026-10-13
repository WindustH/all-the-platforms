//! Chat (Misskey 2025.4+): direct chats with users and chat rooms.
//! Direct chats are `user:<id>`, rooms `room:<id>`; messages carry
//! `<chat>/<message>` ids.

use social_core::{
  Action, Collection, Error, Message, Page, PageReq, Post, Result, User, Value, ValueExt, json,
};

use crate::api::Api;
use crate::notes::{paged, upload};
use crate::parse;
use crate::refs::{self, Chat, ChatMessage, UserRef};
use crate::users;

/// The chat id (`user:<id>` / `room:<id>`) and its display name.
async fn resolve(api: &Api, arg: &str) -> Result<(Chat, String, String)> {
  match refs::chat(arg, &api.instance())? {
    Chat::Room(id) => {
      let v = api
        .call_auth("chat/rooms/show", json!({"roomId": id}))
        .await?;
      let name = v.str("name").unwrap_or_default();
      Ok((Chat::Room(id.clone()), format!("room:{id}"), name))
    }
    Chat::User(r) => {
      let v = users::lookup(api, &r).await?;
      let id = v.str("id").unwrap_or_default();
      let name = v
        .str("username")
        .map(|u| format!("@{u}"))
        .unwrap_or_default();
      Ok((
        Chat::User(UserRef::Id(id.clone())),
        format!("user:{id}"),
        name,
      ))
    }
  }
}

/// Recent direct chats and rooms, then rooms you are in without recent messages.
pub async fn list(api: &Api, parent: Option<&str>) -> Result<Page<Collection>> {
  if parent.is_some() {
    return Err(Error::input(
      "Misskey chats have no channels inside them; `chats` lists them all",
    ));
  }
  let me = users::my_id(api).await?;
  let instance = api.instance();
  let mut out: Vec<Collection> = Vec::new();
  for room in [false, true] {
    let v = api
      .call_auth("chat/history", json!({"limit": 100, "room": room}))
      .await?;
    for m in v.as_array().cloned().unwrap_or_default() {
      let mut c = if room {
        parse::room(m.at("toRoom"), &instance)
      } else {
        let other = if m.str("fromUserId").as_deref() == Some(me.as_str()) {
          m.at("toUser")
        } else {
          m.at("fromUser")
        };
        parse::direct(other, &instance)
      };
      if let Some(at) = m.str("createdAt") {
        c.extra.insert("last_message_at".into(), json!(at));
      }
      if let Some(text) = m.str("text") {
        c.extra.insert("last_message".into(), json!(text));
      }
      if m.bool("isRead") == Some(false) && m.str("fromUserId").as_deref() != Some(&me) {
        c.extra.insert("unread".into(), json!(true));
      }
      if !out.iter().any(|o| o.id == c.id) {
        out.push(c);
      }
    }
  }
  for endpoint in ["chat/rooms/joining", "chat/rooms/owned"] {
    let v = api.call_auth(endpoint, json!({"limit": 100})).await?;
    for r in v.as_array().cloned().unwrap_or_default() {
      let room = if r.at("room").is_null() {
        &r
      } else {
        r.at("room")
      };
      let c = parse::room(room, &instance);
      if !out.iter().any(|o| o.id == c.id) {
        out.push(c);
      }
    }
  }
  Ok(Page::last(out))
}

pub async fn messages(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let (chat, id, name) = resolve(api, arg).await?;
  let size = page.size_within(100);
  let mut body = json!({"limit": size});
  if let Some(c) = &page.cursor {
    body["untilId"] = json!(c);
  }
  let endpoint = match &chat {
    Chat::Room(rid) => {
      body["roomId"] = json!(rid);
      "chat/messages/room-timeline"
    }
    Chat::User(UserRef::Id(uid)) => {
      body["userId"] = json!(uid);
      "chat/messages/user-timeline"
    }
    Chat::User(_) => unreachable!("resolved to an id"),
  };
  let v = api.call_auth(endpoint, body).await?;
  let rows = v.as_array().cloned().unwrap_or_default();
  let instance = api.instance();
  let items = rows
    .iter()
    .map(|m| parse::chat_message(m, &id, &name, &instance))
    .collect();
  Ok(Page::new(items, paged(&rows, size)))
}

/// Send text, then each further file as its own message (one file each).
pub async fn send(api: &Api, arg: &str, message: &Message) -> Result<Action> {
  if message.reply_to.is_some() {
    return Err(Error::unsupported("send --reply-to").with_hint("Misskey chat has no replies"));
  }
  let (chat, id, name) = resolve(api, arg).await?;
  let mut files = Vec::new();
  for f in &message.files {
    files.push(upload(api, f).await?);
  }
  let mut first = None;
  let mut texts: Vec<Option<&str>> = vec![Some(message.text.as_str()).filter(|t| !t.is_empty())];
  texts.resize(files.len().max(1), None);
  for (i, text) in texts.into_iter().enumerate() {
    let mut body = json!({});
    if let Some(t) = text {
      body["text"] = json!(t);
    }
    if let Some(f) = files.get(i) {
      body["fileId"] = json!(f);
    }
    let endpoint = match &chat {
      Chat::Room(rid) => {
        body["toRoomId"] = json!(rid);
        "chat/messages/create-to-room"
      }
      Chat::User(UserRef::Id(uid)) => {
        body["toUserId"] = json!(uid);
        "chat/messages/create-to-user"
      }
      Chat::User(_) => unreachable!("resolved to an id"),
    };
    let v = api.write(endpoint, body).await?;
    first.get_or_insert(v.str("id").unwrap_or_default());
  }
  Ok(Action::done("send", &name).with_id(format!("{id}/{}", first.unwrap_or_default())))
}

pub async fn read(api: &Api, m: &ChatMessage) -> Result<Post> {
  let v = api
    .call_auth("chat/messages/show", json!({"messageId": m.id}))
    .await?;
  let chat = chat_key(&m.chat);
  Ok(parse::chat_message(&v, &chat, "", &api.instance()))
}

fn chat_key(chat: &Chat) -> String {
  match chat {
    Chat::Room(r) => format!("room:{r}"),
    Chat::User(UserRef::Id(u)) => format!("user:{u}"),
    Chat::User(_) => String::new(),
  }
}

pub async fn delete(api: &Api, m: &ChatMessage) -> Result<Action> {
  api
    .write("chat/messages/delete", json!({"messageId": m.id}))
    .await?;
  Ok(Action::done(
    "delete",
    format!("{}/{}", chat_key(&m.chat), m.id),
  ))
}

pub async fn react(api: &Api, m: &ChatMessage, reaction: &str, undo: bool) -> Result<Action> {
  let endpoint = if undo {
    "chat/messages/unreact"
  } else {
    "chat/messages/react"
  };
  api
    .write(
      endpoint,
      json!({"messageId": m.id, "reaction": reaction.trim()}),
    )
    .await?;
  Ok(
    Action::done(
      if undo { "unreact" } else { "react" },
      format!("{}/{}", chat_key(&m.chat), m.id),
    )
    .with_message(reaction.trim()),
  )
}

/// Members of a room; for a direct chat, the two of you.
pub async fn members(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  let instance = api.instance();
  match refs::chat(arg, &instance)? {
    Chat::Room(rid) => {
      let size = page.size_within(100);
      let mut body = json!({"roomId": rid, "limit": size});
      if let Some(c) = &page.cursor {
        body["untilId"] = json!(c);
      }
      let v = api.call_auth("chat/rooms/members", body).await?;
      let rows = v.as_array().cloned().unwrap_or_default();
      let items = rows
        .iter()
        .map(|m| parse::user(m.at("user"), &instance))
        .collect();
      Ok(Page::new(items, paged(&rows, size)))
    }
    Chat::User(r) => {
      let other: Value = users::lookup(api, &r).await?;
      let me = users::me(api).await?;
      Ok(Page::last(vec![
        parse::user(&me, &instance),
        parse::user(&other, &instance),
      ]))
    }
  }
}
