//! The personal account, as a linked device (WhatsApp Web's multi-device
//! protocol, through whatsapp-rust): QR-code or pairing-code login, then
//! chats, messages, sending and live events.
//!
//! One connection serves every operation of a command; each operation ends
//! with the device state and the archive saved, and dropping the platform
//! saves them once more.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use social_core::output::{Stream, note};
use social_core::{
  Action, Collection, Ctx, Error, Message, Page, PageReq, Post, QrStatus, QrTicket, Result, User,
  Value,
};
use tokio::sync::Mutex;
use whatsapp_rust::download::{DownloadParams, MediaDownloader};
use whatsapp_rust::pair_code::PairCodeOptions;
use whatsapp_rust::prelude::{Event, MessageField, wa};
use whatsapp_rust::upload::UploadOptions;
use whatsapp_rust::wacore::download::MediaType;
use whatsapp_rust::{Jid, RevokeType, TokioRuntime};

use crate::archive::{Archive, Record, bare, kind_of};
use crate::convert;
use crate::link::Link;
use crate::refs;
use crate::store::FileBackend;

/// Session extra marking a linked device (the name of its key file).
pub const DEVICE: &str = "device";
const DEVICE_FILE: &str = "device.json";

pub struct Personal {
  device: PathBuf,
  archive_dir: PathBuf,
  cache_dir: PathBuf,
  link: Mutex<Option<Link>>,
  pending: Mutex<Option<Link>>,
  proxy: Option<String>,
}

impl Personal {
  pub fn new(ctx: &Ctx) -> Self {
    let dir = ctx.store.config_dir();
    Self {
      device: dir.join(DEVICE_FILE),
      archive_dir: dir.join("archive"),
      cache_dir: ctx.store.cache_dir().to_owned(),
      link: Mutex::new(None),
      pending: Mutex::new(None),
      proxy: ctx.http.proxy(),
    }
  }

  /// Whether a linked device is saved (not whether it still works).
  pub fn linked(&self, ctx: &Ctx) -> bool {
    ctx.extra(DEVICE).is_some_and(|d| !d.is_empty()) && FileBackend::paired(&self.device)
  }

  /// Run `f` on the connection, connecting first; saves afterwards.
  pub async fn with<T>(&self, ctx: &Ctx, f: impl AsyncFnOnce(&mut Link) -> Result<T>) -> Result<T> {
    if !self.linked(ctx) {
      return Err(not_linked());
    }
    let mut guard = self.link.lock().await;
    if guard.is_none() {
      let backend = Arc::new(FileBackend::open(&self.device)?);
      let archive = Archive::open(&self.archive_dir);
      *guard = Some(Link::connect(backend, archive, self.proxy.as_deref()).await?);
    }
    let link = guard.as_mut().expect("connected above");
    let out = f(link).await;
    link.save().await?;
    out
  }

  // ── login ────────────────────────────────────────────────────────────

  /// Start linking a new device: a fresh key file, the first QR code.
  pub async fn qr_start(&self) -> Result<QrTicket> {
    let mut link = self.fresh(None).await?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let code = loop {
      let wait = deadline.saturating_duration_since(Instant::now());
      let Some(event) = (!wait.is_zero()).then_some(()).and(link.next(wait).await) else {
        link.close().await;
        return Err(Error::network("WhatsApp sent no QR code in time"));
      };
      match &*event {
        Event::PairingQrCode(q) => break q.code.clone(),
        other => {
          if let Some(e) = pair_failure(other) {
            link.close().await;
            return Err(e);
          }
        }
      }
    };
    *self.pending.lock().await = Some(link);
    Ok(QrTicket {
      url: code,
      token: "whatsapp".into(),
      ..QrTicket::default()
    })
  }

