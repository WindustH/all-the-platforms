//! Chats (Misskey 2025.4+): direct chats with users (`user:<id>`) and chat
//! rooms (`room:<id>`), and who is in them.

use social_core::{Collection, Error, Page, PageReq, Result, User, Value, ValueExt, json};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, ChatRef};
use crate::users;

/// A chat with its user looked up.
pub enum Chat {
  User(String),
  Room(String),
}

impl Chat {
  /// `user:<id>` / `room:<id>`.
  pub fn key(&self) -> String {
    match self {
      Self::User(id) => format!("user:{id}"),
      Self::Room(id) => format!("room:{id}"),
    }
  }
}

/// A chat argument → [`Chat`]; only a user named other than by id costs a
/// request.
pub async fn chat(api: &Api, arg: &str) -> Result<Chat> {
  Ok(match refs::chat(arg, &api.instance())? {
    ChatRef::Room(id) => Chat::Room(id),
    ChatRef::User(user) => Chat::User(users::id(api, &user).await?),
  })
}

/// [`chat`] and its display name: the room's name, or `@user`.
pub async fn named(api: &Api, arg: &str) -> Result<(Chat, String)> {
  match refs::chat(arg, &api.instance())? {
    ChatRef::Room(id) => {
      let v = api
        .call_auth("chat/rooms/show", json!({"roomId": id}))
        .await?;
      Ok((Chat::Room(id), v.str("name").unwrap_or_default()))
    }
    ChatRef::User(user) => {
      let v = users::lookup(api, &user).await?;
      let name = v
        .str("username")
        .map(|username| format!("@{username}"))
        .unwrap_or_default();
      Ok((Chat::User(v.str("id").unwrap_or_default()), name))
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
  let mut chats = Vec::new();
  for room in [false, true] {
    for chat in recent(api, &me, room).await? {
      add(&mut chats, chat);
    }
  }
  for chat in rooms(api).await? {
    add(&mut chats, chat);
  }
  Ok(Page::last(chats))
}

fn add(chats: &mut Vec<Collection>, chat: Collection) {
  if !chats.iter().any(|known| known.id == chat.id) {
    chats.push(chat);
  }
}

/// Chats with recent messages (`chat/history`), the last message in `extra`.
async fn recent(api: &Api, me: &str, room: bool) -> Result<Vec<Collection>> {
  let v = api
    .call_auth("chat/history", json!({"limit": 100, "room": room}))
    .await?;
  let instance = api.instance();
  let chats = v.list("").iter().map(|message| {
    let from_me = message.str("fromUserId").as_deref() == Some(me);
    let mut chat = if room {
      parse::room(message.at("toRoom"), &instance)
    } else {
      let other = if from_me { "toUser" } else { "fromUser" };
      parse::direct(message.at(other), &instance)
    };
    if let Some(at) = message.str("createdAt") {
      chat.extra.insert("last_message_at".into(), json!(at));
    }
    if let Some(text) = message.str("text") {
      chat.extra.insert("last_message".into(), json!(text));
    }
    if message.bool("isRead") == Some(false) && !from_me {
      chat.extra.insert("unread".into(), json!(true));
    }
    chat
  });
  Ok(chats.collect())
}

/// Rooms you joined or own.
async fn rooms(api: &Api) -> Result<Vec<Collection>> {
  let instance = api.instance();
  let mut rooms = Vec::new();
  for endpoint in ["chat/rooms/joining", "chat/rooms/owned"] {
    let v = api.call_auth(endpoint, json!({"limit": 100})).await?;
    for row in v.list("") {
      let room = if row.at("room").is_null() {
        row
      } else {
        row.at("room")
      };
      rooms.push(parse::room(room, &instance));
    }
  }
  Ok(rooms)
}

/// Members of a room; for a direct chat, the two of you.
pub async fn members(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  match refs::chat(arg, &api.instance())? {
    ChatRef::Room(id) => {
      api.require()?;
      let body = json!({"roomId": id});
      api
        .list("chat/rooms/members", body, page, |row, instance| {
          parse::user(row.at("user"), instance)
        })
        .await
    }
    ChatRef::User(user) => {
      let other: Value = users::lookup(api, &user).await?;
      let me = users::me(api).await?;
      let instance = api.instance();
      Ok(Page::last(vec![
        parse::user(&me, &instance),
        parse::user(&other, &instance),
      ]))
    }
  }
}
