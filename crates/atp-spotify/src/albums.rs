//! Albums with their track lists.

use atp_core::music::with_tracks;
use atp_core::{Error, Post, Result, ValueExt};

use crate::api::Api;
use crate::parse;

/// Enough for box sets; longer albums list their first tracks.
const MAX_TRACKS: usize = 500;

/// An album and its tracks (`extra.tracks`, numbered in its text).
pub async fn read(api: &Api, id: &str) -> Result<Post> {
  let v = api.get(&format!("/albums/{id}"), &[]).await?;
  let mut post = parse::album(&v).ok_or_else(|| Error::not_found(format!("no album {id}")))?;
  let mut tracks: Vec<Post> = v
    .list("tracks.items")
    .iter()
    .filter_map(parse::track)
    .collect();
  let mut next = v.str("tracks.next");
  while let Some(link) = next.take().filter(|_| tracks.len() < MAX_TRACKS) {
    let page = api.get(&link, &[]).await?;
    tracks.extend(page.list("items").iter().filter_map(parse::track));
    next = page.str("next");
  }
  with_tracks(&mut post, "Tracks", &tracks);
  Ok(post)
}
