//! What a user account does over MTProto: dialogs, history, search, the
//! comments of channel posts, members, sending, editing, reactions, joining.

use std::collections::BTreeMap;

use grammers_client::message::{InputMessage, InputReactions, Message};
use grammers_client::peer::Peer;
use grammers_client::session::types::PeerRef;
use grammers_client::{media::InputMedia, tl};
use social_core::{
  Action, Collection, Comment, Error, Message as Outgoing, Page, PageReq, Post, Result, User,
};

use crate::convert;
use crate::mt::{self, Conn, error};
use crate::refs::{self, Chat};

/// Offset cursors (`skip N`) for listings grammers pages by itself.
fn offset(page: &PageReq) -> usize {
  page.number_or(0) as usize
}

fn next_offset(start: usize, got: usize, wanted: usize, more: bool) -> Option<String> {
  (more && got >= wanted).then(|| (start + got).to_string())
}

pub async fn whoami(conn: &Conn) -> Result<User> {
  let me = conn.client.get_me().await.map_err(error)?;
  Ok(convert::user(&me))
}

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
    items.push(convert::chat(&d.peer, Some(&d)));
  }
  let next = next_offset(start, items.len(), size, more);
  Ok(Page::new(items, next))
}

/// Forum topics of a supergroup.
async fn topics(conn: &Conn, chat: &Chat, page: &PageReq) -> Result<Page<Collection>> {
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
  let parent = convert::dialog_id(peer.id);
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
    items.push(convert::post(&m));
  }
  Ok(page_by_id(items, size))
}

/// `CHAT` or `CHAT/TOPIC` (a forum topic from `chats CHAT`).
fn topic_ref(arg: &str) -> Result<(Chat, Option<i32>)> {
  if let Ok(m) = refs::message(arg)
    && !arg.contains("t.me")
  {
    return Ok((m.chat, Some(m.id)));
  }
  Ok((refs::chat(arg)?, None))
}

fn page_by_id(items: Vec<Post>, size: usize) -> Page<Post> {
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
    return one(conn, group, comment)
      .await
      .map(|msg| convert::post(&msg));
  }
  one(conn, peer, m.id).await.map(|msg| convert::post(&msg))
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
async fn discussion(conn: &Conn, peer: PeerRef, id: i32) -> Result<(PeerRef, i32)> {
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
    posts.items.into_iter().map(convert::comment).collect(),
    posts.next_cursor,
  ))
}

/// `messages.getReplies`: comments of a post, or the messages of a forum topic.
async fn replies(conn: &Conn, peer: PeerRef, id: i32, page: &PageReq) -> Result<Page<Post>> {
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
  let peers = convert::peers(&conn.client, users, chats);
  messages
    .iter()
    .filter_map(|m| convert::raw_post(m, &peers))
    .collect()
}

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
    items.push(convert::post(&m));
  }
  Ok(page_by_id(items, size))
}

async fn collect_skipping(
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
    items.push(convert::post(&m));
  }
  let next = next_offset(start, items.len(), size, more);
  Ok(Page::new(items, next))
}

pub const FILTERS: &[&str] = &[
  "photo", "video", "media", "document", "url", "voice", "music", "gif", "round", "mention",
];

fn messages_filter(name: &str) -> Result<tl::enums::MessagesFilter> {
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
  let peers = convert::peers(&conn.client, found.users, found.chats);
  let items = found
    .my_results
    .iter()
    .chain(&found.results)
    .filter_map(|p| peers.get(&grammers_client::session::types::PeerId::from(p.clone())))
    .map(convert::peer_user)
    .collect();
  Ok(Page::last(items))
}

