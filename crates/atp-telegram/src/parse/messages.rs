//! Messages: posts with metrics, reactions, replies and forwards; comments.

use atp_core::text::from_secs;
use atp_core::{Comment, Post, Value, json};
use grammers_client::media::Media as TgMedia;
use grammers_client::message::Message;
use grammers_client::session::types::PeerId;
use grammers_client::tl;

use crate::refs;

use super::*;

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

pub(super) fn message_fields(
  post: &mut Post,
  m: &tl::types::Message,
  chat_user: Option<&str>,
  chat: i64,
) {
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
  message_extra(post, m, chat_user, chat);
}

/// Replies, topics, signatures, albums, flags and forwards.
pub(super) fn message_extra(
  post: &mut Post,
  m: &tl::types::Message,
  chat_user: Option<&str>,
  chat: i64,
) {
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

pub(super) fn reactions(m: &tl::types::Message) -> Vec<Value> {
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
pub(super) fn variant<T: std::fmt::Debug>(v: &T) -> String {
  let debug = format!("{v:?}");
  debug
    .split(['(', ' ', '{'])
    .next()
    .unwrap_or_default()
    .to_owned()
}
