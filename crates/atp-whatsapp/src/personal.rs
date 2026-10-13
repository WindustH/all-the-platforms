//! The personal account as a linked device (WhatsApp Web's multi-device
//! protocol, through whatsapp-rust): the device's key file, its archive and
//! the one connection every operation of a command shares. Linking is in
//! [`crate::login`].
//!
//! Each operation ends with the device state and the archive saved, and
//! dropping the platform saves them once more.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use atp_core::output::note;
use atp_core::{Action, Ctx, Error, Result};
use tokio::sync::Mutex;
use whatsapp_rust::pair_code::PairCodeOptions;
use whatsapp_rust::prelude::Event;

use crate::archive::Archive;
use crate::link::Link;
use crate::parse;
use crate::store::FileBackend;

/// Session extra marking a linked device (the name of its key file).
pub const DEVICE: &str = "device";
const DEVICE_FILE: &str = "device.json";

pub struct Personal {
  pub(crate) device: PathBuf,
  pub(crate) archive_dir: PathBuf,
  pub(crate) cache_dir: PathBuf,
  link: Mutex<Option<Link>>,
  /// The connection of a QR login in progress.
  pub(crate) pending: Mutex<Option<Link>>,
  pub(crate) proxy: Option<String>,
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

  /// A client on a new, empty key file (a new linked device).
  pub(crate) async fn fresh(&self, pair_code: Option<PairCodeOptions>) -> Result<Link> {
    if let Some(old) = self.link.lock().await.take() {
      old.close().await;
    }
    let _ = std::fs::remove_file(&self.device);
    let backend = Arc::new(FileBackend::open(&self.device)?);
    let archive = Archive::open(&self.archive_dir);
    Link::start(backend, archive, pair_code, self.proxy.as_deref()).await
  }

  /// After pairing: take in the phone's history, remember the device.
  pub(crate) async fn finish(&self, ctx: &Ctx, mut link: Link) -> Result<()> {
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
      .with(ctx, async |link| {
        let me = link.me();
        link.client.logout().await;
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
      Action::done("unlink", parse::phone(&me))
        .with_message("the linked device was removed and its keys and archive deleted"),
    )
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

pub fn not_linked() -> Error {
  Error::auth("no WhatsApp account is linked").with_hint(
    "run `atp whatsapp login` (QR code) or `atp whatsapp login-code +NUMBER`; for the Business Cloud API, `atp whatsapp login --token TOKEN:PHONE_NUMBER_ID`",
  )
}

pub fn pair_failure(event: &Event) -> Option<Error> {
  match event {
    Event::PairError(e) => Some(Error::auth(format!("linking failed: {e:?}"))),
    Event::QrScannedWithoutMultidevice(_) => Some(Error::auth(
      "the phone's WhatsApp is too old for linked devices; update it",
    )),
    Event::ClientOutdated(_) => Some(Error::upstream(
      "WhatsApp rejects this client version; update all-the-platforms",
    )),
    Event::TemporaryBan(b) => Some(Error::auth(format!(
      "WhatsApp temporarily banned this account: {b:?}"
    ))),
    _ => None,
  }
}
