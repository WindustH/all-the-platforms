//! grammers types → the shared model: messages are posts (`id` is
//! `CHAT/MESSAGE`, `url` the t.me link when there is one), dialogs and peers
//! are collections, accounts are users.

use std::collections::HashMap;

use grammers_client::Client;
use grammers_client::media::Media as TgMedia;
use grammers_client::message::Message;
use grammers_client::peer::{Dialog, Peer};
use grammers_client::session::types::PeerId;
use grammers_client::tl;
use social_core::text::from_secs;
use social_core::{Collection, Comment, Media, MediaKind, Post, User, Value, json};

use crate::refs;

/// Bot API id of a peer (the form chats are addressed by).
pub fn dialog_id(id: PeerId) -> i64 {
  id.bot_api_dialog_id().unwrap_or_default()
}

pub fn username(peer: &Peer) -> Option<String> {
  peer.username().map(str::to_owned)
}

pub fn user(u: &grammers_client::peer::User) -> User {
  let id = dialog_id(u.id());
  let handle = u.username().map(str::to_owned);
  let mut out = User {
    id: id.to_string(),
    name: u.full_name(),
    url: handle.as_ref().map(|h| format!("https://t.me/{h}")),
    handle,
    verified: u.verified(),
    ..User::default()
  };
  let kind = if u.is_bot() { "bot" } else { "user" };
  out.extra.insert("kind".into(), kind.into());
  if let Some(phone) = u.phone() {
    out.extra.insert("phone".into(), phone.into());
  }
  if u.deleted() {
    out.extra.insert("deleted".into(), true.into());
  }
  out
}

/// A user, group or channel as a user (profiles, authors, members).
pub fn peer_user(peer: &Peer) -> User {
  match peer {
    Peer::User(u) => user(u),
    other => {
      let handle = username(other);
      let mut out = User {
        id: dialog_id(other.id()).to_string(),
        name: other.name().unwrap_or_default().to_owned(),
        url: handle.as_ref().map(|h| format!("https://t.me/{h}")),
        handle,
        ..User::default()
      };
      out.extra.insert("kind".into(), chat_kind(other).into());
      if let Some(c) = channel(other) {
        out.verified = c.verified;
        out.stats.followers = c.participants_count.map(|n| n as u64);
      }
      out
    }
  }
}

/// The raw channel behind a broadcast channel or a supergroup (grammers
/// files supergroups under `Group`).
pub fn channel(peer: &Peer) -> Option<&tl::types::Channel> {
  match peer {
    Peer::Channel(c) => Some(&c.raw),
    Peer::Group(g) => match &g.raw {
      tl::enums::Chat::Channel(c) => Some(c),
      _ => None,
    },
    Peer::User(_) => None,
  }
}

pub fn chat_kind(peer: &Peer) -> &'static str {
  match (peer, channel(peer)) {
    (Peer::User(u), _) if u.is_bot() => "bot",
    (Peer::User(_), _) => "dm",
    (_, Some(c)) if c.forum => "forum",
    (_, Some(c)) if c.broadcast => "channel",
    (_, Some(_)) => "supergroup",
    _ => "group",
  }
}

/// A chat for `chats`; the dialog adds unread counters.
pub fn chat(peer: &Peer, dialog: Option<&Dialog>) -> Collection {
  let handle = username(peer);
  let mut c = Collection {
    id: dialog_id(peer.id()).to_string(),
    kind: chat_kind(peer).into(),
    name: peer.name().unwrap_or_default().to_owned(),
    url: handle.as_ref().map(|h| format!("https://t.me/{h}")),
    ..Collection::default()
  };
  c.followers = channel(peer)
    .and_then(|ch| ch.participants_count)
    .or(match peer {
      Peer::Group(g) => match &g.raw {
        tl::enums::Chat::Chat(chat) => Some(chat.participants_count),
        _ => None,
      },
      _ => None,
    })
    .map(|n| n.max(0) as u64);
  if let Some(h) = handle {
    c.extra.insert("username".into(), h.into());
  }
  if let Some(tl::enums::Dialog::Dialog(d)) = dialog.map(|d| &d.raw) {
    let extra = &mut c.extra;
    extra.insert("unread".into(), d.unread_count.into());
    if d.unread_mentions_count > 0 {
      extra.insert("unread_mentions".into(), d.unread_mentions_count.into());
    }
    if d.unread_reactions_count > 0 {
      extra.insert("unread_reactions".into(), d.unread_reactions_count.into());
    }
    if d.pinned {
      extra.insert("pinned".into(), true.into());
    }
    if d.folder_id == Some(1) {
      extra.insert("archived".into(), true.into());
    }
  }
  if let Some(last) = dialog.and_then(|d| d.last_message.as_ref()) {
    let at = from_secs(last.date().timestamp()).map(|t| t.to_string());
    c.extra.insert("last_message_at".into(), at.into());
    let preview: String = last.text().chars().take(80).collect();
    if !preview.is_empty() {
      c.description = Some(preview);
    }
  }
  c
}

