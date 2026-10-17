//! Your library: favorites folders and their posts, saving posts into
//! folders and creating or deleting folders, and the reading history.

use atp_core::bulk;
use atp_core::{
  Action, Collection, Error, FolderSpec, Page, PageReq, Post, Result, ValueExt, json,
};

use crate::api::{Api, V4};
use crate::refs::Target;
use crate::{api, parse, users};

/// Favorites folders (收藏夹) of a user, or of the logged-in account.
pub async fn folders(api: &Api, user: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
  let token = users::owner(api, user).await?;
  let v = api
    .list(&format!("members/{token}/favlists"), None, page)
    .await?;
  Ok(Page::new(
    v.list("data").iter().map(parse::folder).collect(),
    api::next_param(&v, "offset"),
  ))
}

/// The first folder of a user (the default one for the logged-in account).
pub async fn first_folder(api: &Api, user: Option<&str>) -> Result<String> {
  let first = folders(
    api,
    user,
    &PageReq {
      cursor: None,
      size: 1,
    },
  )
  .await?;
  first
    .items
    .into_iter()
    .next()
    .map(|c| c.id)
    .ok_or_else(|| Error::not_found("no favorites folder found; pass --folder <id>"))
}

/// Items saved in a folder.
pub async fn folder_items(api: &Api, folder: &str, page: &PageReq) -> Result<Page<Post>> {
  let v = api
    .list(&format!("collections/{folder}/items"), None, page)
    .await?;
  let items = v
    .list("data")
    .iter()
    .filter_map(|item| parse::post(item.at("content")))
    .collect();
  Ok(Page::new(items, api::next_param(&v, "offset")))
}

/// Add to (or remove from) a favorites folder; defaults to the account's first folder.
pub async fn favorite(
  api: &Api,
  target: &Target,
  folder: Option<&str>,
  undo: bool,
) -> Result<Action> {
  if matches!(target, Target::Question(_)) {
    return Err(Error::input(
      "questions cannot be saved; use `follow-question` instead",
    ));
  }
  api.prepare_write().await?;
  let folder = match folder {
    Some(f) => f.to_owned(),
    None => first_folder(api, None).await?,
  };
  let (id, kind) = (target.id(), target.kind());
  let req = if undo {
    api
      .delete(&format!("{V4}/collections/{folder}/contents/{id}"))
      .query("content_type", kind)
  } else {
    api
      .post(&format!("{V4}/collections/{folder}/contents"))
      .query("content_id", id)
      .query("content_type", kind)
  };
  api.call(req).await?;
  let action = if undo { "unfavorite" } else { "favorite" };
  Ok(Action::done(action, target.url()).with_message(format!("folder {folder}")))
}

/// Save posts into a folder (or with `undo` take them out), one call each.
pub async fn file_all(
  api: &Api,
  folder: &str,
  posts: &[String],
  undo: bool,
) -> Result<Vec<Action>> {
  let name = if undo { "folder-remove" } else { "folder-add" };
  bulk::each(name, posts, async |post| {
    let saved = favorite(api, &Target::parse(post)?, Some(folder), undo).await?;
    Ok(Action {
      action: name.to_owned(),
      ..saved
    })
  })
  .await
}

/// Create a favorites folder; it is public unless `private`.
pub async fn create_folder(api: &Api, spec: &FolderSpec) -> Result<Action> {
  api.prepare_write().await?;
  let title = spec.name.as_deref().unwrap_or_default();
  let body = json!({
    "title": title,
    "description": spec.description.as_deref().unwrap_or_default(),
    "is_public": !spec.private.unwrap_or(false),
  });
  let v = api
    .call(api.post(&format!("{V4}/collections")).json(&body))
    .await?;
  let mut done = Action::done("folder-create", title);
  if let Some(id) = v.first_str(&["collection.id", "id"]) {
    done = done
      .with_url(format!("https://www.zhihu.com/collection/{id}"))
      .with_id(id);
  }
  Ok(done)
}

/// Delete favorites folders (not the default one), one call each.
pub async fn delete_folders(api: &Api, folders: &[String]) -> Result<Vec<Action>> {
  bulk::each("folder-delete", folders, async |f| {
    delete_folder(api, f).await
  })
  .await
}

async fn delete_folder(api: &Api, folder: &str) -> Result<Action> {
  api.prepare_write().await?;
  let v = api
    .call(api.delete(&format!("{V4}/collections/{folder}")))
    .await?;
  if v.bool("success") == Some(false) {
    let why = v.str("message").unwrap_or_else(|| "refused".into());
    return Err(Error::upstream(format!(
      "folder {folder} not deleted: {why}"
    )));
  }
  Ok(Action::done("folder-delete", folder).with_id(folder))
}

/// Content you read, newest first, with `extra.read_at`.
pub async fn history(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  let req = api.mobile("/unify-consumption/read_history").queries([
    ("offset", page.number_or(0)),
    ("limit", page.size_within(20) as u64),
  ]);
  let v = api.call(req).await?;
  let items = v
    .list("data")
    .iter()
    .filter_map(parse::history_item)
    .collect();
  Ok(Page::new(items, api::next_param(&v, "offset")))
}
