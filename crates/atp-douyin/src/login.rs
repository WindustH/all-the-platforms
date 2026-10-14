//! Logging in: a QR code of the web passport (login.douyin.com) scanned
//! with the Douyin app, or a phone number and the code Douyin texts to it
//! (`login-sms`). Both register a ticket-guard key first; the ticket the
//! passport issues for it signs writes (see [`crate::sign::guard`]).

use std::io::IsTerminal;

use atp_bytedance::passport::{self, QrState, web};
use atp_bytedance::ticket_guard::SERVER_DATA;
use atp_bytedance::visitor;
use atp_core::cli::prompt;
use atp_core::{Ctx, Error, QrStatus, QrTicket, Result, User};

use crate::api::Api;
use crate::sign::guard;
use crate::users;

/// Cookies of a logged-in session, dropped before a new login so that only
/// the new one's remain.
const SESSION_COOKIES: [&str; 6] = [
  "sessionid",
  "sessionid_ss",
  "sid_tt",
  "sid_guard",
  "uid_tt",
  "uid_tt_ss",
];

pub async fn qr_start(api: &Api) -> Result<QrTicket> {
  let ctx = &api.ctx;
  begin(ctx).await?;
  let qr = web::qr_start(&ctx.http, web::DOUYIN).await?;
  Ok(QrTicket {
    url: qr.url,
    token: qr.token,
    ..QrTicket::default()
  })
}

pub async fn qr_poll(api: &Api, ticket: &QrTicket) -> Result<QrStatus> {
  let ctx = &api.ctx;
  let (state, server_data) = web::qr_poll(&ctx.http, web::DOUYIN, &ticket.token).await?;
  guard::keep(ctx, server_data.as_deref());
  Ok(match state {
    QrState::Waiting => QrStatus::Waiting,
    QrState::Scanned => QrStatus::Scanned,
    QrState::Expired => QrStatus::Expired,
    QrState::Confirmed(redirect) => {
      let resp = passport::finish(&ctx.http, &redirect).await?;
      guard::keep(ctx, resp.header(SERVER_DATA));
      guard::bind(ctx);
      QrStatus::Confirmed
    }
  })
}

/// `login-sms`: the code Douyin texts to `phone` (asked when left out),
/// typed in on the terminal.
pub async fn sms(api: &Api, phone: Option<String>) -> Result<User> {
  let ctx = &api.ctx;
  if !std::io::stdin().is_terminal() {
    return Err(Error::input(
      "`login-sms` asks for the texted code on a terminal",
    ));
  }
  let phone = match phone {
    Some(phone) => phone,
    None => prompt::line("Phone number (mainland China, or +CC NUMBER): ")?,
  };
  let mobile = passport::mobile(&phone)?;
  begin(ctx).await?;
  passport::send_code(&ctx.http, passport::DOUYIN, &mobile).await?;
  let code = prompt::line("Code texted to that number: ")?;
  if code.is_empty() || !code.bytes().all(|b| b.is_ascii_digit()) {
    return Err(Error::input("the code is the digits Douyin texted"));
  }
  let server_data = passport::quick_login(&ctx.http, passport::DOUYIN, &mobile, &code).await?;
  guard::keep(ctx, server_data.as_deref());
  if !ctx.http.has_cookie("sessionid") {
    return Err(Error::auth(
      "the passport took the code but started no session",
    ));
  }
  guard::bind(ctx);
  users::whoami(api).await
}

/// Visitor ids, a new ticket-guard key, and no cookies of an earlier login.
async fn begin(ctx: &Ctx) -> Result<()> {
  for name in SESSION_COOKIES {
    ctx.http.remove_cookie(name);
  }
  visitor::ensure(ctx).await;
  guard::start(ctx)
}
