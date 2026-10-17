//! The tabs of your 收藏 besides the folders: unsaving several posts at
//! once, and the mixes (合集), short dramas (短剧) and music you saved,
//! with their saving and unsaving.

use atp_core::bulk;
use atp_core::http::Method;
use atp_core::{Action, Collection, Error, Page, PageReq, Result, Value, ValueExt, json};

use crate::api::{self, Api, WWW};
use crate::{library, parse, refs, users};

const UNSAVE_POSTS: &str = "/aweme/v1/web/aweme/cancel/collect/multi/";
/// Several at once, as the 收藏 tabs send their 取消.
const BATCH: usize = 50;
/// `music/collect` saying the music was saved already.
const ALREADY: i64 = 3_007_005;

/// What else a 收藏 tab holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Saved {
  /// Mixes (合集).
  Mix,
  /// Short dramas (短剧).
  Drama,
  /// Music (音乐).
  Music,
}

impl Saved {
  /// The listing: path, key of its rows, statuses of rows gone.
  fn list(self) -> (&'static str, &'static str, &'static [i64]) {
    match self {
      Saved::Mix => ("/aweme/v1/web/mix/listcollection/", "mix_infos", &[4]),
      Saved::Drama => ("/aweme/v1/web/series/collections/", "series_infos", &[3, 4]),
      Saved::Music => ("/aweme/v1/web/music/listcollection/", "mc_list", &[]),
    }
  }

  fn parse(self, v: &Value) -> Option<Collection> {
    match self {
      Saved::Mix => parse::mix(v),
      Saved::Drama => parse::series(v),
      Saved::Music => parse::music(v),
    }
  }

  fn id(self, arg: &str) -> Result<String> {
    match self {
      Saved::Mix => refs::mix(arg),
      Saved::Drama => refs::series(arg),
      Saved::Music => refs::music(arg),
    }
  }

  /// The command's name, `save` / `unsave` and kin.
  fn action(self, undo: bool) -> &'static str {
    match (self, undo) {
      (Saved::Mix, false) => "save",
      (Saved::Mix, true) => "unsave",
      (Saved::Drama, false) => "save-drama",
      (Saved::Drama, true) => "unsave-drama",
      (Saved::Music, false) => "save-music",
      (Saved::Music, true) => "unsave-music",
    }
  }

  /// The call that unsaves several at once and its id field.
  fn cancel_multi(self) -> Option<(&'static str, &'static str)> {
    match self {
      Saved::Mix => Some(("/aweme/v1/web/mix/cancel/collect/multi/", "mix_ids")),
      Saved::Drama => Some(("/aweme/v1/web/series/cancel/collect/multi/", "series_ids")),
      Saved::Music => None,
    }
  }

  fn url(self, id: &str) -> Option<String> {
    match self {
      Saved::Mix => Some(format!("{WWW}/collection/{id}")),
      Saved::Drama => None,
      Saved::Music => Some(format!("{WWW}/music/{id}")),
    }
  }
}

/// What you saved of one kind, latest first.
pub async fn list(api: &Api, kind: Saved, page: &PageReq) -> Result<Page<Collection>> {
  api.require_login()?;
  let (path, key, gone) = kind.list();
  let count = if kind == Saved::Music { 20 } else { 12 };
  let v = api
    .get(path)
    .arg("cursor", page.number_or(0))
    .arg("count", page.size_within(count))
    .arg("version_code", "170400")
    .arg("version_name", "17.4.0")
    .send()
    .await?;
  let items: Vec<Collection> = v
    .list(key)
    .iter()
    .filter(|row| !row.i64("status.status").is_some_and(|s| gone.contains(&s)))
    .filter_map(|row| kind.parse(row))
    .collect();
  let more = v.i64("has_more") == Some(1) || v.bool("has_more") == Some(true);
  let next = (more && !items.is_empty())
    .then(|| v.str("cursor"))
    .flatten()
    .filter(|c| c != "0");
  Ok(Page::new(items, next))
}

/// Save (or with `undo` unsave) mixes, short dramas or music: one call
/// each, except unsaving several mixes or dramas, 50 per call.
pub async fn save(api: &Api, kind: Saved, items: &[String], undo: bool) -> Result<Vec<Action>> {
  let ids = items
    .iter()
    .map(|item| kind.id(item))
    .collect::<Result<Vec<_>>>()?;
  let action = kind.action(undo);
  let me = users::uid_hash(api).await?;
  match kind.cancel_multi() {
    Some((path, field)) if undo && ids.len() > 1 => {
      bulk::batches(action, &ids, BATCH, String::clone, async |chunk| {
        let v = api
          .write_as(Method::POST, path, &me)
          .await?
          .header("referer", favorites_page())
          .field(field, chunk.join(","))
          .send()
          .await?;
        Ok(parse::cancelled(&v, action, chunk, |id| kind.url(id)))
      })
      .await
    }
    _ => {
      bulk::each(action, &ids, async |id| {
        save_one(api, &me, kind, id, undo).await
      })
      .await
    }
  }
}

/// One save: mixes and dramas by GET (`action` 1 or 0), music by POST.
async fn save_one(api: &Api, me: &str, kind: Saved, id: &str, undo: bool) -> Result<Action> {
  let action = u8::from(!undo);
  let call = match kind {
    Saved::Mix | Saved::Drama => {
      let (path, key) = match kind {
        Saved::Mix => ("/aweme/v1/web/mix/collect/", "mix_id"),
        _ => ("/aweme/v1/web/series/collect/", "series_id"),
      };
      let call = api.write_as(Method::GET, path, me).await?;
      call.arg(key, id).arg("action", action)
    }
    Saved::Music => {
      let call = api
        .write_as(Method::POST, "/aweme/v1/web/music/collect/", me)
        .await?;
      call
        .field("music_id", id)
        .field("type", action)
        .field("action", action)
    }
  };
  let v = call.send_raw().await?;
  if !(kind == Saved::Music && v.i64("status_code") == Some(ALREADY)) {
    api::check(v)?;
  }
  let done = Action::done(kind.action(undo), id);
  Ok(match kind.url(id) {
    Some(url) => done.with_url(url),
    None => done,
  })
}

/// `favorite` / `unfavorite` of several posts: saving goes one by one,
/// unsaving 50 per call, as the 收藏 tab's 取消 does.
pub async fn favorite_posts(
  api: &Api,
  posts: &[String],
  folder: Option<&str>,
  undo: bool,
) -> Result<Vec<Action>> {
  if folder.is_some() {
    return Err(Error::unsupported("favorite --folder").with_hint(library::FOLDER_HINT));
  }
  if !undo {
    let favorite = async |post: &str| library::favorite(api, post, None, false).await;
    return bulk::each("favorite", posts, favorite).await;
  }
  let ids = refs::awemes(api, posts).await?;
  let me = users::uid_hash(api).await?;
  bulk::batches("unfavorite", &ids, BATCH, String::clone, async |chunk| {
    // Ordinary posts are type 0 (西瓜 videos would be 1).
    let types: serde_json::Map<String, Value> =
      chunk.iter().map(|id| (id.clone(), json!(0))).collect();
    let v = api
      .write_as(Method::POST, UNSAVE_POSTS, &me)
      .await?
      .header("referer", favorites_page())
      .field("aweme_ids", chunk.join(","))
      .field("aweme_type_map", Value::Object(types))
      .send()
      .await?;
    Ok(parse::cancelled(&v, "unfavorite", chunk, |id| {
      Some(refs::video_url(id))
    }))
  })
  .await
}

fn favorites_page() -> String {
  format!("{WWW}/user/self?showTab=favorite_collection")
}
