//! Sending, reacting, editing, deleting and read receipts.

use std::collections::BTreeMap;
use std::path::Path;

use atp_core::{Action, Error, Message, Result};
use whatsapp_rust::RevokeType;
use whatsapp_rust::prelude::{MessageField, wa};
use whatsapp_rust::upload::UploadOptions;
use whatsapp_rust::wacore::download::MediaType;

use crate::api::{Api, Mode, not_logged_in};
use crate::archive::{Record, kind_of};
use crate::link::Link;
use crate::refs::jid;
use crate::{files, parse, refs};

/// On the linked device, or through the Cloud API.
pub async fn send(api: &Api, chat: &str, message: &Message) -> Result<Action> {
  match api.mode() {
    Mode::Personal => {
      let device = async |link: &mut Link| send_on_device(link, chat, message).await;
      api.personal.with(&api.ctx, device).await
    }
    Mode::Cloud(cloud) => cloud.send(&api.ctx, chat, message).await,
    Mode::None => Err(not_logged_in()),
  }
}

async fn send_on_device(link: &mut Link, chat: &str, message: &Message) -> Result<Action> {
  let me = link.me();
  let chat = refs::chat(chat, &link.archive, &me)?;
  let to = jid(&chat)?;
  let quote = match &message.reply_to {
    Some(r) => {
      let (quoted_chat, quoted_id) = refs::message(r)?;
      let original = link.archive.find_message(&quoted_chat, &quoted_id);
      Some(quote_context(&quoted_id, original.as_ref(), &chat))
    }
    None => None,
  };
  let mut sent = Vec::new();
  let mut caption = Some(message.text.clone()).filter(|t| !t.is_empty());
  for (i, path) in message.files.iter().enumerate() {
    let mut msg = media_message(link, path, caption.take()).await?;
    if i == 0
      && let Some(q) = &quote
    {
      use whatsapp_rust::prelude::MessageExt;
      msg.set_context_info(q.clone());
    }
    let result = link.client.send_message(to.clone(), msg).await;
    sent.push((result.map_err(send_error)?.message_id, None));
  }
  if let Some(text) = caption {
    use whatsapp_rust::prelude::MessageBuilderExt;
    let msg = match &quote {
      Some(q) => wa::Message::text_with_context(text.clone(), q.clone()),
      None => wa::Message::text(text.clone()),
    };
    let result = link.client.send_message(to.clone(), msg).await;
    sent.push((result.map_err(send_error)?.message_id, Some(text)));
  }
  let quoted = message
    .reply_to
    .as_ref()
    .and_then(|r| r.rsplit_once('/').map(|(_, id)| id.to_owned()));
  let last = archive_sent(link, &chat, sent, quoted);
  let name = link
    .archive
    .name(&chat)
    .map(str::to_owned)
    .unwrap_or_else(|| parse::phone(&chat));
  Ok(Action::done("send", name).with_id(last))
}

/// Keep what was sent in the archive; the id of the last message.
fn archive_sent(
  link: &mut Link,
  chat: &str,
  sent: Vec<(String, Option<String>)>,
  quoted: Option<String>,
) -> String {
  let me = link.me();
  let mut last = String::new();
  for (id, text) in sent {
    last = atp_core::chat::message_id(chat, &id);
    let kind = if text.is_some() { "text" } else { "file" };
    link.archive.put(Record {
      id,
      chat: chat.to_owned(),
      sender: me.clone(),
      from_me: true,
      ts: jiff::Timestamp::now().as_second(),
      kind: kind.into(),
      text,
      quoted: quoted.clone(),
      ..Record::default()
    });
  }
  last
}

fn quote_context(id: &str, original: Option<&Record>, chat: &str) -> wa::ContextInfo {
  let mut ctx = wa::ContextInfo {
    stanza_id: Some(id.to_owned()),
    ..wa::ContextInfo::default()
  };
  if let Some(r) = original {
    if kind_of(chat) == "group" || !r.from_me {
      ctx.participant = Some(r.sender.clone());
    }
    if let Some(text) = &r.text {
      use whatsapp_rust::prelude::MessageBuilderExt;
      ctx.quoted_message = MessageField::some(wa::Message::text(text.clone()));
    }
  }
  ctx
}

