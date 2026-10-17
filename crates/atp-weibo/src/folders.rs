//! Favorite tags (收藏标签) as folders, as the favorites page and its tag
//! dialog manage them. A favorite is under at most two tags, and a tag
//! exists while favorites carry it: `tags/update` sets a favorite's tags
//! by name (a new name makes a new tag), `update_batch` renames one and
//! `destroy_batch` deletes one, keeping its favorites.
//!
//! The tags a favorite is under show only on the rows of the favorites
//! list, so changing them first looks the favorites up there.

use std::collections::BTreeMap;

use atp_core::bulk;
use atp_core::{Action, Collection, Error, FolderSpec, Page, Result, ValueExt, json};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, post_url};
use crate::{posts, users};

/// Tags one favorite can be under (最多可选2个自定义标签).
const MAX_TAGS: usize = 2;

/// Pages of favorites read to find the posts a command names.
const MAX_PAGES: u64 = 50;

/// Your favorite tags, every page of them.
pub async fn list(api: &Api) -> Result<Page<Collection>> {
  Ok(Page::last(all(api).await?))
}

async fn all(api: &Api) -> Result<Vec<Collection>> {
  api.require_login()?;
  let mut tags = Vec::new();
  for no in 1..=MAX_PAGES {
    let no_text = no.to_string();
    let v = api
      .get(
        "/ajax/favorites/tags",
        &[("page", no_text.as_str()), ("is_show_total", "1")],
      )
      .await?;
    let rows = ["tags", "data.tags"]
      .iter()
      .map(|k| v.list(k))
      .find(|l| !l.is_empty())
      .unwrap_or_default();
    tags.extend(rows.iter().filter_map(parse::tag));
    let total = v.first_count(&["total_number", "data.total_number"]);
    if rows.is_empty() || total.is_none_or(|t| tags.len() as u64 >= t) {
      break;
    }
  }
  Ok(tags)
}

/// An existing tag, by id or exact name.
async fn existing(api: &Api, folder: &str) -> Result<Collection> {
  all(api)
    .await?
    .into_iter()
    .find(|t| t.id == folder || t.name == folder)
    .ok_or_else(|| Error::not_found(format!("no favorite tag {folder}")))
}

/// The name of an existing tag, or of a new one (not all digits), which
/// `tags/update` creates.
async fn name(api: &Api, folder: &str) -> Result<String> {
  match existing(api, folder).await {
    Ok(tag) => Ok(tag.name),
    Err(e) if folder.bytes().all(|b| b.is_ascii_digit()) => Err(e),
    Err(_) => {
      check_name(folder)?;
      Ok(folder.to_owned())
    }
  }
}

/// Tag names as the tag dialog takes them: up to 12 Chinese characters
/// (24 Latin ones), made of Chinese characters, letters, digits, `_` and `:`.
fn check_name(name: &str) -> Result<()> {
  let width: usize = name.chars().map(|c| if c.is_ascii() { 1 } else { 2 }).sum();
  let allowed = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == ':' || is_han(c);
  if name.is_empty() || width > 24 || !name.chars().all(allowed) {
    return Err(Error::input(format!(
      "{name}: a tag name is up to 12 Chinese characters (24 letters) of Chinese, letters, digits, _ and :"
    )));
  }
  Ok(())
}

fn is_han(c: char) -> bool {
  ('\u{4e00}'..='\u{9fa5}').contains(&c)
}

/// Rename a tag (`--name`; tags have nothing else to change). Weibo may
/// give the renamed tag a new id, which the Action carries.
pub async fn edit(api: &Api, folder: &str, spec: &FolderSpec) -> Result<Action> {
  let Some(name) = spec.name.as_deref() else {
    return Err(Error::input("give the tag a new --name"));
  };
  check_name(name)?;
  let id = existing(api, folder).await?.id;
  api.write_pause().await;
  let v = api
    .post_json(
      "/ajax/favorites/tags/update_batch",
      &json!({"id": id, "tag": name}),
    )
    .await?;
  let new_id = v.str("data.id").unwrap_or(id.clone());
  Ok(
    Action::done("folder-edit", id)
      .with_id(new_id)
      .with_message(name.to_owned()),
  )
}

/// Delete tags, one call each; their favorites stay.
pub async fn delete(api: &Api, folders: &[String]) -> Result<Vec<Action>> {
  let mut ids = Vec::with_capacity(folders.len());
  for folder in folders {
    ids.push(existing(api, folder).await?.id);
  }
  bulk::each("folder-delete", &ids, async |id| {
    api.write_pause().await;
    api
      .post_json("/ajax/favorites/tags/destroy_batch", &json!({"id": id}))
      .await?;
    Ok(Action::done("folder-delete", id))
  })
  .await
}

/// What the favorites list shows of the posts a command names.
struct Found {
  /// The tags of each post found among your favorites.
  tags: BTreeMap<String, Vec<String>>,
  /// Whether the whole list (or the tag's) was read.
  complete: bool,
}

/// Read your favorites (under tag `tid` only, when given) until every
/// post of `mids` turned up, or the list ends.
async fn find(api: &Api, mids: &[String], tid: Option<&str>) -> Result<Found> {
  let mut found = Found {
    tags: BTreeMap::new(),
    complete: mids.is_empty(),
  };
  if found.complete {
    return Ok(found);
  }
  let uid = users::own_uid(api).await?;
  for no in 1..=MAX_PAGES {
    let no_text = no.to_string();
    let mut query = vec![
      ("uid", uid.as_str()),
      ("page", no_text.as_str()),
      ("with_total", "true"),
    ];
    if let Some(tid) = tid {
      query.push(("tid", tid));
    }
    let v = api.get("/ajax/favorites/all_fav", &query).await?;
    let rows = match v.list("data.status") {
      [] => v.list("data"),
      rows => rows,
    };
    for row in rows {
      let row = if row.at("status").is_object() {
        row.at("status")
      } else {
        row
      };
      if let Some(mid) = row
        .first_str(&["idstr", "mid", "id"])
        .filter(|m| mids.contains(m))
      {
        found.tags.insert(mid, parse::tag_names(row));
      }
    }
    let last = v.u64("total_page").is_some_and(|t| no >= t);
    if rows.is_empty() || last {
      found.complete = true;
    }
    if found.complete || mids.iter().all(|m| found.tags.contains_key(m)) {
      break;
    }
  }
  Ok(found)
}

