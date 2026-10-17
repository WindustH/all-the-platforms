//! The logged-in account's saved posts (收藏) and folders, saving and
//! unsaving, and the post lists of a mix (合集) or a music; on the creator
//! platform, the account's own mixes and the music library image posts
//! and articles pick from.

use atp_core::{Action, Collection, Error, Page, PageReq, Post, Result, Value, ValueExt};

use crate::api::{Api, WWW};
use crate::creator::Creator;
use crate::{parse, refs, users};

const COLLECT: &str = "/aweme/v1/web/aweme/collect/";
/// Where `favorite --folder` points.
pub const FOLDER_HINT: &str =
  "save the post with `favorite`, then put it into a folder with `folder-add`";

const SAVED: &str = "/aweme/v1/web/aweme/listcollection/";
const FOLDERS: &str = "/aweme/v1/web/collects/list/";
const FOLDER: &str = "/aweme/v1/web/collects/video/list/";
const MIX: &str = "/aweme/v1/web/mix/aweme/";
const MUSIC: &str = "/aweme/v1/web/music/aweme/";
const MIXES: &str = "/web/api/mix/list/";
const MUSIC_CATEGORIES: &str = "/web/api/media/music/category";
const MUSIC_LIST: &str = "/web/api/media/music/list";
/// Pages of mixes `--collection` looks through.
const MIX_PAGES: usize = 10;

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
  Ok(parse::awemes(&v, "aweme_list", "cursor"))
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
  Ok(parse::awemes(&v, "aweme_list", "cursor"))
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
  Ok(parse::awemes(&v, "aweme_list", "cursor"))
}

/// Save a post to the account's favorites, or with `undo` remove it.
pub async fn favorite(api: &Api, arg: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
  if folder.is_some() {
    return Err(Error::unsupported("favorite --folder").with_hint(FOLDER_HINT));
  }
  let id = refs::aweme(api, arg).await?;
  let uid = users::uid_hash(api).await?;
  api
    .write(COLLECT, false, &uid)
    .await?
    .header("referer", format!("{WWW}/?recommend=1"))
    .field("action", if undo { "0" } else { "1" })
    .field("aweme_id", &id)
    .field("aweme_type", "0")
    .send()
    .await?;
  let name = if undo { "unfavorite" } else { "favorite" };
  Ok(Action::done(name, &id).with_url(refs::video_url(&id)))
}

/// The account's mixes, as the publish form lists them (published and
/// under review: `status=0,2`).
pub async fn mixes(api: &Api, page: &PageReq) -> Result<Page<Collection>> {
  let creator = Creator::open(api).await?;
  let v = mix_page(&creator, page.number_or(0)).await?;
  let items: Vec<Collection> = v.list("mix_list").iter().filter_map(parse::mix).collect();
  let next = (v.bool("has_more") == Some(true) && !items.is_empty())
    .then(|| v.str("cursor"))
    .flatten();
  Ok(Page::new(items, next))
}

async fn mix_page(creator: &Creator<'_>, cursor: u64) -> Result<Value> {
  creator
    .get(MIXES)
    .arg("status", "0,2")
    .arg("count", 15)
    .arg("cursor", cursor)
    .send()
    .await
}

/// The mix a new post joins (by id or exact name) and the episode index
/// it takes there: the episodes so far, as the publish form counts.
pub async fn mix_slot(creator: &Creator<'_>, arg: &str) -> Result<(String, i64)> {
  let arg = arg.trim();
  let mut cursor = 0;
  for _ in 0..MIX_PAGES {
    let v = mix_page(creator, cursor).await?;
    let found = v
      .list("mix_list")
      .iter()
      .filter_map(parse::mix)
      .find(|m| m.id == arg || m.name == arg);
    if let Some(mix) = found {
      return Ok((mix.id, mix.items.unwrap_or(0) as i64));
    }
    match v.u64("cursor") {
      Some(next) if v.bool("has_more") == Some(true) => cursor = next,
      _ => break,
    }
  }
  Err(Error::not_found(format!(
    "no collection of yours named {arg}; `atp douyin mixes` lists them"
  )))
}

/// The music library of image posts and articles: its categories, or with
/// `category` (an id `musics` printed) the songs in it.
pub async fn musics(api: &Api, category: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
  let creator = Creator::open(api).await?;
  let Some(category) = category else {
    let v = creator.get(MUSIC_CATEGORIES).send().await?;
    let items = v
      .list("categories")
      .iter()
      .filter_map(music_category)
      .collect();
    return Ok(Page::last(items));
  };
  let (kind, id) = category
    .split_once(':')
    .ok_or_else(|| Error::input("a music category is TYPE:ID, as `atp douyin musics` prints it"))?;
  let v = creator
    .get(MUSIC_LIST)
    .arg("cursor", page.number_or(0))
    .arg("category_id", id)
    .arg("type", kind)
    .arg("count", page.size_within(20))
    .send()
    .await?;
  let items: Vec<Collection> = v.list("songs").iter().filter_map(song).collect();
  let next = (v.bool("has_more") == Some(true) && !items.is_empty())
    .then(|| v.str("cursor"))
    .flatten();
  Ok(Page::new(items, next))
}

fn music_category(c: &Value) -> Option<Collection> {
  Some(Collection {
    id: format!(
      "{}:{}",
      c.str("type").unwrap_or_default(),
      c.str("category_id")?
    ),
    kind: "music-category".into(),
    name: c.str("category_name").unwrap_or_default(),
    raw: Some(c.clone()),
    ..Collection::default()
  })
}

fn song(s: &Value) -> Option<Collection> {
  let mut extra = atp_core::Extra::new();
  if let Some(seconds) = s.f64("duration") {
    extra.insert("duration".into(), seconds.into());
  }
  if let Some(uses) = s.count("user_count") {
    extra.insert("uses".into(), uses.into());
  }
  Some(Collection {
    id: s.str("music_id")?,
    kind: "music".into(),
    name: s.str("music_name").unwrap_or_default(),
    description: s.str("music_author"),
    extra,
    raw: Some(s.clone()),
    ..Collection::default()
  })
}
