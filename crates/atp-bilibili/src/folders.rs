//! Managing favorites folders: create / edit / delete your own, put
//! videos in and move what they hold between them, the folders and
//! collections (合集) you saved from others, and cleaning out videos that
//! became unavailable.

use std::path::Path;

use atp_core::bulk;
use atp_core::file::Image;
use atp_core::http::Part;
use atp_core::{Action, Collection, Error, FolderSpec, Page, PageReq, Result, ValueExt, json};

use crate::api::Api;
use crate::paging::Pn;
use crate::parse;
use crate::refs::{self, Resource, space_url};
use crate::users;

const ADD: &str = "https://api.bilibili.com/x/v3/fav/folder/add";
const EDIT: &str = "https://api.bilibili.com/x/v3/fav/folder/edit";
const INFO: &str = "https://api.bilibili.com/x/v3/fav/folder/info";
const DEL: &str = "https://api.bilibili.com/x/v3/fav/folder/del";
const COVER: &str = "https://api.bilibili.com/x/upload/web/image";
const COLLECTED: &str = "https://api.bilibili.com/x/v3/fav/folder/collected/list";
const FAV_FOLDER: &str = "https://api.bilibili.com/x/v3/fav/folder/fav";
const UNFAV_FOLDER: &str = "https://api.bilibili.com/x/v3/fav/folder/unfav";
const FAV_SEASON: &str = "https://api.bilibili.com/x/v3/fav/season/fav";
const UNFAV_SEASON: &str = "https://api.bilibili.com/x/v3/fav/season/unfav";
const CLEAN: &str = "https://api.bilibili.com/x/v3/fav/resource/clean";
const DEAL: &str = "https://api.bilibili.com/x/v3/fav/resource/deal";
const BATCH_DEL: &str = "https://api.bilibili.com/x/v3/fav/resource/batch-del";
const MOVE: &str = "https://api.bilibili.com/x/v3/fav/resource/move";
const COPY: &str = "https://api.bilibili.com/x/v3/fav/resource/copy";

/// Items (or folders) one batch call takes: what the web sends from one
/// page of a folder.
const BATCH: usize = 20;

pub async fn create(api: &Api, spec: &FolderSpec) -> Result<Action> {
  let title = spec.name.clone().unwrap_or_default();
  let cover = match &spec.cover {
    Some(path) => cover(api, path).await?,
    None => String::new(),
  };
  let data = api
    .post(ADD)
    .arg("title", &title)
    .arg("intro", spec.description.as_deref().unwrap_or_default())
    .arg("privacy", u8::from(spec.private.unwrap_or(false)))
    .arg("cover", cover)
    .send()
    .await?;
  let id = data.str("id").unwrap_or_default();
  let mid = users::my_mid(api).await.unwrap_or_default();
  Ok(
    Action::done("folder-create", title)
      .with_id(&id)
      .with_url(folder_url(&mid, &id)),
  )
}

fn folder_url(mid: &str, id: &str) -> String {
  format!("{}/favlist?fid={id}", space_url(mid))
}

/// Rename a folder or change its description, privacy or cover; what is
/// not given is sent back as it was.
pub async fn edit(api: &Api, folder: &str, spec: &FolderSpec) -> Result<Action> {
  let id = refs::folder(folder)?;
  let info = api.get(INFO).arg("media_id", id).send().await?;
  let private = info.u64("attr").is_some_and(|a| a & 1 == 1);
  let cover = match &spec.cover {
    Some(path) => cover(api, path).await?,
    None => info.str("cover").unwrap_or_default(),
  };
  let kept =
    |new: &Option<String>, key: &str| new.clone().or_else(|| info.str(key)).unwrap_or_default();
  api
    .post(EDIT)
    .arg("media_id", id)
    .arg("title", kept(&spec.name, "title"))
    .arg("intro", kept(&spec.description, "intro"))
    .arg("privacy", u8::from(spec.private.unwrap_or(private)))
    .arg("cover", cover)
    .send()
    .await?;
  Ok(Action::done("folder-edit", id.to_string()))
}

/// Upload a folder cover the way the folder page does; its address.
async fn cover(api: &Api, path: &Path) -> Result<String> {
  let image = Image::read(path).await?;
  let parts = vec![
    Part::file("file", image.data, &image.name, image.mime),
    Part::text("bucket", "medialist"),
    Part::text("dir", "cover"),
  ];
  let data = api.post(COVER).multipart(parts).send().await?;
  data
    .str("location")
    .ok_or_else(|| Error::upstream("the cover upload returned no address"))
}