/// Peers that came with a response, by id.
pub type Peers = HashMap<PeerId, Peer>;

pub fn peers(client: &Client, users: Vec<tl::enums::User>, chats: Vec<tl::enums::Chat>) -> Peers {
  let users = users
    .into_iter()
    .map(|u| Peer::User(grammers_client::peer::User::from_raw(client, u)));
  let chats = chats.into_iter().map(|c| Peer::from_raw(client, c));
  users.chain(chats).map(|p| (p.id(), p)).collect()
}

/// A message grammers fetched, with the peers it carries.
pub fn post(msg: &Message) -> Post {
  let mut peers = Peers::new();
  for p in [msg.peer(), msg.sender()].into_iter().flatten() {
    peers.insert(p.id(), p.clone());
  }
  raw_post(&msg.raw, &peers).unwrap_or_else(|| Post {
    id: msg.id().to_string(),
    kind: "empty".into(),
    ..Post::default()
  })
}

/// A raw message; media point at `tg-media:` so `download` can fetch them.
pub fn raw_post(raw: &tl::enums::Message, peers: &Peers) -> Option<Post> {
  let (id, peer_id, from_id, date, out) = match raw {
    tl::enums::Message::Message(m) => (m.id, &m.peer_id, &m.from_id, m.date, m.out),
    tl::enums::Message::Service(m) => (m.id, &m.peer_id, &m.from_id, m.date, m.out),
    tl::enums::Message::Empty(_) => return None,
  };
  let chat_id = PeerId::from(peer_id.clone());
  let chat_peer = peers.get(&chat_id);
  let chat_dialog = dialog_id(chat_id);
  let chat_user = chat_peer.and_then(username);
  let post_id = refs::message_id(chat_user.as_deref(), chat_dialog, id);
  let sender = match from_id {
    Some(f) => peers.get(&PeerId::from(f.clone())),
    // Private chats and channel posts name no sender.
    None if !out => chat_peer,
    None => None,
  };
  let mut post = Post {
    id: post_id.clone(),
    kind: "message".into(),
    url: refs::message_url(chat_user.as_deref(), chat_dialog, id),
    author: sender.map(peer_user),
    created_at: from_secs(date.into()),
    ..Post::default()
  };
  let extra = &mut post.extra;
  extra.insert("chat".into(), chat_dialog.into());
  if let Some(p) = chat_peer {
    extra.insert("chat_name".into(), p.name().unwrap_or_default().into());
  }
  extra.insert("message_id".into(), id.into());
  if out {
    extra.insert("outgoing".into(), true.into());
  }
  match raw {
    tl::enums::Message::Service(s) => {
      post.kind = "service".into();
      extra.insert("action".into(), variant(&s.action).into());
    }
    tl::enums::Message::Message(m) => {
      message_fields(&mut post, m, chat_user.as_deref(), chat_dialog)
    }
    tl::enums::Message::Empty(_) => {}
  }
  Some(post)
}

fn message_fields(post: &mut Post, m: &tl::types::Message, chat_user: Option<&str>, chat: i64) {
  post.text = Some(m.message.clone()).filter(|t| !t.is_empty());
  post.updated_at = m.edit_date.and_then(|d| from_secs(d.into()));
  let metrics = &mut post.metrics;
  metrics.views = m.views.map(|n| n as u64);
  metrics.shares = m.forwards.map(|n| n as u64);
  if let Some(tl::enums::MessageReplies::Replies(r)) = &m.replies {
    metrics.comments = Some(r.replies.max(0) as u64);
  }
  let reactions = reactions(m);
  if !reactions.is_empty() {
    let total = reactions
      .iter()
      .filter(|r| r["emoji"] != "⭐")
      .filter_map(|r| r["count"].as_u64())
      .sum();
    metrics.likes = Some(total);
    post
      .extra
      .insert("reactions".into(), Value::Array(reactions));
  }
  if let Some(media) = m.media.clone().and_then(TgMedia::from_raw) {
    post.media.extend(media_item(&media, &post.id));
    if let TgMedia::Poll(p) = &media {
      let tl::enums::TextWithEntities::Entities(q) = p.question();
      post.extra.insert("poll".into(), q.text.clone().into());
    }
  }
  let extra = &mut post.extra;
  if let Some(tl::enums::MessageReplyHeader::Header(h)) = &m.reply_to {
    if let Some(r) = h.reply_to_msg_id {
      extra.insert(
        "reply_to".into(),
        refs::message_id(chat_user, chat, r).into(),
      );
    }
    if h.forum_topic {
      extra.insert(
        "topic".into(),
        h.reply_to_top_id.or(h.reply_to_msg_id).into(),
      );
    }
  }
  if let Some(a) = &m.post_author {
    extra.insert("signature".into(), a.clone().into());
  }
  if let Some(g) = m.grouped_id {
    extra.insert("album".into(), g.to_string().into());
  }
  for (flag, on) in [("mentioned", m.mentioned), ("pinned", m.pinned)] {
    if on {
      extra.insert(flag.into(), true.into());
    }
  }
  if let Some(tl::enums::MessageFwdHeader::Header(h)) = &m.fwd_from {
    let from = h.from_id.clone().map(|p| dialog_id(PeerId::from(p)));
    extra.insert(
      "forwarded_from".into(),
      json!({
        "name": h.from_name,
        "id": from,
        "message_id": h.channel_post,
        "signature": h.post_author,
        "date": from_secs(h.date.into()).map(|t| t.to_string()),
      }),
    );
  }
}

