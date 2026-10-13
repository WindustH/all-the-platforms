//! The logged-in account's saved posts (收藏) and folders, viewing history,
//! and the post lists of a mix (合集) or a music.

use social_core::{Collection, Error, Page, PageReq, Post, Result, Value, ValueExt};

use crate::api::Api;
use crate::{parse, refs};

const SAVED: &str = "/aweme/v1/web/aweme/listcollection/";
const FOLDERS: &str = "/aweme/v1/web/collects/list/";
const FOLDER: &str = "/aweme/v1/web/collects/video/list/";
const HISTORY: &str = "/aweme/v1/web/history/read/";
const MIX: &str = "/aweme/v1/web/mix/aweme/";
const MUSIC: &str = "/aweme/v1/web/music/aweme/";

/// A `cursor` listing of awemes.
fn awemes(v: &Value, cursor_key: &str) -> Page<Post> {
  let items: Vec<Post> = v
    .list("aweme_list")
    .iter()
    .filter_map(parse::aweme)
    .collect();
  let next = (v.i64("has_more") == Some(1) && !items.is_empty())
    .then(|| v.str(cursor_key))
    .flatten()
    .filter(|c| c != "0");
  Page::new(items, next)
}

/// Saved posts, all or those of one folder.
pub async fn favorites(api: &Api, folder: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let cursor = page.number_or(0);
  let count = page.size_within(18);
  let v = match folder {
    Some(id) => {
      api
        .get(FOLDER)
        .arg("collects_id", id.trim())
        .arg("cursor", cursor)
        .arg("count", count)
        .arg("version_code", "170400")
        .arg("version_name", "17.4.0")
        .send()
        .await?
    }
    None => {
      api
        .post(SAVED)
        .arg("publish_video_strategy_type", "2")
        .arg("version_code", "170400")
        .arg("version_name", "17.4.0")
        .field("count", count)
        .field("cursor", cursor)
        .send()
        .await?
    }
  };
  Ok(awemes(&v, "cursor"))
}

/// Folders of saved posts.
pub async fn folders(api: &Api, page: &PageReq) -> Result<Page<Collection>> {
  api.require_login()?;
  let v = api
    .get(FOLDERS)
    .arg("cursor", page.number_or(0))
    .arg("count", page.size_within(20))
    .arg("version_code", "170400")
    .arg("version_name", "17.4.0")
    .send()
    .await?;
  let items: Vec<Collection> = v
    .list("collects_list")
    .iter()
    .filter_map(|c| {
      Some(Collection {
        id: c.first_str(&["collects_id_str", "collects_id"])?,
        kind: "folder".into(),
        name: c.str("collects_name").unwrap_or_default(),
        items: c.count("total_number"),
        raw: Some(c.clone()),
        ..Collection::default()
      })
    })
    .collect();
  let next = (v.i64("has_more") == Some(1) && !items.is_empty())
    .then(|| v.str("cursor"))
    .flatten();
  Ok(Page::new(items, next))
}

pub async fn history(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let v = api
    .get(HISTORY)
    .arg("max_cursor", page.number_or(0))
    .arg("count", page.size_within(20))
    .send()
    .await?;
  Ok(awemes(&v, "max_cursor"))
}

pub async fn mix(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let id = refs::mix(arg)?;
  let v = api
    .get(MIX)
    .arg("mix_id", &id)
    .arg("cursor", page.number_or(0))
    .arg("count", page.size_within(20))
    .arg("version_code", "170400")
    .arg("version_name", "17.4.0")
    .send()
    .await?;
  if v.list("aweme_list").is_empty() && page.cursor.is_none() {
    return Err(Error::not_found(format!("no Douyin mix {id}")));
  }
  Ok(awemes(&v, "cursor"))
}

pub async fn music(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let id = refs::music(arg)?;
  let v = api
    .get(MUSIC)
    .arg("music_id", &id)
    .arg("cursor", page.number_or(0))
    .arg("count", page.size_within(20))
    .send()
    .await?;
  Ok(awemes(&v, "cursor"))
}
