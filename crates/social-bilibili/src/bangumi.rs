//! Bangumi, films and documentaries (PGC): seasons with their episodes,
//! following them, your followed list and the release timeline. Metadata
//! only: downloads stay limited to ordinary videos.

use social_core::text::from_secs;
use social_core::{Action, Collection, Page, PageReq, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::paging::Pn;
use crate::refs::{self, PgcRef, season_url};
use crate::{parse, users};

const SEASON: &str = "https://api.bilibili.com/pgc/view/web/season";
const FOLLOW: &str = "https://api.bilibili.com/pgc/web/follow/add";
const UNFOLLOW: &str = "https://api.bilibili.com/pgc/web/follow/del";
const FOLLOWED: &str = "https://api.bilibili.com/x/space/bangumi/follow/list";
const TIMELINE: &str = "https://api.bilibili.com/pgc/web/timeline";
const REVIEW: &str = "https://api.bilibili.com/pgc/review/user";

/// Stable names of Bilibili's season types.
pub fn kind(season_type: Option<u64>) -> &'static str {
  match season_type {
    Some(2) => "film",
    Some(3) => "documentary",
    Some(4) => "guochuang",
    Some(5) => "series",
    Some(7) => "variety",
    _ => "bangumi",
  }
}

/// Raw season data from a season, episode or media (review page) id.
pub async fn season(api: &Api, input: &str) -> Result<Value> {
  let call = api.get(SEASON);
  let call = match refs::season(input)? {
    PgcRef::Season(id) => call.arg("season_id", id),
    PgcRef::Episode(id) => call.arg("ep_id", id),
    PgcRef::Media(id) => {
      let review = api.get(REVIEW).arg("media_id", id).send().await?;
      let sid = review.u64("media.season_id").unwrap_or(0);
      call.arg("season_id", sid)
    }
  };
  call.send().await
}

