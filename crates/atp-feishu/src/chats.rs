//! Chats the bot is in, their details and members.

use atp_core::{Collection, Error, Page, PageReq, Result, User, ValueExt};

use crate::api::Api;
use crate::{parse, refs};

pub async fn list(api: &Api, parent: Option<&str>, req: &PageReq) -> Result<Page<Collection>> {
  if let Some(p) = parent {
    return Err(
      Error::input(format!("Feishu chats have no channels inside them ({p})"))
        .with_hint("list a topic group's thread with `atp feishu messages omt_…`"),
    );
  }
  let mut query = vec![
    ("page_size", req.size_within(100).to_string()),
    ("sort_type", "ByActiveTimeDesc".to_owned()),
  ];
  if let Some(c) = &req.cursor {
    query.push(("page_token", c.clone()));
  }
  let v = api.get("/im/v1/chats", &query).await?;
  let items: Vec<Collection> = v.list("data.items").iter().map(parse::chat).collect();
  remember(api, &items);
  Ok(Page::new(items, next(&v)))
}

pub async fn search(api: &Api, keyword: &str, req: &PageReq) -> Result<Page<Collection>> {
  let mut query = vec![
    ("query", keyword.to_owned()),
    ("page_size", req.size_within(100).to_string()),
  ];
  if let Some(c) = &req.cursor {
    query.push(("page_token", c.clone()));
  }
  let v = api.get("/im/v1/chats/search", &query).await?;
  let items: Vec<Collection> = v.list("data.items").iter().map(parse::chat).collect();
  remember(api, &items);
  Ok(Page::new(items, next(&v)))
}

pub async fn info(api: &Api, chat: &str) -> Result<Collection> {
  let id = refs::chat(chat)?;
  let v = api.get(&format!("/im/v1/chats/{id}"), &[]).await?;
  let mut c = parse::chat(v.at("data"));
  c.id = id;
  remember(api, std::slice::from_ref(&c));
  Ok(c)
}

/// The chat's name, from this run's lists or one lookup (best effort).
pub async fn name(api: &Api, chat: &str) -> Option<String> {
  if let Some(n) = api.names.borrow().get(chat) {
    return Some(n.clone());
  }
  info(api, chat).await.ok().map(|c| c.name)
}

pub async fn members(api: &Api, chat: &str, req: &PageReq) -> Result<Page<User>> {
  let id = refs::chat(chat)?;
  let mut query = vec![
    ("member_id_type", "open_id".to_owned()),
    ("page_size", req.size_within(100).to_string()),
  ];
  if let Some(c) = &req.cursor {
    query.push(("page_token", c.clone()));
  }
  let v = api
    .get(&format!("/im/v1/chats/{id}/members"), &query)
    .await?;
  let users: Vec<User> = v.list("data.items").iter().map(parse::member).collect();
  {
    let mut names = api.names.borrow_mut();
    for u in &users {
      if !u.name.is_empty() {
        names.insert(u.id.clone(), u.name.clone());
      }
    }
  }
  Ok(Page::new(users, next(&v)))
}

fn remember(api: &Api, chats: &[Collection]) {
  let mut names = api.names.borrow_mut();
  for c in chats.iter().filter(|c| !c.name.is_empty()) {
    names.insert(c.id.clone(), c.name.clone());
  }
}

pub fn next(v: &atp_core::Value) -> Option<String> {
  (v.bool("data.has_more") == Some(true))
    .then(|| v.str("data.page_token"))
    .flatten()
}
