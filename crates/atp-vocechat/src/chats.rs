//! Chats: the channels you are in and a direct chat with every other user;
//! names and members.

use atp_core::{Collection, Error, Page, Result, User, Value, ValueExt};

use crate::api::{Api, Mode};
use crate::refs::ChatRef;
use crate::{parse, refs, users};

/// The API path of a channel, as a user or as a bot.
fn group_path(api: &Api, gid: i64) -> String {
  if api.is_bot() {
    format!("/bot/group/{gid}")
  } else {
    format!("/group/{gid}")
  }
}

/// Channels you are in, then direct chats with every other user.
pub async fn list(api: &Api, parent: Option<&str>) -> Result<Page<Collection>> {
  if parent.is_some() {
    return Err(Error::input("VoceChat channels have nothing inside them"));
  }
  let (server, mode) = api.require()?;
  let path = if mode == Mode::Bot { "/bot" } else { "/group" };
  let mut out: Vec<Collection> = api
    .get(path)
    .await?
    .as_array()
    .cloned()
    .unwrap_or_default()
    .iter()
    .map(|g| parse::group(g, &server))
    .collect();
  let me = users::me(api).await?;
  let mut people: Vec<User> = users::all(api).await?.into_values().collect();
  people.sort_by_key(|a| a.name.to_lowercase());
  out.extend(
    people
      .iter()
      .filter(|u| Some(u.id.parse::<i64>().unwrap_or(-1)) != me)
      .map(parse::direct),
  );
  Ok(Page::last(out))
}

/// The display name of a chat: the channel's, or the other person's.
pub async fn name(api: &Api, chat: ChatRef) -> Result<String> {
  Ok(match chat {
    ChatRef::Group(g) => api
      .get(&group_path(api, g))
      .await?
      .str("name")
      .unwrap_or_default(),
    ChatRef::User(u) => users::all(api)
      .await?
      .get(&u)
      .map(|u| u.name.clone())
      .unwrap_or_default(),
  })
}

/// Members of a channel (all users for public channels), or both people of
/// a direct chat.
pub async fn members(api: &Api, arg: &str) -> Result<Page<User>> {
  let users = users::all(api).await?;
  let ids: Vec<i64> = match refs::chat(arg)? {
    ChatRef::Group(g) => {
      let v: Value = api.get(&group_path(api, g)).await?;
      let ids: Vec<i64> = v.list("members").iter().filter_map(Value::as_i64).collect();
      if ids.is_empty() && v.bool("is_public") == Some(true) {
        users.keys().copied().collect()
      } else {
        ids
      }
    }
    ChatRef::User(u) => users::me(api).await?.into_iter().chain([u]).collect(),
  };
  let server = api.server().unwrap_or_default();
  let mut out = Vec::new();
  for id in ids {
    let user = match users.get(&id) {
      Some(u) => u.clone(),
      None if api.is_bot() => parse::user(&api.get(&format!("/bot/user/{id}")).await?, &server),
      None => User {
        id: id.to_string(),
        name: format!("user {id}"),
        ..User::default()
      },
    };
    out.push(user);
  }
  Ok(Page::last(out))
}
