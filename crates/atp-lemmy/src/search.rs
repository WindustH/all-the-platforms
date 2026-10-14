//! Search across posts, users and communities of everything the instance
//! knows (`--filter local`: its own only).

use atp_core::{Collection, Page, PageReq, Post, Query, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::parse;
use crate::posts;

pub const FILTERS: &[&str] = &["all", "local"];
/// `search --sort`; the first is the default.
pub const SORTS: &[&str] = &[
  "top-all",
  "new",
  "old",
  "hot",
  "active",
  "top-day",
  "top-week",
  "top-month",
  "top-year",
];
const PAGE_MAX: usize = 50;

/// One page of `search` for `kind` (`Posts`, `Users`, `Communities`); the
/// rows of `list` and the next page number.
async fn search(
  api: &Api,
  query: &Query,
  kind: &str,
  list: &str,
  page: &PageReq,
) -> Result<(Vec<Value>, Option<String>)> {
  let number = page.number_or(1);
  let limit = page.size_within(PAGE_MAX);
  let listing = match query.filter.as_deref() {
    Some("local") => "Local",
    _ => "All",
  };
  let params = vec![
    ("q", query.keyword.clone()),
    ("type_", kind.to_owned()),
    ("listing_type", listing.to_owned()),
    ("sort", posts::sort(query.sort.as_deref(), "top-all")?),
    ("page", number.to_string()),
    ("limit", limit.to_string()),
  ];
  let v = api.get("search", params).await?;
  let rows = v.list(list).to_vec();
  let next = (rows.len() >= limit).then(|| (number + 1).to_string());
  Ok((rows, next))
}

pub async fn posts(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let (rows, next) = search(api, query, "Posts", "posts", page).await?;
  let instance = api.instance();
  Ok(Page::new(
    rows.iter().map(|row| parse::post(row, &instance)).collect(),
    next,
  ))
}

pub async fn users(api: &Api, query: &Query, page: &PageReq) -> Result<Page<User>> {
  let (rows, next) = search(api, query, "Users", "users", page).await?;
  Ok(Page::new(
    rows.iter().map(parse::person_view).collect(),
    next,
  ))
}

pub async fn communities(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Collection>> {
  let (rows, next) = search(api, query, "Communities", "communities", page).await?;
  Ok(Page::new(rows.iter().map(parse::community).collect(), next))
}
