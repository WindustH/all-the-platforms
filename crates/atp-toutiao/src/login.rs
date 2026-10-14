//! QR login through the Toutiao SSO passport (shared with Douyin in
//! atp-bytedance): scan with the Toutiao app. Risk control may ask for an
//! extra identity scan with the Douyin app, reported as
//! `verification_required`.

use atp_bytedance::passport::{self, QrState, TOUTIAO};
use atp_core::{QrStatus, QrTicket, Result};

use crate::api::Api;

pub async fn qr_start(api: &Api) -> Result<QrTicket> {
  let qr = passport::qr_start(&api.ctx.http, TOUTIAO).await?;
  Ok(QrTicket {
    url: qr.url,
    token: qr.token,
    ..QrTicket::default()
  })
}

pub async fn qr_poll(api: &Api, ticket: &QrTicket) -> Result<QrStatus> {
  let http = &api.ctx.http;
  Ok(
    match passport::qr_poll(http, TOUTIAO, &ticket.token).await? {
      QrState::Waiting => QrStatus::Waiting,
      QrState::Scanned => QrStatus::Scanned,
      QrState::Expired => QrStatus::Expired,
      QrState::Confirmed(redirect) => {
        passport::finish(http, &redirect).await?;
        QrStatus::Confirmed
      }
    },
  )
}
