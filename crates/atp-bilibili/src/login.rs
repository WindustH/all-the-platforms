//! Login with the web QR code.

use atp_core::{Error, QrStatus, QrTicket, Result, ValueExt};

use crate::api::Api;

const QR_GENERATE: &str =
  "https://passport.bilibili.com/x/passport-login/web/qrcode/generate?source=main-fe-header";
const QR_POLL: &str = "https://passport.bilibili.com/x/passport-login/web/qrcode/poll";

/// Cookies the QR login hands over in the confirmation URL.
const QR_COOKIES: &[&str] = &["SESSDATA", "bili_jct", "DedeUserID", "DedeUserID__ckMd5"];

pub async fn qr_start(api: &Api) -> Result<QrTicket> {
  let data = api.get(QR_GENERATE).send().await?;
  match (data.str("url"), data.str("qrcode_key")) {
    (Some(url), Some(token)) => Ok(QrTicket {
      url,
      token,
      ..QrTicket::default()
    }),
    _ => Err(Error::upstream("QR login: no qrcode_key in the response")),
  }
}

pub async fn qr_poll(api: &Api, ticket: &QrTicket) -> Result<QrStatus> {
  let data = api
    .get(QR_POLL)
    .arg("qrcode_key", &ticket.token)
    .send()
    .await?;
  Ok(match data.i64("code") {
    Some(86101) => QrStatus::Waiting,
    Some(86090) => QrStatus::Scanned,
    Some(86038) => QrStatus::Expired,
    Some(0) => {
      // Cookies arrive via set-cookie; the redirect URL repeats them (still URL-encoded).
      let url = data.str("url").unwrap_or_default();
      let query = url.split_once('?').map(|(_, q)| q).unwrap_or_default();
      for (k, v) in query.split('&').filter_map(|kv| kv.split_once('=')) {
        if QR_COOKIES.contains(&k) && !api.ctx.http.has_cookie(k) {
          api.ctx.http.set_cookie(k, v);
        }
      }
      if let Some(token) = data.str("refresh_token") {
        api.ctx.set_extra("refresh_token", &token);
      }
      QrStatus::Confirmed
    }
    other => {
      return Err(Error::upstream(format!(
        "QR login: {} ({})",
        data.str("message").unwrap_or_default(),
        other.unwrap_or(-1)
      )));
    }
  })
}
