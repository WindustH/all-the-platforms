//! Servers (频道), their sub-channels, roles and members, and 私信 sessions.

use atp_core::{Action, Collection, Error, Page, PageReq, Result, User, Value, ValueExt, json};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, Chat};

/// Without a parent: the bot's servers. With one: its sub-channels.
pub async fn list(api: &Api, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
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

/// A server's id from a chat argument that must name one.
fn guild(arg: &str) -> Result<String> {
  match refs::chat(arg)? {
    Chat::Guild(g) => Ok(g),
    _ => Err(Error::input("give a server (guild:<id>)")),
  }
}

/// A server's details.
pub async fn server(api: &Api, arg: &str) -> Result<Value> {
  api.get(&format!("/guilds/{}", guild(arg)?), &[]).await
}

/// A server's roles.
pub async fn roles(api: &Api, arg: &str) -> Result<Value> {
  api
    .get(&format!("/guilds/{}/roles", guild(arg)?), &[])
    .await
}

/// Open a 私信 session with a member of a server; its `dm:` chat.
pub async fn open_dm(api: &Api, user: &str, server: &str) -> Result<Action> {
  let body = json!({"recipient_id": user, "source_guild_id": guild(server)?});
  let v = api.post("/users/@me/dms", &body).await?;
  let session = v
    .str("guild_id")
    .ok_or_else(|| Error::upstream("QQ returned no 私信 session"))?;
  Ok(Action::done("open-dm", user).with_id(format!("dm:{session}")))
}
