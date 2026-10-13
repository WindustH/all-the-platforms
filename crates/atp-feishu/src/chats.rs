//! Chats the bot is in, their details and members.

use atp_core::{Collection, Error, Page, PageReq, Result, User, ValueExt};

use crate::api::{Api, next_page, page_query};
use crate::{parse, refs};

pub async fn list(api: &Api, parent: Option<&str>, req: &PageReq) -> Result<Page<Collection>> {
  if let Some(p) = parent {
    return Err(
      Error::input(format!("Feishu chats have no channels inside them ({p})"))
        .with_hint("list a topic group's thread with `atp feishu messages omt_…`"),
    );
  }
  let query = page_query(vec![("sort_type", "ByActiveTimeDesc".to_owned())], req, 100);
  let v = api.get("/im/v1/chats", &query).await?;
  let items: Vec<Collection> = v.list("data.items").iter().map(parse::chat).collect();
  remember(api, &items);
  Ok(Page::new(items, next_page(&v)))
}

pub async fn search(api: &Api, keyword: &str, req: &PageReq) -> Result<Page<Collection>> {
  let query = page_query(vec![("query", keyword.to_owned())], req, 100);
  let v = api.get("/im/v1/chats/search", &query).await?;
  let items: Vec<Collection> = v.list("data.items").iter().map(parse::chat).collect();
  remember(api, &items);
  Ok(Page::new(items, next_page(&v)))
}

/// One chat's details.
pub async fn chat(api: &Api, arg: &str) -> Result<Collection> {
  let id = refs::chat(arg)?;
  let v = api.get(&format!("/im/v1/chats/{id}"), &[]).await?;
  let mut chat = parse::chat(v.at("data"));
  chat.id = id;
  remember(api, std::slice::from_ref(&chat));
  Ok(chat)
}

/// The chat's name, from this run's lists or one lookup (best effort).
pub async fn name(api: &Api, chat_id: &str) -> Option<String> {
  if let Some(n) = api.names.borrow().get(chat_id) {
    return Some(n.clone());
  }
  chat(api, chat_id).await.ok().map(|c| c.name)
}

pub async fn members(api: &Api, arg: &str, req: &PageReq) -> Result<Page<User>> {
  let id = refs::chat(arg)?;
  let query = page_query(vec![("member_id_type", "open_id".to_owned())], req, 100);
  let v = api
    .get(&format!("/im/v1/chats/{id}/members"), &query)
    .await?;
  let users: Vec<User> = v.list("data.items").iter().map(parse::member).collect();
  {
    let mut names = api.names.borrow_mut();
    for u in users.iter().filter(|u| !u.name.is_empty()) {
      names.insert(u.id.clone(), u.name.clone());
    }
  }
  Ok(Page::new(users, next_page(&v)))
}

fn remember(api: &Api, chats: &[Collection]) {
  let mut names = api.names.borrow_mut();
  for c in chats.iter().filter(|c| !c.name.is_empty()) {
    names.insert(c.id.clone(), c.name.clone());
  }
}
