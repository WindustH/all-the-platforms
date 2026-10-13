//! Rooms (chats): your subscriptions (channels, private groups, direct
//! messages, discussions), the rooms of a team, finding a room by name,
//! opening a direct room, joining and leaving, members and unread counts.

use std::collections::BTreeMap;

use social_core::{Action, Collection, Error, Page, PageReq, Result, User, Value, ValueExt, json};

use crate::api::Api;
use crate::parse::{self, View};
use crate::refs::{self, RoomRef};

/// What message listings need of a room.
#[derive(Debug, Clone)]
pub struct RoomInfo {
  pub name: String,
  pub kind: String,
  /// Link path: `channel/NAME`, `group/NAME`, `direct/RID`.
  pub path: String,
  /// Rocket.Chat's type letter (`c`, `p`, `d`, `l`).
  pub t: String,
}

pub fn info_of(v: &Value) -> RoomInfo {
  let id = v.first_str(&["rid", "_id"]).unwrap_or_default();
  let t = v.str("t").unwrap_or_else(|| "c".into());
  let slug = v.str("name").unwrap_or_default();
  let display = v.str("fname").unwrap_or_else(|| slug.clone());
  let kind = if v.str("prid").is_some() {
    "discussion"
  } else if v.bool("teamMain") == Some(true) {
    "team"
  } else {
    match t.as_str() {
      "p" => "private",
      "d" => "dm",
      "l" => "livechat",
      _ => "channel",
    }
  };
  let (name, path) = match t.as_str() {
    "d" => {
      let who = if display.is_empty() {
        v.list("usernames")
          .iter()
          .filter_map(|u| u.as_str())
          .collect::<Vec<_>>()
          .join(", ")
      } else {
        display
      };
      (format!("@{who}"), format!("direct/{id}"))
    }
    "p" => (display, format!("group/{slug}")),
    "l" => (display, format!("live/{id}")),
    _ => (format!("#{display}"), format!("channel/{slug}")),
  };
  RoomInfo {
    name,
    kind: kind.into(),
    path,
    t,
  }
}

pub fn remember(api: &Api, rows: &[Value]) {
  let mut rooms = api.rooms.borrow_mut();
  for r in rows {
    if let Some(id) = r.first_str(&["rid", "_id"]) {
      rooms.insert(id, info_of(r));
    }
  }
}

/// Run `f` with what parsing needs.
pub fn render<T>(api: &Api, f: impl FnOnce(&View) -> T) -> Result<T> {
  let creds = api.require()?;
  let rooms = api.rooms.borrow();
  Ok(f(&View {
    creds: &creds,
    rooms: &rooms,
  }))
}

pub async fn list(api: &Api, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
  match parent {
    None => subscriptions(api).await,
    Some(team) => team_rooms(api, team, page).await,
  }
}

/// The rooms you are in, most recently active first.
async fn subscriptions(api: &Api) -> Result<Page<Collection>> {
  let v = api.get("subscriptions.get", &[]).await?;
  let mut rows = v.list("update").to_vec();
  rows.sort_by_key(|s| {
    std::cmp::Reverse(
      parse::time(s.at("_updatedAt"))
        .map(|t| t.as_millisecond())
        .unwrap_or_default(),
    )
  });
  remember(api, &rows);
  let items = render(api, |view| {
    rows.iter().map(|r| parse::room(r, view)).collect()
  })?;
  Ok(Page::last(items))
}

/// The rooms of a team (by id or name), paged by offset.
async fn team_rooms(api: &Api, team: &str, page: &PageReq) -> Result<Page<Collection>> {
  let offset = page.number_or(0);
  let count = page.size_within(100);
  let key = if refs::is_id(team) {
    "teamId"
  } else {
    "teamName"
  };
  let v = api
    .get(
      "teams.listRooms",
      &[
        (key, team.trim_start_matches('#').to_owned()),
        ("offset", offset.to_string()),
        ("count", count.to_string()),
      ],
    )
    .await?;
  let rows = v.list("rooms").to_vec();
  remember(api, &rows);
  let total = v.u64("total").unwrap_or_default();
  let seen = offset + rows.len() as u64;
  let next = (seen < total).then(|| seen.to_string());
  let items = render(api, |view| {
    rows.iter().map(|r| parse::room(r, view)).collect()
  })?;
  Ok(Page::new(items, next))
}

