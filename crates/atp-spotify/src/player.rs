//! Playback control over Spotify Connect: what plays where, the devices,
//! and commands to the active device or to one picked by id, name or `#N`.
//! Spotify allows control with Premium only, and only of a device that has
//! a Spotify app open. ATP never touches the audio itself.

use atp_core::http::Method;
use atp_core::text::fmt_duration;
use atp_core::{Action, Collection, Error, Post, Result, Value, ValueExt, json};

use crate::api::{Api, NO_DEVICE_HINT};
use crate::parse;
use crate::refs::{Item, Kind};

const TYPES: &str = "track,episode";

/// The playback state: the item, its progress, the device, shuffle and
/// repeat; `is_playing: false` alone when no device plays.
pub async fn status(api: &Api) -> Result<Value> {
  let query = [("additional_types", TYPES.to_owned())];
  let v = api.get("/me/player", &query).await?;
  if v.is_null() {
    return Ok(json!({"is_playing": false, "message": "nothing is playing on any device"}));
  }
  let item = parse::item(v.at("item")).map(|p| Post { raw: None, ..p });
  let progress = v.u64("progress_ms").map(|ms| {
    let total = v.u64("item.duration_ms").unwrap_or(0);
    format!(
      "{} / {}",
      fmt_duration(ms as f64 / 1000.0),
      fmt_duration(total as f64 / 1000.0)
    )
  });
  let device = parse::device(v.at("device")).map(|d| Collection { raw: None, ..d });
  Ok(json!({
    "is_playing": v.bool("is_playing").unwrap_or(false),
    "item": item,
    "progress": progress,
    "progress_ms": v.at("progress_ms"),
    "device": device,
    "shuffle": v.at("shuffle_state"),
    "repeat": v.at("repeat_state"),
    "context": v.str("context.uri"),
  }))
}

/// The devices Spotify Connect sees for the account.
pub async fn devices(api: &Api) -> Result<Vec<Collection>> {
  let v = api.get("/me/player/devices", &[]).await?;
  Ok(v.list("devices").iter().filter_map(parse::device).collect())
}

/// A device argument: `#N` from `player devices`, an id, or a name.
pub async fn device(api: &Api, arg: &str) -> Result<String> {
  let arg = api.ctx.collection_ref(arg)?;
  if arg.len() == 40 && arg.bytes().all(|b| b.is_ascii_hexdigit()) {
    return Ok(arg);
  }
  let found = devices(api).await?;
  found
    .iter()
    .find(|d| d.id == arg || d.name.eq_ignore_ascii_case(&arg))
    .map(|d| d.id.clone())
    .ok_or_else(|| Error::not_found(format!("no device {arg}")).with_hint(NO_DEVICE_HINT))
}

/// One command to the player: `device_id` when a device was picked.
pub async fn control(
  api: &Api,
  method: Method,
  path: &str,
  mut query: Vec<(&str, String)>,
  device: Option<&str>,
  body: Option<Value>,
) -> Result<()> {
  if let Some(arg) = device {
    query.push(("device_id", self::device(api, arg).await?));
  }
  let path = format!("/me/player{path}");
  api.send(method, &path, &query, body.as_ref()).await?;
  Ok(())
}

/// What `play` sends: one album, playlist, artist or show to play from its
/// start, or tracks and episodes in order; nothing resumes.
pub fn play_body(items: &[Item]) -> Result<Option<Value>> {
  let context = [Kind::Album, Kind::Playlist, Kind::Artist, Kind::Show];
  match items {
    [] => Ok(None),
    [one] if context.contains(&one.kind) => Ok(Some(json!({"context_uri": one.uri()}))),
    many
      if many
        .iter()
        .all(|i| matches!(i.kind, Kind::Track | Kind::Episode)) =>
    {
      let uris: Vec<String> = many.iter().map(Item::uri).collect();
      Ok(Some(json!({"uris": uris})))
    }
    _ => Err(Error::input(
      "play takes one album, playlist, artist or show, or any number of tracks and episodes",
    )),
  }
}

/// The current item and what is queued after it (`extra.now_playing` on
/// the first).
pub async fn queue(api: &Api) -> Result<Vec<Post>> {
  let v = api.get("/me/player/queue", &[]).await?;
  let mut posts = Vec::new();
  if let Some(mut now) = parse::item(v.at("currently_playing")) {
    now.extra.insert("now_playing".into(), json!(true));
    posts.push(now);
  }
  posts.extend(v.list("queue").iter().filter_map(parse::item));
  Ok(posts)
}

/// Make `device` the one that plays, playing or as it was.
pub async fn transfer(api: &Api, device: &str, play: bool) -> Result<Action> {
  let id = self::device(api, device).await?;
  let body = json!({"device_ids": [id], "play": play});
  api
    .send(Method::PUT, "/me/player", &[], Some(&body))
    .await?;
  Ok(Action::done("player transfer", id))
}

/// `90`, `1:30` or `1:02:03` as milliseconds.
pub fn position(text: &str) -> Result<u64> {
  let mut ms = 0u64;
  for part in text.trim().split(':') {
    let n: f64 = part
      .parse()
      .ok()
      .filter(|n: &f64| *n >= 0.0)
      .ok_or_else(|| Error::input(format!("not a position: {text} (seconds, M:SS or H:MM:SS)")))?;
    ms = ms * 60 + (n * 1000.0) as u64;
  }
  Ok(ms)
}
