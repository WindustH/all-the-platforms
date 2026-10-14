//! Login with the web player's QR code: a key from `login/qrcode/unikey`,
//! shown as `music.163.com/login?codekey=…`, then `login/qrcode/client/login`
//! until the app confirms (the session cookies arrive with that answer).

use atp_core::{Error, QrStatus, QrTicket, Result, ValueExt};

use crate::api::{Api, WEB};

/// The web player's QR type.
const QR_TYPE: u64 = 1;

pub async fn qr_start(api: &Api) -> Result<QrTicket> {
  let v = api
    .web("login/qrcode/unikey")
    .arg("type", QR_TYPE)
    .arg("noCheckToken", true)
    .send()
    .await?;
  let key = v
    .str("unikey")
    .ok_or_else(|| Error::upstream("QR login: no unikey in the answer"))?;
  // The web player tags the code with a chain id of the same form.
  let device = api
    .ctx
    .http
    .cookie("sDeviceId")
    .unwrap_or_else(|| format!("unknown-{}", rand::random_range(0..1_000_000)));
  let now = jiff::Timestamp::now().as_millisecond();
  Ok(QrTicket {
    url: format!("{WEB}/login?codekey={key}&chainId=v1_{device}_web_login_{now}"),
    token: key,
    ..QrTicket::default()
  })
}

pub async fn qr_poll(api: &Api, ticket: &QrTicket) -> Result<QrStatus> {
  let v = api
    .web("login/qrcode/client/login")
    .arg("key", ticket.token.as_str())
    .arg("type", QR_TYPE)
    .arg("noCheckToken", true)
    .send_raw()
    .await?;
  Ok(match v.i64("code") {
    Some(801) => QrStatus::Waiting,
    Some(802) => QrStatus::Scanned,
    Some(800) => QrStatus::Expired,
    Some(803) if api.ctx.http.has_cookie("MUSIC_U") => QrStatus::Confirmed,
    Some(803) => {
      return Err(Error::upstream(
        "QR login confirmed, but no MUSIC_U cookie came with it",
      ));
    }
    code => return Err(api.error(code.unwrap_or(-1), &v)),
  })
}
