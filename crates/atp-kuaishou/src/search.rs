//! Search for videos and users. The first page opens a search session;
//! later pages send its `searchSessionId` back, so the cursor carries both
//! (`<pcursor>:<session>`).

use atp_core::{Page, PageReq, Post, Query, Result, User, Value, ValueExt, json};

use crate::api::Api;
use crate::{parse, refs};

fn split(page: &PageReq) -> (String, String) {
  match page.cursor.as_deref().and_then(|c| c.split_once(':')) {
    Some((pcursor, session)) => (pcursor.to_owned(), session.to_owned()),
    None => (String::new(), String::new()),
  }
}

/// The next cursor: the answer's `pcursor` and the session it belongs to.
fn next(v: &Value, session: &str) -> Option<String> {
  let session = v
    .str("searchSessionId")
    .unwrap_or_else(|| session.to_owned());
  parse::more(v.str("pcursor")).map(|p| format!("{p}:{session}"))
}

pub async fn search(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let keyword = query.keyword.as_str();
  let (pcursor, session) = split(page);
  // The page's own first request; dropping fields here gets flagged.
  let body = if pcursor.is_empty() {
    json!({ "keyword": keyword, "page": "search", "webPageArea": "", "pcursor": "" })
  } else {
    json!({ "keyword": keyword, "pcursor": pcursor, "searchSessionId": session })
  };
  let v = api
    .www("/rest/v/search/feed")
    .json(body)
    .referer(&refs::search_url(keyword))
    .send()
    .await?;
  let posts: Vec<Post> = ["feeds", "list"]
    .iter()
    .flat_map(|k| v.list(k))
    .filter_map(parse::post)
    .collect();
  let next = next(&v, &session).filter(|_| !posts.is_empty());
  Ok(Page::new(posts, next))
}

pub async fn search_users(api: &Api, query: &Query, page: &PageReq) -> Result<Page<User>> {
  let keyword = query.keyword.as_str();
  let (pcursor, session) = split(page);
  let v = api
    .www("/rest/v/search/user")
    .json(json!({ "keyword": keyword, "pcursor": pcursor, "searchSessionId": session }))
    .referer(&refs::search_url(keyword))
    .send()
    .await?;
  let users: Vec<User> = ["users", "list"]
    .iter()
    .flat_map(|k| v.list(k))
    .filter_map(parse::listed_user)
    .collect();
  let next = next(&v, &session).filter(|_| !users.is_empty());
  Ok(Page::new(users, next))
}
