//! Searches (`/api/v2/search`): statuses, accounts and hashtags; statuses
//! with a hashtag; trending hashtags.

use atp_core::{Collection, Error, Page, PageReq, Post, Query, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::parse;
use crate::refs;

/// The most results one search answers.
const SEARCH_MAX: usize = 40;

/// Statuses with the words; `#tag` alone lists the hashtag's timeline.
/// Mastodon searches statuses for accounts only, and among the statuses
/// their authors opted in (or that you wrote, liked, boosted or saw
/// mentioning you).
pub async fn posts(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let keyword = query.keyword.trim();
  if keyword.starts_with('#') && !keyword.contains(' ') {
    return tagged(api, keyword, page).await;
  }
  if !api.logged_in() {
    return Err(
      Error::auth("Mastodon searches statuses for logged-in accounts only")
        .with_hint("`search '#tag'` lists a hashtag without an account; log in for the rest"),
    );
  }
  let rows = search(api, keyword, "statuses", page).await?;
  Ok(rows.map(parse::post))
}

pub async fn users(api: &Api, query: &Query, page: &PageReq) -> Result<Page<User>> {
  let rows = search(api, query.keyword.trim(), "accounts", page).await?;
  Ok(rows.map(parse::user))
}

pub async fn topics(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Collection>> {
  let keyword = query.keyword.trim().trim_start_matches('#');
  let rows = search(api, keyword, "hashtags", page).await?;
  Ok(rows.map(parse::tag))
}

/// One page of one kind of result, offset-paged.
struct Rows {
  values: Vec<Value>,
  next: Option<String>,
  instance: String,
}

impl Rows {
  fn map<T>(self, item: impl Fn(&Value, &str) -> T) -> Page<T> {
    let items = self
      .values
      .iter()
      .map(|v| item(v, &self.instance))
      .collect();
    Page::new(items, self.next)
  }
}

async fn search(api: &Api, keyword: &str, kind: &str, page: &PageReq) -> Result<Rows> {
  let size = page.size_within(SEARCH_MAX);
  let offset = page.number_or(0);
  let mut query = vec![
    ("q", keyword.to_owned()),
    ("type", kind.to_owned()),
    ("limit", size.to_string()),
  ];
  if offset > 0 {
    query.push(("offset", offset.to_string()));
  }
  if api.logged_in() {
    query.push(("resolve", "true".to_owned()));
  }
  let v = api.get("v2/search", query).await?;
  let values = v.list(kind).to_vec();
  // Paging past the first page needs an account.
  let more = values.len() >= size && api.logged_in();
  Ok(Rows {
    next: more.then(|| (offset + values.len() as u64).to_string()),
    values,
    instance: api.instance(),
  })
}

/// A hashtag's public timeline.
pub async fn tagged(api: &Api, tag: &str, page: &PageReq) -> Result<Page<Post>> {
  let tag = refs::tag(tag)?;
  let path = format!(
    "v1/timelines/tag/{}",
    atp_core::text::encode_uri_component(&tag)
  );
  api.list(&path, Vec::new(), page, parse::post).await
}

/// Hashtags in use now (`trends/tags`).
pub async fn trends(api: &Api, page: &PageReq) -> Result<Page<Collection>> {
  api
    .list("v1/trends/tags", Vec::new(), page, parse::tag)
    .await
}
