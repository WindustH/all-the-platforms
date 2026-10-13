//! Search over threads, forums and users.

use atp_core::{Collection, Page, PageReq, Post, Query, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::{parse, refs};

pub const SEARCH_SORTS: &[&str] = &["relevant", "new", "old"];

/// `threads`: only thread starts; `posts`: replies too.
pub const SEARCH_FILTERS: &[&str] = &["threads", "posts"];

/// Thread search, over all forums or inside one.
pub async fn search(
  api: &Api,
  q: &Query,
  forum: Option<&str>,
  req: &PageReq,
) -> Result<Page<Post>> {
  let pn = req.number_or(1);
  let st = match q.sort.as_deref() {
    Some("new") => "1",
    Some("old") => "0",
    _ => "2",
  };
  let tt = if q.filter.as_deref() == Some("posts") {
    "2"
  } else {
    "1"
  };
  let mut query = vec![
    ("word", q.keyword.clone()),
    ("pn", pn.to_string()),
    ("st", st.into()),
    ("tt", tt.into()),
    ("ct", "1".into()),
    ("rn", req.size_within(30).to_string()),
    ("is_use_zonghe", "1".into()),
    ("cv", "99.9.101".into()),
  ];
  if let Some(f) = forum {
    query.push(("fname", refs::forum(f)?));
  }
  let v = api.web("/mo/q/search/thread", &query).await?;
  let none = parse::Users::new();
  let items: Vec<Post> = v
    .list("data.post_list")
    .iter()
    .map(|t| parse::thread(t, &none))
    .collect();
  let more = v.str("data.has_more").as_deref() == Some("1") && !items.is_empty();
  Ok(Page::new(items, more.then(|| (pn + 1).to_string())))
}

/// Exact match first, then the fuzzy ones.
fn matches(v: &Value) -> Vec<&Value> {
  let exact = v.at("data.exactMatch");
  std::iter::once(exact)
    .filter(|e| e.is_object())
    .chain(v.list("data.fuzzyMatch"))
    .collect()
}

pub async fn search_forums(api: &Api, query: &Query) -> Result<Page<Collection>> {
  let v = api
    .web("/mo/q/search/forum", &[("word", query.keyword.clone())])
    .await?;
  Ok(Page::last(
    matches(&v).into_iter().filter_map(parse::forum).collect(),
  ))
}

pub async fn search_users(api: &Api, query: &Query) -> Result<Page<User>> {
  let v = api
    .web("/mo/q/search/user", &[("word", query.keyword.clone())])
    .await?;
  Ok(Page::last(
    matches(&v).into_iter().filter_map(parse::user).collect(),
  ))
}