/// The season as a collection (`--info`).
pub fn info(v: &Value) -> Collection {
  let id = v.str("season_id").unwrap_or_default();
  let mut c = Collection {
    url: Some(season_url(&id)),
    id: format!("ss{id}"),
    kind: kind(v.u64("type").or_else(|| v.u64("season_type"))).into(),
    name: v.str("title").unwrap_or_default(),
    description: v.str("evaluate").filter(|d| !d.is_empty()),
    items: Some(v.list("episodes").len() as u64),
    followers: v.count("stat.favorites"),
    views: v.count("stat.views"),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  for (key, path) in [
    ("type_name", "type_name"),
    ("status", "new_ep.desc"),
    ("publish", "publish.pub_time"),
    ("media_id", "media_id"),
    ("series", "series.series_title"),
  ] {
    if let Some(x) = v.str(path).filter(|x| !x.is_empty() && x != "0") {
      c.extra.insert(key.into(), json!(x));
    }
  }
  for (key, path) in [
    ("raters", "rating.count"),
    ("danmaku", "stat.danmakus"),
    ("coins", "stat.coins"),
    ("likes", "stat.likes"),
  ] {
    if let Some(n) = v.count(path).filter(|n| *n > 0) {
      c.extra.insert(key.into(), json!(n));
    }
  }
  if let Some(cover) = v.str("cover") {
    c.extra.insert("cover".into(), json!(parse::https(cover)));
  }
  if let Some(score) = v.f64("rating.score") {
    c.extra.insert("score".into(), json!(score));
  }
  let styles: Vec<String> = v
    .list("styles")
    .iter()
    .filter_map(|s| s.as_str().map(str::to_owned))
    .collect();
  if !styles.is_empty() {
    c.extra.insert("styles".into(), json!(styles));
  }
  c
}

/// Episodes of a season (main episodes, then extras such as PVs).
pub fn episodes(v: &Value) -> Vec<Post> {
  let season = v.str("season_id").unwrap_or_default();
  let sections = v
    .list("section")
    .iter()
    .flat_map(|s| s.list("episodes").iter());
  v.list("episodes")
    .iter()
    .chain(sections)
    .map(|e| {
      let ep = e.str("ep_id").or_else(|| e.str("id")).unwrap_or_default();
      // `show_title` already reads "第1话 <long title>".
      let title = e.str("show_title").unwrap_or_else(|| {
        [e.str("title"), e.str("long_title")]
          .into_iter()
          .flatten()
          .filter(|t| !t.is_empty())
          .collect::<Vec<_>>()
          .join(" ")
      });
      let mut p = Post {
        id: format!("ep{ep}"),
        kind: "episode".into(),
        title: Some(title).filter(|t| !t.is_empty()),
        url: Some(format!("https://www.bilibili.com/bangumi/play/ep{ep}")),
        created_at: e.i64("pub_time").and_then(from_secs),
        raw: Some(e.clone()),
        ..Post::default()
      };
      for (key, path) in [("bvid", "bvid"), ("badge", "badge")] {
        if let Some(x) = e.str(path).filter(|x| !x.is_empty()) {
          p.extra.insert(key.into(), json!(x));
        }
      }
      if let Some(cover) = e.str("cover") {
        p.extra.insert("cover".into(), json!(parse::https(cover)));
      }
      if let Some(ms) = e.u64("duration").filter(|d| *d > 0) {
        p.extra.insert("duration".into(), json!(ms / 1000));
      }
      p.extra
        .insert("season".into(), json!(format!("ss{season}")));
      p
    })
    .collect()
}

pub async fn follow(api: &Api, input: &str, undo: bool) -> Result<Action> {
  let v = season(api, input).await?;
  let id = v.str("season_id").unwrap_or_default();
  api
    .post(if undo { UNFOLLOW } else { FOLLOW })
    .arg("season_id", &id)
    .send()
    .await?;
  let action = if undo { "unfollow" } else { "follow" };
  Ok(
    Action::done(action, format!("ss{id}"))
      .with_url(season_url(&id))
      .with_message(v.str("title").unwrap_or_default()),
  )
}

/// Bangumi (`anime`) or films and series (`drama`) a user follows.
pub async fn followed(
  api: &Api,
  input: Option<&str>,
  drama: bool,
  page: &PageReq,
) -> Result<Page<Collection>> {
  let mid = match input {
    Some(u) => users::mid(api, u).await?,
    None => users::my_mid(api).await?,
  };
  let pn = Pn::of(page, 30);
  let data = api
    .get(FOLLOWED)
    .arg("vmid", &mid)
    .arg("type", if drama { 2 } else { 1 })
    .arg("follow_status", 0)
    .arg("pn", pn.pn)
    .arg("ps", pn.ps)
    .send()
    .await?;
  let items = data
    .list("list")
    .iter()
    .map(|s| {
      let mut c = info(s);
      c.description = s.str("evaluate").filter(|d| !d.is_empty());
      if let Some(x) = s.str("progress").filter(|x| !x.is_empty()) {
        c.extra.insert("progress".into(), json!(x));
      }
      let status = match s.u64("follow_status") {
        Some(1) => Some("want"),
        Some(2) => Some("watching"),
        Some(3) => Some("watched"),
        _ => None,
      };
      if let Some(status) = status {
        c.extra.insert("status".into(), json!(status));
      }
      c
    })
    .collect();
  Ok(pn.page(items, pn.before(data.u64("total"))))
}

/// Episodes airing from `before` days ago to `after` days ahead.
pub async fn timeline(api: &Api, drama: bool, before: u8, after: u8) -> Result<Page<Post>> {
  let data = api
    .get(TIMELINE)
    .arg("types", if drama { 4 } else { 1 })
    .arg("before", before)
    .arg("after", after)
    .send()
    .await?;
  let days = match &data {
    Value::Array(a) => a.clone(),
    _ => data.list("result").to_vec(),
  };
  let posts = days
    .iter()
    .flat_map(|day| day.list("episodes").to_vec())
    .map(|e| {
      let season = e.str("season_id").unwrap_or_default();
      let ep = e.str("episode_id").unwrap_or_default();
      let mut p = Post {
        id: format!("ep{ep}"),
        kind: "episode".into(),
        title: e.str("title"),
        text: e.str("pub_index"),
        url: Some(if ep.is_empty() {
          season_url(&season)
        } else {
          format!("https://www.bilibili.com/bangumi/play/ep{ep}")
        }),
        created_at: e.i64("pub_ts").and_then(from_secs),
        raw: Some(e.clone()),
        ..Post::default()
      };
      p.extra
        .insert("season".into(), json!(format!("ss{season}")));
      if let Some(t) = e.str("pub_time") {
        p.extra.insert("airs".into(), json!(t));
      }
      if let Some(cover) = e.str("cover") {
        p.extra.insert("cover".into(), json!(parse::https(cover)));
      }
      p
    })
    .collect();
  Ok(Page::last(posts))
}
