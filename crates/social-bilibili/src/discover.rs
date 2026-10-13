//! Discovery beyond `hot`: trending searches, the weekly picks (每周必看),
//! the all-time picks (入站必刷), ranking boards by region and topic feeds.

use social_core::{Collection, Error, Page, PageReq, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::{parse, videos};

const TRENDING: &str = "https://api.bilibili.com/x/v2/search/trending/ranking";
const WEEKLY: &str = "https://api.bilibili.com/x/web-interface/popular/series/one";
const WEEKLY_LIST: &str = "https://api.bilibili.com/x/web-interface/popular/series/list";
const PRECIOUS: &str = "https://api.bilibili.com/x/web-interface/popular/precious";
const TOPIC_FEED: &str = "https://api.bilibili.com/x/polymer/web-dynamic/v1/feed/topic";

/// Trending search words (热搜), best first.
pub async fn trending(api: &Api, limit: usize) -> Result<Page<Collection>> {
  let data = api
    .get(TRENDING)
    .arg("limit", limit.clamp(1, 50))
    .send()
    .await?;
  let items = data
    .list("list")
    .iter()
    .map(|w| {
      let keyword = w.str("keyword").unwrap_or_default();
      let mut c = Collection {
        url: Some(format!(
          "https://search.bilibili.com/all?keyword={}",
          url_escape(&keyword)
        )),
        id: w.str("hot_id").unwrap_or_else(|| keyword.clone()),
        kind: "trend".into(),
        name: w.str("show_name").unwrap_or_else(|| keyword.clone()),
        raw: Some(w.clone()),
        ..Collection::default()
      };
      c.extra.insert("keyword".into(), json!(keyword));
      if let Some(pos) = w.u64("position") {
        c.extra.insert("position".into(), json!(pos));
      }
      c
    })
    .collect();
  Ok(Page::last(items))
}

fn url_escape(s: &str) -> String {
  s.bytes()
    .map(|b| match b {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
        (b as char).to_string()
      }
      _ => format!("%{b:02X}"),
    })
    .collect()
}

/// One issue of 每周必看 (`number`), or the latest when omitted.
pub async fn weekly(api: &Api, number: Option<u64>) -> Result<Page<Post>> {
  let number = match number {
    Some(n) => n,
    None => {
      let list = api.get(WEEKLY_LIST).send().await?;
      list
        .list("list")
        .first()
        .and_then(|i| i.u64("number"))
        .ok_or_else(|| Error::upstream("no weekly issue listed"))?
    }
  };
  let data = api.get(WEEKLY).arg("number", number).send().await?;
  Ok(Page::last(with_reason(data.list("list"))))
}

/// 入站必刷: the all-time picks.
pub async fn precious(api: &Api) -> Result<Page<Post>> {
  let data = api
    .get(PRECIOUS)
    .arg("page_size", 100)
    .arg("page", 1)
    .send()
    .await?;
  Ok(Page::last(with_reason(data.list("list"))))
}

/// Videos with the editors' reason (`rcmd_reason`, `achievement`).
fn with_reason(list: &[Value]) -> Vec<Post> {
  list
    .iter()
    .map(|v| {
      let mut p = parse::video(v);
      if let Some(r) = v
        .first_str(&["rcmd_reason", "achievement"])
        .filter(|r| !r.is_empty())
      {
        p.extra.insert("reason".into(), json!(r));
      }
      p
    })
    .filter(|p| !p.id.is_empty())
    .collect()
}

/// A ranking board: the whole site, or a region of `hot --category`.
pub async fn rank(api: &Api, region: Option<&str>) -> Result<Page<Post>> {
  match region {
    None | Some("all") => videos::ranking(api, 0, "all").await,
    Some(name) => videos::hot(api, Some(name), &PageReq::default()).await,
  }
}

/// Dynamics under a topic (`sort`: hot, new or the topic's default).
pub async fn topic(api: &Api, id: &str, sort: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let id = id
    .rsplit("topic_id=")
    .next()
    .unwrap_or(id)
    .split('&')
    .next()
    .unwrap_or(id);
  let sort_by = match sort {
    Some("hot") => 1,
    Some("new") => 2,
    _ => 0,
  };
  let data = api
    .get(TOPIC_FEED)
    .arg("topic_id", id)
    .arg("sort_by", sort_by)
    .arg("offset", page.cursor.as_deref().unwrap_or_default())
    .arg("page_size", page.size_within(20))
    .arg("source", "Web")
    .send()
    .await?;
  let list = data.at("topic_card_list");
  let posts = list
    .list("items")
    .iter()
    .filter_map(|i| {
      let card = i.at("dynamic_card_item");
      (!card.is_null()).then(|| parse::dynamic(card))
    })
    .collect();
  let next = (list.bool("has_more") == Some(true))
    .then(|| list.str("offset"))
    .flatten();
  Ok(Page::new(posts, next))
}
