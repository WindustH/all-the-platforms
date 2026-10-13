//! The transport: the context, the linked device ([`Personal`]) and the
//! Business Cloud API credentials ([`Cloud`]), and which of them a command
//! runs on: a linked device wins, else saved or environment Cloud API
//! credentials.

use social_core::{Ctx, Error, Result};

use crate::cloud::{self, Cloud};
use crate::link::Link;
use crate::personal::Personal;

pub enum Mode {
  Personal,
  Cloud(Cloud),
  None,
}

pub struct Api {
  pub ctx: Ctx,
  pub personal: Personal,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self {
      personal: Personal::new(&ctx),
      ctx,
    }
  }

  pub fn mode(&self) -> Mode {
    if self.personal.linked(&self.ctx) {
      return Mode::Personal;
    }
    match Cloud::from_ctx(&self.ctx) {
      Some(c) => Mode::Cloud(c),
      None => Mode::None,
    }
  }

  /// Run on the linked device, or fail with what each mode allows.
  pub async fn on_device<T>(
    &self,
    what: &str,
    f: impl AsyncFnOnce(&mut Link) -> Result<T>,
  ) -> Result<T> {
    match self.mode() {
      Mode::Personal => self.personal.with(&self.ctx, f).await,
      Mode::Cloud(_) => Err(cloud_cannot(what)),
      Mode::None => Err(not_logged_in()),
    }
  }

  /// The Cloud API, or fail with what each mode allows.
  pub fn cloud(&self, what: &str) -> Result<Cloud> {
    match self.mode() {
      Mode::Cloud(c) => Ok(c),
      Mode::Personal => Err(personal_cannot(what)),
      Mode::None => Err(Error::auth("no Cloud API credentials").with_hint(
        "`social whatsapp login --token TOKEN:PHONE_NUMBER_ID[:WABA_ID]` or WHATSAPP_TOKEN / WHATSAPP_PHONE_NUMBER_ID",
      )),
    }
  }

  /// Drop saved Cloud API credentials (a device was linked instead).
  pub fn forget_cloud(&self) {
    for key in [cloud::TOKEN, cloud::PHONE, cloud::WABA] {
      self.ctx.set_extra(key, "");
    }
  }
}

pub fn not_logged_in() -> Error {
  Error::auth("not logged in to WhatsApp").with_hint(
    "link your account with `social whatsapp login` (QR code) or `social whatsapp login-code +NUMBER`; for the Business Cloud API use `social whatsapp login --token TOKEN:PHONE_NUMBER_ID`",
  )
}

fn cloud_cannot(what: &str) -> Error {
  Error::unsupported(what).with_hint(
    "the Cloud API only sends: incoming messages go to a webhook, and there is no chat list or history; link a personal account with `social whatsapp login` for this",
  )
}

fn personal_cannot(what: &str) -> Error {
  Error::unsupported(what)
    .with_hint("this is a Business Cloud API feature: `social whatsapp login --token TOKEN:PHONE_NUMBER_ID[:WABA_ID]`")
}
