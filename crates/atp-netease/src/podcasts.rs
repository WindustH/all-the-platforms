//! Podcasts (播客 / 电台): a show with its latest episodes, one episode,
//! and subscribing to a show. Episode audio is never fetched.

use atp_core::{Action, Error, Post, Result, ValueExt};

use crate::api::Api;
use crate::parse;

/// Latest episodes a show page lists.
const SHOW_EPISODES: usize = 20;

pub async fn episode(api: &Api, id: u64) -> Result<Post> {
  let v = api.web("dj/program/detail").arg("id", id).send().await?;
  let mut episode = parse::episode(v.at("program"))
    .ok_or_else(|| Error::not_found(format!("no podcast episode {id}")))?;
  episode.raw = Some(v);
  Ok(episode)
}

/// A show with its latest episodes (in the text and in `extra.tracks`).
pub async fn show(api: &Api, id: u64) -> Result<Post> {
  let v = api.web("djradio/v2/get").arg("id", id).send().await?;
  let mut show =
    parse::podcast(v.at("data")).ok_or_else(|| Error::not_found(format!("no podcast {id}")))?;
  let programs = api
    .web("dj/program/byradio")
    .arg("radioId", id)
    .arg("limit", SHOW_EPISODES)
    .arg("offset", 0)
    .arg("asc", false)
    .send()
    .await?;
  let episodes: Vec<Post> = programs
    .list("programs")
    .iter()
    .filter_map(parse::episode)
    .collect();
  parse::with_tracks(&mut show, "Latest episodes", &episodes);
  show.raw = Some(v);
  Ok(show)
}

/// Subscribe to a show (订阅播客), or stop.
pub async fn favorite(api: &Api, id: u64, undo: bool) -> Result<Action> {
  let path = if undo { "djradio/unsub" } else { "djradio/sub" };
  api.web(path).arg("id", id).write().send().await?;
  let action = if undo { "unfavorite" } else { "favorite" };
  Ok(Action::done(action, format!("podcast:{id}")))
}
