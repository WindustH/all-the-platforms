//! Protocol messages into archive records, and records into the shared model.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use social_core::text::from_secs;
use social_core::{Collection, Media, MediaKind, Post, User, Value, json};
use whatsapp_rust::prelude::{MessageExt, wa};

use crate::archive::{Archive, ChatEntry, MediaRef, Record};

/// What one protocol message means for the archive.
pub enum Item {
  Message(Box<Record>),
  Edit {
    chat: String,
    id: String,
    text: Option<String>,
  },
  Revoke {
    chat: String,
    id: String,
  },
  Reaction {
    chat: String,
    id: String,
    sender: String,
    emoji: String,
  },
  Skip,
}

/// Where a message came from.
pub struct Origin {
  pub chat: String,
  pub sender: String,
  pub id: String,
  pub from_me: bool,
  pub ts: i64,
  pub push_name: Option<String>,
}

pub fn item(m: &wa::Message, o: Origin) -> Item {
  let base = m.get_base_message();
  if let Some(p) = base.protocol_message.as_option() {
    let target = p
      .key
      .as_option()
      .and_then(|k| k.id.clone())
      .unwrap_or_default();
    return match p.r#type {
      Some(wa::message::protocol_message::Type::REVOKE) => Item::Revoke {
        chat: o.chat,
        id: target,
      },
      Some(wa::message::protocol_message::Type::MESSAGE_EDIT) => Item::Edit {
        chat: o.chat,
        id: target,
        text: p.edited_message.as_option().and_then(text_of),
      },
      _ => Item::Skip,
    };
  }
  if let Some(r) = base.reaction_message.as_option() {
    return Item::Reaction {
      chat: o.chat,
      id: r
        .key
        .as_option()
        .and_then(|k| k.id.clone())
        .unwrap_or_default(),
      sender: o.sender,
      emoji: r.text.clone().unwrap_or_default(),
    };
  }
  let (kind, media) = content(base);
  if kind.is_empty() {
    return Item::Skip;
  }
  let context = context(base);
  Item::Message(Box::new(Record {
    id: o.id,
    chat: o.chat,
    sender: o.sender,
    from_me: o.from_me,
    ts: o.ts,
    push_name: o.push_name.filter(|n| !n.is_empty()),
    kind: kind.into(),
    text: text_of(base),
    media,
    quoted: context.and_then(|c| c.stanza_id.clone()),
    forwarded: m.is_forwarded(),
    ..Record::default()
  }))
}

/// Text or caption.
fn text_of(m: &wa::Message) -> Option<String> {
  let m = m.get_base_message();
  if let Some(t) = m.text_content().or_else(|| m.get_caption()) {
    return Some(t.to_owned()).filter(|t| !t.is_empty());
  }
  if let Some(l) = m.location_message.as_option() {
    let place = [l.name.as_deref(), l.address.as_deref()]
      .into_iter()
      .flatten()
      .collect::<Vec<_>>()
      .join(", ");
    let at = format!(
      "{:.5},{:.5}",
      l.degrees_latitude.unwrap_or_default(),
      l.degrees_longitude.unwrap_or_default()
    );
    return Some(if place.is_empty() {
      at
    } else {
      format!("{place} ({at})")
    });
  }
  if let Some(c) = m.contact_message.as_option() {
    return c.display_name.clone();
  }
  if let Some(p) = m
    .poll_creation_message_v3
    .as_option()
    .or(m.poll_creation_message.as_option())
  {
    let options: Vec<&str> = p
      .options
      .iter()
      .filter_map(|o| o.option_name.as_deref())
      .collect();
    return Some(format!(
      "{} [{}]",
      p.name.as_deref().unwrap_or_default(),
      options.join(" / ")
    ));
  }
  m.document_message
    .as_option()
    .and_then(|d| d.file_name.clone())
}