  pub async fn qr_poll(&self, ctx: &Ctx) -> Result<QrStatus> {
    let mut guard = self.pending.lock().await;
    let Some(link) = guard.as_mut() else {
      return Err(Error::internal("no QR login in progress"));
    };
    let mut status = QrStatus::Waiting;
    while let Some(event) = link.next(Duration::from_millis(1500)).await {
      match &*event {
        Event::PairingQrCode(q) => {
          let path = social_core::qr::show(&q.code, &self.cache_dir)?;
          note(&format!(
            "New QR code (the previous one expired). QR image: {}",
            path.display()
          ));
        }
        Event::PairSuccess(_) => status = QrStatus::Scanned,
        Event::PairingQrCodesExhausted(_) => return Ok(QrStatus::Expired),
        Event::Connected(_) => {
          let link = guard.take().expect("checked above");
          self.finish(ctx, link).await?;
          return Ok(QrStatus::Confirmed);
        }
        other => {
          if let Some(e) = pair_failure(other) {
            return Err(e);
          }
          link.absorb(other);
        }
      }
    }
    Ok(status)
  }

  /// Link with a code typed on the phone (Linked devices → Link with phone
  /// number) instead of scanning.
  pub async fn pair_code(&self, ctx: &Ctx, phone: &str) -> Result<User> {
    let Some(number) = refs::phone(phone) else {
      return Err(Error::input(
        "give the account's phone number with its country code, e.g. +44 7700 900123",
      ));
    };
    let options = PairCodeOptions {
      phone_number: number,
      ..PairCodeOptions::default()
    };
    let mut link = self.fresh(Some(options)).await?;
    let deadline = Instant::now() + Duration::from_secs(240);
    loop {
      let wait = deadline.saturating_duration_since(Instant::now());
      let Some(event) = (!wait.is_zero()).then_some(()).and(link.next(wait).await) else {
        link.close().await;
        return Err(Error::auth(
          "the pairing code was not entered in time; run it again",
        ));
      };
      match &*event {
        Event::PairingCode(p) => note(&format!(
          "On the phone: WhatsApp → Settings → Linked devices → Link a device → Link with phone number instead, and enter {}",
          p.code
        )),
        Event::PairingCodeError(e) => {
          link.close().await;
          return Err(Error::auth(format!(
            "WhatsApp refused the pairing code: {e:?}"
          )));
        }
        Event::PairSuccess(_) => note("Linked; waiting for the connection…"),
        Event::Connected(_) => {
          self.finish(ctx, link).await?;
          return self.with(ctx, async |l| Ok(whoami(l))).await;
        }
        other => {
          if let Some(e) = pair_failure(other) {
            link.close().await;
            return Err(e);
          }
          link.absorb(other);
        }
      }
    }
  }

  /// A client on a new, empty key file (a new linked device).
  async fn fresh(&self, pair_code: Option<PairCodeOptions>) -> Result<Link> {
    if let Some(old) = self.link.lock().await.take() {
      old.close().await;
    }
    let _ = std::fs::remove_file(&self.device);
    let backend = Arc::new(FileBackend::open(&self.device)?);
    let archive = Archive::open(&self.archive_dir);
    Link::start(backend, archive, pair_code, self.proxy.as_deref()).await
  }

  /// After pairing: take in the phone's history, remember the device.
  async fn finish(&self, ctx: &Ctx, mut link: Link) -> Result<()> {
    note("Linked. Receiving recent chats from the phone (up to two minutes)…");
    let start = Instant::now();
    let mut last = Instant::now();
    while start.elapsed() < Duration::from_secs(120) && last.elapsed() < Duration::from_secs(12) {
      if let Some(event) = link.next(Duration::from_secs(2)).await {
        if matches!(&*event, Event::HistorySync(_)) {
          last = Instant::now();
        }
        link.absorb(&event);
      }
    }
    link.save().await?;
    ctx.set_extra(DEVICE, DEVICE_FILE);
    *self.link.lock().await = Some(link);
    Ok(())
  }

  /// Log the device out on WhatsApp and delete its keys and archive.
  pub async fn unlink(&self, ctx: &Ctx) -> Result<Action> {
    let me = self
      .with(ctx, async |l| {
        let me = l.me();
        l.client.logout().await;
        Ok(me)
      })
      .await
      .unwrap_or_default();
    if let Some(link) = self.link.lock().await.take() {
      link.close().await;
    }
    let _ = std::fs::remove_file(&self.device);
    let _ = std::fs::remove_dir_all(&self.archive_dir);
    ctx.set_extra(DEVICE, "");
    Ok(
      Action::done("unlink", convert::phone(&me))
        .with_message("the linked device was removed and its keys and archive deleted"),
    )
  }

