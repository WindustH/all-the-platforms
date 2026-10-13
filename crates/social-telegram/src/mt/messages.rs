//! History, single messages, the comments of channel posts, media files.

use grammers_client::message::Message;
use grammers_client::session::types::PeerRef;
use grammers_client::tl;
use social_core::{Comment, Error, Page, PageReq, Post, Result};

use crate::mt::{self, Conn, error};
use crate::parse;
use crate::refs::{self, Chat};

/// History, newest first; the cursor is the oldest message id seen.
pub async fn messages(conn: &Conn, chat: &str, page: &PageReq) -> Result<Page<Post>> {
  let (chat, topic) = topic_ref(chat)?;
  let peer = mt::peer(conn, &chat).await?;
  let size = page.size_within(100);
  if let Some(topic) = topic {
    return replies(conn, peer, topic, page).await;
  }
  let mut iter = conn.client.iter_messages(peer).limit(size);
  if let Some(before) = page.cursor.as_deref().and_then(|c| c.parse().ok()) {
    iter = iter.offset_id(before);
  }
  let mut items = Vec::new();
  while let Some(m) = iter.next().await.map_err(error)? {
    items.push(parse::post(&m));
  }
  Ok(page_by_id(items, size))
}

/// `CHAT` or `CHAT/TOPIC` (a forum topic from `chats CHAT`).
pub(super) fn topic_ref(arg: &str) -> Result<(Chat, Option<i32>)> {
  if let Ok(m) = refs::message(arg)
    && !arg.contains("t.me")
  {
    return Ok((m.chat, Some(m.id)));
  }
  Ok((refs::chat(arg)?, None))
}

pub(super) fn page_by_id(items: Vec<Post>, size: usize) -> Page<Post> {
  let oldest = items
    .last()
    .and_then(|p| p.extra.get("message_id"))
    .and_then(|v| v.as_i64());
  let next = oldest
    .filter(|id| items.len() >= size && *id > 1)
    .map(|id| id.to_string());
  Page::new(items, next)
}

pub async fn read(conn: &Conn, arg: &str) -> Result<Post> {
  let m = refs::message(arg)?;
  let peer = mt::peer(conn, &m.chat).await?;
  if let Some(comment) = m.comment {
    // A comment lives in the channel's discussion group.
    let (group, _) = discussion(conn, peer, m.id).await?;
    return one(conn, group, comment).await.map(|msg| parse::post(&msg));
  }
  one(conn, peer, m.id).await.map(|msg| parse::post(&msg))
}

pub async fn one(conn: &Conn, peer: PeerRef, id: i32) -> Result<Message> {
  conn
    .client
    .get_messages_by_id(peer, &[id])
    .await
    .map_err(error)?
    .into_iter()
    .flatten()
    .next()
    .ok_or_else(|| Error::not_found(format!("no message {id} in this chat")))
}

/// The discussion group of a channel post and the post's id in it.
pub(super) async fn discussion(conn: &Conn, peer: PeerRef, id: i32) -> Result<(PeerRef, i32)> {
  let request = tl::functions::messages::GetDiscussionMessage {
    peer: peer.into(),
    msg_id: id,
  };
  let tl::enums::messages::DiscussionMessage::Message(d) =
    conn.client.invoke(&request).await.map_err(error)?;
  let first = d
    .messages
    .into_iter()
    .find_map(|m| match m {
      tl::enums::Message::Message(m) => Some(m),
      _ => None,
    })
    .ok_or_else(|| Error::not_found("this post has no comments"))?;
  let chat = d
    .chats
    .into_iter()
    .find(|c| {
      let id = match c {
        tl::enums::Chat::Channel(c) => c.id,
        tl::enums::Chat::ChannelForbidden(c) => c.id,
        _ => 0,
      };
      grammers_client::session::types::PeerId::from(first.peer_id.clone())
        .bare_id()
        .is_some_and(|b| b == id)
    })
    .ok_or_else(|| Error::not_found("the discussion group is not accessible"))?;
  Ok((PeerRef::from(chat), first.id))
}

/// Comments under a channel post (replies in its discussion group).
pub async fn comments(conn: &Conn, arg: &str, page: &PageReq) -> Result<Page<Comment>> {
  let m = refs::message(arg)?;
  let peer = mt::peer(conn, &m.chat).await?;
  let posts = replies(conn, peer, m.id, page).await?;
  Ok(Page::new(
    posts.items.into_iter().map(parse::comment).collect(),
    posts.next_cursor,
  ))
}

/// `messages.getReplies`: comments of a post, or the messages of a forum topic.
pub(super) async fn replies(
  conn: &Conn,
  peer: PeerRef,
  id: i32,
  page: &PageReq,
) -> Result<Page<Post>> {
  let size = page.size_within(100);
  let request = tl::functions::messages::GetReplies {
    peer: peer.into(),
    msg_id: id,
    offset_id: page
      .cursor
      .as_deref()
      .and_then(|c| c.parse().ok())
      .unwrap_or(0),
    offset_date: 0,
    add_offset: 0,
    limit: size as i32,
    max_id: 0,
    min_id: 0,
    hash: 0,
  };
  let result = conn.client.invoke(&request).await.map_err(error)?;
  Ok(page_by_id(posts_of(conn, result), size))
}

/// Posts of a raw `messages.Messages`, with their peers.
pub fn posts_of(conn: &Conn, result: tl::enums::messages::Messages) -> Vec<Post> {
  use tl::enums::messages::Messages as M;
  let (messages, chats, users) = match result {
    M::Messages(m) => (m.messages, m.chats, m.users),
    M::Slice(m) => (m.messages, m.chats, m.users),
    M::ChannelMessages(m) => (m.messages, m.chats, m.users),
    M::NotModified(_) => return Vec::new(),
  };
  let peers = parse::peers(&conn.client, users, chats);
  messages
    .iter()
    .filter_map(|m| parse::raw_post(m, &peers))
    .collect()
}

/// Download the media of a `tg-media:CHAT/MESSAGE` post into `dir/name.<ext>`.
pub async fn save_media(
  conn: &Conn,
  post: &str,
  dir: &std::path::Path,
  name: &str,
) -> Result<std::path::PathBuf> {
  let m = refs::message(post)?;
  let peer = mt::peer(conn, &m.chat).await?;
  let msg = one(conn, peer, m.id).await?;
  let media = msg
    .media()
    .ok_or_else(|| Error::not_found("the message has no media"))?;
  let path = dir.join(format!("{name}.{}", parse::extension(&media)));
  conn
    .client
    .download_media(&media, &path)
    .await
    .map_err(error)?;
  Ok(path)
}