fn context(m: &wa::Message) -> Option<&wa::ContextInfo> {
  m.extended_text_message
    .as_option()
    .and_then(|x| x.context_info.as_option())
    .or_else(|| {
      m.image_message
        .as_option()
        .and_then(|x| x.context_info.as_option())
    })
    .or_else(|| {
      m.video_message
        .as_option()
        .and_then(|x| x.context_info.as_option())
    })
    .or_else(|| {
      m.audio_message
        .as_option()
        .and_then(|x| x.context_info.as_option())
    })
    .or_else(|| {
      m.document_message
        .as_option()
        .and_then(|x| x.context_info.as_option())
    })
    .or_else(|| {
      m.sticker_message
        .as_option()
        .and_then(|x| x.context_info.as_option())
    })
}

fn b64(v: &Option<Vec<u8>>) -> String {
  v.as_deref().map(|b| STANDARD.encode(b)).unwrap_or_default()
}

/// Kind of content and its downloadable media, if any.
fn content(m: &wa::Message) -> (&'static str, Option<MediaRef>) {
  macro_rules! media {
    ($x:expr, $kind:expr) => {
      MediaRef {
        kind: $kind.into(),
        mime: $x.mimetype.clone(),
        direct_path: $x.direct_path.clone().unwrap_or_default(),
        key: b64(&$x.media_key),
        sha256: b64(&$x.file_sha256),
        enc_sha256: b64(&$x.file_enc_sha256),
        length: $x.file_length.unwrap_or_default(),
        ..MediaRef::default()
      }
    };
  }
  if let Some(x) = m.image_message.as_option() {
    let mut r = media!(x, "image");
    r.width = x.width;
    r.height = x.height;
    return ("image", Some(r));
  }
  if let Some(x) = m.video_message.as_option() {
    let gif = x.gif_playback.unwrap_or(false);
    let mut r = media!(x, if gif { "gif" } else { "video" });
    r.width = x.width;
    r.height = x.height;
    r.seconds = x.seconds;
    return (if gif { "gif" } else { "video" }, Some(r));
  }
  if let Some(x) = m.audio_message.as_option() {
    let voice = x.ptt.unwrap_or(false);
    let mut r = media!(x, if voice { "voice" } else { "audio" });
    r.seconds = x.seconds;
    return (if voice { "voice" } else { "audio" }, Some(r));
  }
  if let Some(x) = m.document_message.as_option() {
    let mut r = media!(x, "document");
    r.file_name = x.file_name.clone();
    return ("document", Some(r));
  }
  if let Some(x) = m.sticker_message.as_option() {
    let mut r = media!(x, "sticker");
    r.width = x.width;
    r.height = x.height;
    return ("sticker", Some(r));
  }
  if m.conversation.is_some() || m.extended_text_message.is_set() {
    return ("text", None);
  }
  if m.location_message.is_set() || m.live_location_message.is_set() {
    return ("location", None);
  }
  if m.contact_message.is_set() || m.contacts_array_message.is_set() {
    return ("contact", None);
  }
  if m.poll_creation_message.is_set() || m.poll_creation_message_v3.is_set() {
    return ("poll", None);
  }
  ("", None)
}

/// Apply an item to the archive; returns the record it produced or changed.
pub fn apply(archive: &mut Archive, item: Item) -> Option<(Record, &'static str)> {
  match item {
    Item::Message(r) => {
      let r = *r;
      if let Some(name) = &r.push_name
        && !r.from_me
      {
        archive.set_name(&r.sender, name, false);
      }
      archive.put(r.clone());
      Some((r, "message"))
    }
    Item::Edit { chat, id, text } => {
      archive.update(&chat, &id, |r| {
        r.text = text;
        r.edited = true;
      });
      archive.find_message(&chat, &id).map(|r| (r, "edit"))
    }
    Item::Revoke { chat, id } => {
      archive.update(&chat, &id, |r| {
        r.deleted = true;
        r.text = None;
        r.media = None;
      });
      archive.find_message(&chat, &id).map(|r| (r, "delete"))
    }
    Item::Reaction {
      chat,
      id,
      sender,
      emoji,
    } => {
      archive.update(&chat, &id, |r| {
        if emoji.is_empty() {
          r.reactions.remove(&sender);
        } else {
          r.reactions.insert(sender.clone(), emoji.clone());
        }
      });
      archive.find_message(&chat, &id).map(|r| (r, "reaction"))
    }
    Item::Skip => None,
  }
}

