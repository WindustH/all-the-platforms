//! Comment threads. The whole tree under an item comes from Algolia in one
//! request; the top level is put in Hacker News' own order with the item's
//! `kids` from Firebase, and comments Algolia has not indexed yet are
//! fetched from Firebase (their replies then load through [`replies`]).

use std::cmp::Reverse;
use std::collections::HashMap;

use atp_core::paging::slice;
use atp_core::{Comment, ErrorCode, Page, PageReq, Result, Value, ValueExt};

use crate::api::{self, Api};
use crate::{parse, refs};

/// `comments --sort`: Hacker News' ranking, newest first, oldest first.
pub const SORTS: &[&str] = &["top", "new", "old"];

pub async fn comments(
  api: &Api,
  arg: &str,
  sort: Option<&str>,
  page: &PageReq,
) -> Result<Page<Comment>> {
  let mut all = thread(api, refs::item(arg)?).await?;
  match sort {
    Some("new") => order(&mut all, true),
    Some("old") => order(&mut all, false),
    _ => {}
  }
  Ok(slice(all, page))
}

/// Replies under one comment: from the thread when it holds them, else
/// from Firebase.
pub async fn replies(api: &Api, arg: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
  let id = refs::item(comment)?;
  let all = thread(api, refs::item(arg)?).await?;
  if let Some(found) = find(&all, &id.to_string())
    && found.replies.len() as u64 >= found.reply_count.unwrap_or_default()
  {
    return Ok(slice(found.replies.clone(), page));
  }
  let item = api.item(id).await?;
  let kids = api::ids(&item, "kids");
  let start = page.number_or(0) as usize;
  let size = page.size.max(1);
  let ids: Vec<u64> = kids.iter().skip(start).take(size).copied().collect();
  let items = api.items(&ids).await?;
  let replies = items
    .iter()
    .filter(|item| !parse::hidden(item))
    .map(parse::item_comment)
    .collect();
  let next = (start + size < kids.len()).then(|| (start + size).to_string());
  Ok(Page::new(replies, next))
}

/// The top-level comments of an item in Hacker News' order, each with its
/// replies; built once per run.
async fn thread(api: &Api, id: u64) -> Result<Vec<Comment>> {
  if let Some(thread) = api.threads.borrow().get(&id) {
    return Ok(thread.clone());
  }
  let (tree, item) = futures_util::join!(api.tree(id), api.item(id));
  let item = item?;
  // A story too new for Algolia: everything comes from Firebase.
  let tree = match tree {
    Err(e) if e.code == ErrorCode::NotFound => Value::Null,
    tree => tree?,
  };
  let mut by_id: HashMap<String, Comment> = tree
    .list("children")
    .iter()
    .filter_map(parse::tree_comment)
    .map(|comment| (comment.id.clone(), comment))
    .collect();
  let kids = api::ids(&item, "kids");
  let missing: Vec<u64> = kids
    .iter()
    .filter(|kid| !by_id.contains_key(&kid.to_string()))
    .copied()
    .collect();
  for item in api.items(&missing).await? {
    if !parse::hidden(&item) {
      let comment = parse::item_comment(&item);
      by_id.insert(comment.id.clone(), comment);
    }
  }
  let thread: Vec<Comment> = kids
    .iter()
    .filter_map(|kid| by_id.remove(&kid.to_string()))
    .collect();
  api.threads.borrow_mut().insert(id, thread.clone());
  Ok(thread)
}

/// Sort by time at every level.
fn order(comments: &mut [Comment], newest_first: bool) {
  if newest_first {
    comments.sort_by_key(|c| Reverse(c.created_at));
  } else {
    comments.sort_by_key(|c| c.created_at);
  }
  for comment in comments {
    order(&mut comment.replies, newest_first);
  }
}

fn find<'a>(comments: &'a [Comment], id: &str) -> Option<&'a Comment> {
  comments.iter().find_map(|comment| {
    if comment.id == id {
      Some(comment)
    } else {
      find(&comment.replies, id)
    }
  })
}