fn send_error(e: impl std::fmt::Display) -> Error {
  Error::upstream(format!("WhatsApp did not take the message: {e}"))
}

/// Upload a file and wrap it as an image, video, audio or document message.
async fn media_message(link: &Link, path: &Path, caption: Option<String>) -> Result<wa::Message> {
  let file = files::read(path).await?;
  let kind = match file.mime.split('/').next() {
    Some("image") if file.mime != "image/gif" => MediaType::Image,
    Some("video") => MediaType::Video,
    Some("audio") => MediaType::Audio,
    _ => MediaType::Document,
  };
  let size = (kind == MediaType::Image).then(|| files::image_size(&file));
  let upload = link
    .client
    .upload(file.data, kind, UploadOptions::default())
    .await
    .map_err(|e| Error::upstream(format!("upload of {} failed: {e}", path.display())))?;
  let media = Uploaded {
    upload,
    mime: file.mime.to_owned(),
  };
  Ok(match kind {
    MediaType::Image => media.image(caption, size),
    MediaType::Video => media.video(caption),
    MediaType::Audio => media.audio(),
    _ => media.document(caption, file.name),
  })
}

/// An uploaded file, wrapped into the message type that carries it.
struct Uploaded {
  upload: whatsapp_rust::upload::UploadResponse,
  mime: String,
}

macro_rules! fill {
  ($x:expr, $u:expr) => {{
    let up = &$u.upload;
    $x.url = Some(up.url.clone());
    $x.direct_path = Some(up.direct_path.clone());
    $x.media_key = Some(up.media_key.to_vec());
    $x.file_sha256 = Some(up.file_sha256.to_vec());
    $x.file_enc_sha256 = Some(up.file_enc_sha256.to_vec());
    $x.file_length = Some(up.file_length);
    $x.mimetype = Some($u.mime.clone());
    $x.media_key_timestamp = Some(up.media_key_timestamp);
  }};
}

impl Uploaded {
  fn image(&self, caption: Option<String>, size: Option<(u32, u32)>) -> wa::Message {
    let mut x = wa::message::ImageMessage::default();
    fill!(x, self);
    x.caption = caption;
    x.width = size.map(|s| s.0);
    x.height = size.map(|s| s.1);
    wa::Message {
      image_message: MessageField::some(x),
      ..wa::Message::default()
    }
  }

  fn video(&self, caption: Option<String>) -> wa::Message {
    let mut x = wa::message::VideoMessage::default();
    fill!(x, self);
    x.caption = caption;
    wa::Message {
      video_message: MessageField::some(x),
      ..wa::Message::default()
    }
  }

  fn audio(&self) -> wa::Message {
    let mut x = wa::message::AudioMessage::default();
    fill!(x, self);
    wa::Message {
      audio_message: MessageField::some(x),
      ..wa::Message::default()
    }
  }

  fn document(&self, caption: Option<String>, file_name: String) -> wa::Message {
    let mut x = wa::message::DocumentMessage::default();
    fill!(x, self);
    x.caption = caption;
    x.title = Some(file_name.clone());
    x.file_name = Some(file_name);
    wa::Message {
      document_message: MessageField::some(x),
      ..wa::Message::default()
    }
  }
}

/// The key of an archived message, for reactions.
fn key(link: &mut Link, post: &str) -> Result<(String, wa::MessageKey, Option<Record>)> {
  let (chat, id) = refs::message(post)?;
  let record = link.archive.find_message(&chat, &id);
  let from_me = record.as_ref().is_some_and(|r| r.from_me);
  let participant = record
    .as_ref()
    .filter(|r| !r.from_me && kind_of(&chat) == "group")
    .map(|r| r.sender.clone());
  let key = wa::MessageKey {
    remote_jid: Some(chat.clone()),
    from_me: Some(from_me),
    id: Some(id),
    participant,
  };
  Ok((chat, key, record))
}

/// On the linked device, or through the Cloud API.
pub async fn react(api: &Api, post: &str, emoji: &str, undo: bool) -> Result<Action> {
  match api.mode() {
    Mode::Personal => {
      let device = async |link: &mut Link| react_on_device(link, post, emoji, undo).await;
      api.personal.with(&api.ctx, device).await
    }
    Mode::Cloud(cloud) => cloud.react(&api.ctx, post, emoji, undo).await,
    Mode::None => Err(not_logged_in()),
  }
}

