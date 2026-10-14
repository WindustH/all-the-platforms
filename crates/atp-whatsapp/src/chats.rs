//! Chats and their messages, from the local archive (WhatsApp keeps no
//! history on its servers); group names from the server.

use std::collections::BTreeMap;

use atp_core::{Action, Collection, Error, Page, PageReq, Post, Result};

use crate::api::Api;
use crate::archive::Record;
use crate::link::Link;
use crate::{parse, refs};

fn offset(req: &PageReq) -> usize {
  req.number_or(0) as usize
}

/// A page of an in-memory list, from offset `start`.
fn paged<T>(items: Vec<T>, start: usize, more: bool) -> Page<T> {
  let next = more.then(|| (start + items.len()).to_string());
  Page::new(items, next)
}

/// Group names come from the server; everything else from the archive.
pub async fn chats(link: &mut Link, page: &PageReq) -> Result<Page<Collection>> {
  if page.cursor.is_none() {
    match link.client.groups().get_participating().await {
      Ok(groups) => {
        for (jid, meta) in groups {
          let entry = link.archive.chat_mut(&jid.to_string());
          entry.kind = "group".into();
          if !meta.subject.is_empty() {
            entry.name = Some(meta.subject.clone());
          }
          if let Some(d) = &meta.description {
            entry.description = Some(d.clone());
          }
        }
      }
      Err(e) => tracing::warn!("listing groups failed: {e}"),
    }
  }
  let start = offset(page);
  let size = page.size_within(100);
  let all: Vec<Collection> = link
    .archive
    .chats()
    .into_iter()
    .map(|c| parse::collection(c, &link.archive))
    .collect();
  let more = all.len() > start + size;
  let items = all.into_iter().skip(start).take(size).collect();
  Ok(paged(items, start, more))
}

/// Newest first; past the archive's start, asks the phone for older ones.
pub async fn messages(link: &mut Link, chat: &str, page: &PageReq) -> Result<Page<Post>> {
  let me = link.me();
  let chat = refs::chat(chat, &link.archive, &me)?;
  let start = offset(page);
  let size = page.size_within(100);
  if start > 0 && start >= link.archive.messages(&chat).len() {
    let added = link
      .fetch_older(&chat, size.min(50) as i32)
      .await
      .unwrap_or(0);
    if added == 0 {
      return Ok(Page::new(Vec::new(), None));
    }
  }
  let records: Vec<Record> = link.archive.messages(&chat).iter().rev().cloned().collect();
  if records.is_empty() && start == 0 {
    return Err(
      Error::not_found("nothing of this chat is archived yet").with_hint(
        "WhatsApp keeps no history on its servers: the archive fills from the phone's history sync at login and from messages that arrive while ATP is connected (`listen`)",
      ),
    );
  }
  let page_items: Vec<Post> = records
    .iter()
    .skip(start)
    .take(size)
    .map(|r| parse::post(r, &link.archive))
    .collect();
  // Past the archive the next page asks the phone for older messages.
  Ok(paged(page_items, start, true))
}

pub fn read(link: &mut Link, post: &str) -> Result<Post> {
  let (chat, id) = refs::message(post)?;
  let record = link
    .archive
    .find_message(&chat, &id)
    .ok_or_else(|| Error::not_found("this message is not in the archive"))?;
  Ok(parse::post(&record, &link.archive))
}

pub fn search(link: &mut Link, words: &str, page: &PageReq) -> Result<Page<Post>> {
  let q = words.to_lowercase();
  let hits = link.archive.search(|r| {
    r.text
      .as_deref()
      .is_some_and(|t| t.to_lowercase().contains(&q))
  });
  let start = offset(page);
  let size = page.size_within(100);
  let more = hits.len() > start + size;
  let items = hits
    .iter()
    .skip(start)
    .take(size)
    .map(|r| parse::post(r, &link.archive))
    .collect();
  Ok(paged(items, start, more))
}

pub fn unread(link: &Link) -> BTreeMap<String, u64> {
  let mut out = BTreeMap::new();
  let mut total = 0;
  for c in link.archive.chats() {
    if c.unread > 0 {
      total += u64::from(c.unread);
      let name = link
        .archive
        .name(&c.jid)
        .map(str::to_owned)
        .unwrap_or_else(|| parse::phone(&c.jid));
      out.insert(name, u64::from(c.unread));
    }
  }
  out.insert("total".into(), total);
  out
}

/// Ask the phone for older messages of a chat (it must be online).
pub async fn older(api: &Api, chat: &str, count: i32) -> Result<Action> {
  let added = api
    .on_device("older", async |link| {
      let me = link.me();
      let jid = refs::chat(chat, &link.archive, &me)?;
      link.fetch_older(&jid, count.clamp(1, 500)).await
    })
    .await?;
  Ok(Action::done("older", chat).with_message(format!("{added} older messages archived")))
}
