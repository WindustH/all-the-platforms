//! Chats: servers (`guild:<id>`), their channels, direct chats (opened on
//! use) and members.

use atp_core::{Collection, Error, Page, PageReq, Result, User, ValueExt, json};

use crate::api::Api;
use crate::refs::{self, ChatRef};
use crate::{parse, users};

pub async fn list(api: &Api, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
  let mut body = json!({});
  if let Some(n) = &page.cursor {
    body["next"] = json!(n);
  }
  match parent.map(refs::chat).transpose()? {
    None => {
      let (rows, next) = users::page(&api.call("guild.list", body).await?);
      Ok(Page::new(rows.iter().map(parse::guild).collect(), next))
    }
    Some(ChatRef::Guild(g)) => {
      body["guild_id"] = json!(g);
      let (rows, next) = users::page(&api.call("channel.list", body).await?);
      Ok(Page::new(rows.iter().map(parse::channel).collect(), next))
    }
    Some(_) => Err(Error::input(
      "give a server (guild:<id>) to list its channels",
    )),
  }
}

/// Members of a server, or of the server a channel belongs to.
pub async fn members(api: &Api, chat: &str, page: &PageReq) -> Result<Page<User>> {
  let guild = match refs::chat(chat)? {
    ChatRef::Guild(g) => g,
    ChatRef::Channel(c) => api
      .call("channel.get", json!({"channel_id": c}))
      .await?
      .str("guild_id")
      .ok_or_else(|| Error::input("give a server (guild:<id>)"))?,
    ChatRef::User(_) => return Err(Error::input("give a server (guild:<id>)")),
  };
  users::members(api, &guild, page.cursor.as_deref()).await
}

/// The channel id of a chat argument; direct chats are opened first.
pub async fn channel_id(api: &Api, chat: &str) -> Result<String> {
  match refs::chat(chat)? {
    ChatRef::Channel(c) => Ok(c),
    ChatRef::User(u) => api
      .call("user.channel.create", json!({"user_id": u}))
      .await?
      .str("id")
      .ok_or_else(|| Error::upstream("the server returned no direct channel")),
    ChatRef::Guild(_) => Err(Error::input(
      "a server has channels; list them with `chats guild:<id>`",
    )),
  }
}

pub async fn channel_name(api: &Api, id: &str) -> Option<String> {
  api
    .call("channel.get", json!({"channel_id": id}))
    .await
    .ok()
    .and_then(|v| v.str("name"))
}
