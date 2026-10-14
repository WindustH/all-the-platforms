//! Your library: liked tracks, saved albums and shows, followed playlists,
//! saving and removing, and what you played lately. Spotify serves only
//! the logged-in account's library.

use atp_core::{Action, Error, Page, PageReq, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::refs::{self, Kind};
use crate::{parse, playlists, users};

/// `favorites --folder` values; the first is the default.
pub const FOLDERS: &[&str] = &["tracks", "albums", "shows", "playlists"];
const PAGE: usize = 50;

/// Saved tracks (Liked Songs), albums or shows, or followed playlists.
pub async fn favorites(
  api: &Api,
  user: Option<&str>,
  folder: Option<&str>,
  page: &PageReq,
) -> Result<Page<Post>> {
  if let Some(user) = user {
    users::mine(api, user).await?;
  }
  match folder.unwrap_or(FOLDERS[0]) {
    "tracks" | "track" | "songs" => saved(api, "tracks", parse::track, page).await,
    "albums" | "album" => saved(api, "albums", parse::album, page).await,
    "shows" | "show" | "podcasts" => saved(api, "shows", parse::show, page).await,
    "playlists" | "playlist" => playlists::mine(api, false, page).await,
    other => Err(Error::input(format!(
      "no folder {other}; expected one of {}",
      FOLDERS.join(", ")
    ))),
  }
}

/// Liked Songs.
pub async fn liked(api: &Api, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  if let Some(user) = user {
    users::mine(api, user).await?;
  }
  saved(api, "tracks", parse::track, page).await
}

/// `/me/tracks`, `/me/albums` or `/me/shows`: each entry holds the item
/// under the singular key and when it was saved.
async fn saved(
  api: &Api,
  what: &str,
  make: fn(&Value) -> Option<Post>,
  page: &PageReq,
) -> Result<Page<Post>> {
  let key = what.trim_end_matches('s');
  let (v, next) = api
    .offset_page(&format!("/me/{what}"), &[], page, PAGE)
    .await?;
  let items = v
    .list("items")
    .iter()
    .filter_map(|entry| {
      let mut post = make(entry.at(key))?;
      if let Some(at) = entry.str("added_at") {
        post.extra.insert("added_at".into(), json!(at));
      }
      Some(post)
    })
    .collect();
  Ok(Page::new(items, next))
}

/// Like a track (add it to Liked Songs), or take it out.
pub async fn like(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let item = refs::item(api, arg, Kind::Track).await?;
  if item.kind != Kind::Track {
    return Err(Error::input(format!(
      "like takes a track, not {}; save albums, shows and episodes with favorite, follow \
       artists, users and playlists",
      item.kind.a()
    )));
  }
  api.library(&[item.uri()], undo).await?;
  let action = if undo { "unlike" } else { "like" };
  Ok(Action::done(action, &item.id).with_url(item.url()))
}

/// Save an album, a show or an episode to the library, or remove it.
pub async fn favorite(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let kinds = [Kind::Album, Kind::Show, Kind::Episode];
  let item = refs::item(api, arg, Kind::Album).await?;
  if !kinds.contains(&item.kind) {
    let hint = match item.kind {
      Kind::Track => "tracks are liked: like TRACK",
      _ => "artists, users and playlists are followed: follow ARG",
    };
    return Err(Error::input(format!(
      "favorite takes an album, a show or an episode; {hint}"
    )));
  }
  api.library(&[item.uri()], undo).await?;
  let action = if undo { "unfavorite" } else { "favorite" };
  Ok(Action::done(action, &item.id).with_url(item.url()))
}

/// Tracks played lately, latest first, with when (`extra.played_at`) and
/// from where (`extra.context`). Episodes are not in it.
pub async fn history(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  let mut query = vec![("limit", page.size_within(PAGE).to_string())];
  if let Some(before) = &page.cursor {
    query.push(("before", before.clone()));
  }
  let v = api.get("/me/player/recently-played", &query).await?;
  let items = v
    .list("items")
    .iter()
    .filter_map(|entry| {
      let mut post = parse::track(entry.at("track"))?;
      if let Some(at) = entry.str("played_at") {
        post.extra.insert("played_at".into(), json!(at));
      }
      if let Some(context) = entry.str("context.uri") {
        post.extra.insert("context".into(), json!(context));
      }
      Some(post)
    })
    .collect();
  let next = v.str("next").and_then(|_| v.str("cursors.before"));
  Ok(Page::new(items, next))
}