  // ── media ────────────────────────────────────────────────────────────

  /// Download an archived message's media (no connection needed).
  pub async fn save_media(&self, url: &str, dir: &Path, name: &str) -> Result<PathBuf> {
    let reference = url.strip_prefix("wa-media:").unwrap_or(url);
    let (chat, id) = refs::message(reference)?;
    let mut archive = Archive::open(&self.archive_dir);
    let Some(m) = archive.find_message(&chat, &id).and_then(|r| r.media) else {
      return Err(Error::not_found("this message has no archived media"));
    };
    let decode = |s: &str| {
      use base64::Engine;
      base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|_| Error::internal("corrupt media key in the archive"))
    };
    let kind = match m.kind.as_str() {
      "image" => MediaType::Image,
      "video" | "gif" => MediaType::Video,
      "audio" | "voice" => MediaType::Audio,
      "sticker" => MediaType::Sticker,
      _ => MediaType::Document,
    };
    let params = DownloadParams::encrypted(
      m.direct_path.clone(),
      &decode(&m.key)?,
      &decode(&m.sha256)?,
      &decode(&m.enc_sha256)?,
      m.length,
      kind,
    );
    let downloader = MediaDownloader::with_default_hosts(
      Arc::new(crate::proxy::http_client(self.proxy.as_deref())),
      Arc::new(TokioRuntime),
    );
    let data = downloader.download(&params).await.map_err(|e| {
      Error::upstream(format!("media download failed: {e}"))
        .with_hint("old media expire on WhatsApp's servers after a while")
    })?;
    let ext = extension(&m.kind, m.mime.as_deref(), m.file_name.as_deref());
    let file_name = match (&m.file_name, m.kind.as_str()) {
      (Some(f), "document") => f.clone(),
      _ => format!("{name}.{ext}"),
    };
    let path = dir.join(file_name);
    std::fs::create_dir_all(dir)?;
    std::fs::write(&path, data)?;
    Ok(path)
  }
}

impl Drop for Personal {
  fn drop(&mut self) {
    if let Ok(mut guard) = self.link.try_lock()
      && let Some(link) = guard.as_mut()
    {
      link.save_blocking();
    }
  }
}

fn not_linked() -> Error {
  Error::auth("no WhatsApp account is linked").with_hint(
    "run `social whatsapp login` (QR code) or `social whatsapp login-code +NUMBER`; for the Business Cloud API, `social whatsapp login --token TOKEN:PHONE_NUMBER_ID`",
  )
}

fn pair_failure(event: &Event) -> Option<Error> {
  match event {
    Event::PairError(e) => Some(Error::auth(format!("linking failed: {e:?}"))),
    Event::QrScannedWithoutMultidevice(_) => Some(Error::auth(
      "the phone's WhatsApp is too old for linked devices; update it",
    )),
    Event::ClientOutdated(_) => Some(Error::upstream(
      "WhatsApp rejects this client version; update social-cli",
    )),
    Event::TemporaryBan(b) => Some(Error::auth(format!(
      "WhatsApp temporarily banned this account: {b:?}"
    ))),
    _ => None,
  }
}

fn extension(kind: &str, mime: Option<&str>, file_name: Option<&str>) -> String {
  if let Some(ext) = file_name.and_then(|f| f.rsplit_once('.')).map(|(_, e)| e) {
    return ext.to_owned();
  }
  let from_mime = mime
    .and_then(|m| m.split(';').next())
    .and_then(|m| m.split('/').nth(1))
    .map(|e| match e {
      "jpeg" => "jpg",
      "ogg" => "ogg",
      "mpeg" => "mp3",
      other => other,
    });
  from_mime
    .unwrap_or(match kind {
      "image" => "jpg",
      "video" | "gif" => "mp4",
      "voice" => "ogg",
      "audio" => "mp3",
      "sticker" => "webp",
      _ => "bin",
    })
    .to_owned()
}

