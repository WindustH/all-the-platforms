//! Servers, channels, threads and direct chats; who is in them.

use social_core::{
  Collection, Error, ErrorCode, Page, PageReq, Result, User, Value, ValueExt, json,
};

use crate::api::{Api, Kind};
use crate::parse;
use crate::refs::{self, Chat};

/// The channel a chat argument names: a user's direct chat is opened (or
/// found) first; a server is refused with a pointer to its channels.
pub async fn channel_id(api: &Api, arg: &str) -> Result<String> {
  match refs::chat(arg)? {
    Chat::Channel(id) | Chat::Id(id) => Ok(id),
    Chat::User(user) => open_dm(api, &user).await,
    Chat::Server(id) => Err(server_not_channel(&id)),
  }
}

fn server_not_channel(id: &str) -> Error {
  Error::input(format!("{id} is a server, not a channel"))
    .with_hint(format!("`social discord chats {id}` lists its channels"))
}

/// The direct chat with a user (created if there is none yet).
pub async fn open_dm(api: &Api, user: &str) -> Result<String> {
  let v = api
    .post("/users/@me/channels", json!({"recipients": [user]}))
    .await?;
  api.remember_channel(&v);
  v.str("id")
    .ok_or_else(|| Error::upstream("Discord opened no direct chat"))
}

/// Top level: servers (200 per page) and, on the first page, direct chats
/// with the most recent first. Inside a server: its channels in the order of
/// the channel list, then active threads. Inside a channel: its threads.
pub async fn list(api: &Api, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
  match parent {
    None => top(api, page).await,
    Some(arg) => match refs::chat(arg)? {
      Chat::Server(id) => server_channels(api, &id).await,
      Chat::Channel(id) => threads(api, &id, page).await,
      Chat::User(_) => Err(Error::input("a direct chat has no channels inside")),
      Chat::Id(id) => match server_channels(api, &id).await {
        Err(e) if e.code == ErrorCode::NotFound || e.code == ErrorCode::PermissionDenied => {
          threads(api, &id, page).await
        }
        other => other,
      },
    },
  }
}

async fn top(api: &Api, page: &PageReq) -> Result<Page<Collection>> {
  let mut query = vec![
    ("with_counts", "true".to_owned()),
    ("limit", "200".to_owned()),
  ];
  if let Some(after) = &page.cursor {
    query.push(("after", after.clone()));
  }
  let guilds = api.get("/users/@me/guilds", &query).await?;
  let guilds = guilds.as_array().cloned().unwrap_or_default();
  let mut items = Vec::new();
  if page.cursor.is_none() && !api.is_bot() {
    let dms = api.get("/users/@me/channels", &[]).await?;
    let mut dms = dms.as_array().cloned().unwrap_or_default();
    dms.sort_by_key(|c| std::cmp::Reverse(c.u64("last_message_id").unwrap_or_default()));
    for c in &dms {
      api.remember_channel(c);
    }
    items.extend(dms.iter().map(parse::channel));
  }
  let next = (guilds.len() >= 200)
    .then(|| guilds.last().and_then(|g| g.str("id")))
    .flatten();
  items.extend(guilds.iter().map(parse::guild));
  Ok(Page::new(items, next))
}

/// A server's channels: categories each followed by their channels, by
/// position, voice after text; then the active threads (where Discord lists
/// them for this kind of token).
async fn server_channels(api: &Api, guild: &str) -> Result<Page<Collection>> {
  let v = api.get(&format!("/guilds/{guild}/channels"), &[]).await?;
  let mut channels = v.as_array().cloned().unwrap_or_default();
  for c in &mut channels {
    if let Some(o) = c.as_object_mut() {
      o.entry("guild_id").or_insert(json!(guild));
    }
    api.remember_channel(c);
  }
  let position = |c: &Value| c.i64("position").unwrap_or_default();
  let category_of = |c: &Value| -> i64 {
    let parent = if c.u64("type") == Some(4) {
      c.str("id")
    } else {
      c.str("parent_id")
    };
    match parent {
      None => -1,
      Some(p) => channels
        .iter()
        .find(|x| x.str("id").as_deref() == Some(p.as_str()))
        .map(position)
        .unwrap_or_default(),
    }
  };
  let voice = |c: &Value| matches!(c.u64("type"), Some(2 | 13)) as i64;
  let mut sorted = channels.clone();
  sorted.sort_by_key(|c| {
    (
      category_of(c),
      (c.u64("type") != Some(4)) as i64,
      voice(c),
      position(c),
    )
  });
  let mut items: Vec<Collection> = sorted.iter().map(parse::channel).collect();
  // Active threads: bots read them from the server; ignore refusals.
  if api.is_bot()
    && let Ok(t) = api
      .get(&format!("/guilds/{guild}/threads/active"), &[])
      .await
  {
    for th in t.list("threads") {
      api.remember_channel(th);
      items.push(parse::channel(th));
    }
  }
  Ok(Page::last(items))
}