/// Delete folders and what they hold, several per call.
pub async fn delete(api: &Api, folders: &[String]) -> Result<Vec<Action>> {
  let ids = folders
    .iter()
    .map(|f| refs::folder(f).map(|id| id.to_string()))
    .collect::<Result<Vec<_>>>()?;
  let target = |id: &String| id.clone();
  bulk::batches("folder-delete", &ids, BATCH, target, async |chunk| {
    api
      .post(DEL)
      .arg("media_ids", chunk.join(","))
      .arg("platform", "web")
      .send()
      .await?;
    Ok(done(chunk, "folder-delete", |id| id.clone()))
  })
  .await
}

/// An Action per item of a batch that went through.
fn done<T>(chunk: &[T], action: &str, name: impl Fn(&T) -> String) -> Vec<Action> {
  chunk
    .iter()
    .map(|t| Action::done(action, name(t)))
    .collect()
}

/// Every post argument as a folder item; the first that is not one fails
/// the command before anything changes.
async fn resources(api: &Api, posts: &[String]) -> Result<Vec<Resource>> {
  let mut out = Vec::with_capacity(posts.len());
  for post in posts {
    out.push(refs::resource(api, post).await?);
  }
  Ok(out)
}

/// Save videos into a folder, one call each (the web saves one at a time).
pub async fn add(api: &Api, folder: &str, posts: &[String]) -> Result<Vec<Action>> {
  let media_id = refs::folder(folder)?;
  let items = resources(api, posts).await?;
  if let Some(other) = items.iter().find(|r| r.kind != Resource::VIDEO) {
    return Err(Error::input(format!(
      "only videos are added to folders here, not {}",
      other.name
    )));
  }
  let name = |r: &Resource| r.name.clone();
  bulk::batches("folder-add", &items, 1, name, async |one| {
    let video = &one[0];
    api
      .post(DEAL)
      .arg("rid", video.id)
      .arg("type", Resource::VIDEO)
      .arg("add_media_ids", media_id)
      .arg("del_media_ids", "")
      .send()
      .await?;
    let url = refs::Video::from_aid(video.id).url();
    let action = Action::done("folder-add", &video.name).with_url(url);
    Ok(vec![action.with_message(format!("folder {media_id}"))])
  })
  .await
}

/// Take videos, audio and collections out of a folder, several per call.
pub async fn remove(api: &Api, folder: &str, posts: &[String]) -> Result<Vec<Action>> {
  let media_id = refs::folder(folder)?;
  let items = resources(api, posts).await?;
  let name = |r: &Resource| r.name.clone();
  bulk::batches("folder-remove", &items, BATCH, name, async |chunk| {
    api
      .post(BATCH_DEL)
      .arg("resources", keys(chunk))
      .arg("media_id", media_id)
      .arg("platform", "web")
      .send()
      .await?;
    Ok(done(chunk, "folder-remove", name))
  })
  .await
}

/// Move (or `copy`) items from one folder to another, several per call.
pub async fn shift(
  api: &Api,
  from: &str,
  to: &str,
  posts: &[String],
  copy: bool,
) -> Result<Vec<Action>> {
  let (src, tar) = (refs::folder(from)?, refs::folder(to)?);
  let items = resources(api, posts).await?;
  let mid = users::my_mid(api).await?;
  let (url, action) = if copy {
    (COPY, "folder-copy")
  } else {
    (MOVE, "folder-move")
  };
  let name = |r: &Resource| r.name.clone();
  bulk::batches(action, &items, BATCH, name, async |chunk| {
    api
      .post(url)
      .arg("resources", keys(chunk))
      .arg("src_media_id", src)
      .arg("tar_media_id", tar)
      .arg("mid", &mid)
      .arg("platform", "web")
      .send()
      .await?;
    let moved = done(chunk, action, name).into_iter();
    Ok(
      moved
        .map(|a| a.with_message(format!("{src} → {tar}")))
        .collect(),
    )
  })
  .await
}

/// `ID:TYPE,ID:TYPE`, as `resources` takes them.
fn keys(items: &[Resource]) -> String {
  let keys: Vec<String> = items.iter().map(Resource::key).collect();
  keys.join(",")
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