/// A message as a post: `id` is `CHAT/MESSAGE`, which commands accept back.
pub fn post(r: &Record, archive: &Archive) -> Post {
  let sender_name = if r.from_me {
    Some("me".to_owned())
  } else {
    archive
      .name(&r.sender)
      .map(str::to_owned)
      .or_else(|| r.push_name.clone())
  };
  let author = User {
    id: r.sender.clone(),
    name: sender_name.unwrap_or_else(|| phone(&r.sender)),
    handle: Some(phone(&r.sender)).filter(|p| p.starts_with('+')),
    ..User::default()
  };
  let mut post = Post {
    id: format!("{}/{}", r.chat, r.id),
    kind: if r.deleted {
      "deleted".into()
    } else {
      r.kind.clone()
    },
    text: r.text.clone(),
    author: Some(author),
    created_at: from_secs(r.ts),
    media: r.media.iter().map(|m| media(r, m)).collect(),
    ..Post::default()
  };
  if !r.reactions.is_empty() {
    post.metrics.likes = Some(r.reactions.len() as u64);
  }
  let mut put = |k: &str, v: Value| {
    post.extra.insert(k.into(), v);
  };
  put("chat", r.chat.clone().into());
  if let Some(name) = archive.name(&r.chat) {
    put("chat_name", name.into());
  }
  put("from_me", r.from_me.into());
  if let Some(q) = &r.quoted {
    put("reply_to", format!("{}/{q}", r.chat).into());
  }
  if r.forwarded {
    put("forwarded", true.into());
  }
  if r.edited {
    put("edited", true.into());
  }
  if !r.reactions.is_empty() {
    put("reactions", json!(r.reactions));
  }
  post
}

/// Archived media are fetched by the platform (`wa-media:` URLs).
fn media(r: &Record, m: &MediaRef) -> Media {
  let kind = match m.kind.as_str() {
    "image" | "sticker" => MediaKind::Image,
    "video" => MediaKind::Video,
    "gif" => MediaKind::Gif,
    "audio" | "voice" => MediaKind::Audio,
    _ => MediaKind::File,
  };
  let mut media = Media::new(kind, format!("wa-media:{}/{}", r.chat, r.id));
  media.width = m.width;
  media.height = m.height;
  media.duration = m.seconds.map(f64::from);
  media.size = Some(m.length).filter(|l| *l > 0);
  media.alt = m.file_name.clone();
  media
}

pub fn collection(c: &ChatEntry, archive: &Archive) -> Collection {
  let mut out = Collection {
    id: c.jid.clone(),
    kind: c.kind.clone(),
    name: archive
      .name(&c.jid)
      .map(str::to_owned)
      .unwrap_or_else(|| phone(&c.jid)),
    description: c.description.clone(),
    ..Collection::default()
  };
  let mut put = |k: &str, v: Value| {
    out.extra.insert(k.into(), v);
  };
  if c.unread > 0 {
    put("unread", c.unread.into());
  }
  if let Some(t) = from_secs(c.last) {
    put("last_message_at", t.to_string().into());
  }
  if c.pinned {
    put("pinned", true.into());
  }
  if c.archived {
    put("archived", true.into());
  }
  out
}

/// `+<number>` for phone-number JIDs, else the JID.
pub fn phone(jid: &str) -> String {
  match jid.split_once('@') {
    Some((user, "s.whatsapp.net")) => {
      let user = user.split(':').next().unwrap_or(user);
      format!("+{user}")
    }
    _ => jid.to_owned(),
  }
}
