//! Members, the token's own account and its notifications, and nodes.

use atp_core::paging::slice;
use atp_core::{
  Action, Collection, Error, Notification, Page, PageReq, Result, User, ValueExt, json,
};

use crate::api::{Api, TOKEN};
use crate::{parse, refs};

/// Rows per page of notifications, when the answer does not say.
const NOTIFICATIONS_PAGE: usize = 10;

pub async fn whoami(api: &Api) -> Result<User> {
  let v = api.v2("member", Vec::new()).await?;
  Ok(parse::member(v.at("result")))
}

/// Keep the token; `whoami` checks it right after.
pub fn login(api: &Api, token: &str) -> Result<()> {
  let token = token.trim();
  if token.is_empty() || token.contains(char::is_whitespace) {
    return Err(Error::input(
      "expected `login --token TOKEN` (a V2EX personal access token)",
    ));
  }
  atp_core::credentials::save(&api.ctx, &[(TOKEN, token)]);
  Ok(())
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let name = refs::member(arg)?;
  let v = api
    .v1("members/show.json", vec![("username", name.clone())])
    .await?;
  if v.str("username").is_none() {
    return Err(Error::not_found(format!("no V2EX member {name}")));
  }
  Ok(parse::member(&v))
}

pub async fn notifications(api: &Api, page: &PageReq) -> Result<Page<Notification>> {
  let no = page.number_or(1);
  let v = api.v2("notifications", vec![("p", no.to_string())]).await?;
  let rows = v.list("result");
  let per_page = v
    .u64("pagination.per_page")
    .map_or(NOTIFICATIONS_PAGE, |n| n as usize);
  let more = match v.u64("pagination.pages") {
    Some(pages) => no < pages,
    None => rows.len() >= per_page,
  };
  let items = rows.iter().map(parse::notification).collect();
  Ok(Page::new(items, more.then(|| (no + 1).to_string())))
}

pub async fn delete_notification(api: &Api, arg: &str) -> Result<Action> {
  let id = refs::id(arg, "notification")?;
  api.v2_delete(&format!("notifications/{id}")).await?;
  Ok(Action::done("delete", arg))
}

/// The token in use: its scope, lifetime and use.
pub async fn token(api: &Api) -> Result<atp_core::Value> {
  let v = api.v2("token", Vec::new()).await?;
  let mut result = v.at("result").clone();
  // Never print the token itself.
  if let Some(map) = result.as_object_mut()
    && let Some(token) = map.get("token").and_then(|t| t.as_str()).map(str::to_owned)
  {
    map.insert("token".into(), json!(atp_core::credentials::mask(&token)));
  }
  Ok(result)
}

pub async fn node(api: &Api, arg: &str) -> Result<Collection> {
  let name = refs::node(arg)?;
  if api.token().is_some() {
    let v = api.v2(&format!("nodes/{name}"), Vec::new()).await?;
    return Ok(parse::node(v.at("result")));
  }
  let v = api
    .v1("nodes/show.json", vec![("name", name.clone())])
    .await?;
  if v.str("name").is_none() {
    return Err(Error::not_found(format!("no V2EX node {name}")));
  }
  Ok(parse::node(&v))
}

/// Every node, the most topics first.
pub async fn nodes(api: &Api, page: &PageReq) -> Result<Page<Collection>> {
  let v = api.v1("nodes/all.json", Vec::new()).await?;
  let mut nodes: Vec<Collection> = v.list("").iter().map(parse::node).collect();
  nodes.sort_by_key(|node| std::cmp::Reverse(node.items));
  Ok(slice(nodes, page))
}
