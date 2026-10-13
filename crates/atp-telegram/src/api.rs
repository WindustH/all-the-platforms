//! The transport: the context and the three routes a command can take — the
//! user account over MTProto ([`Mt`]), a bot ([`Bot`]), or no account (the
//! t.me web preview) — and which of them runs a command.

use std::rc::Rc;

use atp_core::{Ctx, Error, Result};

use crate::bot::{self, Bot};
use crate::mt::{self, Conn, Mt};

pub struct Api {
  pub ctx: Ctx,
  pub mt: Mt,
}

/// Who is acting.
pub enum Mode {
  User(Rc<Conn>),
  Bot(Bot),
  Web,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self {
      ctx,
      mt: Mt::default(),
    }
  }

  pub fn logged_in(&self) -> bool {
    Mt::logged_in(&self.ctx) || Bot::from_ctx(&self.ctx).is_some()
  }

  /// A saved bot token, else a saved user account, else `TELEGRAM_BOT_TOKEN`,
  /// else no account.
  pub fn mode(&self) -> Result<Mode> {
    let ctx = &self.ctx;
    let saved_bot = ctx.extra(bot::TOKEN).is_some_and(|t| !t.is_empty());
    if !saved_bot && Mt::logged_in(ctx) {
      return Ok(Mode::User(self.mt.conn(ctx)?));
    }
    Ok(match Bot::from_ctx(ctx) {
      Some(b) => Mode::Bot(b),
      None => Mode::Web,
    })
  }

  /// Run as the user account, then keep what the session learned.
  pub async fn as_user<T>(&self, f: impl AsyncFnOnce(&Conn) -> Result<T>) -> Result<T> {
    let conn = mt::require(&self.ctx, &self.mt)?;
    let out = f(&conn).await;
    self.mt.save(&self.ctx);
    out
  }

  pub fn needs_account(&self, what: &str) -> Error {
    Error::auth(format!("{what} needs a Telegram account")).with_hint(self.ctx.login_hint())
  }
}

pub fn bots_cannot(what: &str) -> Error {
  Error::unsupported(what).with_hint(
    "the Bot API cannot do this (bots only receive messages as updates); log in a user account with `atp telegram login`",
  )
}
