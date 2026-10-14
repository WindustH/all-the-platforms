//! Artists: their page with their latest albums and singles. Top tracks
//! are gone: Spotify removed them for development-mode apps in February
//! 2026.

use atp_core::{Error, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::parse;

/// Spotify sends at most ten albums a page.
const ALBUMS: &str = "10";

/// An artist, with their albums and singles in the text and
/// `extra.albums`.
pub async fn read(api: &Api, id: &str) -> Result<Post> {
  let v = api.get(&format!("/artists/{id}"), &[]).await?;
  let mut post = parse::artist(&v).ok_or_else(|| Error::not_found(format!("no artist {id}")))?;
  let query = [
    ("include_groups", "album,single".to_owned()),
    ("limit", ALBUMS.to_owned()),
  ];
  let found = api.get(&format!("/artists/{id}/albums"), &query).await?;
  let albums: Vec<Post> = found
    .list("items")
    .iter()
    .filter_map(parse::album)
    .collect();
  if albums.is_empty() {
    return Ok(post);
  }
  let total = found.u64("total").unwrap_or(albums.len() as u64);
  let mut text = format!("Albums and singles ({} of {total}):", albums.len());
  for (i, album) in albums.iter().enumerate() {
    let date = album.extra.get("release_date").and_then(Value::as_str);
    let kind = album.extra.get("type").and_then(Value::as_str);
    let about: Vec<&str> = [date, kind].into_iter().flatten().collect();
    text.push_str(&format!(
      "\n{:>3}. {} ({})",
      i + 1,
      album.title.as_deref().unwrap_or("?"),
      about.join(", ")
    ));
  }
  let list: Vec<Value> = albums
    .iter()
    .map(|a| json!({"id": a.id, "title": a.title, "type": a.extra.get("type"), "release_date": a.extra.get("release_date"), "url": a.url}))
    .collect();
  post.text = Some(text);
  post.extra.insert("albums".into(), Value::Array(list));
  post.extra.insert("album_count".into(), json!(total));
  Ok(post)
}
