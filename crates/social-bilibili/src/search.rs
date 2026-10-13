//! Searches beyond videos and users: articles, bangumi / films (`search
//! --filter`) and topics (`search -t topic`).

use social_core::text::from_secs;
use social_core::{
  Collection, Ctx, Error, Page, PageReq, Post, Query, Result, Value, ValueExt, json,
};

use crate::page::Pn;
use crate::{api, article, live, parse, refs, video};

const SEARCH: &str = "https://api.bilibili.com/x/web-interface/wbi/search/type";
const TOPICS: &str = "https://app.bilibili.com/x/topic/pub/search";

/// `search --filter`: what to search for (`video` when omitted).
pub const FILTERS: &[&str] = &["video", "article", "bangumi", "film", "live"];

pub async fn posts(ctx: &Ctx, q: &Query, page: &PageReq) -> Result<Page<Post>> {
  let kind = match q.filter.as_deref().unwrap_or("video") {
    "video" => return video::search(ctx, q, page).await,
    "article" => "article",
    "bangumi" => "media_bangumi",
    "film" => "media_ft",
    "live" => "live_room",
    other => return Err(Error::input(format!("unknown search filter: {other}"))),
  };
  let pn = Pn::of(page, 50);
  let mut call = api::get(ctx, SEARCH)
    .arg("search_type", kind)
    .arg("keyword", &q.keyword)
    .arg("page", pn.pn)
    .arg("page_size", pn.ps);
  if kind == "article"
    && let Some(order) = q.sort.as_deref()
  {
    // Article search orders: totalrank, pubdate, click, attention, scores.
    call = call.arg("order", order);
  }
  let data = call.wbi().send().await?;
  let more = pn.pn < data.u64("numPages").unwrap_or(0);
  let items = data
    .list("result")
    .iter()
    .map(|v| match kind {
      "article" => article_result(v),
      "live_room" => live::room_post(v),
      _ => season_result(v),
    })
    .collect();
  Ok(pn.page(items, more))
}

fn article_result(v: &Value) -> Post {
  let id = v.u64("id").unwrap_or(0);
  let mut post = article::post(id, v.clone());
  if post.author.is_none() {
    post.author = parse::author(v, &["mid"], &["author"], &[]);
  }
  post.created_at = v.i64("pub_time").and_then(from_secs);
  post
}

/// A bangumi / film search result as a post pointing at its season.
pub fn season_result(v: &Value) -> Post {
  let season = v.str("season_id").unwrap_or_default();
  let mut post = Post {
    id: format!("ss{season}"),
    kind: crate::pgc::kind(v.u64("season_type")).into(),
    title: parse::plain(v, &["title"]),
    text: parse::plain(v, &["desc"]),
    url: Some(refs::season_url(&season)),
    created_at: v.i64("pubtime").and_then(from_secs),
    raw: Some(v.clone()),
    ..Post::default()
  };
  for (key, path) in [
    ("type_name", "season_type_name"),
    ("score", "media_score.score"),
    ("episodes", "ep_size"),
    ("styles", "styles"),
    ("areas", "areas"),
    ("media_id", "media_id"),
    ("cover", "cover"),
  ] {
    if let Some(x) = v.str(path).filter(|x| !x.is_empty() && x != "0") {
      post.extra.insert(key.into(), json!(x));
    }
  }
  post
}

/// Topics (话题) whose name matches; ids work with `topic`.
pub async fn topics(ctx: &Ctx, q: &Query, page: &PageReq) -> Result<Page<Collection>> {
  let offset = page.number_or(0);
  let data = api::get(ctx, TOPICS)
    .arg("keywords", &q.keyword)
    .arg("page_size", page.size_within(20))
    .arg("offset", offset)
    .send()
    .await?;
  let items = data
    .list("topic_items")
    .iter()
    .map(|t| {
      let id = t.str("id").unwrap_or_default();
      let mut c = Collection {
        url: Some(format!(
          "https://www.bilibili.com/v/topic/detail/?topic_id={id}"
        )),
        id,
        kind: "topic".into(),
        name: t.str("name").unwrap_or_default(),
        description: t.str("description").filter(|d| !d.is_empty()),
        views: t.count("view"),
        raw: Some(t.clone()),
        ..Collection::default()
      };
      if let Some(n) = t.count("discuss") {
        c.extra.insert("discussions".into(), json!(n));
      }
      c
    })
    .collect();
  let next = (data.bool("page_info.has_more") == Some(true))
    .then(|| data.str("page_info.offset"))
    .flatten();
  Ok(Page::new(items, next))
}
