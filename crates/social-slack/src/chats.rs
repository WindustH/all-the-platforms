//! Conversations: the account's channels, private channels, group and
//! direct messages (`users.conversations`), finding one by name, opening a
//! direct conversation with a user, and members.

use std::collections::HashMap;

use social_core::{Collection, Error, Page, PageReq, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::parse::{self, View};
use crate::refs::{self, is_chat_id};
use crate::users::{self, Brief};

const TYPES: &str = "public_channel,private_channel,mpim,im";

/// A conversation's name and kind, for message listings.
#[derive(Debug, Clone)]
pub struct ChatInfo {
  pub name: String,
  pub kind: String,
}

pub fn info_of(v: &Value, users: &HashMap<String, Brief>) -> ChatInfo {
  let kind = if v.bool("is_im") == Some(true) {
    "dm"
  } else if v.bool("is_mpim") == Some(true) {
    "group"
  } else if v.bool("is_private") == Some(true) {
    "private"
  } else {
    "channel"
  };
  let name = match (kind, v.str("user")) {
    ("dm", Some(u)) => users
      .get(&u)
      .map(|b| format!("@{}", b.name))
      .unwrap_or_else(|| format!("@{u}")),
    _ => v
      .first_str(&["name_normalized", "name"])
      .map(|n| match kind {
        "channel" | "private" => format!("#{n}"),
        _ => n,
      })
      .unwrap_or_default(),
  };
  ChatInfo {
    name,
    kind: kind.into(),
  }
}

/// Learn the names of conversations (and the users of direct ones).
async fn remember(api: &Api, rows: &[Value]) {
  users::resolve(api, rows.iter().filter_map(|c| c.str("user"))).await;
  let known = api.users.borrow().clone();
  let mut chats = api.chats.borrow_mut();
  for c in rows {
    if let Some(id) = c.str("id") {
      chats.insert(id, info_of(c, &known));
    }
  }
}

/// The conversations the account is in, most useful first; `parent` (an
/// Enterprise Grid workspace id, `T…`) narrows to one workspace.
pub async fn list(api: &Api, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
  let mut params = vec![
    ("types", TYPES.to_owned()),
    ("exclude_archived", "true".to_owned()),
    ("limit", page.size_within(200).to_string()),
  ];
  if let Some(team) = parent {
    if !team.starts_with('T') {
      return Err(Error::input(
        "Slack conversations are not nested; `chats` lists them all (PARENT is an Enterprise Grid workspace id, T…)",
      ));
    }
    params.push(("team_id", team.to_owned()));
  }
  if let Some(c) = &page.cursor {
    params.push(("cursor", c.clone()));
  }
  let v = api.call("users.conversations", params).await?;
  let rows = v.list("channels").to_vec();
  remember(api, &rows).await;
  let team_url = api.team_url().await;
  let token = api.require()?;
  let users = api.users.borrow();
  let chats = api.chats.borrow();
  let view = View {
    team_url: team_url.as_deref(),
    token: &token,
    users: &users,
    chats: &chats,
  };
  let items = rows.iter().map(|c| parse::chat(c, &view)).collect();
  let next = v
    .str("response_metadata.next_cursor")
    .filter(|c| !c.is_empty());
  Ok(Page::new(items, next))
}

/// The conversation id an argument names: an id or link, `#name` / `name`,
/// or a user (`@name`, `U…`, `<@U…>`) for the direct conversation.
pub async fn id(api: &Api, arg: &str) -> Result<String> {
  let arg = arg.trim();
  if is_chat_id(arg) {
    return Ok(arg.to_owned());
  }
  if let Some(id) = refs::chat_link(arg) {
    return Ok(id);
  }
  if arg.starts_with('@') || arg.starts_with("<@") || users::is_user_id(arg) {
    let user = users::user_id(api, arg).await?;
    return open_dm(api, &user).await;
  }
  let name = arg.trim_start_matches('#').to_lowercase();
  for method in ["users.conversations", "conversations.list"] {
    let mut cursor = String::new();
    for _ in 0..20 {
      let mut params = vec![
        ("types", "public_channel,private_channel,mpim".to_owned()),
        ("exclude_archived", "true".to_owned()),
        ("limit", "200".to_owned()),
      ];
      if !cursor.is_empty() {
        params.push(("cursor", cursor.clone()));
      }
      let v = api.call(method, params).await?;
      if let Some(c) = v
        .list("channels")
        .iter()
        .find(|c| c.str("name").map(|n| n.to_lowercase()) == Some(name.clone()))
      {
        remember(api, std::slice::from_ref(c)).await;
        return c
          .str("id")
          .ok_or_else(|| Error::upstream("conversation without id"));
      }
      cursor = v.str("response_metadata.next_cursor").unwrap_or_default();
      if cursor.is_empty() {
        break;
      }
    }
  }
  Err(Error::not_found(format!(
    "no conversation named {arg}; `social slack chats` lists yours"
  )))
}

/// The direct conversation with a user (opened if needed).
pub async fn open_dm(api: &Api, user: &str) -> Result<String> {
  let v = api
    .write("conversations.open", vec![("users", user.to_owned())])
    .await?;
  let c = v.at("channel");
  remember(api, std::slice::from_ref(c)).await;
  c.str("id")
    .ok_or_else(|| Error::upstream("conversations.open returned no channel"))
}

/// Name and kind of a conversation (asked once, then remembered).
pub async fn info(api: &Api, id: &str) -> ChatInfo {
  if let Some(i) = api.chats.borrow().get(id) {
    return i.clone();
  }
  match api
    .call("conversations.info", vec![("channel", id.to_owned())])
    .await
  {
    Ok(v) => {
      remember(api, std::slice::from_ref(v.at("channel"))).await;
      api.chats.borrow().get(id).cloned().unwrap_or(ChatInfo {
        name: id.to_owned(),
        kind: "channel".into(),
      })
    }
    Err(_) => ChatInfo {
      name: id.to_owned(),
      kind: "channel".into(),
    },
  }
}

pub async fn members(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  let channel = id(api, arg).await?;
  let mut params = vec![
    ("channel", channel),
    ("limit", page.size_within(200).to_string()),
  ];
  if let Some(c) = &page.cursor {
    params.push(("cursor", c.clone()));
  }
  let v = api.call("conversations.members", params).await?;
  let ids: Vec<String> = v
    .list("members")
    .iter()
    .filter_map(|m| m.as_str().map(str::to_owned))
    .collect();
  let next = v
    .str("response_metadata.next_cursor")
    .filter(|c| !c.is_empty());
  Ok(Page::new(users::many(api, &ids).await, next))
}
