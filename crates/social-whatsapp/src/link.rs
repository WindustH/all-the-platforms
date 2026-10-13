//! One connection of the linked device: the whatsapp-rust client running on
//! this process's runtime, its events read from a channel. Every event it
//! sees goes into the archive, so any command also catches up on what
//! arrived since the last one.

use std::sync::Arc;
use std::time::{Duration, Instant};

use social_core::{Error, ErrorCode, Result};
use tokio::sync::mpsc;
use whatsapp_rust::bot::BotHandle;
use whatsapp_rust::pair_code::PairCodeOptions;
use whatsapp_rust::prelude::{Bot, Client, Event};
use whatsapp_rust::wacore::types::events::LazyHistorySync;

use crate::archive::{Archive, Record, bare, kind_of};
use crate::convert::{self, Item, Origin};
use crate::proxy::Dialer;
use crate::store::FileBackend;

pub struct Link {
  pub client: Arc<Client>,
  handle: Option<BotHandle>,
  events: mpsc::UnboundedReceiver<Arc<Event>>,
  backend: Arc<FileBackend>,
  pub archive: Archive,
  last_flush: Instant,
}

/// How long a quiet connection counts as caught up.
const IDLE: Duration = Duration::from_millis(2500);
/// Upper bound for catching up on queued messages after connecting.
const CATCH_UP: Duration = Duration::from_secs(25);

impl Link {
  /// Start the client; it connects (and, unpaired, pairs) in the background.
  pub async fn start(
    backend: Arc<FileBackend>,
    archive: Archive,
    pair_code: Option<PairCodeOptions>,
    proxy: Option<&str>,
  ) -> Result<Self> {
    let (tx, events) = mpsc::unbounded_channel();
    let mut builder = Bot::builder()
      .with_backend_arc(backend.clone())
      .with_transport_factory(Dialer::new(proxy))
      .with_http_client(crate::proxy::http_client(proxy))
      .on_event(move |event, _client| {
        let _ = tx.send(event);
        async {}
      });
    if let Some(options) = pair_code {
      builder = builder.with_pair_code(options);
    }
    let bot = builder
      .build()
      .await
      .map_err(|e| Error::internal(format!("whatsapp client: {e}")))?;
    let client = bot.client();
    let handle = bot.spawn();
    Ok(Self {
      client,
      handle: Some(handle),
      events,
      backend,
      archive,
      last_flush: Instant::now(),
    })
  }

