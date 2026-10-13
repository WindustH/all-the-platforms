//! Global and in-chat message search, and finding users.

use grammers_client::tl;
use social_core::{Error, Page, PageReq, Post, Result, User};

use crate::mt::{self, Conn, error};
use crate::parse;
use crate::refs::Chat;

use super::*;

/// Global search across the account's chats, newest first.
pub async fn search(
  conn: &Conn,
  query: &str,
  filter: Option<&str>,
  page: &PageReq,
) -> Result<Page<Post>> {
  let (start, size) = (offset(page), page.size_within(100));
  let mut iter = conn.client.search_all_messages().query(query);
  if let Some(f) = filter {
    iter = iter.filter(messages_filter(f)?);
  }
  collect_skipping(&mut iter, start, size).await
}

/// Search inside one chat.
pub async fn search_chat(
  conn: &Conn,
  chat: &Chat,
  query: &str,
  filter: Option<&str>,
  page: &PageReq,
) -> Result<Page<Post>> {
  let peer = mt::peer(conn, chat).await?;
  let size = page.size_within(100);
  let mut iter = conn.client.search_messages(peer).query(query).limit(size);
  if let Some(f) = filter {
    iter = iter.filter(messages_filter(f)?);
  }
  if let Some(before) = page.cursor.as_deref().and_then(|c| c.parse().ok()) {
    iter = iter.offset_id(before);
  }
  let mut items = Vec::new();
  while let Some(m) = iter.next().await.map_err(error)? {
    items.push(parse::post(&m));
  }
  Ok(page_by_id(items, size))
}

pub(super) async fn collect_skipping(
  iter: &mut grammers_client::client::GlobalSearchIter,
  start: usize,
  size: usize,
) -> Result<Page<Post>> {
  let mut items = Vec::new();
  let mut seen = 0;
  let mut more = false;
  while let Some(m) = iter.next().await.map_err(error)? {
    seen += 1;
    if seen <= start {
      continue;
    }
    if items.len() == size {
      more = true;
      break;
    }
    items.push(parse::post(&m));
  }
  let next = next_offset(start, items.len(), size, more);
  Ok(Page::new(items, next))
}

pub const FILTERS: &[&str] = &[
  "photo", "video", "media", "document", "url", "voice", "music", "gif", "round", "mention",
];

pub(super) fn messages_filter(name: &str) -> Result<tl::enums::MessagesFilter> {
  use tl::enums::MessagesFilter as F;
  Ok(match name {
    "photo" => F::InputMessagesFilterPhotos,
    "video" => F::InputMessagesFilterVideo,
    "media" => F::InputMessagesFilterPhotoVideo,
    "document" => F::InputMessagesFilterDocument,
    "url" => F::InputMessagesFilterUrl,
    "voice" => F::InputMessagesFilterVoice,
    "music" => F::InputMessagesFilterMusic,
    "gif" => F::InputMessagesFilterGif,
    "round" => F::InputMessagesFilterRoundVideo,
    "mention" => F::InputMessagesFilterMyMentions,
    other => {
      return Err(Error::input(format!(
        "unknown filter `{other}` (one of {})",
        FILTERS.join(", ")
      )));
    }
  })
}

/// Public users, groups and channels by name, and matching contacts.
pub async fn search_users(conn: &Conn, query: &str, page: &PageReq) -> Result<Page<User>> {
  let request = tl::functions::contacts::Search {
    broadcasts: false,
    bots: false,
    q: query.to_owned(),
    limit: page.size_within(100) as i32,
  };
  let tl::enums::contacts::Found::Found(found) =
    conn.client.invoke(&request).await.map_err(error)?;
  let peers = parse::peers(&conn.client, found.users, found.chats);
  let items = found
    .my_results
    .iter()
    .chain(&found.results)
    .filter_map(|p| peers.get(&grammers_client::session::types::PeerId::from(p.clone())))
    .map(parse::peer_user)
    .collect();
  Ok(Page::last(items))
}
