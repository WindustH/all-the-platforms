//! Rooms: the joined ones (names, unread counts, direct chats) from one
//! filtered sync, a space's rooms, members, joining and leaving, and the
//! public room directory.

use std::collections::{BTreeMap, HashSet};

use atp_core::http::Method;
use atp_core::{Action, Collection, Page, PageReq, Query, Result, User, Value, ValueExt, json};

use crate::api::{Api, seg};
use crate::parse::{self, Members};
use crate::refs::{self, Room};

/// The room id behind an argument (aliases resolved through the directory).
pub async fn id(api: &Api, arg: &str) -> Result<String> {
  match refs::room(arg)? {
    Room::Id(id) => Ok(id),
    Room::Alias(alias) => resolve_alias(api, &alias).await,
  }
}

pub async fn resolve_alias(api: &Api, alias: &str) -> Result<String> {
  let v = api
    .call(
      Method::GET,
      &format!("/directory/room/{}", seg(alias)),
      &[],
      None,
    )
    .await?;
  Ok(v.str("room_id").unwrap_or_default())
}

/// The filter of the sync behind `chats` and `unread`: room names, kinds and
/// counts, one timeline event each, no presence or receipts.
fn summary_filter() -> String {
  json!({
    "presence": {"types": []},
    "account_data": {"types": ["m.direct"]},
    "room": {
      "timeline": {"limit": 1},
      "state": {
        "lazy_load_members": true,
        "types": ["m.room.name", "m.room.canonical_alias", "m.room.create", "m.room.topic",
                  "m.room.encryption", "m.room.member"]
      },
      "ephemeral": {"types": []},
      "account_data": {"types": []}
    }
  })
  .to_string()
}

pub async fn summary_sync(api: &Api) -> Result<Value> {
  api
    .get(
      "/sync",
      &[("filter", summary_filter()), ("timeout", "0".into())],
    )
    .await
}

/// Joined and invited rooms, most recently active first.
pub async fn list(api: &Api) -> Result<Page<Collection>> {
  let v = summary_sync(api).await?;
  let me = api.user_id().unwrap_or_default();
  let direct = direct_rooms(&v);
  let mut out: Vec<(i64, Collection)> = Vec::new();
  for section in ["join", "invite"] {
    let Some(rooms) = v.at("rooms").get(section).and_then(Value::as_object) else {
      continue;
    };
    for (rid, r) in rooms {
      out.push(room(rid, r, section == "invite", direct.contains(rid), &me));
    }
  }
  out.sort_by_key(|a| std::cmp::Reverse(a.0));
  Ok(Page::last(out.into_iter().map(|(_, c)| c).collect()))
}

/// Rooms the account marked as direct chats (`m.direct` account data).
fn direct_rooms(v: &Value) -> HashSet<String> {
  v.list("account_data.events")
    .iter()
    .filter(|e| e.str("type").as_deref() == Some("m.direct"))
    .filter_map(|e| e.at("content").as_object().cloned())
    .flat_map(|m| m.into_values())
    .flat_map(|rooms| {
      rooms
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|r| r.as_str().map(str::to_owned))
    })
    .collect()
}

/// One room of the sync summary, with its last activity for sorting.
fn room(rid: &str, r: &Value, invite: bool, direct: bool, me: &str) -> (i64, Collection) {
  let state: Vec<Value> = r
    .list("state.events")
    .iter()
    .chain(r.list("timeline.events"))
    .chain(r.list("invite_state.events"))
    .cloned()
    .collect();
  let get = |t: &str, key: &str| {
    state
      .iter()
      .rev()
      .find(|e| e.str("type").as_deref() == Some(t))
      .and_then(|e| e.at("content").str(key))
  };
  let space = get("m.room.create", "type").as_deref() == Some("m.space");
  let kind = match (invite, space, direct) {
    (true, ..) => "invite",
    (_, true, _) => "space",
    (_, _, true) => "dm",
    _ => "room",
  };
  let mut c = Collection {
    id: rid.to_owned(),
    kind: kind.into(),
    name: room_name(rid, r, &state, me),
    description: get("m.room.topic", "topic"),
    url: Some(refs::permalink(rid, None)),
    followers: r
      .at("summary")
      .get("m.joined_member_count")
      .and_then(Value::as_u64),
    raw: Some(r.clone()),
    ..Collection::default()
  };
  if let Some(alias) = get("m.room.canonical_alias", "alias") {
    c.extra.insert("alias".into(), json!(alias));
  }
  room_flags(&mut c, r, &state);
  let last = r
    .list("timeline.events")
    .last()
    .and_then(|e| e.i64("origin_server_ts"))
    .unwrap_or_default();
  (last, c)
}

