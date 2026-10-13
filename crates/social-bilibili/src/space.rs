//! A user's space beyond uploads: collections (合集) and series (系列) with
//! their videos, articles, and the videos they recently coined or liked.

use social_core::{Collection, Ctx, Page, PageReq, Post, Result, Value, ValueExt};

use crate::page::Pn;
use crate::refs::{self, space_url};
use crate::{api, article, parse, user};

const LISTS: &str = "https://api.bilibili.com/x/polymer/web-space/home/seasons_series";
const SEASON: &str = "https://api.bilibili.com/x/polymer/web-space/seasons_archives_list";
const SERIES: &str = "https://api.bilibili.com/x/series/archives";
const ARTICLES: &str = "https://api.bilibili.com/x/space/wbi/article";
const COINED: &str = "https://api.bilibili.com/x/space/coin/video";
const LIKED: &str = "https://api.bilibili.com/x/space/like/video";

/// Collections and series of a user; ids work with `list`.
pub async fn lists(ctx: &Ctx, input: &str, page: &PageReq) -> Result<Page<Collection>> {
  let mid = user::mid(ctx, input).await?;
  let pn = Pn::of(page, 20);
  let data = api::get(ctx, LISTS)
    .arg("mid", &mid)
    .arg("page_num", pn.pn)
    .arg("page_size", pn.ps)
    .wbi()
    .send()
    .await?;
  let lists = data.at("items_lists");
  let seasons = lists
    .list("seasons_list")
    .iter()
    .map(|s| list(&mid, s, "season"));
  let series = lists
    .list("series_list")
    .iter()
    .map(|s| list(&mid, s, "series"));
  let items: Vec<Collection> = seasons.chain(series).collect();
  Ok(pn.page(items, pn.before(lists.u64("page.total"))))
}

fn list(mid: &str, v: &Value, kind: &str) -> Collection {
  let meta = v.at("meta");
  let id = meta
    .first_str(&["season_id", "series_id"])
    .unwrap_or_default();
  Collection {
    id: if kind == "season" {
      format!("{mid}/{id}")
    } else {
      format!("{mid}/series/{id}")
    },
    kind: kind.into(),
    name: meta.str("name").unwrap_or_default(),
    description: meta.str("description").filter(|d| !d.is_empty()),
    url: Some(format!("{}/lists/{id}?type={kind}", space_url(mid))),
    items: meta.count("total"),
    raw: Some(v.clone()),
    ..Collection::default()
  }
}

/// The videos of a collection or series, in their own order.
pub async fn videos(ctx: &Ctx, input: &str, page: &PageReq) -> Result<Page<Post>> {
  let (mid, id, kind) = refs::list(input)?;
  let pn = Pn::of(page, 30);
  let data = if kind == "season" {
    api::get(ctx, SEASON)
      .arg("mid", &mid)
      .arg("season_id", &id)
      .arg("page_num", pn.pn)
      .arg("page_size", pn.ps)
      .arg("sort_reverse", false)
      .send()
      .await?
  } else {
    api::get(ctx, SERIES)
      .arg("mid", &mid)
      .arg("series_id", &id)
      .arg("pn", pn.pn)
      .arg("ps", pn.ps)
      .arg("sort", "asc")
      .send()
      .await?
  };
  let posts = data
    .list("archives")
    .iter()
    .map(parse::video)
    .filter(|p| !p.id.is_empty())
    .collect();
  Ok(pn.page(posts, pn.before(data.u64("page.total"))))
}

/// Column articles of a user, newest first.
pub async fn articles(ctx: &Ctx, input: &str, page: &PageReq) -> Result<Page<Post>> {
  let mid = user::mid(ctx, input).await?;
  let pn = Pn::of(page, 30);
  let data = api::get(ctx, ARTICLES)
    .arg("mid", &mid)
    .arg("pn", pn.pn)
    .arg("ps", pn.ps)
    .arg("sort", "publish_time")
    .wbi()
    .send()
    .await?;
  let posts = data
    .list("articles")
    .iter()
    .filter_map(|a| Some(article::post(a.u64("id")?, a.clone())))
    .collect();
  Ok(pn.page(posts, pn.before(data.u64("count"))))
}

/// Videos a user recently coined (`coined`) or liked (`liked`); many users
/// keep these private.
pub async fn recent(ctx: &Ctx, input: &str, liked: bool) -> Result<Page<Post>> {
  let mid = user::mid(ctx, input).await?;
  let data = api::get(ctx, if liked { LIKED } else { COINED })
    .arg("vmid", &mid)
    .send()
    .await?;
  let list = match &data {
    Value::Array(a) => a.clone(),
    _ => data.list("list").to_vec(),
  };
  Ok(Page::last(
    list
      .iter()
      .map(parse::video)
      .filter(|p| !p.id.is_empty())
      .collect(),
  ))
}
