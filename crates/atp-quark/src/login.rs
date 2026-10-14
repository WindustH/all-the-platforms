//! QR login through Quark's CAS (`uop.quark.cn`, client 532): a token, the
//! `su.quark.cn` link the Quark app scans, polling for a service ticket,
//! and the account page redeeming the ticket for the drive cookies
//! (`__pus`, `__puus` ...) along its redirects.

use atp_core::{Error, QrStatus, QrTicket, Result, Value, ValueExt};
use rand::Rng;

use crate::api::{Api, PAN};

const CAS: &str = "https://uop.quark.cn/cas/ajax";
const CLIENT_ID: &str = "532";
const OK: i64 = 2_000_000;
/// The ticket is not there yet.
const WAITING: i64 = 50_004_001;
/// The token expired or was used.
const GONE: [i64; 3] = [50_004_002, 50_004_003, 50_004_004];
const SCAN_PAGE: &str = "https://su.quark.cn/4_eMHBJ";
const SCAN_BIZ: &str =
  "S%3Acustom%7COPT%3ASAREA%400%7COPT%3AIMMERSIVE%401%7COPT%3ABACK_BTN_STYLE%400";

pub async fn qr_start(api: &Api) -> Result<QrTicket> {
  let v = cas(api, "getTokenForQrcodeLogin", &[]).await?;
  let Some(token) = v
    .str("data.members.token")
    .filter(|_| v.i64("status") == Some(OK))
  else {
    return Err(failed("getTokenForQrcodeLogin", &v));
  };
  let url = format!(
    "{SCAN_PAGE}?token={token}&client_id={CLIENT_ID}&ssb=weblogin&uc_param_str=&uc_biz_str={SCAN_BIZ}"
  );
  Ok(QrTicket {
    url,
    token,
    ..QrTicket::default()
  })
}

pub async fn qr_poll(api: &Api, ticket: &QrTicket) -> Result<QrStatus> {
  let token = [("token", ticket.token.clone())];
  let v = cas(api, "getServiceTicketByQrcodeToken", &token).await?;
  let status = v.i64("status").unwrap_or_default();
  let expired = v
    .str("message")
    .is_some_and(|m| m.to_lowercase().contains("expire"));
  match (status, v.str("data.members.service_ticket")) {
    (OK, Some(service_ticket)) => {
      redeem(api, &service_ticket).await?;
      Ok(QrStatus::Confirmed)
    }
    (WAITING, _) => Ok(QrStatus::Waiting),
    (code, _) if GONE.contains(&code) || expired => Ok(QrStatus::Expired),
    _ => Err(failed("getServiceTicketByQrcodeToken", &v)),
  }
}

/// The account page takes the ticket and sets the drive cookies on its way.
async fn redeem(api: &Api, service_ticket: &str) -> Result<()> {
  api
    .ctx
    .http
    .get(format!("{PAN}/account/info"))
    .query("st", service_ticket)
    .query("lw", "scan")
    .query("fr", "pc")
    .query("platform", "pc")
    .header("referer", format!("{PAN}/"))
    .send()
    .await?;
  if !api.ctx.http.has_cookie("__pus") {
    return Err(
      Error::auth("Quark accepted the scan but set no drive cookie (`__pus`)")
        .with_hint("log in with `atp quark login --browser` or `--cookie '...'` instead"),
    );
  }
  Ok(())
}

async fn cas(api: &Api, method: &str, extra: &[(&str, String)]) -> Result<Value> {
  api
    .ctx
    .http
    .get(format!("{CAS}/{method}"))
    .no_cookies()
    .query("client_id", CLIENT_ID)
    .query("v", "1.2")
    .query("request_id", request_id())
    .queries(extra.iter().cloned())
    .header("referer", format!("{PAN}/"))
    .header("origin", PAN)
    .send()
    .await?
    .check()?
    .value()
}

fn failed(method: &str, v: &Value) -> Error {
  let message = v.str("message").unwrap_or_else(|| "no answer".into());
  let status = v.str("status").unwrap_or_default();
  Error::upstream(format!("Quark login {method}: {message} (status {status})"))
}

/// A random version-4 UUID.
fn request_id() -> String {
  let mut bytes: [u8; 16] = rand::rng().random();
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
  format!(
    "{}-{}-{}-{}-{}",
    &hex[..8],
    &hex[8..12],
    &hex[12..16],
    &hex[16..20],
    &hex[20..]
  )
}
