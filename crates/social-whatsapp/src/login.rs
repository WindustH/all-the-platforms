//! Linking this computer as a device: a QR code scanned in WhatsApp, or a
//! code typed on the phone; and `login --token` for the Cloud API.

use std::time::{Duration, Instant};

use social_core::output::note;
use social_core::{Error, QrStatus, QrTicket, Result, User};
use whatsapp_rust::pair_code::PairCodeOptions;
use whatsapp_rust::prelude::Event;

use crate::api::Api;
use crate::cloud::{self, Cloud};
use crate::personal::{DEVICE, Personal, pair_failure};
use crate::{refs, users};

/// `login --token TOKEN:PHONE_NUMBER_ID[:WABA_ID]`: Cloud API credentials
/// replace a linked device.
pub fn token(api: &Api, token: &str) -> Result<()> {
  let (token, phone, waba) = Cloud::parse_token(token)?;
  social_core::credentials::save(
    &api.ctx,
    &[
      (cloud::TOKEN, &token),
      (cloud::PHONE, &phone),
      (cloud::WABA, waba.as_deref().unwrap_or("")),
      (DEVICE, ""),
    ],
  );
  Ok(())
}

/// Start linking a new device: a fresh key file, the first QR code.
pub async fn qr_start(personal: &Personal) -> Result<QrTicket> {
  let mut link = personal.fresh(None).await?;
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
  *personal.pending.lock().await = Some(link);
  Ok(QrTicket {
    url: code,
    token: "whatsapp".into(),
    ..QrTicket::default()
  })
}

/// The QR login so far; a confirmed link drops saved Cloud API credentials.
pub async fn qr_poll(api: &Api) -> Result<QrStatus> {
  let personal = &api.personal;
  let mut guard = personal.pending.lock().await;
  let Some(link) = guard.as_mut() else {
    return Err(Error::internal("no QR login in progress"));
  };
  let mut status = QrStatus::Waiting;
  while let Some(event) = link.next(Duration::from_millis(1500)).await {
    match &*event {
      Event::PairingQrCode(q) => {
        let path = social_core::qr::show(&q.code, &personal.cache_dir)?;
        note(&format!(
          "New QR code (the previous one expired). QR image: {}",
          path.display()
        ));
      }
      Event::PairSuccess(_) => status = QrStatus::Scanned,
      Event::PairingQrCodesExhausted(_) => return Ok(QrStatus::Expired),
      Event::Connected(_) => {
        let link = guard.take().expect("checked above");
        personal.finish(&api.ctx, link).await?;
        api.forget_cloud();
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
pub async fn pair_code(api: &Api, phone: &str) -> Result<User> {
  let personal = &api.personal;
  let Some(number) = refs::phone(phone) else {
    return Err(Error::input(
      "give the account's phone number with its country code, e.g. +44 7700 900123",
    ));
  };
  let options = PairCodeOptions {
    phone_number: number,
    ..PairCodeOptions::default()
  };
  let mut link = personal.fresh(Some(options)).await?;
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
        personal.finish(&api.ctx, link).await?;
        api.forget_cloud();
        return personal
          .with(&api.ctx, async |link| Ok(users::whoami(link)))
          .await;
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