/// A message under a channel post, as a comment.
pub fn comment(post: Post) -> Comment {
  Comment {
    id: post.id,
    author: post.author,
    text: post.text.unwrap_or_default(),
    created_at: post.created_at,
    likes: post.metrics.likes,
    reply_count: post.metrics.comments,
    extra: post.extra,
    ..Comment::default()
  }
}

fn reactions(m: &tl::types::Message) -> Vec<Value> {
  let Some(tl::enums::MessageReactions::Reactions(r)) = &m.reactions else {
    return Vec::new();
  };
  r.results
    .iter()
    .map(|tl::enums::ReactionCount::Count(c)| {
      let emoji = match &c.reaction {
        tl::enums::Reaction::Emoji(e) => e.emoticon.clone(),
        tl::enums::Reaction::CustomEmoji(e) => format!("custom:{}", e.document_id),
        tl::enums::Reaction::Paid => "⭐".into(),
        tl::enums::Reaction::Empty => String::new(),
      };
      json!({"emoji": emoji, "count": c.count})
    })
    .collect()
}

/// The constructor name of a TL value (`MessageActionChatAddUser` …).
fn variant<T: std::fmt::Debug>(v: &T) -> String {
  let debug = format!("{v:?}");
  debug
    .split(['(', ' ', '{'])
    .next()
    .unwrap_or_default()
    .to_owned()
}

/// What a message carries, as one media entry addressed by `tg-media:`.
pub fn media_item(media: &TgMedia, post_id: &str) -> Option<Media> {
  let url = format!("tg-media:{post_id}");
  let item = match media {
    TgMedia::Photo(p) => {
      let mut m = Media::new(MediaKind::Image, url);
      m.size = p.size().map(|s| s as u64);
      m
    }
    TgMedia::Sticker(s) => {
      let mut m = Media::new(MediaKind::Image, url);
      m.alt = Some(s.emoji().to_owned());
      m
    }
    TgMedia::Document(d) => {
      let mime = d.mime_type().unwrap_or_default();
      let kind = if d.is_animated() || mime == "image/gif" {
        MediaKind::Gif
      } else if mime.starts_with("video/") {
        MediaKind::Video
      } else if mime.starts_with("audio/") {
        MediaKind::Audio
      } else if mime.starts_with("image/") {
        MediaKind::Image
      } else {
        MediaKind::File
      };
      let mut m = Media::new(kind, url);
      m.size = d.size().map(|s| s as u64);
      m.duration = d.duration();
      if let Some((w, h)) = d.resolution() {
        m.width = Some(w as u32);
        m.height = Some(h as u32);
      }
      m.alt = d.name().filter(|n| !n.is_empty()).map(str::to_owned);
      m
    }
    _ => return None,
  };
  Some(item)
}

/// File extension for a downloaded media.
pub fn extension(media: &TgMedia) -> String {
  match media {
    TgMedia::Photo(_) => "jpg".into(),
    TgMedia::Sticker(s) => match s.document.mime_type() {
      Some("application/x-tgsticker") => "tgs".into(),
      Some("video/webm") => "webm".into(),
      _ => "webp".into(),
    },
    TgMedia::Document(d) => {
      if let Some(ext) = d
        .name()
        .and_then(|n| n.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase())
        .filter(|e| !e.is_empty() && e.len() <= 8)
      {
        return ext;
      }
      match d.mime_type().unwrap_or_default() {
        "video/mp4" => "mp4",
        "video/quicktime" => "mov",
        "video/webm" => "webm",
        "audio/ogg" => "ogg",
        "audio/mpeg" => "mp3",
        "audio/mp4" | "audio/x-m4a" => "m4a",
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "application/pdf" => "pdf",
        "application/zip" => "zip",
        _ => "bin",
      }
      .into()
    }
    _ => "bin".into(),
  }
}