async fn react_on_device(link: &mut Link, post: &str, emoji: &str, undo: bool) -> Result<Action> {
  let (chat, key, _) = key(link, post)?;
  let emoji = if undo { "" } else { emoji };
  link
    .client
    .send_reaction(jid(&chat)?, key, emoji)
    .await
    .map_err(send_error)?;
  let me = link.me();
  let (_, id) = refs::message(post)?;
  link.archive.update(&chat, &id, |r| {
    if emoji.is_empty() {
      r.reactions.remove(&me);
    } else {
      r.reactions.insert(me.clone(), emoji.to_owned());
    }
  });
  Ok(Action::done(if undo { "unreact" } else { "react" }, post))
}

pub async fn edit(link: &mut Link, post: &str, text: &str) -> Result<Action> {
  use whatsapp_rust::prelude::MessageBuilderExt;
  let (chat, id) = refs::message(post)?;
  if let Some(r) = link.archive.find_message(&chat, &id)
    && !r.from_me
  {
    return Err(Error::input("only your own messages can be edited"));
  }
  link
    .client
    .edit_message(jid(&chat)?, id.clone(), wa::Message::text(text))
    .await
    .map_err(send_error)?;
  link.archive.update(&chat, &id, |r| {
    r.text = Some(text.to_owned());
    r.edited = true;
  });
  Ok(Action::done("edit", post))
}

pub async fn delete(link: &mut Link, post: &str) -> Result<Action> {
  let (chat, id) = refs::message(post)?;
  let record = link.archive.find_message(&chat, &id);
  let revoke = match &record {
    Some(r) if !r.from_me && kind_of(&chat) == "group" => RevokeType::Admin {
      original_sender: jid(&r.sender)?,
    },
    Some(r) if !r.from_me => {
      return Err(Error::input(
        "only your own messages (or, as a group admin, members' messages) can be deleted for everyone",
      ));
    }
    _ => RevokeType::Sender,
  };
  link
    .client
    .revoke_message(jid(&chat)?, id.clone(), revoke)
    .await
    .map_err(send_error)?;
  link.archive.update(&chat, &id, |r| {
    r.deleted = true;
    r.text = None;
    r.media = None;
  });
  Ok(Action::done("delete", post))
}

/// Cloud API: a message; personal: a chat's unread messages.
pub async fn mark_read(api: &Api, target: &str) -> Result<Action> {
  let ctx = &api.ctx;
  if let Mode::Cloud(cloud) = api.mode() {
    return cloud.mark_read(ctx, &ctx.post_ref(target)?).await;
  }
  let chat = ctx.collection_ref(target)?;
  api
    .on_device("mark-read", async |link| mark_chat_read(link, &chat).await)
    .await
}

/// Send read receipts for a chat's unread messages.
async fn mark_chat_read(link: &mut Link, chat: &str) -> Result<Action> {
  let me = link.me();
  let chat = refs::chat(chat, &link.archive, &me)?;
  let unread = link
    .archive
    .chat(&chat)
    .map(|c| c.unread)
    .unwrap_or(0)
    .max(1) as usize;
  let recent: Vec<Record> = link
    .archive
    .messages(&chat)
    .iter()
    .rev()
    .filter(|r| !r.from_me)
    .take(unread.min(100))
    .cloned()
    .collect();
  let mut by_sender: BTreeMap<String, Vec<String>> = BTreeMap::new();
  for r in &recent {
    by_sender
      .entry(r.sender.clone())
      .or_default()
      .push(r.id.clone());
  }
  let group = kind_of(&chat) == "group";
  for (sender, ids) in &by_sender {
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    let sender = if group { Some(jid(sender)?) } else { None };
    link
      .client
      .mark_as_read(&jid(&chat)?, sender.as_ref(), &ids)
      .await
      .map_err(|e| Error::upstream(format!("read receipts failed: {e}")))?;
  }
  link.archive.chat_mut(&chat).unread = 0;
  Ok(Action::done("mark-read", chat).with_message(format!("{} messages", recent.len())))
}
