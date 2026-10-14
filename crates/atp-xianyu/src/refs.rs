//! Links and ids.
//!
//! Items are numeric ids behind `goofish.com/item?id=<id>` (also the old
//! `2.taobao.com/item.htm?id=`), `m.tb.cn` share links, or share text with
//! such a link in it. Users are numeric ids behind
//! `goofish.com/personal?userId=<id>`, or `me`. Chats are the numeric ids
//! `chats` lists (`<id>@goofish` too); `send` also takes an item
//! (`item:<id>` or its link) to write to its seller.

use std::sync::LazyLock;

use atp_core::{Error, Result};
use regex::Regex;

use crate::api::Api;

static LINK: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"https?://[^\s"'<>，。「」【】]+"#).expect("valid"));
static ITEM_PARAM: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"[?&#](?:id|itemId|item_id)=(\d+)").expect("valid"));
static ITEM_FIELD: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"["']itemId["']\s*[:=]\s*["']?(\d+)"#).expect("valid"));
static USER_PARAM: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"[?&#](?:userId|userid|user_id)=(\d+)").expect("valid"));

/// Where `send` writes.
pub enum ChatRef {
  /// A chat that exists, by id.
  Chat(String),
  /// A new (or the existing) chat with the seller of an item.
  Item(String),
}

fn digits(s: &str) -> Option<&str> {
  let id = s.strip_suffix("@goofish").unwrap_or(s);
  (!id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())).then_some(id)
}

fn captured(re: &Regex, text: &str) -> Option<String> {
  re.captures(text).map(|m| m[1].to_owned())
}

/// An item id from an id, a link, a share link or share text.
pub async fn item(api: &Api, arg: &str) -> Result<String> {
  let arg = arg.trim();
  let arg = arg.strip_prefix("item:").unwrap_or(arg);
  if let Some(id) = digits(arg) {
    return Ok(id.to_owned());
  }
  let link = LINK.find(arg).map(|m| m.as_str());
  if let Some(id) = link.and_then(|l| captured(&ITEM_PARAM, l)) {
    return Ok(id);
  }
  if let Some(link) = link {
    let (url, page) = expand(api, link).await?;
    if let Some(id) = captured(&ITEM_PARAM, &url)
      .or_else(|| captured(&ITEM_PARAM, &page))
      .or_else(|| captured(&ITEM_FIELD, &page))
    {
      return Ok(id);
    }
  }
  Err(Error::input(format!(
    "not a Xianyu item: `{arg}` (expected an item id, https://www.goofish.com/item?id=<id> or a share link)"
  )))
}

/// A user id from an id, a profile link or a share link; `me` is the
/// logged-in account.
pub async fn user(api: &Api, arg: &str) -> Result<String> {
  let arg = arg.trim();
  if arg == "me" {
    return api.me();
  }
  if let Some(id) = digits(arg) {
    return Ok(id.to_owned());
  }
  let link = LINK.find(arg).map(|m| m.as_str());
  if let Some(id) = link.and_then(|l| captured(&USER_PARAM, l)) {
    return Ok(id);
  }
  if let Some(link) = link {
    let (url, page) = expand(api, link).await?;
    if let Some(id) = captured(&USER_PARAM, &url).or_else(|| captured(&USER_PARAM, &page)) {
      return Ok(id);
    }
  }
  Err(Error::input(format!(
    "not a Xianyu user: `{arg}` (expected a user id, `me` or https://www.goofish.com/personal?userId=<id>)"
  )))
}

/// A chat id, or an item to write to the seller of.
pub async fn chat(api: &Api, arg: &str) -> Result<ChatRef> {
  let arg = arg.trim();
  if let Some(id) = digits(arg) {
    return Ok(ChatRef::Chat(id.to_owned()));
  }
  if arg.starts_with("item:") || LINK.is_match(arg) {
    return Ok(ChatRef::Item(item(api, arg).await?));
  }
  Err(Error::input(format!(
    "not a Xianyu chat: `{arg}` (expected a chat id from `chats`, or item:<id> / an item link to write to its seller)"
  )))
}

/// Where a share link leads, and the page there (share pages redirect by
/// script, so the item id may only be in the page).
async fn expand(api: &Api, link: &str) -> Result<(String, String)> {
  let resp = api.ctx.http.get(link).no_cookies().send().await?;
  Ok((resp.url.clone(), resp.text().replace("&amp;", "&")))
}
