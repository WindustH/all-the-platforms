//! Dialogs, forum topics, members, joining and leaving, read marks.

use std::collections::BTreeMap;

use grammers_client::tl;
use social_core::{Action, Collection, Error, Page, PageReq, Result, User};

use crate::mt::{self, Conn, error};
use crate::parse;
use crate::refs::{self, Chat};

use super::*;

pub async fn chats(conn: &Conn, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
  if let Some(parent) = parent {
    return topics(conn, &refs::chat(parent)?, page).await;
  }
  let (start, size) = (offset(page), page.size_within(100));
  let mut dialogs = conn.client.iter_dialogs();
  let mut items = Vec::new();
  let mut seen = 0;
  let mut more = false;
  while let Some(d) = dialogs.next().await.map_err(error)? {
    seen += 1;
    if seen <= start {
      continue;
    }
    if items.len() == size {
      more = true;
      break;
    }
    items.push(parse::chat(&d.peer, Some(&d)));
  }
  let next = next_offset(start, items.len(), size, more);
  Ok(Page::new(items, next))
}

/// Forum topics of a supergroup.
pub(super) async fn topics(conn: &Conn, chat: &Chat, page: &PageReq) -> Result<Page<Collection>> {
  let peer = mt::peer(conn, chat).await?;
  let request = tl::functions::messages::GetForumTopics {
    peer: peer.into(),
    q: None,
    offset_date: 0,
    offset_id: 0,
    offset_topic: page.number_or(0) as i32,
    limit: page.size_within(100) as i32,
  };
  let tl::enums::messages::ForumTopics::Topics(t) =
    conn.client.invoke(&request).await.map_err(error)?;
  let parent = parse::dialog_id(peer.id);
  let mut items = Vec::new();
  let mut last = None;
  for topic in t.topics {
    let tl::enums::ForumTopic::Topic(topic) = topic else {
      continue;
    };
    last = Some(topic.id);
    let mut c = Collection {
      id: format!("{parent}/{}", topic.id),
      kind: "topic".into(),
      name: topic.title.clone(),
      ..Collection::default()
    };
    c.extra.insert("unread".into(), topic.unread_count.into());
    c.extra
      .insert("top_message".into(), topic.top_message.into());
    if topic.closed {
      c.extra.insert("closed".into(), true.into());
    }
    items.push(c);
  }
  let more = items.len() >= page.size_within(100) && (items.len() as i32) < t.count;
  Ok(Page::new(
    items,
    last.filter(|_| more).map(|id| id.to_string()),
  ))
}

pub async fn members(conn: &Conn, chat: &str, page: &PageReq) -> Result<Page<User>> {
  let peer = mt::peer(conn, &refs::chat(chat)?).await?;
  let (start, size) = (offset(page), page.size_within(200));
  let mut iter = conn.client.iter_participants(peer);
  let mut items = Vec::new();
  let mut seen = 0;
  let mut more = false;
  while let Some(p) = iter.next().await.map_err(error)? {
    seen += 1;
    if seen <= start {
      continue;
    }
    if items.len() == size {
      more = true;
      break;
    }
    let mut u = parse::user(&p.user);
    let role = match &p.role {
      grammers_client::peer::Role::Creator(_) => "creator",
      grammers_client::peer::Role::Admin(_) => "admin",
      grammers_client::peer::Role::Banned(_) => "banned",
      grammers_client::peer::Role::Left(_) => "left",
      _ => "member",
    };
    u.extra.insert("role".into(), role.into());
    items.push(u);
  }
  let next = next_offset(start, items.len(), size, more);
  Ok(Page::new(items, next))
}

/// Join a public group / channel or an invite link; leave with `undo`.
pub async fn join(conn: &Conn, arg: &str, undo: bool) -> Result<Action> {
  let chat = refs::chat(arg)?;
  if let (Chat::Invite(hash), false) = (&chat, undo) {
    let request = tl::functions::messages::ImportChatInvite { hash: hash.clone() };
    conn.client.invoke(&request).await.map_err(error)?;
    return Ok(Action::done("join", arg));
  }
  let peer = mt::peer(conn, &chat).await?;
  if !matches!(
    peer.id.kind(),
    grammers_client::session::types::PeerKind::Channel
  ) {
    return Err(Error::input(
      "only channels and supergroups can be joined or left here",
    ));
  }
  if undo {
    let request = tl::functions::channels::LeaveChannel {
      channel: peer.into(),
    };
    conn.client.invoke(&request).await.map_err(error)?;
    Ok(Action::done("leave", arg))
  } else {
    let request = tl::functions::channels::JoinChannel {
      channel: peer.into(),
    };
    conn.client.invoke(&request).await.map_err(error)?;
    Ok(Action::done("join", arg))
  }
}

pub async fn mark_read(conn: &Conn, arg: &str) -> Result<Action> {
  let peer = mt::peer(conn, &refs::chat(arg)?).await?;
  conn.client.mark_as_read(peer).await.map_err(error)?;
  Ok(Action::done("mark-read", arg))
}

/// Unread counters over all dialogs.
pub async fn unread(conn: &Conn) -> Result<BTreeMap<String, u64>> {
  let mut dialogs = conn.client.iter_dialogs();
  let (mut messages, mut chats, mut mentions, mut reactions) = (0u64, 0u64, 0u64, 0u64);
  while let Some(d) = dialogs.next().await.map_err(error)? {
    if let tl::enums::Dialog::Dialog(d) = &d.raw {
      let n = d.unread_count.max(0) as u64;
      messages += n;
      chats += u64::from(n > 0 || d.unread_mark);
      mentions += d.unread_mentions_count.max(0) as u64;
      reactions += d.unread_reactions_count.max(0) as u64;
    }
  }
  Ok(BTreeMap::from([
    ("messages".into(), messages),
    ("chats".into(), chats),
    ("mentions".into(), mentions),
    ("reactions".into(), reactions),
  ]))
}
