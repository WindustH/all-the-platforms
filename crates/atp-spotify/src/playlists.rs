//! Playlists: reading one, the account's own and followed ones, and the
//! changes `playlist create|add|remove|delete` make. Since February 2026
//! Spotify lists the items only of playlists the account owns or
//! collaborates on; others come as their details alone.

use atp_core::cli::confirm;
use atp_core::http::Method;
use atp_core::music::with_tracks;
use atp_core::{Action, Error, ErrorCode, Page, PageReq, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, Item, Kind};

/// Items per page of a playlist, and per change.
const PAGE: usize = 50;
const BATCH: usize = 100;
const TYPES: &str = "track,episode";
const HIDDEN: &str = "Spotify lists the items of your own and collaborative playlists only";

/// A playlist with its first items when Spotify shows them.
pub async fn read(api: &Api, id: &str) -> Result<Post> {
  let query = [("additional_types", TYPES.to_owned())];
  let v = api.get(&format!("/playlists/{id}"), &query).await?;
  let mut post =
    parse::playlist(&v).ok_or_else(|| Error::not_found(format!("no playlist {id}")))?;
  let page = match v.at("items") {
    Value::Null => v.at("tracks"),
    items => items,
  };
  // Others' playlists come without their entries (or without the key).
  if !page.at("items").is_array() {
    let text = post
      .text
      .take()
      .map(|t| format!("{t}\n\n"))
      .unwrap_or_default();
    post.text = Some(format!("{text}({HIDDEN}.)"));
    return Ok(post);
  }
  let tracks: Vec<Post> = page.list("items").iter().filter_map(entry).collect();
  with_tracks(&mut post, "Tracks", &tracks);
  Ok(post)
}

/// One entry of a playlist: its track or episode, with when it was added
/// and by whom.
fn entry(v: &Value) -> Option<Post> {
  let item = match v.at("item") {
    Value::Null => v.at("track"),
    item => item,
  };
  let mut post = parse::item(item)?;
  if let Some(at) = v.str("added_at") {
    post.extra.insert("added_at".into(), json!(at));
  }
  if let Some(by) = v.str("added_by.id") {
    post.extra.insert("added_by".into(), json!(by));
  }
  Some(post)
}

/// The items of a playlist, page by page.
pub async fn tracks(api: &Api, id: &str, page: &PageReq) -> Result<Page<Post>> {
  let query = [("additional_types", TYPES.to_owned())];
  let path = format!("/playlists/{id}/items");
  let (v, next) = api
    .offset_page(&path, &query, page, PAGE)
    .await
    .map_err(|e| match e.code {
      ErrorCode::PermissionDenied => e.with_hint(HIDDEN),
      _ => e,
    })?;
  Ok(Page::new(
    v.list("items").iter().filter_map(entry).collect(),
    next,
  ))
}

/// The playlists in the account's library: those it made (`owned`) or
/// those of others it follows.
pub async fn mine(api: &Api, owned: bool, page: &PageReq) -> Result<Page<Post>> {
  let me = api.my_id().await?;
  let (v, next) = api.offset_page("/me/playlists", &[], page, PAGE).await?;
  let items = v
    .list("items")
    .iter()
    .filter_map(parse::playlist)
    .filter(|p| p.author.as_ref().is_some_and(|a| a.id == me) == owned)
    .collect();
  Ok(Page::new(items, next))
}

/// Settings of a new playlist.
pub struct New {
  pub name: String,
  pub description: Option<String>,
  pub private: bool,
  pub collaborative: bool,
}

pub async fn create(api: &Api, new: New) -> Result<Action> {
  // Spotify makes collaborative playlists private only.
  let public = !new.private && !new.collaborative;
  let mut body = json!({"name": new.name, "public": public, "collaborative": new.collaborative});
  if let Some(text) = new.description {
    body["description"] = json!(text);
  }
  let v = api
    .send(Method::POST, "/me/playlists", &[], Some(&body))
    .await?;
  let id = v
    .str("id")
    .ok_or_else(|| Error::upstream("Spotify created no playlist"))?;
  Ok(
    Action::done("playlist create", &new.name)
      .with_url(refs::url(Kind::Playlist, &id))
      .with_id(id),
  )
}

/// Add tracks or episodes at the end, or take them out (every occurrence).
pub async fn change(api: &Api, id: &str, items: &[Item], remove: bool) -> Result<Action> {
  let path = format!("/playlists/{id}/items");
  let uris: Vec<String> = items.iter().map(Item::uri).collect();
  let mut snapshot = None;
  for batch in uris.chunks(BATCH) {
    let (method, body) = if remove {
      let list: Vec<Value> = batch.iter().map(|u| json!({"uri": u})).collect();
      (Method::DELETE, json!({"items": list}))
    } else {
      (Method::POST, json!({"uris": batch}))
    };
    let v = api.send(method, &path, &[], Some(&body)).await?;
    snapshot = v.str("snapshot_id").or(snapshot);
  }
  let (action, done) = if remove {
    ("playlist remove", "removed")
  } else {
    ("playlist add", "added")
  };
  let mut result = Action::done(action, id)
    .with_url(refs::url(Kind::Playlist, id))
    .with_message(format!("{done} {} item(s)", uris.len()));
  result.id = snapshot;
  Ok(result)
}

/// "Delete" a playlist: Spotify only lets its owner unfollow it, which takes
/// it out of the library; followers keep their copy.
pub async fn delete(api: &Api, id: &str, yes: bool) -> Result<Action> {
  confirm(yes, &format!("Unfollow (delete) playlist {id}"))?;
  let item = Item {
    kind: Kind::Playlist,
    id: id.to_owned(),
  };
  api.library(&[item.uri()], true).await?;
  Ok(Action::done("playlist delete", id).with_message("unfollowed: gone from your library"))
}
