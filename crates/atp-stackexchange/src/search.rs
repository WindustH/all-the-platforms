//! Search: questions through `/search/advanced` (`[tag]` words in the
//! query become tag filters), users by name, tags by name.

use atp_core::{Collection, Page, PageReq, Post, Query, Result, User};

use crate::api::{Api, FILTER_LIST, Params};
use crate::parse;

pub const SORTS: &[&str] = &["relevance", "votes", "newest", "active"];
pub const FILTERS: &[&str] = &["accepted", "unanswered", "closed", "open"];

/// Words and `[tag]`s of a query: `[rust] [async] lifetimes` →
/// (`lifetimes`, `rust;async`).
fn split_tags(query: &str) -> (String, String) {
  let mut words = Vec::new();
  let mut tags = Vec::new();
  for word in query.split_whitespace() {
    match word.strip_prefix('[').and_then(|w| w.strip_suffix(']')) {
      Some(tag) if !tag.is_empty() => tags.push(tag.to_owned()),
      _ => words.push(word),
    }
  }
  (words.join(" "), tags.join(";"))
}

pub async fn questions(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let (words, tags) = split_tags(&query.keyword);
  let sort = match query.sort.as_deref().unwrap_or("relevance") {
    "newest" => "creation",
    "active" => "activity",
    other => other,
  };
  let mut params: Params = vec![("sort", sort.into()), ("order", "desc".into())];
  if !words.is_empty() {
    params.push(("q", words));
  }
  if !tags.is_empty() {
    params.push(("tagged", tags));
  }
  match query.filter.as_deref() {
    Some("accepted") => params.push(("accepted", "True".into())),
    Some("unanswered") => params.push(("answers", "0".into())),
    Some("closed") => params.push(("closed", "True".into())),
    Some("open") => params.push(("closed", "False".into())),
    _ => {}
  }
  let site = api.site();
  api
    .list(
      "search/advanced",
      Some(&site),
      params,
      FILTER_LIST,
      page,
      parse::question,
    )
    .await
}

pub async fn users(api: &Api, query: &Query, page: &PageReq) -> Result<Page<User>> {
  let params: Params = vec![
    ("inname", query.keyword.clone()),
    ("sort", "reputation".into()),
    ("order", "desc".into()),
  ];
  let site = api.site();
  api
    .list("users", Some(&site), params, FILTER_LIST, page, parse::user)
    .await
}

/// Tags, most used first; with `words`, those whose names contain them.
pub async fn tags(api: &Api, words: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
  let mut params: Params = vec![("sort", "popular".into()), ("order", "desc".into())];
  if let Some(words) = words.filter(|w| !w.is_empty()) {
    params.push(("inname", words.to_owned()));
  }
  let site = api.site();
  api
    .list("tags", Some(&site), params, FILTER_LIST, page, parse::tag)
    .await
}
