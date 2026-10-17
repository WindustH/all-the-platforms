//! Managing your favorites folders (收藏夹): creating, renaming, making
//! them private or public, deleting them, and putting saved posts in and
//! taking them out, as the profile page's 收藏 tab does.

use atp_core::bulk;
use atp_core::http::Method;
use atp_core::{Action, Error, FolderSpec, PageReq, Result, Value, ValueExt};

use crate::api::Api;
use crate::{library, refs, users};

const MAINTAIN: &str = "/aweme/v1/web/collects/maintain/";
const MOVE: &str = "/aweme/v1/web/collects/video/move/";

/// The longest folder name the page takes.
const NAME_MAX: usize = 15;
/// Posts one `collects/video/move` call takes (the page's selection limit).
const BATCH: usize = 100;
/// Pages of folders a lookup goes through.
const FOLDER_PAGES: usize = 20;

/// The `action` of `collects/maintain`.
#[derive(Debug, Clone, Copy)]
enum Maintain {
  Edit = 0,
  Create = 1,
  Delete = 2,
}

fn checked_name(name: &str) -> Result<&str> {
  if name.chars().count() > NAME_MAX {
    return Err(Error::input(format!(
      "a Douyin folder name has at most {NAME_MAX} characters"
    )));
  }
  Ok(name)
}

/// `secret` as the page sends it: 1 private, 0 public.
fn secret(private: bool) -> u8 {
  u8::from(private)
}

pub async fn create(api: &Api, spec: &FolderSpec) -> Result<Action> {
  let name = checked_name(spec.name.as_deref().unwrap_or_default())?;
  let me = users::uid_hash(api).await?;
  let v = api
    .write_as(Method::POST, MAINTAIN, &me)
    .await?
    .arg("action", Maintain::Create as u8)
    .arg("collects_name", name)
    .arg("secret", secret(spec.private.unwrap_or(false)))
    .send()
    .await?;
  let action = Action::done("folder-create", name);
  Ok(
    match v.first_str(&["collects_info.collects_id_str", "collects_info.collects_id"]) {
      Some(id) => action.with_id(id),
      None => action,
    },
  )
}

/// Rename a folder or make it private or public. The page sends the name
/// only when it changes, and the privacy always: what is not given is
/// read from the folder.
pub async fn edit(api: &Api, folder: &str, spec: &FolderSpec) -> Result<Action> {
  let id = folder_id(folder)?;
  let private = match spec.private {
    Some(private) => private,
    // `status` 0 is a private folder.
    None => find(api, &id).await?.i64("status") == Some(0),
  };
  let me = users::uid_hash(api).await?;
  let mut call = api
    .write_as(Method::POST, MAINTAIN, &me)
    .await?
    .arg("collects_id", &id)
    .arg("action", Maintain::Edit as u8);
  if let Some(name) = &spec.name {
    call = call.arg("collects_name", checked_name(name)?);
  }
  call.arg("secret", secret(private)).send().await?;
  Ok(Action::done("folder-edit", id))
}

/// Delete folders, one call each.
pub async fn delete(api: &Api, folders: &[String]) -> Result<Vec<Action>> {
  bulk::each("folder-delete", folders, async |folder| {
    let id = folder_id(folder)?;
    let me = users::uid_hash(api).await?;
    api
      .write_as(Method::POST, MAINTAIN, &me)
      .await?
      .arg("action", Maintain::Delete as u8)
      .arg("collects_id", &id)
      .send()
      .await?;
    Ok(Action::done("folder-delete", id))
  })
  .await
}

/// Put saved posts into a folder, 100 per call.
pub async fn add(api: &Api, folder: &str, posts: &[String]) -> Result<Vec<Action>> {
  shift(api, folder, posts, true).await
}

/// Take posts out of a folder, 100 per call.
pub async fn remove(api: &Api, folder: &str, posts: &[String]) -> Result<Vec<Action>> {
  shift(api, folder, posts, false).await
}

/// `collects/video/move` into the folder (`to_collects_id`, with the
/// folders chosen in `move_collects_list`) or out of it
/// (`from_collects_id`), the posts as repeated `item_ids`.
async fn shift(api: &Api, folder: &str, posts: &[String], into: bool) -> Result<Vec<Action>> {
  let id = folder_id(folder)?;
  let name = find(api, &id)
    .await?
    .str("collects_name")
    .unwrap_or_default();
  let ids = refs::awemes(api, posts).await?;
  let action = if into { "folder-add" } else { "folder-remove" };
  let me = users::uid_hash(api).await?;
  bulk::batches(action, &ids, BATCH, String::clone, async |chunk| {
    let mut call = api
      .write_as(Method::POST, MOVE, &me)
      .await?
      .args("item_ids", chunk)
      .arg("item_type", 2);
    call = if into {
      call
        .arg("to_collects_id", &id)
        .arg("collects_name", &name)
        .args("move_collects_list", [&id])
        .arg("update_collects_sort", "true")
    } else {
      call
        .arg("from_collects_id", &id)
        .arg("collects_name", &name)
    };
    call.send().await?;
    let done = |post: &String| {
      Action::done(action, post)
        .with_url(refs::video_url(post))
        .with_message(format!("folder {id}"))
    };
    Ok(chunk.iter().map(done).collect())
  })
  .await
}

/// A folder id as `collections` lists it.
fn folder_id(folder: &str) -> Result<String> {
  let id = folder.trim();
  if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
    return Err(Error::input(format!(
      "not a Douyin folder: {folder} (an id or #N after `atp douyin collections`)"
    )));
  }
  Ok(id.to_owned())
}

/// One of your folders as `collects/list` lists it.
async fn find(api: &Api, id: &str) -> Result<Value> {
  let mut page = PageReq::first(20);
  for _ in 0..FOLDER_PAGES {
    let folders = library::folders(api, &page).await?;
    if let Some(folder) = folders.items.iter().find(|f| f.id == id) {
      return Ok(folder.raw.clone().unwrap_or_default());
    }
    match folders.next_cursor {
      Some(cursor) => page = PageReq::at(cursor, 20),
      None => break,
    }
  }
  Err(Error::not_found(format!(
    "no folder {id} among yours; `atp douyin collections` lists them"
  )))
}