/// One room: topic, members, message count.
pub async fn room(api: &Api, arg: &str) -> Result<Page<Collection>> {
  let rid = room_id(api, arg).await?;
  let v = api.get("rooms.info", &[("roomId", rid)]).await?;
  let c = render(api, |view| parse::room(v.at("room"), view))?;
  Ok(Page::last(vec![c]))
}

/// Join a public channel, or leave a room with `undo`.
pub async fn join(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let rid = room_id(api, arg).await?;
  if undo {
    let info = info(api, &rid).await?;
    api
      .post(
        &format!("{}.leave", family(&info.t)),
        json!({"roomId": rid}),
      )
      .await?;
  } else {
    api.post("channels.join", json!({"roomId": rid})).await?;
  }
  Ok(Action::done(if undo { "leave" } else { "join" }, rid))
}

/// Unread messages per room.
pub async fn unread(api: &Api) -> Result<BTreeMap<String, u64>> {
  let v = api.get("subscriptions.get", &[]).await?;
  let rows = v.list("update").to_vec();
  remember(api, &rows);
  let known = api.rooms.borrow();
  Ok(
    rows
      .iter()
      .filter_map(|s| {
        let n = s.u64("unread").filter(|n| *n > 0)?;
        let rid = s.str("rid")?;
        Some((known.get(&rid).map(|r| r.name.clone()).unwrap_or(rid), n))
      })
      .collect(),
  )
}

/// The room id an argument names.
pub async fn room_id(api: &Api, arg: &str) -> Result<String> {
  let v = match refs::room(arg) {
    RoomRef::Id(id) => {
      info(api, &id).await?;
      return Ok(id);
    }
    RoomRef::Name(name) => api.get("rooms.info", &[("roomName", name)]).await?,
    RoomRef::User(user) => api.post("im.create", json!({"username": user})).await?,
  };
  let room = v.at("room");
  remember(api, std::slice::from_ref(room));
  room
    .first_str(&["_id", "rid"])
    .ok_or_else(|| Error::not_found(format!("no room {arg}")))
}

/// What a room is, asked once.
pub async fn info(api: &Api, rid: &str) -> Result<RoomInfo> {
  if let Some(r) = api.rooms.borrow().get(rid) {
    return Ok(r.clone());
  }
  let v = api.get("rooms.info", &[("roomId", rid.to_owned())]).await?;
  remember(api, std::slice::from_ref(v.at("room")));
  api
    .rooms
    .borrow()
    .get(rid)
    .cloned()
    .ok_or_else(|| Error::not_found(format!("no room {rid}")))
}

/// `channels`, `groups` or `im`: the API family of a room's type.
pub fn family(t: &str) -> &'static str {
  match t {
    "p" => "groups",
    "d" => "im",
    _ => "channels",
  }
}

pub async fn members(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  let rid = room_id(api, arg).await?;
  let room = info(api, &rid).await?;
  let offset = page.number_or(0);
  let count = page.size_within(100);
  let v = api
    .get(
      &format!("{}.members", family(&room.t)),
      &[
        ("roomId", rid),
        ("offset", offset.to_string()),
        ("count", count.to_string()),
      ],
    )
    .await?;
  let server = api.require()?.server;
  let users: Vec<User> = v
    .list("members")
    .iter()
    .map(|m| parse::user(m, &server))
    .collect();
  let total = v.u64("total").unwrap_or_default();
  let seen = offset + users.len() as u64;
  let next = (seen < total).then(|| seen.to_string());
  Ok(Page::new(users, next))
}
