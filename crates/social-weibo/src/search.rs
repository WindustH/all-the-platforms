//! Search through m.weibo.cn's search containers: posts, users and topics,
//! and the posts under a hashtag.

use social_core::{Collection, Error, Page, PageReq, Post, Query, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::parse;

/// One page of a m.weibo.cn search container (`100103type=<type>&q=<kw>`).
async fn search_cards(api: &Api, kind: &str, keyword: &str, page: u64) -> Result<Vec<Value>> {
  let container = format!("100103type={kind}&q={keyword}");
  let no = page.to_string();
  let v = api
    .get_mobile(
      "/api/container/getIndex",
      &[
        ("containerid", &container),
        ("page_type", "searchall"),
        ("page", &no),
      ],
    )
    .await?;
  Ok(v.list("data.cards").to_vec())
}

fn search_type(query: &Query) -> &'static str {
  match (query.filter.as_deref(), query.sort.as_deref()) {
    (Some("video"), _) => "64",
    (Some("image"), _) => "63",
    (_, Some("latest")) => "61",
    (_, Some("hot")) => "60",
    _ => "1",
  }
}

/// Search pages are numbered; an empty page ends the listing.
fn numbered<T>(items: Vec<T>, page: u64) -> Page<T> {
  let next = (!items.is_empty() && page < 50).then(|| (page + 1).to_string());
  Page::new(items, next)
}

pub async fn search(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let no = page.number_or(1);
  let cards = search_cards(api, search_type(query), &query.keyword, no).await?;
  Ok(numbered(parse::card_posts(&cards), no))
}

pub async fn search_users(api: &Api, query: &Query, page: &PageReq) -> Result<Page<User>> {
  let no = page.number_or(1);
  let cards = search_cards(api, "3", &query.keyword, no).await?;
  Ok(numbered(parse::card_users(&cards), no))
}

pub async fn search_topics(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Collection>> {
  let no = page.number_or(1);
  let cards = search_cards(api, "38", &query.keyword, no).await?;
  Ok(numbered(parse::card_topics(&cards), no))
}

/// Posts under a hashtag (`#name#`), newest first.
pub async fn topic(api: &Api, name: &str, page: &PageReq) -> Result<Page<Post>> {
  let name = name.trim().trim_matches('#');
  if name.is_empty() {
    return Err(Error::input("empty topic"));
  }
  let no = page.number_or(1);
  let container = format!("231522type=1&t=10&q=#{name}#");
  let v = api
    .get_mobile(
      "/api/container/getIndex",
      &[
        ("containerid", &container),
        ("page_type", "searchall"),
        ("page", &no.to_string()),
      ],
    )
    .await?;
  Ok(numbered(parse::card_posts(v.list("data.cards")), no))
}
