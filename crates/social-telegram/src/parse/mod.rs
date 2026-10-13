//! grammers types → the shared model: messages are posts (`id` is
//! `CHAT/MESSAGE`, `url` the t.me link when there is one), dialogs and peers
//! are collections, accounts are users.

mod media;
mod messages;

use grammers_client::Client;
use grammers_client::peer::{Dialog, Peer};
use grammers_client::session::types::PeerId;
use grammers_client::tl;
use social_core::text::from_secs;
use social_core::{Collection, User};
use std::collections::HashMap;

pub use self::media::*;
pub use self::messages::*;

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
