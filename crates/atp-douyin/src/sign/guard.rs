//! The ticket-guard material of a login, kept in the session: the key ATP
//! registered with the passport and the ticket the passport issued for it,
//! bound to the `sessionid` of that login. Writes are refused without it:
//! a session copied from a browser (`--browser`, `--cookie`) lacks the
//! browser's private key.

use atp_bytedance::ticket_guard::{
  self, CLIENT_DATA_COOKIE, Key, SERVER_DATA_COOKIE, Ticket, WEB_DOMAIN_COOKIE,
};
use atp_core::{Ctx, Error, Result};

const KEY: &str = "ticket_guard_key";
const TICKET: &str = "ticket_guard_ticket";
const TS_SIGN: &str = "ticket_guard_ts_sign";
const CLIENT_CERT: &str = "ticket_guard_client_cert";
/// The `sessionid` the ticket was issued with.
const SESSION: &str = "ticket_guard_session";

const HINT: &str = "writes need a login made by ATP: run `atp douyin login` (QR code) or \
   `atp douyin login-sms`; sessions copied with `--browser` or `--cookie` lack the browser's key";

/// Before a login: a new key, handed to the passport in cookies; the
/// ticket of an earlier login is dropped.
pub fn start(ctx: &Ctx) -> Result<()> {
  let key = Key::generate()?;
  ctx
    .http
    .set_cookie(CLIENT_DATA_COOKIE, &key.client_data_cookie());
  ctx.http.set_cookie(WEB_DOMAIN_COOKIE, "2");
  ctx.set_extra(KEY, &key.to_base64());
  for name in [TICKET, TS_SIGN, CLIENT_CERT, SESSION] {
    ctx.set_extra(name, "");
  }
  Ok(())
}

/// Keep the ticket of a passport answer (`bd-ticket-guard-server-data`).
pub fn keep(ctx: &Ctx, server_data: Option<&str>) {
  let ticket = server_data
    .or(ctx.http.cookie(SERVER_DATA_COOKIE).as_deref())
    .and_then(Ticket::from_server_data);
  let Some(ticket) = ticket else {
    return;
  };
  ctx.set_extra(TICKET, &ticket.ticket);
  ctx.set_extra(TS_SIGN, &ticket.ts_sign);
  ctx.set_extra(CLIENT_CERT, &ticket.client_cert);
}

/// After a login: tie the ticket to the session it came with.
pub fn bind(ctx: &Ctx) {
  if ctx.extra(TICKET).is_some_and(|t| !t.is_empty()) {
    let session = ctx.http.cookie("sessionid").unwrap_or_default();
    ctx.set_extra(SESSION, &session);
  } else {
    tracing::debug!("the login answered without a ticket-guard ticket; writes stay unavailable");
  }
}

/// The key and ticket for writes with the current session.
pub struct Guard {
  key: Key,
  ticket: Ticket,
}

impl Guard {
  pub fn load(ctx: &Ctx) -> Result<Self> {
    let value = |name| ctx.extra(name).filter(|v| !v.is_empty());
    let (Some(key), Some(ticket), Some(session)) = (value(KEY), value(TICKET), value(SESSION))
    else {
      return Err(Error::auth("this session cannot sign writes").with_hint(HINT));
    };
    if ctx.http.cookie("sessionid").as_deref() != Some(session.as_str()) {
      return Err(
        Error::auth("the write key belongs to another login of this session").with_hint(HINT),
      );
    }
    Ok(Self {
      key: Key::from_base64(&key)?,
      ticket: Ticket {
        ticket,
        ts_sign: value(TS_SIGN).unwrap_or_default(),
        client_cert: value(CLIENT_CERT).unwrap_or_default(),
      },
    })
  }

  /// Headers of a write the creator platform's consumer SDK guards
  /// (publishing, deleting), signed at `timestamp`.
  pub fn consumer_headers(
    &self,
    path: &str,
    timestamp: i64,
    trusted: bool,
  ) -> Result<Vec<(&'static str, String)>> {
    ticket_guard::consumer_headers(&self.key, &self.ticket, path, timestamp, trusted)
  }

  /// Headers of a write: signed over the path for comments, naming the key
  /// for the rest.
  pub fn headers(&self, path: &str, signed: bool) -> Result<Vec<(&'static str, String)>> {
    if signed {
      let now = jiff::Timestamp::now().as_second();
      ticket_guard::write_headers(&self.key, &self.ticket, path, now)
    } else {
      Ok(ticket_guard::key_headers(&self.key, &self.ticket))
    }
  }
}
