//! QR login: the code is scanned with WeChat on the phone and confirmed;
//! the answer is the bot's token, its id, the owner's id and the API host
//! to use from then on. WeChat may move polling to another host
//! (`scaned_but_redirect`), ask for the number shown on the phone
//! (`need_verifycode`), or expire the code (refreshed up to three times).

use std::cell::{Cell, RefCell};
use std::io::IsTerminal;

use social_core::output::note;
use social_core::{Ctx, Error, QrStatus, QrTicket, Result, ValueExt, json};
use url::form_urlencoded::byte_serialize;

use crate::api::{self, Account, BASE, BASE_URL, BOT_ID, LONG_POLL, OWNER, TOKEN};

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

pub async fn start(ctx: &Ctx, login: &Login) -> Result<QrTicket> {
  let (qrcode, url) = fetch(ctx).await?;
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
async fn fetch(ctx: &Ctx) -> Result<(String, String)> {
  let held: Vec<String> = Account::from_ctx(ctx)
    .map(|a| a.token)
    .into_iter()
    .collect();
  let endpoint = format!("ilink/bot/get_bot_qrcode?bot_type={BOT_TYPE}");
  let v = api::post_anonymous(ctx, BASE, &endpoint, json!({ "local_token_list": held })).await?;
  match (v.str("qrcode"), v.str("qrcode_img_content")) {
    (Some(code), Some(url)) => Ok((code, url)),
    _ => Err(Error::upstream("WeChat sent no login QR code")),
  }
}

pub async fn poll(ctx: &Ctx, login: &Login) -> Result<QrStatus> {
  let qrcode = login.qrcode.borrow().clone();
  let base = login.base.borrow().clone();
  let mut endpoint = format!(
    "ilink/bot/get_qrcode_status?qrcode={}",
    byte_serialize(qrcode.as_bytes()).collect::<String>()
  );
  if let Some(code) = login.verify.borrow().as_deref() {
    endpoint += &format!(
      "&verify_code={}",
      byte_serialize(code.as_bytes()).collect::<String>()
    );
  }
  let Some(v) = api::get(ctx, &base, &endpoint, LONG_POLL).await? else {
    return Ok(QrStatus::Waiting);
  };
  Ok(match v.str("status").as_deref().unwrap_or("wait") {
    "scaned" => {
      *login.verify.borrow_mut() = None;
      QrStatus::Scanned
    }
    "scaned_but_redirect" => {
      if let Some(host) = v.str("redirect_host") {
        *login.base.borrow_mut() = format!("https://{host}");
      }
      QrStatus::Scanned
    }
    "need_verifycode" => {
      let again = login.verify.borrow().is_some();
      *login.verify.borrow_mut() = Some(ask_code(again)?);
      QrStatus::Scanned
    }
    "verify_code_blocked" => {
      return Err(Error::auth(
        "WeChat blocked the login after wrong numbers; try again later",
      ));
    }
    "binded_redirect" => {
      if Account::from_ctx(ctx).is_some() {
        note("WeChat says this account is already connected; keeping the saved login.");
        QrStatus::Confirmed
      } else {
        return Err(
          Error::auth("WeChat says this account is already connected to a ClawBot client")
            .with_hint(
              "disconnect it in WeChat (the ClawBot contact's settings), then log in again",
            ),
        );
      }
    }
    "expired" => {
      let n = login.refreshes.get() + 1;
      if n > MAX_REFRESH {
        return Ok(QrStatus::Expired);
      }
      login.refreshes.set(n);
      let (code, url) = fetch(ctx).await?;
      *login.qrcode.borrow_mut() = code;
      social_core::qr::show(&url, ctx.store.cache_dir())?;
      note("The QR code expired; scan the new one.");
      QrStatus::Waiting
    }
    "confirmed" => {
      let token = v
        .str("bot_token")
        .ok_or_else(|| Error::upstream("login confirmed without a bot token"))?;
      ctx.set_extra(TOKEN, &token);
      ctx.set_extra(BOT_ID, &v.str("ilink_bot_id").unwrap_or_default());
      ctx.set_extra(OWNER, &v.str("ilink_user_id").unwrap_or_default());
      let host = v
        .str("baseurl")
        .map(|b| b.trim_end_matches('/').to_owned())
        .unwrap_or(base);
      ctx.set_extra(BASE_URL, &host);
      QrStatus::Confirmed
    }
    _ => QrStatus::Waiting,
  })
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