/// The room's name, else its alias, else the names of its other members.
fn room_name(rid: &str, r: &Value, state: &[Value], me: &str) -> String {
  let get = |t: &str, key: &str| {
    state
      .iter()
      .rev()
      .find(|e| e.str("type").as_deref() == Some(t))
      .and_then(|e| e.at("content").str(key))
  };
  let mut members = Members::default();
  members.absorb(state);
  let heroes: Vec<String> = r
    .at("summary")
    .get("m.heroes")
    .and_then(Value::as_array)
    .map(|h| {
      h.iter()
        .filter_map(Value::as_str)
        .filter(|id| *id != me)
        .map(|id| members.user(id).name)
        .collect()
    })
    .unwrap_or_default();
  get("m.room.name", "name")
    .or_else(|| get("m.room.canonical_alias", "alias"))
    .or_else(|| (!heroes.is_empty()).then(|| heroes.join(", ")))
    .unwrap_or_else(|| rid.to_owned())
}

/// Unread and highlight counts, and whether the room is encrypted.
fn room_flags(collection: &mut Collection, r: &Value, state: &[Value]) {
  for (key, path) in [
    ("unread", "unread_notifications.notification_count"),
    ("highlights", "unread_notifications.highlight_count"),
  ] {
    if let Some(n) = r.u64(path).filter(|n| *n > 0) {
      collection.extra.insert(key.into(), json!(n));
    }
  }
  if state
    .iter()
    .any(|e| e.str("type").as_deref() == Some("m.room.encryption"))
  {
    collection.extra.insert("encrypted".into(), json!(true));
  }
}

/// Unread counts: per kind of room, and highlights (mentions).
pub async fn unread(api: &Api) -> Result<BTreeMap<String, u64>> {
  let page = list(api).await?;
  let mut out = BTreeMap::new();
  for c in &page.items {
    let n = c.extra.get("unread").and_then(Value::as_u64).unwrap_or(0);
    let h = c
      .extra
      .get("highlights")
      .and_then(Value::as_u64)
      .unwrap_or(0);
    *out.entry("notifications".to_owned()).or_insert(0) += n;
    *out.entry("highlights".to_owned()).or_insert(0) += h;
    if c.kind == "invite" {
      *out.entry("invites".to_owned()).or_insert(0) += 1;
    }
    if n > 0 {
      *out.entry("rooms_with_unread".to_owned()).or_insert(0) += 1;
    }
  }
  Ok(out)
}

