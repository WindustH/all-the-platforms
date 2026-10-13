//! Search: posts, users and topics.

use social_core::{Collection, Page, PageReq, Post, Query, Result, User, Value, ValueExt};

use crate::api::{Api, V4};
use crate::{api, parse};

/// `search_v3` for one result type (`general`, `people`, `topic`).
async fn search_v3(
  api: &Api,
  kind: &str,
  q: &Query,
  page: &PageReq,
) -> Result<(Vec<Value>, Option<String>)> {
  let offset = page.number_or(0);
  let mut req = api.get(&format!("{V4}/search_v3")).queries([
    ("gk_version", "gz-gaokao".to_owned()),
    ("t", kind.to_owned()),
    ("q", q.keyword.clone()),
    ("correction", "1".to_owned()),
    ("offset", offset.to_string()),
    ("limit", page.size_within(20).to_string()),
    ("filter_fields", String::new()),
    ("lc_idx", offset.to_string()),
    ("show_all_topics", "0".to_owned()),
    ("search_source", "Normal".to_owned()),
  ]);
  match q.sort.as_deref() {
    Some("upvoted") => req = req.query("sort", "upvoted_count"),
    Some("newest") => req = req.query("sort", "created_time"),
    _ => {}
  }
  if let Some(vertical) = &q.filter {
    req = req.query("vertical", vertical);
  }
  let v = api.call(req).await?;
  let objects = v
    .list("data")
    .iter()
    .filter(|item| item.str("type").as_deref() == Some("search_result"))
    .map(|item| item.at("object").clone())
    .collect();
  Ok((objects, api::next_param(&v, "offset")))
}

pub async fn search(api: &Api, q: &Query, page: &PageReq) -> Result<Page<Post>> {
  let (objects, next) = search_v3(api, "general", q, page).await?;
  Ok(Page::new(
    objects.iter().filter_map(parse::post).collect(),
    next,
  ))
}

pub async fn search_users(api: &Api, q: &Query, page: &PageReq) -> Result<Page<User>> {
  let (objects, next) = search_v3(api, "people", q, page).await?;
  let users = objects
    .iter()
    .filter(|o| o.str("type").as_deref() == Some("people"))
    .map(parse::user)
    .collect();
  Ok(Page::new(users, next))
}

pub async fn search_topics(api: &Api, q: &Query, page: &PageReq) -> Result<Page<Collection>> {
  let (objects, next) = search_v3(api, "topic", q, page).await?;
  let topics = objects
    .iter()
    .filter(|o| o.str("type").as_deref() == Some("topic"))
    .map(parse::topic)
    .collect();
  Ok(Page::new(topics, next))
}