  /// Connect a paired device and catch up on what queued up meanwhile.
  pub async fn connect(
    backend: Arc<FileBackend>,
    archive: Archive,
    proxy: Option<&str>,
  ) -> Result<Self> {
    let mut link = Self::start(backend, archive, None, proxy).await?;
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
      let wait = deadline.saturating_duration_since(Instant::now());
      if wait.is_zero() {
        link.close().await;
        return Err(Error::network(
          "WhatsApp did not accept the connection in time",
        ));
      }
      let Some(event) = link.next(wait).await else {
        continue;
      };
      if matches!(&*event, Event::Connected(_)) {
        break;
      }
      if let Some(e) = fatal(&event) {
        link.close().await;
        return Err(e);
      }
      link.absorb(&event);
    }
    link.catch_up().await?;
    Ok(link)
  }

  /// Read events until the offline queue is drained or the line is quiet.
  pub async fn catch_up(&mut self) -> Result<()> {
    let until = Instant::now() + CATCH_UP;
    while let Some(wait) = until.checked_duration_since(Instant::now()) {
      let Some(event) = self.next(wait.min(IDLE)).await else {
        break;
      };
      if let Some(e) = fatal(&event) {
        return Err(e);
      }
      self.absorb(&event);
      if matches!(&*event, Event::OfflineSyncCompleted(_)) {
        // Drain what was dispatched together with it.
        while let Some(event) = self.next(Duration::from_millis(300)).await {
          self.absorb(&event);
        }
        break;
      }
    }
    self.save().await
  }

  /// The next event, or `None` after `wait`.
  pub async fn next(&mut self, wait: Duration) -> Option<Arc<Event>> {
    tokio::time::timeout(wait, self.events.recv())
      .await
      .ok()
      .flatten()
  }

  /// Read events for `wait`, absorbing them; stops early when `done` says so.
  pub async fn pump(&mut self, wait: Duration, mut done: impl FnMut(&Event) -> bool) -> Result<()> {
    let until = Instant::now() + wait;
    while let Some(left) = until.checked_duration_since(Instant::now()) {
      let Some(event) = self.next(left).await else {
        break;
      };
      if let Some(e) = fatal(&event) {
        return Err(e);
      }
      self.absorb(&event);
      if done(&event) {
        break;
      }
    }
    Ok(())
  }

  /// Fold one event into the archive; returns the messages it added or
  /// changed, with what happened (`message`, `edit`, `delete`, `reaction`).
  pub fn absorb(&mut self, event: &Event) -> Vec<(Record, &'static str)> {
    let mut out = Vec::new();
    match event {
      Event::Messages(batch) => {
        for m in batch.iter() {
          let info = &m.info;
          let chat = info.source.chat.to_string();
          let origin = Origin {
            chat: chat.clone(),
            sender: bare(&info.source.sender.to_string()),
            id: info.id.to_string(),
            from_me: info.source.is_from_me,
            ts: info.timestamp.timestamp(),
            push_name: Some(info.push_name.clone()),
          };
          let item = convert::item(&m.message, origin);
          let is_new = matches!(item, Item::Message(_));
          if let Some(changed) = convert::apply(&mut self.archive, item) {
            if is_new && !info.source.is_from_me {
              self.archive.chat_mut(&chat).unread += 1;
            }
            out.push(changed);
          }
        }
      }
      Event::HistorySync(sync) => self.history(sync),
      Event::PushNameUpdate(u) => {
        self
          .archive
          .set_name(&u.jid.to_string(), &u.new_push_name, false);
      }
      Event::ContactUpdate(u) => {
        let a = &u.action;
        if let Some(name) = a.full_name.as_deref().or(a.first_name.as_deref()) {
          self.archive.set_name(&u.jid.to_string(), name, true);
        }
      }
      Event::PinUpdate(u) => {
        let pinned = u.action.pinned.unwrap_or(false);
        self.archive.chat_mut(&u.jid.to_string()).pinned = pinned;
      }
      Event::ArchiveUpdate(u) => {
        let archived = u.action.archived.unwrap_or(false);
        self.archive.chat_mut(&u.jid.to_string()).archived = archived;
      }
      Event::MuteUpdate(u) => {
        let until = if u.action.muted.unwrap_or(false) {
          u.action.mute_end_timestamp.unwrap_or(-1)
        } else {
          0
        };
        self.archive.chat_mut(&u.jid.to_string()).muted_until = until;
      }
      Event::MarkChatAsReadUpdate(u) if u.action.read.unwrap_or(false) => {
        self.archive.chat_mut(&u.jid.to_string()).unread = 0;
      }
      _ => {}
    }
    out
  }

  /// A history sync chunk: conversations, their messages and push names.
  fn history(&mut self, sync: &LazyHistorySync) {
    let Some(h) = sync.get() else {
      return;
    };
    for p in &h.pushnames {
      if let (Some(id), Some(name)) = (&p.id, &p.pushname) {
        self.archive.set_name(id, name, false);
      }
    }
    let me = self.me();
    for c in &h.conversations {
      let jid = c.id.clone();
      {
        let entry = self.archive.chat_mut(&jid);
        entry.kind = kind_of(&jid).into();
        if let Some(name) = c
          .name
          .as_ref()
          .or(c.display_name.as_ref())
          .filter(|n| !n.is_empty())
        {
          entry.name = Some(name.clone());
        }
        if let Some(n) = c.unread_count {
          entry.unread = n;
        }
        entry.archived = c.archived.unwrap_or(entry.archived);
        entry.pinned = c.pinned.is_some_and(|p| p > 0) || entry.pinned;
        if let Some(d) = &c.description {
          entry.description = Some(d.clone());
        }
        let ts = c
          .conversation_timestamp
          .or(c.last_msg_timestamp)
          .unwrap_or_default() as i64;
        entry.last = entry.last.max(ts);
      }
      for hm in &c.messages {
        let Some(w) = hm.message.as_option() else {
          continue;
        };
        let Some(key) = w.key.as_option() else {
          continue;
        };
        let Some(body) = w.message.as_option() else {
          continue;
        };
        let from_me = key.from_me.unwrap_or(false);
        let sender = if from_me {
          me.clone()
        } else {
          key
            .participant
            .clone()
            .or_else(|| w.participant.clone())
            .unwrap_or_else(|| jid.clone())
        };
        let origin = Origin {
          chat: jid.clone(),
          sender: bare(&sender),
          id: key.id.clone().unwrap_or_default(),
          from_me,
          ts: w.message_timestamp.unwrap_or_default() as i64,
          push_name: w.push_name.clone(),
        };
        convert::apply(&mut self.archive, convert::item(body, origin));
      }
    }
  }

  /// This account's own JID (phone number form).
  pub fn me(&self) -> String {
    self
      .client
      .pn()
      .map(|j| bare(&j.to_string()))
      .unwrap_or_default()
  }

  /// Write the archive and the device state.
  pub async fn save(&mut self) -> Result<()> {
    self.archive.save()?;
    self.backend.flush().await?;
    self.last_flush = Instant::now();
    Ok(())
  }

  /// Save without awaiting (when the platform is dropped).
  pub fn save_blocking(&mut self) {
    let _ = self.archive.save();
    let _ = self.backend.flush_blocking();
  }

  /// Save when the last save is older than `every` (long-running commands).
  pub async fn save_every(&mut self, every: Duration) -> Result<()> {
    if self.last_flush.elapsed() >= every {
      self.save().await?;
    }
    Ok(())
  }

  /// Disconnect and save everything.
  pub async fn close(mut self) {
    if let Some(handle) = self.handle.take() {
      let _ = tokio::time::timeout(Duration::from_secs(8), handle.shutdown()).await;
    }
    if let Err(e) = self.save().await {
      tracing::warn!("saving the WhatsApp state failed: {e}");
    }
  }

  /// Ask the phone for older messages of a chat (it answers while online).
  pub async fn fetch_older(&mut self, chat: &str, count: i32) -> Result<usize> {
    let jid: whatsapp_rust::Jid = chat
      .parse()
      .map_err(|_| Error::input(format!("not a chat id: {chat}")))?;
    let oldest = self.archive.messages(chat).first().cloned();
    let Some(oldest) = oldest else {
      return Err(Error::not_found(
        "no message of this chat is archived yet to continue from",
      ));
    };
    let before = self.archive.messages(chat).len();
    self
      .client
      .fetch_message_history(&jid, &oldest.id, oldest.from_me, oldest.ts * 1000, count)
      .await
      .map_err(|e| Error::upstream(format!("history request failed: {e}")))?;
    self
      .pump(Duration::from_secs(20), |e| {
        matches!(e, Event::HistorySync(_))
      })
      .await?;
    // More chunks may follow the first.
    self.pump(Duration::from_secs(2), |_| false).await?;
    self.save().await?;
    Ok(self.archive.messages(chat).len().saturating_sub(before))
  }
}

/// Events after which the connection is useless.
fn fatal(event: &Event) -> Option<Error> {
  match event {
    Event::LoggedOut(_) => Some(
      Error::auth("WhatsApp logged this device out (it was removed from Linked Devices)")
        .with_hint("link it again with `social whatsapp login`"),
    ),
    Event::TemporaryBan(b) => Some(Error::new(
      ErrorCode::IpBlocked,
      format!("WhatsApp temporarily banned this account: {b:?}"),
    )),
    Event::ClientOutdated(_) => Some(Error::upstream(
      "WhatsApp rejects this client version; update social-cli",
    )),
    Event::StreamReplaced(_) => Some(Error::upstream(
      "another connection of this linked device took over (is `social whatsapp listen` running elsewhere?)",
    )),
    Event::ConnectFailure(f) => Some(Error::network(format!(
      "WhatsApp refused the connection: {f:?}"
    ))),
    _ => None,
  }
}
