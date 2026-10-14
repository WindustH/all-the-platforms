//! Links and ids.
//!
//! Items are numeric ids behind `item.taobao.com/item.htm?id=<id>`,
//! `detail.tmall.com/item.htm?id=<id>` (and their mobile forms), `m.tb.cn`
//! share links, or share text with such a link in it. Taobao has no public
//! user pages: sellers are found by their items. Chats are ids shaped
//! `<uid>.1-<uid>.1#11001` (as `chats` lists them); `send` also takes an
//! item (`item:<id>` or its link) to write to its seller.

use std::sync::LazyLock;

use atp_core::{Error, Result};
use regex::Regex;

use crate::api::Api;

static LINK: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"https?://[^\s"'<>，。「」【】]+"#).expect("valid"));
static ITEM_PARAM: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"[?&#](?:id|itemId|item_id)=(\d{6,})").expect("valid"));
static ITEM_PAGE: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(?:item\.taobao\.com|detail\.tmall\.com|detail\.m\.tmall\.com|item\.m\.taobao\.com)[^\s\x22']*?[?&]id=(\d{6,})")
    .expect("valid")
});
static CHAT: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^\d+\.\d+-\d+\.\d+#\d+$").expect("valid"));

/// Where `send` writes.
pub enum ChatRef {
  Chat(String),
  /// The chat with the seller of an item.
  Item(String),
}

fn digits(s: &str) -> Option<&str> {
  (s.len() >= 6 && s.bytes().all(|b| b.is_ascii_digit())).then_some(s)
}

/// An item id from an id, a link, a share link or share text.
pub async fn item(api: &Api, arg: &str) -> Result<String> {
  let arg = arg.trim();
  let arg = arg.strip_prefix("item:").unwrap_or(arg);
  if let Some(id) = digits(arg) {
    return Ok(id.to_owned());
  }
  let link = LINK.find(arg).map(|m| m.as_str());
  if let Some(m) = link.and_then(|l| ITEM_PARAM.captures(l)) {
    return Ok(m[1].to_owned());
  }
  if let Some(link) = link.filter(|l| l.contains("tb.cn")) {
    let resp = api.ctx.http.get(link).no_cookies().send().await?;
    let found = format!("{}\n{}", resp.url, resp.text().replace("&amp;", "&"));
    if let Some(m) = ITEM_PAGE.captures(&found) {
      return Ok(m[1].to_owned());
    }
  }
  Err(Error::input(format!(
    "not a Taobao item: `{arg}` (expected an item id, an item.taobao.com / detail.tmall.com link or a share link)"
  )))
}

/// A chat id, or an item to write to the seller of.
pub async fn chat(api: &Api, arg: &str) -> Result<ChatRef> {
  let arg = arg.trim();
  let cid = arg.strip_suffix("@cntaobao").unwrap_or(arg);
  if CHAT.is_match(cid) {
    return Ok(ChatRef::Chat(cid.to_owned()));
  }
  if arg.starts_with("item:") || digits(arg).is_some() || LINK.is_match(arg) {
    return Ok(ChatRef::Item(item(api, arg).await?));
  }
  Err(Error::input(format!(
    "not a Taobao chat: `{arg}` (expected a chat id from `chats`, or item:<id> / an item link to write to its seller)"
  )))
}
