//! QR login: the code is scanned with WeChat on the phone and confirmed;
//! the answer is the bot's token, its id, the owner's id and the API host
//! to use from then on. WeChat may move polling to another host
//! (`scaned_but_redirect`), ask for the number shown on the phone
//! (`need_verifycode`), or expire the code (refreshed up to three times).

use std::cell::{Cell, RefCell};
use std::io::IsTerminal;
use std::time::Duration;

use social_core::credentials;
use social_core::output::note;
use social_core::{Error, QrStatus, QrTicket, Result, User, Value, ValueExt, json};
use url::form_urlencoded::byte_serialize;

use crate::api::{Api, BASE, BASE_URL, BOT_ID, LONG_POLL, OWNER, TOKEN};
use crate::live;

/// `bot_type` of the ClawBot channel.
const BOT_TYPE: &str = "3";
const MAX_REFRESH: u32 = 3;

#[derive(Default)]
pub struct Login {
  qrcode: RefCell<String>,
  base: RefCell<String>,
  verify: RefCell<Option<String>>,
  refreshes: Cell<u32>,
}

pub async fn start(api: &Api, login: &Login) -> Result<QrTicket> {
  let (qrcode, url) = fetch(api).await?;
  *login.qrcode.borrow_mut() = qrcode.clone();
  *login.base.borrow_mut() = BASE.to_owned();
  *login.verify.borrow_mut() = None;
  login.refreshes.set(0);
  Ok(QrTicket {
    url,
    token: qrcode,
    ..QrTicket::default()
  })
}

/// A new code: `(qrcode, what the QR shows)`. Tokens already held here go
/// along, as the official client does.
async fn fetch(api: &Api) -> Result<(String, String)> {
  let held: Vec<String> = api.account().map(|a| a.token).into_iter().collect();
  let endpoint = format!("ilink/bot/get_bot_qrcode?bot_type={BOT_TYPE}");
  let body = json!({ "local_token_list": held });
  let v = api.post_anonymous(BASE, &endpoint, body).await?;
  match (v.str("qrcode"), v.str("qrcode_img_content")) {
    (Some(code), Some(url)) => Ok((code, url)),
    _ => Err(Error::upstream("WeChat sent no login QR code")),
  }
}

pub async fn poll(api: &Api, login: &Login) -> Result<QrStatus> {
  let base = login.base.borrow().clone();
  let Some(v) = api
    .get_anonymous(&base, &status_endpoint(login), LONG_POLL)
    .await?
  else {
    return Ok(QrStatus::Waiting);
  };
  match v.str("status").as_deref().unwrap_or("wait") {
    "scaned" => {
      *login.verify.borrow_mut() = None;
      Ok(QrStatus::Scanned)
    }
    "scaned_but_redirect" => {
      if let Some(host) = v.str("redirect_host") {
        *login.base.borrow_mut() = format!("https://{host}");
      }
      Ok(QrStatus::Scanned)
    }
    "need_verifycode" => {
      let again = login.verify.borrow().is_some();
      *login.verify.borrow_mut() = Some(ask_code(again)?);
      Ok(QrStatus::Scanned)
    }
    "verify_code_blocked" => Err(Error::auth(
      "WeChat blocked the login after wrong numbers; try again later",
    )),
    "binded_redirect" => already_connected(api),
    "expired" => refresh(api, login).await,
    "confirmed" => confirmed(api, &v, base),
    _ => Ok(QrStatus::Waiting),
  }
}

/// `get_qrcode_status` for the code on screen (and a typed number).
fn status_endpoint(login: &Login) -> String {
  let encode = |s: &str| byte_serialize(s.as_bytes()).collect::<String>();
  let mut endpoint = format!(
    "ilink/bot/get_qrcode_status?qrcode={}",
    encode(&login.qrcode.borrow())
  );
  if let Some(code) = login.verify.borrow().as_deref() {
    endpoint += &format!("&verify_code={}", encode(code));
  }
  endpoint
}

/// The account is connected already: fine when that login is saved here.
fn already_connected(api: &Api) -> Result<QrStatus> {
  if api.account().is_some() {
    note("WeChat says this account is already connected; keeping the saved login.");
    return Ok(QrStatus::Confirmed);
  }
  Err(
    Error::auth("WeChat says this account is already connected to a ClawBot client")
      .with_hint("disconnect it in WeChat (the ClawBot contact's settings), then log in again"),
  )
}

/// The code expired: show a new one, up to [`MAX_REFRESH`] times.
async fn refresh(api: &Api, login: &Login) -> Result<QrStatus> {
  let n = login.refreshes.get() + 1;
  if n > MAX_REFRESH {
    return Ok(QrStatus::Expired);
  }
  login.refreshes.set(n);
  let (code, url) = fetch(api).await?;
  *login.qrcode.borrow_mut() = code;
  social_core::qr::show(&url, api.ctx.store.cache_dir())?;
  note("The QR code expired; scan the new one.");
  Ok(QrStatus::Waiting)
}

/// Keep the bot's token, ids and API host.
fn confirmed(api: &Api, v: &Value, base: String) -> Result<QrStatus> {
  let token = v
    .str("bot_token")
    .ok_or_else(|| Error::upstream("login confirmed without a bot token"))?;
  let host = v
    .str("baseurl")
    .map(|b| b.trim_end_matches('/').to_owned())
    .unwrap_or(base);
  credentials::save(
    &api.ctx,
    &[
      (TOKEN, &token),
      (BOT_ID, &v.str("ilink_bot_id").unwrap_or_default()),
      (OWNER, &v.str("ilink_user_id").unwrap_or_default()),
      (BASE_URL, &host),
    ],
  );
  Ok(QrStatus::Confirmed)
}

/// The bot, checked with a short poll (an expired login answers -14);
/// messages it brings are recorded like any other.
pub async fn whoami(api: &Api) -> Result<User> {
  let account = api.require_account()?;
  live::fetch(api, &account, Duration::from_secs(2)).await?;
  let mut user = User {
    id: account.bot_id.clone(),
    name: "ClawBot".into(),
    ..User::default()
  };
  user.extra.insert("owner".into(), account.owner.into());
  user.extra.insert("api".into(), account.base.into());
  Ok(user)
}

/// The number WeChat shows on the phone during some logins.
fn ask_code(again: bool) -> Result<String> {
  if !std::io::stdin().is_terminal() {
    return Err(Error::input(
      "WeChat asks for the number shown on the phone; run `social wechat login` in a terminal",
    ));
  }
  if again {
    eprint!("That number did not match. Type the number shown in WeChat: ");
  } else {
    eprint!("Type the number shown in WeChat on your phone: ");
  }
  let mut line = String::new();
  std::io::stdin().read_line(&mut line)?;
  Ok(line.trim().to_owned())
}