impl Found {
  /// The tags of a post under the tag read; one not found fails.
  fn under(&self, mid: &str, tag: &str) -> Result<&Vec<String>> {
    match self.tags.get(mid) {
      Some(tags) if tags.iter().any(|t| t == tag) => Ok(tags),
      None if !self.complete => Err(deep(mid)),
      _ => Err(Error::not_found(format!("{mid} is not under tag {tag}"))),
    }
  }
}

fn deep(mid: &str) -> Error {
  Error::not_found(format!(
    "{mid} did not turn up in the latest {MAX_PAGES} pages of your favorites, where Weibo shows its tags"
  ))
}

/// Every post argument as a mid; the first that is not one stops the
/// command before anything changes.
fn mids(posts: &[String]) -> Result<Vec<String>> {
  posts.iter().map(|p| refs::mid(p)).collect()
}

/// Set a favorite's tags (by name; empty takes it out of every tag).
async fn set(api: &Api, mid: &str, tags: &[String]) -> Result<()> {
  api.write_pause().await;
  api
    .post_json(
      "/ajax/favorites/tags/update",
      &json!({"id": mid, "tags": tags.join(",")}),
    )
    .await?;
  Ok(())
}

fn done(action: &str, mid: &str, tags: &[String]) -> Action {
  let tags = if tags.is_empty() {
    "no tags".to_owned()
  } else {
    format!("tags {}", tags.join(","))
  };
  Action::done(action, mid)
    .with_url(post_url(None, mid, None))
    .with_message(tags)
}

/// Put posts under a tag. A post that is no favorite yet is favorited
/// first, as the tag dialog follows a new favorite; only the favorites
/// already there are looked up for the tags they keep.
pub async fn add(api: &Api, folder: &str, posts: &[String], action: &str) -> Result<Vec<Action>> {
  let name = name(api, folder).await?;
  let mids = mids(posts)?;
  let mut saved = Vec::new();
  for mid in &mids {
    if favorited(api, mid).await? {
      saved.push(mid.clone());
    }
  }
  let found = find(api, &saved, None).await?;
  bulk::batches(action, &mids, 1, String::clone, async |one| {
    let mid = &one[0];
    let mut tags = if saved.contains(mid) {
      found.tags.get(mid).cloned().ok_or_else(|| deep(mid))?
    } else {
      favorite(api, mid).await?;
      Vec::new()
    };
    if tags.contains(&name) {
      return Ok(vec![done(action, mid, &tags)]);
    }
    if tags.len() >= MAX_TAGS {
      return Err(full(&tags));
    }
    tags.push(name.clone());
    set(api, mid, &tags).await?;
    Ok(vec![done(action, mid, &tags)])
  })
  .await
}

/// Whether you favorited a post (`favorited` of the post itself).
async fn favorited(api: &Api, mid: &str) -> Result<bool> {
  Ok(posts::show(api, mid).await?.bool("favorited") == Some(true))
}

async fn favorite(api: &Api, mid: &str) -> Result<()> {
  api.write_pause().await;
  api
    .post_json("/ajax/statuses/createFavorites", &json!({"id": mid}))
    .await?;
  Ok(())
}

fn full(tags: &[String]) -> Error {
  Error::input(format!(
    "a favorite is under {MAX_TAGS} tags at most, and this one is under {}",
    tags.join(" and ")
  ))
}

/// Take posts off a tag; they stay favorites.
pub async fn remove(api: &Api, folder: &str, posts: &[String]) -> Result<Vec<Action>> {
  let tag = existing(api, folder).await?;
  let mids = mids(posts)?;
  let found = find(api, &mids, Some(&tag.id)).await?;
  bulk::batches("folder-remove", &mids, 1, String::clone, async |one| {
    let mid = &one[0];
    let tags: Vec<String> = found
      .under(mid, &tag.name)?
      .iter()
      .filter(|t| **t != tag.name)
      .cloned()
      .collect();
    set(api, mid, &tags).await?;
    Ok(vec![done("folder-remove", mid, &tags)])
  })
  .await
}

/// Move posts from one tag to another in one change of their tags each,
/// or with `copy` add the second tag and keep the first.
pub async fn shift(
  api: &Api,
  from: &str,
  to: &str,
  posts: &[String],
  copy: bool,
) -> Result<Vec<Action>> {
  let old = existing(api, from).await?;
  let new = name(api, to).await?;
  let mids = mids(posts)?;
  let found = find(api, &mids, Some(&old.id)).await?;
  let action = if copy { "folder-copy" } else { "folder-move" };
  bulk::batches(action, &mids, 1, String::clone, async |one| {
    let mid = &one[0];
    let mut tags: Vec<String> = found
      .under(mid, &old.name)?
      .iter()
      .filter(|t| copy || **t != old.name)
      .cloned()
      .collect();
    if !tags.contains(&new) {
      if tags.len() >= MAX_TAGS {
        return Err(full(&tags));
      }
      tags.push(new.clone());
    }
    set(api, mid, &tags).await?;
    Ok(vec![done(action, mid, &tags)])
  })
  .await
}
