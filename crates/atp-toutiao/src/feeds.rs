//! The hot board and the feeds: the signed recommend feed, and the legacy
//! channel feeds (one page each: they ignore their cursor).

use atp_core::{Error, Page, PageReq, Post, Result, Value, ValueExt};

use crate::api::{self, Api};
use crate::parse;

/// `feed --type` values: the recommend feed, then the channels.
pub const FEED_KINDS: &[&str] = &[
  "recommend",
  "hot",
  "tech",
  "finance",
  "world",
  "sports",
  "entertainment",
  "military",
  "video",
  "all",
];

pub async fn hot(api: &Api) -> Result<Page<Post>> {
  let v = api
    .get("/hot-event/hot-board/", &[("origin", "toutiao_pc".into())])
    .await?;
  if v.str("status").as_deref() != Some("success") {
    return Err(Error::upstream("the hot board answered without data"));
  }
  Ok(Page::last(
    v.list("data").iter().filter_map(parse::hot).collect(),
  ))
}

pub async fn feed(api: &Api, kind: Option<&str>, req: &PageReq) -> Result<Page<Post>> {
  match kind.unwrap_or("recommend") {
    "recommend" => recommend(api, req).await,
    channel => legacy(api, channel).await,
  }
}

/// The signed recommend feed, paged by the last item's `behot_time`.
async fn recommend(api: &Api, req: &PageReq) -> Result<Page<Post>> {
  let before = req
    .cursor
    .clone()
    .unwrap_or_else(|| jiff::Timestamp::now().as_second().to_string());
  let query = [
    ("channel_id", "0".to_owned()),
    ("max_behot_time", before),
    ("offset", "0".into()),
    ("category", "pc_profile_recommend".into()),
  ];
  let v = api::check(
    api
      .signed(api::GET, "/api/pc/list/feed", &query, &[])
      .await?,
  )?;
  let cells = v.list("data");
  let posts: Vec<Post> = cells.iter().filter_map(parse::post).collect();
  let next = more(&v)
    .then(|| cells.iter().rev().find_map(|c| c.i64("behot_time")))
    .flatten()
    .map(|t| t.to_string());
  Ok(Page::new(posts, next))
}

/// A channel of the legacy feed (no signature, a single page).
async fn legacy(api: &Api, channel: &str) -> Result<Page<Post>> {
  let category = match channel {
    "all" => "__all__".to_owned(),
    "video" => "video".to_owned(),
    other => format!("news_{other}"),
  };
  let v = api::check(api.get("/api/pc/feed/", &[("category", category)]).await?)?;
  Ok(Page::last(
    v.list("data")
      .iter()
      .filter_map(parse::channel_post)
      .collect(),
  ))
}

pub fn more(v: &Value) -> bool {
  v.bool("has_more").unwrap_or(false)
}