// ── operations on a connection ─────────────────────────────────────────

pub fn whoami(l: &Link) -> User {
  let me = l.me();
  User {
    id: me.clone(),
    name: Some(l.client.push_name())
      .filter(|n| !n.is_empty())
      .unwrap_or_else(|| convert::phone(&me)),
    handle: Some(convert::phone(&me)),
    ..User::default()
  }
}

fn offset(req: &PageReq) -> usize {
  req.number_or(0) as usize
}

fn paged<T>(items: Vec<T>, start: usize, size: usize, more: bool) -> Page<T> {
  let next = (more).then(|| (start + items.len()).to_string());
  let _ = size;
  Page::new(items, next)
}

/// Group names come from the server; everything else from the archive.
pub async fn chats(l: &mut Link, page: &PageReq) -> Result<Page<Collection>> {
  if page.cursor.is_none() {
    match l.client.groups().get_participating().await {
      Ok(groups) => {
        for (jid, meta) in groups {
          let entry = l.archive.chat_mut(&jid.to_string());
          entry.kind = "group".into();
          if !meta.subject.is_empty() {
            entry.name = Some(meta.subject.clone());
          }
          if let Some(d) = &meta.description {
            entry.description = Some(d.clone());
          }
        }
      }
      Err(e) => tracing::warn!("listing groups failed: {e}"),
    }
  }
  let start = offset(page);
  let size = page.size_within(100);
  let all: Vec<Collection> = l
    .archive
    .chats()
    .into_iter()
    .map(|c| convert::collection(c, &l.archive))
    .collect();
  let more = all.len() > start + size;
  let items = all.into_iter().skip(start).take(size).collect();
  Ok(paged(items, start, size, more))
}

/// Newest first; past the archive's start, asks the phone for older ones.
pub async fn messages(l: &mut Link, chat: &str, page: &PageReq) -> Result<Page<Post>> {
  let me = l.me();
  let chat = refs::chat(chat, &l.archive, &me)?;
  let start = offset(page);
  let size = page.size_within(100);
  if start > 0 && start >= l.archive.messages(&chat).len() {
    let added = l.fetch_older(&chat, size.min(50) as i32).await.unwrap_or(0);
    if added == 0 {
      return Ok(Page::new(Vec::new(), None));
    }
  }
  let records: Vec<Record> = l.archive.messages(&chat).iter().rev().cloned().collect();
  if records.is_empty() && start == 0 {
    return Err(
      Error::not_found("nothing of this chat is archived yet").with_hint(
        "WhatsApp keeps no history on its servers: the archive fills from the phone's history sync at login and from messages that arrive while social-cli is connected (`listen`)",
      ),
    );
  }
  let page_items: Vec<Post> = records
    .iter()
    .skip(start)
    .take(size)
    .map(|r| convert::post(r, &l.archive))
    .collect();
  // Past the archive the next page asks the phone for older messages.
  Ok(paged(page_items, start, size, true))
}

pub fn read(l: &mut Link, post: &str) -> Result<Post> {
  let (chat, id) = refs::message(post)?;
  let record = l
    .archive
    .find_message(&chat, &id)
    .ok_or_else(|| Error::not_found("this message is not in the archive"))?;
  Ok(convert::post(&record, &l.archive))
}

pub fn search(l: &mut Link, words: &str, page: &PageReq) -> Result<Page<Post>> {
  let q = words.to_lowercase();
  let hits = l.archive.search(|r| {
    r.text
      .as_deref()
      .is_some_and(|t| t.to_lowercase().contains(&q))
  });
  let start = offset(page);
  let size = page.size_within(100);
  let more = hits.len() > start + size;
  let items = hits
    .iter()
    .skip(start)
    .take(size)
    .map(|r| convert::post(r, &l.archive))
    .collect();
  Ok(paged(items, start, size, more))
}