/// Threads of a text or forum channel: user accounts search them (active and
/// archived, newest activity first); bots list the archived public ones.
async fn threads(api: &Api, channel: &str, page: &PageReq) -> Result<Page<Collection>> {
  let info = api.channel(channel).await?;
  if matches!(info.kind, 1 | 3) {
    return Err(Error::input(format!(
      "{} is a direct chat; read it with `social discord messages {channel}`",
      info.name
    )));
  }
  let token = api.require()?;
  let (rows, next) = if token.kind == Kind::User {
    let offset = page.number_or(0);
    let v = api
      .get(
        &format!("/channels/{channel}/threads/search"),
        &[
          ("archived", "true".into()),
          ("sort_by", "last_message_time".into()),
          ("sort_order", "desc".into()),
          ("limit", "25".into()),
          ("offset", offset.to_string()),
        ],
      )
      .await?;
    let rows = v.list("threads").to_vec();
    let next = v
      .bool("has_more")
      .unwrap_or(false)
      .then(|| (offset + rows.len() as u64).to_string());
    (rows, next)
  } else {
    let mut query = vec![("limit", "50".to_owned())];
    if let Some(before) = &page.cursor {
      query.push(("before", before.clone()));
    }
    let v = api
      .get(
        &format!("/channels/{channel}/threads/archived/public"),
        &query,
      )
      .await?;
    let rows = v.list("threads").to_vec();
    let next = v
      .bool("has_more")
      .unwrap_or(false)
      .then(|| {
        rows
          .last()
          .and_then(|t| t.str("thread_metadata.archive_timestamp"))
      })
      .flatten();
    (rows, next)
  };
  let items = rows
    .iter()
    .map(|t| {
      api.remember_channel(t);
      parse::channel(t)
    })
    .collect();
  Ok(Page::new(items, next))
}

/// Members of a server (bots: the member list, up to 1000 a page; user
/// accounts: Discord keeps full lists to moderators and bots), or the people
/// of a direct or group chat.
pub async fn members(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  let id = match refs::chat(arg)? {
    Chat::Server(id) => return guild_members(api, &id, page).await,
    Chat::Channel(id) | Chat::Id(id) => id,
    Chat::User(_) => return Err(Error::input("give a server or a chat, not a user")),
  };
  let v = match api.get(&format!("/channels/{id}"), &[]).await {
    Ok(v) => v,
    // Not a channel: a server id.
    Err(e) if e.code == ErrorCode::NotFound => return guild_members(api, &id, page).await,
    Err(e) => return Err(e),
  };
  api.remember_channel(&v);
  if let Some(guild) = v.str("guild_id") {
    return guild_members(api, &guild, page).await;
  }
  let mut users: Vec<User> = v.list("recipients").iter().map(parse::user).collect();
  if let Ok(me) = api.get("/users/@me", &[]).await {
    users.insert(0, parse::user(&me));
  }
  Ok(Page::last(users))
}

async fn guild_members(api: &Api, guild: &str, page: &PageReq) -> Result<Page<User>> {
  let size = page.size_within(1000);
  let mut query = vec![("limit", size.to_string())];
  if let Some(after) = &page.cursor {
    query.push(("after", after.clone()));
  }
  let v = api
    .get(&format!("/guilds/{guild}/members"), &query)
    .await
    .map_err(|e| {
      if e.code == ErrorCode::PermissionDenied && !api.is_bot() {
        e.with_hint(
          "Discord lists server members to bots (with the Server Members intent) and moderators; \
           use a bot token, or `members` on a direct or group chat",
        )
      } else {
        e
      }
    })?;
  let rows = v.as_array().cloned().unwrap_or_default();
  let next = (rows.len() >= size)
    .then(|| rows.last().and_then(|m| m.str("user.id")))
    .flatten();
  Ok(Page::new(rows.iter().map(parse::member).collect(), next))
}

/// Members whose name starts with `query`, in one server.
pub async fn search_members(
  api: &Api,
  guild: &str,
  query: &str,
  page: &PageReq,
) -> Result<Page<User>> {
  let v = api
    .get(
      &format!("/guilds/{guild}/members/search"),
      &[
        ("query", query.to_owned()),
        ("limit", page.size_within(1000).to_string()),
      ],
    )
    .await?;
  let rows = v.as_array().cloned().unwrap_or_default();
  Ok(Page::last(rows.iter().map(parse::member).collect()))
}
