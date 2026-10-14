//! Topics and their replies. With a token, topics, replies and node
//! listings come page by page from API 2.0; without one, from API 1.0,
//! which hands out whole lists (all replies of a topic, the first page of
//! a node).

use atp_core::paging::slice;
use atp_core::{Comment, Error, Page, PageReq, Post, Result, Value, ValueExt};

use crate::api::Api;
use crate::{parse, refs};

/// `hot --category`: today's hot topics, or the latest ones.
pub const HOT_CATEGORIES: &[&str] = &["hot", "latest"];
/// Rows per page of API 2.0 listings, when the answer does not say.
const V2_PAGE: usize = 20;

pub async fn hot(api: &Api, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let path = match category.unwrap_or("hot") {
    "latest" => "topics/latest.json",
    _ => "topics/hot.json",
  };
  let v = api.v1(path, Vec::new()).await?;
  Ok(slice(v.list("").iter().map(parse::topic).collect(), page))
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let id = refs::topic(arg)?;
  if api.token().is_some() {
    let v = api.v2(&format!("topics/{id}"), Vec::new()).await?;
    return Ok(parse::topic(v.at("result")));
  }
  let v = api
    .v1("topics/show.json", vec![("id", id.to_string())])
    .await?;
  v.list("")
    .first()
    .map(parse::topic)
    .ok_or_else(|| Error::not_found(format!("no V2EX topic {id}")))
}

/// Replies in floor order.
pub async fn replies(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Comment>> {
  let id = refs::topic(arg)?;
  let topic = id.to_string();
  if api.token().is_some() {
    let no = page.number_or(1);
    let v = api
      .v2(&format!("topics/{id}/replies"), vec![("p", no.to_string())])
      .await?;
    let per_page = v.u64("pagination.per_page").map_or(V2_PAGE, |n| n as usize);
    let first = (no as usize - 1) * per_page;
    let rows = v.list("result");
    let items = rows
      .iter()
      .enumerate()
      .map(|(i, row)| parse::reply(row, first + i + 1, &topic))
      .collect();
    return Ok(Page::new(items, next_page(&v, no, rows.len(), per_page)));
  }
  let v = api
    .v1("replies/show.json", vec![("topic_id", topic.clone())])
    .await?;
  let all = v
    .list("")
    .iter()
    .enumerate()
    .map(|(i, row)| parse::reply(row, i + 1, &topic))
    .collect();
  Ok(slice(all, page))
}

/// The next page number of an API 2.0 listing: from `pagination.pages`, or
/// after every full page.
fn next_page(v: &Value, no: u64, rows: usize, per_page: usize) -> Option<String> {
  let more = match v.u64("pagination.pages") {
    Some(pages) => no < pages,
    None => rows >= per_page,
  };
  more.then(|| (no + 1).to_string())
}

/// Topics of a node: page by page with a token, else the first page.
pub async fn node_topics(api: &Api, node: &str, page: &PageReq) -> Result<Page<Post>> {
  if api.token().is_some() {
    let no = page.number_or(1);
    let v = api
      .v2(&format!("nodes/{node}/topics"), vec![("p", no.to_string())])
      .await?;
    let rows = v.list("result");
    let items = rows.iter().map(parse::topic).collect();
    return Ok(Page::new(items, next_page(&v, no, rows.len(), V2_PAGE)));
  }
  let v = api
    .v1("topics/show.json", vec![("node_name", node.to_owned())])
    .await?;
  Ok(slice(v.list("").iter().map(parse::topic).collect(), page))
}

/// A member's topics (API 1.0: the most recent ones).
pub async fn member_topics(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let name = refs::member(arg)?;
  let v = api.v1("topics/show.json", vec![("username", name)]).await?;
  Ok(slice(v.list("").iter().map(parse::topic).collect(), page))
}