/// A user, group or channel with its full profile (bio, member count).
pub async fn user(conn: &Conn, arg: &str) -> Result<User> {
  let peer_ref = mt::peer(conn, &refs::chat(arg)?).await?;
  let peer = conn.client.resolve_peer(peer_ref).await.map_err(error)?;
  let mut out = convert::peer_user(&peer);
  match &peer {
    Peer::User(_) => {
      let request = tl::functions::users::GetFullUser {
        id: peer_ref.into(),
      };
      if let Ok(tl::enums::users::UserFull::Full(full)) = conn.client.invoke(&request).await {
        let tl::enums::UserFull::Full(f) = full.full_user;
        out.bio = f.about.filter(|a| !a.is_empty());
        out
          .extra
          .insert("common_chats".into(), f.common_chats_count.into());
      }
    }
    _ if convert::channel(&peer).is_some() => {
      let request = tl::functions::channels::GetFullChannel {
        channel: peer_ref.into(),
      };
      if let Ok(tl::enums::messages::ChatFull::Full(full)) = conn.client.invoke(&request).await
        && let tl::enums::ChatFull::ChannelFull(f) = full.full_chat
      {
        out.bio = Some(f.about).filter(|a| !a.is_empty());
        out.stats.followers = f.participants_count.map(|n| n as u64);
        if let Some(online) = f.online_count {
          out.stats.other.insert("online".into(), online as u64);
        }
        if let Some(linked) = f.linked_chat_id {
          out
            .extra
            .insert("linked_chat".into(), (-1_000_000_000_000 - linked).into());
        }
        if f.can_view_stats {
          out.extra.insert("can_view_stats".into(), true.into());
        }
      }
    }
    _ => {}
  }
  Ok(out)
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
    let mut u = convert::user(&p.user);
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

pub async fn send(conn: &Conn, chat: &str, message: &Outgoing) -> Result<Action> {
  let chat = refs::chat(chat)?;
  let peer = mt::peer(conn, &chat).await?;
  let reply = match &message.reply_to {
    Some(r) => Some(refs::message(r)?.id),
    None => None,
  };
  let sent = match message.files.as_slice() {
    [] => conn
      .client
      .send_message(
        peer,
        InputMessage::new().text(&message.text).reply_to(reply),
      )
      .await
      .map_err(error)?,
    [file] => {
      let uploaded = upload(conn, file).await?;
      let input = InputMessage::new().text(&message.text).reply_to(reply);
      let input = if is_photo(file) {
        input.photo(uploaded)
      } else {
        input.document(uploaded)
      };
      conn.client.send_message(peer, input).await.map_err(error)?
    }
    files => {
      let mut album = Vec::new();
      for (i, file) in files.iter().enumerate() {
        let uploaded = upload(conn, file).await?;
        let mut media = InputMedia::new();
        if i == 0 {
          media = media.caption(&message.text).reply_to(reply);
        }
        album.push(if is_photo(file) {
          media.photo(uploaded)
        } else {
          media.document(uploaded)
        });
      }
      conn
        .client
        .send_album(peer, album)
        .await
        .map_err(error)?
        .into_iter()
        .flatten()
        .next()
        .ok_or_else(|| Error::upstream("Telegram returned no message for the album"))?
    }
  };
  let post = convert::post(&sent);
  let mut action = Action::done("send", chat.to_string()).with_id(&post.id);
  if let Some(url) = post.url {
    action = action.with_url(url);
  }
  Ok(action)
}

async fn upload(conn: &Conn, path: &std::path::Path) -> Result<grammers_client::media::Uploaded> {
  conn
    .client
    .upload_file(path)
    .await
    .map_err(|e| Error::input(format!("cannot upload {}: {e}", path.display())))
}

fn is_photo(path: &std::path::Path) -> bool {
  let ext = path
    .extension()
    .and_then(|e| e.to_str())
    .map(str::to_ascii_lowercase)
    .unwrap_or_default();
  matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp")
}

pub async fn edit(conn: &Conn, arg: &str, text: &str) -> Result<Action> {
  let m = refs::message(arg)?;
  let peer = mt::peer(conn, &m.chat).await?;
  conn
    .client
    .edit_message(peer, m.id, InputMessage::new().text(text))
    .await
    .map_err(error)?;
  Ok(Action::done("edit", arg))
}

pub async fn delete(conn: &Conn, arg: &str) -> Result<Action> {
  let m = refs::message(arg)?;
  let peer = mt::peer(conn, &m.chat).await?;
  conn
    .client
    .delete_messages(peer, &[m.id])
    .await
    .map_err(error)?;
  Ok(Action::done("delete", arg))
}

pub async fn react(conn: &Conn, arg: &str, emoji: &str, undo: bool) -> Result<Action> {
  let m = refs::message(arg)?;
  let peer = mt::peer(conn, &m.chat).await?;
  let reactions = if undo {
    InputReactions::remove()
  } else {
    InputReactions::emoticon(emoji)
  };
  conn
    .client
    .send_reactions(peer, m.id, reactions)
    .await
    .map_err(error)?;
  Ok(Action::done(if undo { "unreact" } else { "react" }, arg).with_message(emoji))
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

pub async fn pin(conn: &Conn, arg: &str, undo: bool) -> Result<Action> {
  let m = refs::message(arg)?;
  let peer = mt::peer(conn, &m.chat).await?;
  if undo {
    conn.client.unpin_message(peer, m.id).await.map_err(error)?;
  } else {
    conn.client.pin_message(peer, m.id).await.map_err(error)?;
  }
  Ok(Action::done(if undo { "unpin" } else { "pin" }, arg))
}

pub async fn forward(conn: &Conn, arg: &str, to: &str) -> Result<Action> {
  let m = refs::message(arg)?;
  let from = mt::peer(conn, &m.chat).await?;
  let to_chat = refs::chat(to)?;
  let target = mt::peer(conn, &to_chat).await?;
  let sent = conn
    .client
    .forward_messages(target, &[m.id], from)
    .await
    .map_err(error)?;
  let mut action = Action::done("forward", arg);
  if let Some(post) = sent.into_iter().flatten().next().map(|m| convert::post(&m)) {
    action = action.with_id(&post.id);
    if let Some(url) = post.url {
      action = action.with_url(url);
    }
  }
  Ok(action)
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
  let path = dir.join(format!("{name}.{}", convert::extension(&media)));
  conn
    .client
    .download_media(&media, &path)
    .await
    .map_err(error)?;
  Ok(path)
}