/// The rooms of a space (`/hierarchy`), paged by its token.
pub async fn space(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Collection>> {
  let rid = id(api, arg).await?;
  let mut query = vec![("limit", page.size_within(50).to_string())];
  if let Some(c) = &page.cursor {
    query.push(("from", c.clone()));
  }
  let v = api
    .get(
      &format!("/_matrix/client/v1/rooms/{}/hierarchy", seg(&rid)),
      &query,
    )
    .await?;
  let items = v
    .list("rooms")
    .iter()
    .filter(|r| r.str("room_id").as_deref() != Some(rid.as_str()))
    .map(parse::directory_room)
    .collect();
  Ok(Page::new(items, v.str("next_batch")))
}

pub async fn members(api: &Api, arg: &str) -> Result<Page<User>> {
  let rid = id(api, arg).await?;
  let v = api
    .get(&format!("/rooms/{}/joined_members", seg(&rid)), &[])
    .await?;
  let mut items: Vec<User> = v
    .at("joined")
    .as_object()
    .map(|m| {
      m.iter()
        .map(|(id, info)| parse::user(id, info.str("display_name"), info.str("avatar_url")))
        .collect()
    })
    .unwrap_or_default();
  items.sort_by_key(|a| a.name.to_lowercase());
  Ok(Page::last(items))
}

pub async fn join(api: &Api, arg: &str, leave: bool) -> Result<Action> {
  if leave {
    let rid = id(api, arg).await?;
    api
      .write(
        Method::POST,
        &format!("/rooms/{}/leave", seg(&rid)),
        &json!({}),
      )
      .await?;
    return Ok(Action::done("leave", &rid));
  }
  let target = match refs::room(arg)? {
    Room::Id(i) | Room::Alias(i) => i,
  };
  let v = api
    .write(Method::POST, &format!("/join/{}", seg(&target)), &json!({}))
    .await?;
  let rid = v.str("room_id").unwrap_or(target);
  Ok(Action::done("join", &rid).with_url(refs::permalink(&rid, None)))
}

/// The public room directory of the homeserver (or `server`). With a query
/// it searches when logged in; anonymously, the first pages are filtered.
pub async fn directory(
  api: &Api,
  query: Option<&Query>,
  server: Option<&str>,
  page: &PageReq,
) -> Result<Page<Collection>> {
  let size = page.size_within(100);
  let term = query
    .map(|q| q.keyword.trim().to_lowercase())
    .filter(|t| !t.is_empty());
  match term {
    Some(term) if !api.logged_in() => filter_directory(api, &term, server, page, size).await,
    term => directory_page(api, term.as_deref(), server, page, size).await,
  }
}

/// One page of the directory, searched by the server when logged in.
async fn directory_page(
  api: &Api,
  term: Option<&str>,
  server: Option<&str>,
  page: &PageReq,
  size: usize,
) -> Result<Page<Collection>> {
  let mut q: Vec<(&str, String)> = Vec::new();
  if let Some(s) = server {
    q.push(("server", s.to_owned()));
  }
  let v = if api.logged_in() {
    let mut body = json!({"limit": size});
    if let Some(c) = &page.cursor {
      body["since"] = json!(c);
    }
    if let Some(t) = term {
      body["filter"] = json!({"generic_search_term": t});
    }
    api
      .auth(Method::POST, "/publicRooms", &q, Some(&body))
      .await?
  } else {
    q.push(("limit", size.to_string()));
    if let Some(c) = &page.cursor {
      q.push(("since", c.clone()));
    }
    api.call(Method::GET, "/publicRooms", &q, None).await?
  };
  let items = v.list("chunk").iter().map(parse::directory_room).collect();
  Ok(Page::new(items, v.str("next_batch")))
}

/// Anonymous search: filter a few pages of the directory.
async fn filter_directory(
  api: &Api,
  term: &str,
  server: Option<&str>,
  page: &PageReq,
  size: usize,
) -> Result<Page<Collection>> {
  let mut since = page.cursor.clone();
  let mut found = Vec::new();
  for _ in 0..5 {
    let mut q = vec![("limit", "100".to_owned())];
    if let Some(s) = server {
      q.push(("server", s.to_owned()));
    }
    if let Some(c) = &since {
      q.push(("since", c.clone()));
    }
    let v = api.call(Method::GET, "/publicRooms", &q, None).await?;
    for r in v.list("chunk") {
      let hay = format!(
        "{} {} {}",
        r.str("name").unwrap_or_default(),
        r.str("topic").unwrap_or_default(),
        r.str("canonical_alias").unwrap_or_default()
      )
      .to_lowercase();
      if hay.contains(term) {
        found.push(parse::directory_room(r));
      }
    }
    since = v.str("next_batch");
    if since.is_none() || found.len() >= size {
      break;
    }
  }
  Ok(Page::new(found, since))
}