pub fn unread(l: &Link) -> BTreeMap<String, u64> {
  let mut out = BTreeMap::new();
  let mut total = 0;
  for c in l.archive.chats() {
    if c.unread > 0 {
      total += u64::from(c.unread);
      let name = l
        .archive
        .name(&c.jid)
        .map(str::to_owned)
        .unwrap_or_else(|| convert::phone(&c.jid));
      out.insert(name, u64::from(c.unread));
    }
  }
  out.insert("total".into(), total);
  out
}

fn jid(s: &str) -> Result<Jid> {
  s.parse()
    .map_err(|_| Error::input(format!("not a WhatsApp id: {s}")))
}

pub async fn send(l: &mut Link, chat: &str, m: &Message) -> Result<Action> {
  let me = l.me();
  let chat = refs::chat(chat, &l.archive, &me)?;
  let to = jid(&chat)?;
  let quote = match &m.reply_to {
    Some(r) => {
      let (qchat, qid) = refs::message(r)?;
      let original = l.archive.find_message(&qchat, &qid);
      Some(quote_context(&qid, original.as_ref(), &chat))
    }
    None => None,
  };
  let mut sent = Vec::new();
  let mut caption = Some(m.text.clone()).filter(|t| !t.is_empty());
  for (i, path) in m.files.iter().enumerate() {
    let mut msg = media_message(l, path, caption.take()).await?;
    if i == 0
      && let Some(q) = &quote
    {
      use whatsapp_rust::prelude::MessageExt;
      msg.set_context_info(q.clone());
    }
    sent.push((
      l.client
        .send_message(to.clone(), msg)
        .await
        .map_err(send_error)?,
      None,
    ));
  }
  if let Some(text) = caption {
    let msg = match &quote {
      Some(q) => {
        use whatsapp_rust::prelude::MessageBuilderExt;
        wa::Message::text_with_context(text.clone(), q.clone())
      }
      None => {
        use whatsapp_rust::prelude::MessageBuilderExt;
        wa::Message::text(text.clone())
      }
    };
    sent.push((
      l.client
        .send_message(to.clone(), msg)
        .await
        .map_err(send_error)?,
      Some(text),
    ));
  }
  let mut last = String::new();
  for (result, text) in sent {
    last = format!("{chat}/{}", result.message_id);
    l.archive.put(Record {
      id: result.message_id.clone(),
      chat: chat.clone(),
      sender: me.clone(),
      from_me: true,
      ts: jiff::Timestamp::now().as_second(),
      kind: if text.is_some() {
        "text".into()
      } else {
        "file".into()
      },
      text,
      quoted: m
        .reply_to
        .as_ref()
        .and_then(|r| r.rsplit_once('/').map(|(_, id)| id.to_owned())),
      ..Record::default()
    });
  }
  let name = l
    .archive
    .name(&chat)
    .map(str::to_owned)
    .unwrap_or_else(|| convert::phone(&chat));
  Ok(Action::done("send", name).with_id(last))
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
async fn media_message(l: &Link, path: &Path, caption: Option<String>) -> Result<wa::Message> {
  let data = std::fs::read(path)
    .map_err(|e| Error::input(format!("cannot read {}: {e}", path.display())))?;
  let file_name = path
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or("file")
    .to_owned();
  let mime = mime_of(path, &data);
  let kind = match mime.split('/').next() {
    Some("image") if mime != "image/gif" => MediaType::Image,
    Some("video") => MediaType::Video,
    Some("audio") => MediaType::Audio,
    _ => MediaType::Document,
  };
  let (width, height) = if kind == MediaType::Image {
    let image = social_core::file::Image {
      data: data.clone(),
      mime: "image/jpeg",
      name: file_name.clone(),
    };
    let (w, h) = image.size();
    (Some(w as u32), Some(h as u32))
  } else {
    (None, None)
  };
  let up = l
    .client
    .upload(data, kind, UploadOptions::default())
    .await
    .map_err(|e| Error::upstream(format!("upload of {} failed: {e}", path.display())))?;
  let mut msg = wa::Message::default();
  macro_rules! fill {
    ($x:expr) => {{
      $x.url = Some(up.url.clone());
      $x.direct_path = Some(up.direct_path.clone());
      $x.media_key = Some(up.media_key.to_vec());
      $x.file_sha256 = Some(up.file_sha256.to_vec());
      $x.file_enc_sha256 = Some(up.file_enc_sha256.to_vec());
      $x.file_length = Some(up.file_length);
      $x.mimetype = Some(mime.clone());
      $x.media_key_timestamp = Some(up.media_key_timestamp);
    }};
  }
  match kind {
    MediaType::Image => {
      let mut x = wa::message::ImageMessage::default();
      fill!(x);
      x.caption = caption;
      x.width = width;
      x.height = height;
      msg.image_message = MessageField::some(x);
    }
    MediaType::Video => {
      let mut x = wa::message::VideoMessage::default();
      fill!(x);
      x.caption = caption;
      msg.video_message = MessageField::some(x);
    }
    MediaType::Audio => {
      let mut x = wa::message::AudioMessage::default();
      fill!(x);
      msg.audio_message = MessageField::some(x);
    }
    _ => {
      let mut x = wa::message::DocumentMessage::default();
      fill!(x);
      x.caption = caption;
      x.title = Some(file_name.clone());
      x.file_name = Some(file_name);
      msg.document_message = MessageField::some(x);
    }
  }
  Ok(msg)
}

pub(crate) fn mime_of(path: &Path, data: &[u8]) -> String {
  if let Some(ext) = social_core::file::sniff(data) {
    return match ext {
      "jpg" => "image/jpeg".into(),
      other => format!("image/{other}"),
    };
  }
  let ext = path
    .extension()
    .and_then(|e| e.to_str())
    .unwrap_or_default()
    .to_ascii_lowercase();
  match ext.as_str() {
    "mp4" | "m4v" => "video/mp4",
    "mov" => "video/quicktime",
    "3gp" => "video/3gpp",
    "mp3" => "audio/mpeg",
    "m4a" | "aac" => "audio/aac",
    "ogg" | "opus" => "audio/ogg; codecs=opus",
    "wav" => "audio/wav",
    "pdf" => "application/pdf",
    "txt" => "text/plain",
    "zip" => "application/zip",
    "doc" => "application/msword",
    "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "xls" => "application/vnd.ms-excel",
    "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "ppt" => "application/vnd.ms-powerpoint",
    "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    _ => "application/octet-stream",
  }
  .to_owned()
}

/// The key of an archived message, for reactions.
fn key(l: &mut Link, post: &str) -> Result<(String, wa::MessageKey, Option<Record>)> {
  let (chat, id) = refs::message(post)?;
  let record = l.archive.find_message(&chat, &id);
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

pub async fn react(l: &mut Link, post: &str, emoji: &str, undo: bool) -> Result<Action> {
  let (chat, key, _) = key(l, post)?;
  let emoji = if undo { "" } else { emoji };
  l.client
    .send_reaction(jid(&chat)?, key, emoji)
    .await
    .map_err(send_error)?;
  let me = l.me();
  let (_, id) = refs::message(post)?;
  l.archive.update(&chat, &id, |r| {
    if emoji.is_empty() {
      r.reactions.remove(&me);
    } else {
      r.reactions.insert(me.clone(), emoji.to_owned());
    }
  });
  Ok(Action::done(if undo { "unreact" } else { "react" }, post))
}

pub async fn edit(l: &mut Link, post: &str, text: &str) -> Result<Action> {
  use whatsapp_rust::prelude::MessageBuilderExt;
  let (chat, id) = refs::message(post)?;
  if let Some(r) = l.archive.find_message(&chat, &id)
    && !r.from_me
  {
    return Err(Error::input("only your own messages can be edited"));
  }
  l.client
    .edit_message(jid(&chat)?, id.clone(), wa::Message::text(text))
    .await
    .map_err(send_error)?;
  l.archive.update(&chat, &id, |r| {
    r.text = Some(text.to_owned());
    r.edited = true;
  });
  Ok(Action::done("edit", post))
}

pub async fn delete(l: &mut Link, post: &str) -> Result<Action> {
  let (chat, id) = refs::message(post)?;
  let record = l.archive.find_message(&chat, &id);
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
  l.client
    .revoke_message(jid(&chat)?, id.clone(), revoke)
    .await
    .map_err(send_error)?;
  l.archive.update(&chat, &id, |r| {
    r.deleted = true;
    r.text = None;
    r.media = None;
  });
  Ok(Action::done("delete", post))
}

pub async fn members(l: &mut Link, chat: &str, page: &PageReq) -> Result<Page<User>> {
  let me = l.me();
  let chat = refs::chat(chat, &l.archive, &me)?;
  if kind_of(&chat) != "group" {
    return Err(Error::input("members lists the members of a group"));
  }
  let meta = l
    .client
    .groups()
    .get_metadata(&jid(&chat)?)
    .await
    .map_err(|e| Error::upstream(format!("group info failed: {e}")))?;
  let users: Vec<User> = meta
    .participants
    .iter()
    .map(|p| {
      let id = p.phone_number.as_ref().unwrap_or(&p.jid).to_string();
      let mut u = User {
        name: l
          .archive
          .name(&id)
          .map(str::to_owned)
          .unwrap_or_else(|| convert::phone(&id)),
        handle: Some(convert::phone(&id)).filter(|h| h.starts_with('+')),
        id,
        ..User::default()
      };
      u.extra.insert(
        "role".into(),
        format!("{:?}", p.participant_type).to_lowercase().into(),
      );
      u
    })
    .collect();
  let start = offset(page);
  let size = page.size_within(1000);
  let more = users.len() > start + size;
  Ok(paged(
    users.into_iter().skip(start).take(size).collect(),
    start,
    size,
    more,
  ))
}

pub async fn user(l: &mut Link, arg: &str) -> Result<User> {
  let me = l.me();
  let id = refs::chat(arg, &l.archive, &me)?;
  let j = jid(&id)?;
  if kind_of(&id) == "group" {
    let meta = l
      .client
      .groups()
      .get_metadata(&j)
      .await
      .map_err(|e| Error::upstream(format!("group info failed: {e}")))?;
    let mut u = User {
      id: id.clone(),
      name: meta.subject.clone(),
      bio: meta.description.clone(),
      ..User::default()
    };
    u.stats.followers = Some(meta.participants.len() as u64);
    return Ok(u);
  }
  let registered = l
    .client
    .contacts()
    .is_on_whatsapp(std::slice::from_ref(&j))
    .await
    .map_err(|e| Error::upstream(format!("lookup failed: {e}")))?
    .first()
    .is_some_and(|r| r.is_registered);
  if !registered {
    return Err(Error::not_found(format!(
      "{} is not on WhatsApp",
      convert::phone(&id)
    )));
  }
  let info = l
    .client
    .contacts()
    .get_user_info(std::slice::from_ref(&j))
    .await
    .ok();
  let info = info.as_ref().and_then(|m| m.values().next());
  let picture = l
    .client
    .contacts()
    .get_profile_picture(&j, false)
    .await
    .ok()
    .flatten();
  let mut u = User {
    id: bare(&id),
    name: l
      .archive
      .name(&id)
      .map(str::to_owned)
      .unwrap_or_else(|| convert::phone(&id)),
    handle: Some(convert::phone(&id)),
    bio: info.and_then(|i| i.status.clone()),
    avatar: picture.map(|p| p.url),
    ..User::default()
  };
  if let Some(i) = info {
    u.extra.insert("business".into(), i.is_business.into());
    u.extra
      .insert("devices".into(), (i.devices.len() as u64).into());
  }
  Ok(u)
}

/// Join a group by invite link, or leave one.
pub async fn follow(l: &mut Link, arg: &str, undo: bool) -> Result<Action> {
  if undo {
    let me = l.me();
    let chat = refs::chat(arg, &l.archive, &me)?;
    l.client
      .groups()
      .leave(jid(&chat)?)
      .await
      .map_err(|e| Error::upstream(format!("leaving failed: {e}")))?;
    return Ok(Action::done("leave", chat));
  }
  let joined = l
    .client
    .groups()
    .join_with_invite_code(arg)
    .await
    .map_err(|e| {
      Error::upstream(format!("joining failed: {e}"))
        .with_hint("give a chat.whatsapp.com invite link")
    })?;
  let group = joined.group_jid().to_string();
  Ok(Action::done("join", group.clone()).with_id(group))
}

/// Send read receipts for a chat's unread messages.
pub async fn mark_read(l: &mut Link, chat: &str) -> Result<Action> {
  let me = l.me();
  let chat = refs::chat(chat, &l.archive, &me)?;
  let unread = l.archive.chat(&chat).map(|c| c.unread).unwrap_or(0).max(1) as usize;
  let recent: Vec<Record> = l
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
    l.client
      .mark_as_read(&jid(&chat)?, sender.as_ref(), &ids)
      .await
      .map_err(|e| Error::upstream(format!("read receipts failed: {e}")))?;
  }
  l.archive.chat_mut(&chat).unread = 0;
  Ok(Action::done("mark-read", chat).with_message(format!("{} messages", recent.len())))
}

pub async fn groups(l: &mut Link) -> Result<Page<Collection>> {
  let groups = l
    .client
    .groups()
    .get_participating()
    .await
    .map_err(|e| Error::upstream(format!("listing groups failed: {e}")))?;
  let mut items: Vec<Collection> = groups
    .values()
    .map(|g| {
      let mut c = Collection {
        id: g.id.to_string(),
        kind: "group".into(),
        name: g.subject.clone(),
        description: g.description.clone(),
        items: None,
        followers: Some(g.participants.len() as u64),
        ..Collection::default()
      };
      if let Some(t) = g
        .creation_time
        .and_then(|t| social_core::text::from_secs(t as i64))
      {
        c.extra
          .insert("created_at".into(), Value::from(t.to_string()));
      }
      c
    })
    .collect();
  items.sort_by(|a, b| a.name.cmp(&b.name));
  Ok(Page::new(items, None))
}

/// Which numbers are on WhatsApp.
pub async fn check(l: &mut Link, numbers: &[String]) -> Result<Page<User>> {
  let jids: Vec<Jid> = numbers
    .iter()
    .map(|n| {
      refs::phone(n)
        .ok_or_else(|| Error::input(format!("not a phone number: {n}")))
        .and_then(|d| jid(&format!("{d}@s.whatsapp.net")))
    })
    .collect::<Result<_>>()?;
  let results = l
    .client
    .contacts()
    .is_on_whatsapp(&jids)
    .await
    .map_err(|e| Error::upstream(format!("lookup failed: {e}")))?;
  let users = results
    .iter()
    .map(|r| {
      let id = r.pn_jid.as_ref().unwrap_or(&r.jid).to_string();
      let mut u = User {
        name: convert::phone(&id),
        handle: Some(convert::phone(&id)),
        id,
        ..User::default()
      };
      u.extra.insert("registered".into(), r.is_registered.into());
      u.extra.insert("business".into(), r.is_business.into());
      u
    })
    .collect();
  Ok(Page::new(users, None))
}

/// Print messages (and edits, deletions, reactions) as they arrive.
pub async fn listen(l: &mut Link, chats: &[String], out: &Stream) -> Result<()> {
  let me = l.me();
  let wanted: Vec<String> = chats
    .iter()
    .map(|c| refs::chat(c, &l.archive, &me))
    .collect::<Result<_>>()?;
  note("Listening; press Ctrl-C to stop.");
  loop {
    let event = tokio::select! {
      _ = tokio::signal::ctrl_c() => break,
      event = l.next(Duration::from_secs(10)) => event,
    };
    if let Some(event) = event {
      if let Event::LoggedOut(_) = &*event {
        return Err(not_linked());
      }
      for (record, what) in l.absorb(&event) {
        if !wanted.is_empty() && !wanted.contains(&record.chat) {
          continue;
        }
        let mut post = convert::post(&record, &l.archive);
        if what != "message" {
          post.kind = what.into();
        }
        out.post(&post);
      }
    }
    l.save_every(Duration::from_secs(10)).await?;
  }
  Ok(())
}
