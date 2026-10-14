//! Shows, episodes and users as arguments: their page links
//! (`https://www.xiaoyuzhoufm.com/podcast/<id>`, `/episode/<id>`,
//! `/user/<id>`, with or without query parameters),
//! `podcast:<id>`, `episode:<id>`, `user:<id>`, or a bare id. Ids are 24
//! hex digits. Links on other hosts (link shorteners) are followed to
//! where they lead.

use atp_core::{Error, Result};
use url::Url;

use crate::api::Api;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
  Show(String),
  Episode(String),
  User(String),
  /// An id without its kind.
  Bare(String),
}

fn is_id(text: &str) -> bool {
  text.len() == 24 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

fn invalid(arg: &str) -> Error {
  Error::input(format!(
    "not a Xiaoyuzhou show, episode or user: {arg} (give a xiaoyuzhoufm.com link or a 24-digit id)"
  ))
}

/// `kind` and `id` as an item.
fn typed(kind: &str, id: &str) -> Option<Item> {
  if !is_id(id) {
    return None;
  }
  let id = id.to_ascii_lowercase();
  match kind {
    "podcast" | "show" => Some(Item::Show(id)),
    "episode" => Some(Item::Episode(id)),
    "user" => Some(Item::User(id)),
    _ => None,
  }
}

/// An item from a xiaoyuzhoufm.com link.
fn from_url(url: &Url) -> Option<Item> {
  let mut parts = url.path_segments()?.filter(|s| !s.is_empty());
  let kind = parts.next()?;
  typed(kind, parts.next()?)
}

fn is_site(url: &Url) -> bool {
  url
    .host_str()
    .is_some_and(|host| host == "xiaoyuzhoufm.com" || host.ends_with(".xiaoyuzhoufm.com"))
}

/// The item an argument names, without network.
pub fn item(arg: &str) -> Result<Item> {
  let arg = arg.trim();
  if is_id(arg) {
    return Ok(Item::Bare(arg.to_ascii_lowercase()));
  }
  if let Some((kind, id)) = arg.split_once(':')
    && !kind.starts_with("http")
  {
    return typed(kind, id.trim()).ok_or_else(|| invalid(arg));
  }
  let url = Url::parse(arg).map_err(|_| invalid(arg))?;
  if !is_site(&url) {
    return Err(invalid(arg));
  }
  from_url(&url).ok_or_else(|| invalid(arg))
}

/// The item an argument names; links on other hosts (link shorteners)
/// are followed first.
pub async fn resolve(api: &Api, arg: &str) -> Result<Item> {
  let parsed = Url::parse(arg.trim()).ok();
  let Some(url) = parsed.filter(|u| matches!(u.scheme(), "http" | "https") && !is_site(u)) else {
    return item(arg);
  };
  let target = api.ctx.http.final_url(url.as_str()).await?;
  item(&target).map_err(|_| invalid(arg))
}

pub fn show_url(id: &str) -> String {
  format!("{}/podcast/{id}", crate::api::BASE)
}

pub fn episode_url(id: &str) -> String {
  format!("{}/episode/{id}", crate::api::BASE)
}

pub fn user_url(id: &str) -> String {
  format!("{}/user/{id}", crate::api::BASE)
}
