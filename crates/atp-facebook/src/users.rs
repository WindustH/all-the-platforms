//! Profiles and Pages on facebook.com: who they are, their timelines,
//! friends, people search, and who is logged in.

use atp_core::{Error, ErrorCode, Page, PageReq, Post, Result, User, Value, ValueExt};
use url::form_urlencoded::byte_serialize;

use crate::api::Api;
use crate::parse;
use crate::refs::{self, HOME};
use crate::relay::find_all;
use crate::web::{self, advance, stop};

const PROFILE_OPS: [&str; 2] = [
  "ProfileCometHeaderQuery",
  "ProfilePlusCometLoggedOutRootQuery",
];

/// The web session's account, else the Graph token's.
pub async fn whoami(api: &Api) -> Result<User> {
  if api.has_session() {
    return user(api, &api.my_id()).await;
  }
  if api.token().is_some() {
    return atp_meta::graph::me(&api.graph()?, "id,name,link").await;
  }
  Err(Error::auth("not logged in").with_hint(api.ctx.login_hint()))
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let page = api.page(&refs::profile_url(arg)).await?;
  page.check(&format!("profile {arg}"), &PROFILE_OPS)?;
  let u = parse::profile(&page.docs, &page.head);
  if u.id.is_empty() && u.name.is_empty() {
    return Err(Error::not_found(format!("no profile or Page `{arg}`")));
  }
  Ok(u)
}

/// A profile's timeline. Logged out Facebook shows its newest post and
/// refuses the cursor.
pub async fn user_posts(api: &Api, arg: &str, req: &PageReq) -> Result<Page<Post>> {
  let page = api.page(&refs::profile_url(arg)).await?;
  page.check(&format!("profile {arg}"), &PROFILE_OPS)?;
  let Some(cursor) = &req.cursor else {
    let (posts, next) = page
      .docs
      .values()
      .map(parse::timeline)
      .find(|(posts, _)| !posts.is_empty())
      .unwrap_or_default();
    return Ok(Page::new(posts, next));
  };
  let operation = page
    .preload("ProfileCometTimelineFeedQuery")
    .or_else(|| page.paged(&["Timeline"]))
    .filter(|p| p.has_cursor());
  let Some(operation) = operation else {
    return Ok(stop(api, "the timeline"));
  };
  let data = match advance(&api.ctx, &page, operation, cursor, req.size_within(10)).await {
    Err(e) if e.code == ErrorCode::NotAuthenticated => return Ok(stop(api, "the timeline")),
    other => other?,
  };
  let (posts, next) = parse::timeline(&data);
  Ok(Page::new(posts, next))
}

/// People or Pages found by name (logged in).
pub async fn search(api: &Api, query: &str) -> Result<Page<User>> {
  api.require_session("search")?;
  let query: String = byte_serialize(query.as_bytes()).collect();
  let page = api
    .page(&format!("{HOME}/search/people/?q={query}"))
    .await?;
  page.check("search", &[])?;
  Ok(Page::last(people(&page)))
}

/// Friends of a profile (logged in; as far as Facebook shows them).
pub async fn friends(api: &Api, arg: Option<&str>) -> Result<Page<User>> {
  api.require_session("friend lists")?;
  let me = api.my_id();
  let who = arg.unwrap_or(&me);
  let page = api.page(&refs::tab_url(who, "friends")).await?;
  page.check("friends", &[])?;
  let owner = parse::profile(&page.docs, &page.head).id;
  Ok(Page::last(
    people(&page)
      .into_iter()
      .filter(|u| u.id != owner && u.id != me)
      .collect(),
  ))
}

/// Distinct user-like nodes (an id, a name and a profile link) of a page.
fn people(page: &web::Page) -> Vec<User> {
  let mut out: Vec<User> = Vec::new();
  for doc in page.docs.values() {
    let mut found = Vec::new();
    find_all(doc, &is_person, &mut found);
    for node in found {
      let u = parse::user(node);
      if !u.id.is_empty() && !out.iter().any(|o| o.id == u.id) {
        out.push(u);
      }
    }
  }
  out
}

fn is_person(v: &Value) -> bool {
  matches!(v.str("__typename").as_deref(), Some("User" | "Page"))
    && v.str("name").is_some()
    && v.first_str(&["url", "profile_url"]).is_some()
}
