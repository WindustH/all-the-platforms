//! Managing favorites folders: create / edit / delete your own, the
//! folders and collections (合集) you saved from others, and cleaning out
//! videos that became unavailable.

use social_core::{Action, Collection, Page, PageReq, Result, ValueExt, json};

use crate::api::Api;
use crate::paging::Pn;
use crate::parse;
use crate::refs::{self, space_url};
use crate::users;

const ADD: &str = "https://api.bilibili.com/x/v3/fav/folder/add";
const EDIT: &str = "https://api.bilibili.com/x/v3/fav/folder/edit";
const DEL: &str = "https://api.bilibili.com/x/v3/fav/folder/del";
const COLLECTED: &str = "https://api.bilibili.com/x/v3/fav/folder/collected/list";
const FAV_FOLDER: &str = "https://api.bilibili.com/x/v3/fav/folder/fav";
const UNFAV_FOLDER: &str = "https://api.bilibili.com/x/v3/fav/folder/unfav";
const FAV_SEASON: &str = "https://api.bilibili.com/x/v3/fav/season/fav";
const UNFAV_SEASON: &str = "https://api.bilibili.com/x/v3/fav/season/unfav";
const CLEAN: &str = "https://api.bilibili.com/x/v3/fav/resource/clean";

pub async fn create(api: &Api, title: &str, intro: &str, private: bool) -> Result<Action> {
  let data = api
    .post(ADD)
    .arg("title", title)
    .arg("intro", intro)
    .arg("privacy", u8::from(private))
    .arg("cover", "")
    .send()
    .await?;
  let id = data.str("id").unwrap_or_default();
  let mid = users::my_mid(api).await.unwrap_or_default();
  Ok(
    Action::done("folder-create", title)
      .with_id(&id)
      .with_url(format!("{}/favlist?fid={id}", space_url(mid))),
  )
}

/// Rename a folder or change its description / privacy (unset values stay).
pub async fn edit(
  api: &Api,
  folder: &str,
  title: Option<&str>,
  intro: Option<&str>,
  private: Option<bool>,
) -> Result<Action> {
  let id = refs::folder(folder)?;
  let info = api
    .get("https://api.bilibili.com/x/v3/fav/folder/info")
    .arg("media_id", id)
    .send()
    .await?;
  let current_private = info.u64("attr").is_some_and(|a| a & 1 == 1);
  api
    .post(EDIT)
    .arg("media_id", id)
    .arg(
      "title",
      title
        .map(str::to_owned)
        .or_else(|| info.str("title"))
        .unwrap_or_default(),
    )
    .arg(
      "intro",
      intro
        .map(str::to_owned)
        .or_else(|| info.str("intro"))
        .unwrap_or_default(),
    )
    .arg("privacy", u8::from(private.unwrap_or(current_private)))
    .arg("cover", info.str("cover").unwrap_or_default())
    .send()
    .await?;
  Ok(Action::done("folder-edit", id.to_string()))
}

pub async fn delete(api: &Api, folder: &str) -> Result<Action> {
  let id = refs::folder(folder)?;
  api
    .post(DEL)
    .arg("media_ids", id)
    .arg("platform", "web")
    .send()
    .await?;
  Ok(Action::done("folder-delete", id.to_string()))
}

/// Drop the videos that became unavailable from a folder.
pub async fn clean(api: &Api, folder: &str) -> Result<Action> {
  let id = refs::folder(folder)?;
  api.post(CLEAN).arg("media_id", id).send().await?;
  Ok(Action::done("folder-clean", id.to_string()))
}

/// Folders and collections you saved from other people.
pub async fn collected(api: &Api, page: &PageReq) -> Result<Page<Collection>> {
  let mid = users::my_mid(api).await?;
  let pn = Pn::of(page, 20);
  let data = api
    .get(COLLECTED)
    .arg("pn", pn.pn)
    .arg("ps", pn.ps)
    .arg("up_mid", &mid)
    .arg("platform", "web")
    .send()
    .await?;
  let items = data
    .list("list")
    .iter()
    .map(|f| {
      let owner = f.str("upper.mid").unwrap_or_default();
      let mut c = parse::folder(f, &owner);
      // type 11 = a folder, 21 = a collection (合集) of videos.
      if f.u64("type") == Some(21) {
        c.kind = "season".into();
        c.url = Some(format!("{}/lists/{}?type=season", space_url(&owner), c.id));
      }
      c.owner = Some(parse::user(f.at("upper"))).filter(|u| !u.id.is_empty());
      c.views = f.count("view_count");
      c.extra.remove("default");
      c.extra.insert("saved".into(), json!(true));
      c
    })
    .collect();
  Ok(pn.page(items, data.bool("has_more") == Some(true)))
}

/// Save (or with `undo` drop) someone's folder or collection.
pub async fn save(api: &Api, target: &str, undo: bool) -> Result<Action> {
  let (url, key, id) = match refs::list(target) {
    Ok((_, id, "season")) => (
      if undo { UNFAV_SEASON } else { FAV_SEASON },
      "season_id",
      id,
    ),
    _ => (
      if undo { UNFAV_FOLDER } else { FAV_FOLDER },
      "media_id",
      refs::folder(target)?.to_string(),
    ),
  };
  api
    .post(url)
    .arg(key, &id)
    .arg("platform", "web")
    .send()
    .await?;
  let action = if undo { "unsave" } else { "save" };
  Ok(Action::done(action, id))
}
