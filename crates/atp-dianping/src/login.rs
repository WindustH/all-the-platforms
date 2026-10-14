//! QR login, as the web login page does it: `getQrCodeImg` answers the QR
//! image and sets the `qruuid` cookie; the code holds
//! `m.dianping.com/mlogin/qrcodeconfirm?qruuid=…`, which the Dianping app
//! opens when it scans it. `check?qruuid=` tells how far the login got
//! (`data.status` 200, "用户未扫描二维码", while nobody has scanned); the
//! answer that completes it sets the login cookies (`dper`).

use atp_core::{Error, QrStatus, QrTicket, Result, Value, ValueExt};
use atp_mtsec::guard::{self, Call, DIANPING};

use crate::api::{Api, LOGIN_COOKIES, MOBILE};

const QR: &str = "https://accountapi.dianping.com/mlogin/dp/api/v1/qrlogin";
const FROM: &str = "https://account.dianping.com/";

pub async fn qr_start(api: &Api) -> Result<QrTicket> {
  let call = Call::xhr(format!("{QR}/getQrCodeImg"), FROM);
  let resp = guard::send(&api.ctx, &DIANPING, call).await?.check()?;
  let token = api
    .ctx
    .http
    .cookie("qruuid")
    .ok_or_else(|| Error::upstream("Dianping's QR login gave no qruuid"))?;
  Ok(QrTicket {
    url: format!("{MOBILE}/mlogin/qrcodeconfirm?qruuid={token}"),
    image: Some(resp.body).filter(|b| !b.is_empty()),
    token,
    ..QrTicket::default()
  })
}

pub async fn qr_poll(api: &Api, ticket: &QrTicket) -> Result<QrStatus> {
  let call = Call::xhr(format!("{QR}/check"), FROM).query("qruuid", &ticket.token);
  let v = guard::json(&api.ctx, &DIANPING, call).await?;
  if logged_in(api) {
    return Ok(QrStatus::Confirmed);
  }
  if let Some(next) = v.first_str(&["data.redirectUrl", "data.url", "data.redir"]) {
    follow(api, &next).await?;
    if logged_in(api) {
      return Ok(QrStatus::Confirmed);
    }
  }
  Ok(status(&v))
}

fn logged_in(api: &Api) -> bool {
  LOGIN_COOKIES.iter().all(|c| api.ctx.http.has_cookie(c))
}

/// A page the last answer points to, for the cookies it sets.
async fn follow(api: &Api, next: &str) -> Result<()> {
  if !next.starts_with("http") {
    return Ok(());
  }
  guard::send(&api.ctx, &DIANPING, Call::page(next, FROM)).await?;
  Ok(())
}

/// The state the answer describes, by its wording (only the waiting state
/// has been seen; the others follow the site's usual phrases).
fn status(v: &Value) -> QrStatus {
  let text = v
    .first_str(&["data.description", "data.message", "message", "msg"])
    .unwrap_or_default();
  let has = |words: &[&str]| words.iter().any(|w| text.contains(w));
  if has(&["过期", "失效", "超时"]) {
    QrStatus::Expired
  } else if has(&["未扫描", "等待扫码"]) {
    QrStatus::Waiting
  } else if has(&["扫描", "扫码", "确认"]) {
    QrStatus::Scanned
  } else {
    QrStatus::Waiting
  }
}
