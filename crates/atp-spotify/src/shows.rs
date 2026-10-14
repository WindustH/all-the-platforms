//! Shows (podcasts) with their latest episodes, and single episodes.

use atp_core::music::with_tracks;
use atp_core::{Error, Post, Result, ValueExt};

use crate::api::Api;
use crate::parse;

/// A show, with the episodes its page lists first (the latest).
pub async fn read(api: &Api, id: &str) -> Result<Post> {
  let v = api.get(&format!("/shows/{id}"), &[]).await?;
  let mut post = parse::show(&v).ok_or_else(|| Error::not_found(format!("no show {id}")))?;
  let episodes: Vec<Post> = v
    .list("episodes.items")
    .iter()
    .filter_map(parse::episode)
    .collect();
  with_tracks(&mut post, "Episodes", &episodes);
  Ok(post)
}

pub async fn episode(api: &Api, id: &str) -> Result<Post> {
  let v = api.get(&format!("/episodes/{id}"), &[]).await?;
  parse::episode(&v).ok_or_else(|| Error::not_found(format!("no episode {id}")))
}
