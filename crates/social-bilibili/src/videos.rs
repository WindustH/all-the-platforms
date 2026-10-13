//! Video endpoints: details, search, popular / ranking, related, uploads of a
//! user, watch later, and the video-only writes (like, coin, triple).

use social_core::{Action, Error, Page, PageReq, Post, Query, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::paging::Pn;
use crate::refs::Video;
use crate::{parse, users};

const VIEW: &str = "https://api.bilibili.com/x/web-interface/view";
const SEARCH: &str = "https://api.bilibili.com/x/web-interface/wbi/search/type";
const POPULAR: &str = "https://api.bilibili.com/x/web-interface/popular";
const RANKING: &str = "https://api.bilibili.com/x/web-interface/ranking/v2";
const RELATED: &str = "https://api.bilibili.com/x/web-interface/archive/related";
const UPLOADS: &str = "https://api.bilibili.com/x/space/wbi/arc/search";
const TOVIEW: &str = "https://api.bilibili.com/x/v2/history/toview";
const RELATION: &str = "https://api.bilibili.com/x/web-interface/archive/relation";

/// `hot --category`: `popular` (default) and the ranking regions below.
pub const HOT_CATEGORIES: &[&str] = &[
  "popular",
  "douga",
  "music",
  "dance",
  "game",
  "knowledge",
  "tech",
  "sports",
  "car",
  "life",
  "food",
  "animal",
  "kichiku",
  "fashion",
  "ent",
  "cinephile",
  "guochuang",
  "origin",
  "rookie",
];

/// Ranking regions `(category, rid, type)` behind `hot --category`.
const REGIONS: &[(&str, u32, &str)] = &[
  ("douga", 1005, "all"),
  ("music", 1003, "all"),
  ("dance", 1004, "all"),
  ("game", 1008, "all"),
  ("knowledge", 1010, "all"),
  ("tech", 1012, "all"),
  ("sports", 1018, "all"),
  ("car", 1013, "all"),
  ("life", 160, "all"),
  ("food", 1020, "all"),
  ("animal", 1024, "all"),
  ("kichiku", 1007, "all"),
  ("fashion", 1014, "all"),
  ("ent", 1002, "all"),
  ("cinephile", 1001, "all"),
  ("guochuang", 168, "all"),
  ("origin", 0, "origin"),
  ("rookie", 0, "rookie"),
];
const COINED: &str = "https://api.bilibili.com/x/space/coin/video";
const LIKED: &str = "https://api.bilibili.com/x/space/like/video";

fn videos(list: &[Value]) -> Vec<Post> {
  list
    .iter()
    .map(parse::video)
    .filter(|p| !p.id.is_empty())
    .collect()
}

/// Raw `view` data (title, owner, stat, pages ...).
pub async fn view(api: &Api, v: &Video) -> Result<Value> {
  api.get(VIEW).arg("bvid", &v.bvid).send().await
}

pub async fn read(api: &Api, v: &Video) -> Result<Post> {
  let mut post = parse::video(&view(api, v).await?);
  if api.ctx.http.has_cookie("SESSDATA")
    && let Ok(r) = api
      .get(RELATION)
      .arg("aid", v.aid)
      .arg("bvid", &v.bvid)
      .send()
      .await
  {
    // Your own relation to the video: liked, coins given, favorited ...
    let mine: serde_json::Map<String, Value> =
      ["like", "dislike", "favorite", "attention", "season_fav"]
        .into_iter()
        .filter_map(|k| Some((k.to_owned(), Value::Bool(r.bool(k)?))))
        .chain(r.u64("coin").map(|n| ("coins".to_owned(), Value::from(n))))
        .collect();
    if !mine.is_empty() {
      post.extra.insert("mine".into(), Value::Object(mine));
    }
  }
  Ok(post)
}

/// `cid` of part `page` (1-based) from `view` data.
pub fn cid(view: &Value, page: usize) -> Result<u64> {
  let pages = view.list("pages");
  pages
    .get(page.saturating_sub(1))
    .and_then(|p| p.u64("cid"))
    .or_else(|| (page == 1).then(|| view.u64("cid")).flatten())
    .ok_or_else(|| Error::not_found(format!("part {page} not found ({} parts)", pages.len())))
}

/// One page of `search/type`; `order` only applies to videos.
async fn search_type(
  api: &Api,
  kind: &str,
  keyword: &str,
  order: Option<&str>,
  page: &PageReq,
) -> Result<(Value, Pn)> {
  let pn = Pn::of(page, 50);
  let mut call = api
    .get(SEARCH)
    .arg("search_type", kind)
    .arg("keyword", keyword)
    .arg("page", pn.pn)
    .arg("page_size", pn.ps);
  if let Some(order) = order {
    call = call.arg("order", order);
  }
  Ok((call.wbi().send().await?, pn))
}

pub async fn search(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let (data, pn) = search_type(api, "video", &query.keyword, query.sort.as_deref(), page).await?;
  let more = pn.pn < data.u64("numPages").unwrap_or(0);
  Ok(pn.page(videos(data.list("result")), more))
}

pub async fn search_users(api: &Api, query: &Query, page: &PageReq) -> Result<Page<User>> {
  let (data, pn) = search_type(api, "bili_user", &query.keyword, None, page).await?;
  let more = pn.pn < data.u64("numPages").unwrap_or(0);
  let users = data.list("result").iter().map(parse::user).collect();
  Ok(pn.page(users, more))
}

/// `popular` (paged) or one ranking region (single page).
pub async fn hot(api: &Api, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let category = category.unwrap_or("popular");
  if category == "popular" {
    let pn = Pn::of(page, 50);
    let data = api
      .get(POPULAR)
      .arg("pn", pn.pn)
      .arg("ps", pn.ps)
      .arg("web_location", "333.934")
      .wbi()
      .send()
      .await?;
    let more = data.bool("no_more") == Some(false);
    return Ok(pn.page(videos(data.list("list")), more));
  }
  let (_, rid, kind) = REGIONS
    .iter()
    .find(|(name, ..)| *name == category)
    .ok_or_else(|| Error::input(format!("unknown category: {category}")))?;
  ranking(api, *rid, kind).await
}

/// One ranking board (`rid` 0 = whole site).
pub async fn ranking(api: &Api, rid: u32, kind: &str) -> Result<Page<Post>> {
  let data = api
    .get(RANKING)
    .arg("rid", rid)
    .arg("type", kind)
    .arg("web_location", "333.934")
    .wbi()
    .send()
    .await?;
  Ok(Page::last(videos(data.list("list"))))
}

pub async fn related(api: &Api, v: &Video) -> Result<Page<Post>> {
  let data = api.get(RELATED).arg("bvid", &v.bvid).send().await?;
  Ok(Page::last(videos(data.list(""))))
}

/// Videos uploaded by `mid`, newest first.
pub async fn uploads(api: &Api, mid: &str, page: &PageReq) -> Result<Page<Post>> {
  let pn = Pn::of(page, 50);
  let data = api
    .get(UPLOADS)
    .arg("mid", mid)
    .arg("pn", pn.pn)
    .arg("ps", pn.ps)
    .arg("order", "pubdate")
    .arg("platform", "web")
    .arg("web_location", "333.1387")
    .dm()
    .wbi()
    .send()
    .await?;
  let more = pn.before(data.u64("page.count"));
  Ok(pn.page(videos(data.list("list.vlist")), more))
}

pub async fn watch_later(api: &Api) -> Result<Page<Post>> {
  api.require_login()?;
  let data = api.get(TOVIEW).send().await?;
  Ok(Page::last(videos(data.list("list"))))
}

// ── writes ───────────────────────────────────────────────────────────────

pub async fn like(api: &Api, v: &Video, undo: bool) -> Result<Action> {
  api
    .post("https://api.bilibili.com/x/web-interface/archive/like")
    .arg("aid", v.aid)
    .arg("like", if undo { 2 } else { 1 })
    .send()
    .await?;
  let action = if undo { "unlike" } else { "like" };
  Ok(Action::done(action, &v.bvid).with_url(v.url()))
}

pub async fn coin(api: &Api, v: &Video, count: u8, like: bool) -> Result<Action> {
  api
    .post("https://api.bilibili.com/x/web-interface/coin/add")
    .arg("aid", v.aid)
    .arg("multiply", count)
    .arg("select_like", u8::from(like))
    .send()
    .await?;
  Ok(
    Action::done("coin", &v.bvid)
      .with_url(v.url())
      .with_message(format!(
        "{count} coin(s){}",
        if like { " + like" } else { "" }
      )),
  )
}

/// Like + coin + favorite in one request.
pub async fn triple(api: &Api, v: &Video) -> Result<Action> {
  let data = api
    .post("https://api.bilibili.com/x/web-interface/archive/like/triple")
    .arg("aid", v.aid)
    .send()
    .await?;
  let done: Vec<&str> = [("like", "like"), ("coin", "coin"), ("fav", "favorite")]
    .into_iter()
    .filter(|(k, _)| data.bool(k) == Some(true))
    .map(|(_, label)| label)
    .collect();
  Ok(
    Action::done("triple", &v.bvid)
      .with_url(v.url())
      .with_message(format!("done: {}", done.join(", "))),
  )
}

/// Videos a user recently coined (`coined`) or liked (`liked`); many users
/// keep these private.
pub async fn recent(api: &Api, input: &str, liked: bool) -> Result<Page<Post>> {
  let mid = users::mid(api, input).await?;
  let data = api
    .get(if liked { LIKED } else { COINED })
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
