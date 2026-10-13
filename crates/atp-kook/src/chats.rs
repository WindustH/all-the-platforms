//! Servers, their channels and members, and direct chats.

use atp_core::{Collection, Error, Page, PageReq, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, Chat};

/// The items of a numbered page and the next page number.
fn paged(v: &Value, page: u64) -> (Vec<Value>, Option<u64>) {
  let items = v.list("items").to_vec();
  let total = v.u64("meta.page_total").unwrap_or(page);
  (items, (page < total).then_some(page + 1))
}

/// Without a parent: servers, then direct chats. With one: its channels.
pub async fn list(api: &Api, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
  match parent {
    Some(parent) => channels(api, parent, page).await,
    None => top(api, page).await,
  }
}

async fn channels(api: &Api, parent: &str, page: &PageReq) -> Result<Page<Collection>> {
  let guild = guild_of(api, &refs::chat(parent)?).await?;
  let number = page.number_or(1);
  let query = [
    ("guild_id", guild),
    ("page", number.to_string()),
    ("page_size", page.size_within(50).to_string()),
  ];
  let v = api.get("/channel/list", &query).await?;
  let (items, next) = paged(&v, number);
  Ok(Page::new(
    items.iter().map(parse::channel).collect(),
    next.map(|n| n.to_string()),
  ))
}

/// Servers (`g:N` cursors), then direct chats (`d:N`).
async fn top(api: &Api, page: &PageReq) -> Result<Page<Collection>> {
  let cursor = page.cursor.clone().unwrap_or_else(|| "g:1".into());
  let (part, number) = cursor
    .split_once(':')
    .and_then(|(p, n)| Some((p.to_owned(), n.parse::<u64>().ok()?)))
    .ok_or_else(|| Error::input(format!("bad cursor {cursor}")))?;
  let query = [
    ("page", number.to_string()),
    ("page_size", page.size_within(50).to_string()),
  ];
  if part == "g" {
    let v = api.get("/guild/list", &query).await?;
    let (items, next) = paged(&v, number);
    let next = Some(next.map_or_else(|| "d:1".to_owned(), |n| format!("g:{n}")));
    return Ok(Page::new(items.iter().map(parse::guild).collect(), next));
  }
  let v = api.get("/user-chat/list", &query).await?;
  let (items, next) = paged(&v, number);
  Ok(Page::new(
    items.iter().map(parse::direct).collect(),
    next.map(|n| format!("d:{n}")),
  ))
}

/// The server a chat belongs to.
async fn guild_of(api: &Api, chat: &Chat) -> Result<String> {
  match chat {
    Chat::Guild(g) => Ok(g.clone()),
    Chat::Channel(c) => {
      let v = api
        .get("/channel/view", &[("target_id", c.clone())])
        .await?;
      v.str("guild_id")
        .ok_or_else(|| Error::not_found(format!("channel {c} has no server")))
    }
    Chat::Direct(_) => Err(Error::input("direct chats have no server")),
  }
}

pub async fn members(api: &Api, chat: &str, page: &PageReq) -> Result<Page<User>> {
  let guild = guild_of(api, &refs::chat(chat)?).await?;
  let number = page.number_or(1);
  let query = [
    ("guild_id", guild),
    ("page", number.to_string()),
    ("page_size", page.size_within(50).to_string()),
  ];
  let v = api.get("/guild/user-list", &query).await?;
  let (items, next) = paged(&v, number);
  Ok(Page::new(
    items.iter().map(parse::user).collect(),
    next.map(|n| n.to_string()),
  ))
}

/// `#name` of a channel, best effort.
pub async fn name(api: &Api, chat: &Chat) -> Option<String> {
  match chat {
    Chat::Channel(c) => api
      .get("/channel/view", &[("target_id", c.clone())])
      .await
      .ok()
      .and_then(|v| v.str("name"))
      .map(|n| format!("#{n}")),
    _ => None,
  }
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
  api.get("/guild/view", &[("guild_id", guild(arg)?)]).await
}

/// A server's roles.
pub async fn roles(api: &Api, arg: &str) -> Result<Value> {
  api
    .get("/guild-role/list", &[("guild_id", guild(arg)?)])
    .await
}
