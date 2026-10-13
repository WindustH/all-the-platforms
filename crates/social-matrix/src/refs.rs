//! Rooms, events and users as arguments: Matrix ids (`!room:server`,
//! `#alias:server`, `@user:server`), `matrix.to` links and `matrix:` URIs.
//! An event is `ROOM/$event` (the `id` of listed messages) or a link.

use social_core::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Room {
  Id(String),
  Alias(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
  pub room: Room,
  pub id: String,
}

/// What follows `matrix.to/#/` or a `matrix:` URI, as plain ids.
fn unlink(arg: &str) -> String {
  let arg = arg.trim();
  let decoded = |s: &str| {
    url::form_urlencoded::parse(format!("x={s}").as_bytes())
      .next()
      .map(|(_, v)| v.into_owned())
      .unwrap_or_else(|| s.to_owned())
  };
  if let Some(rest) = arg
    .strip_prefix("https://matrix.to/#/")
    .or_else(|| arg.strip_prefix("http://matrix.to/#/"))
  {
    let rest = rest.split('?').next().unwrap_or(rest);
    return decoded(rest);
  }
  if let Some(rest) = arg.strip_prefix("matrix:") {
    let rest = rest.split('?').next().unwrap_or(rest);
    let mut out = String::new();
    let mut parts = rest.split('/');
    while let Some(kind) = parts.next() {
      let Some(value) = parts.next() else { break };
      let sigil = match kind {
        "roomid" => '!',
        "r" => '#',
        "u" => '@',
        "e" => '$',
        _ => continue,
      };
      if !out.is_empty() {
        out.push('/');
      }
      out.push(sigil);
      out.push_str(&decoded(value));
    }
    return out;
  }
  arg.to_owned()
}

pub fn room(arg: &str) -> Result<Room> {
  let s = unlink(arg);
  let s = s.split('/').next().unwrap_or(&s);
  match s.chars().next() {
    Some('!') if s.contains(':') => Ok(Room::Id(s.to_owned())),
    Some('#') if s.contains(':') => Ok(Room::Alias(s.to_owned())),
    _ => Err(Error::input(format!(
      "not a room: {arg} (expected !id:server, #alias:server or a matrix.to link)"
    ))),
  }
}

pub fn event(arg: &str) -> Result<Event> {
  let s = unlink(arg);
  let Some((room_part, ev)) = s.split_once('/') else {
    return Err(Error::input(format!(
      "not a message: {arg} (expected ROOM/$event or a matrix.to link to an event)"
    )));
  };
  let ev = ev.split('?').next().unwrap_or(ev);
  if !ev.starts_with('$') {
    return Err(Error::input(format!("not an event id: {ev}")));
  }
  Ok(Event {
    room: room(room_part)?,
    id: ev.to_owned(),
  })
}

pub fn user(arg: &str) -> Result<Option<String>> {
  let s = unlink(arg);
  if s == "me" {
    return Ok(None);
  }
  if s.starts_with('@') && s.contains(':') {
    return Ok(Some(s));
  }
  Err(Error::input(format!(
    "not a user: {arg} (expected @name:server, a matrix.to link or `me`)"
  )))
}

pub fn permalink(room: &str, event: Option<&str>) -> String {
  match event {
    Some(e) => format!("https://matrix.to/#/{room}/{e}"),
    None => format!("https://matrix.to/#/{room}"),
  }
}
