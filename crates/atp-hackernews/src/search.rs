//! Search through Algolia: stories and comments by words, and a user's
//! stories or comments (`author_<name>` tags).

use atp_core::{Page, PageReq, Post, Query, Result, ValueExt};

use crate::api::{self, Api};
use crate::{parse, refs};

/// `search --sort`: relevance (`search`) or newest first (`search_by_date`).
pub const SORTS: &[&str] = &["relevance", "date"];
/// `search --filter`: Algolia tags; `all` searches stories and comments.
pub const FILTERS: &[&str] = &[
  "story",
  "comment",
  "ask_hn",
  "show_hn",
  "job",
  "poll",
  "front_page",
  "all",
];

/// One page of hits for `query` with `tags` (`story,author_pg`: both).
async fn hits(
  api: &Api,
  query: &str,
  tags: &str,
  by_date: bool,
  page: &PageReq,
) -> Result<Page<Post>> {
  let (no, size) = api::page_of(page);
  let mut params = vec![
    ("query", query.to_owned()),
    ("page", no.to_string()),
    ("hitsPerPage", size.to_string()),
  ];
  if !tags.is_empty() {
    params.push(("tags", tags.to_owned()));
  }
  let path = if by_date { "search_by_date" } else { "search" };
  let v = api.algolia(path, params).await?;
  let posts = v.list("hits").iter().map(parse::hit).collect();
  let pages = v.u64("nbPages").unwrap_or_default();
  let next = (no + 1 < pages).then(|| format!("{}:{size}", no + 1));
  Ok(Page::new(posts, next))
}

pub async fn posts(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let tags = match query.filter.as_deref().unwrap_or("story") {
    "all" => "",
    other => other,
  };
  let by_date = query.sort.as_deref() == Some("date");
  hits(api, &query.keyword, tags, by_date, page).await
}

/// A user's stories (polls and jobs included), newest first.
pub async fn stories(api: &Api, user: &str, page: &PageReq) -> Result<Page<Post>> {
  let name = refs::user(user)?;
  let tags = format!("(story,poll,job),author_{name}");
  hits(api, "", &tags, true, page).await
}

/// A user's comments, newest first.
pub async fn comments(api: &Api, user: &str, page: &PageReq) -> Result<Page<Post>> {
  let name = refs::user(user)?;
  hits(api, "", &format!("comment,author_{name}"), true, page).await
}
