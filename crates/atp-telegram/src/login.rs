//! Logging in a user account: a QR code scanned in the Telegram app
//! (Settings → Devices → Link Desktop Device), or the phone number and the
//! code Telegram sends; either may end with the two-step verification
//! password. Bots log in with `login --token` instead ([`crate::bot`]).
//!
//! The QR login exports a login token (`tg://login?token=…`, valid about 30
//! seconds), waits for the `updateLoginToken` push that follows the scan,
//! and exports again to collect the authorization — importing it in the
//! account's home datacenter when Telegram says it lives elsewhere.

use atp_core::output::note;
use atp_core::{Ctx, Error, QrStatus, QrTicket, Result, User};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use grammers_client::session::types::PeerInfo;
use grammers_client::{SignInError, tl};
use grammers_session::Session as _;
use grammers_session::updates::UpdatesLike;

use crate::api::Api;
use crate::bot::{self, Bot};
use crate::mt::{self, Conn, Mt, error};
use crate::parse;
use crate::prompt;

/// Start a QR login on a fresh connection.
pub async fn qr_start(api: &Api) -> Result<QrTicket> {
  let (ctx, mt) = (&api.ctx, &api.mt);
  let conn = mt.fresh(ctx)?;
  match export(&conn).await? {
    Exported::Token(url, expires) => {
      *mt.qr.borrow_mut() = (url.clone(), expires);
      Ok(QrTicket {
        url: url.clone(),
        token: url,
        ..QrTicket::default()
      })
    }
    Exported::Done => {
      finish(ctx, mt, &conn).await?;
      Ok(QrTicket {
        url: "tg://login".into(),
        extra: [("done".to_owned(), "1".to_owned())].into(),
        ..QrTicket::default()
      })
    }
  }
}

pub async fn qr_poll(api: &Api, ticket: &QrTicket) -> Result<QrStatus> {
  let (ctx, mt) = (&api.ctx, &api.mt);
  if ticket.extra.contains_key("done") {
    return Ok(QrStatus::Confirmed);
  }
  let conn = mt.conn(ctx)?;
  let scanned = drain_login_updates(&conn);
  let (shown, expires) = mt.qr.borrow().clone();
  let now = jiff::Timestamp::now().as_second();
  if !scanned && now < expires {
    return Ok(QrStatus::Waiting);
  }
  match export(&conn).await {
    Ok(Exported::Done) => {
      finish(ctx, mt, &conn).await?;
      Ok(QrStatus::Confirmed)
    }
    Ok(Exported::Token(url, expires)) => {
      if url != shown {
        atp_core::qr::show(&url, ctx.store.cache_dir())?;
        note("The QR code expired; scan the new one above.");
      }
      *mt.qr.borrow_mut() = (url, expires);
      Ok(QrStatus::Waiting)
    }
    Err(e) if e.message.contains("SESSION_PASSWORD_NEEDED") => {
      password(&conn).await?;
      finish(ctx, mt, &conn).await?;
      Ok(QrStatus::Confirmed)
    }
    Err(e) => Err(e),
  }
}

enum Exported {
  /// The `tg://login` link to show, and when it expires (unix seconds).
  Token(String, i64),
  Done,
}

async fn export(conn: &Conn) -> Result<Exported> {
  let request = tl::functions::auth::ExportLoginToken {
    api_id: conn.app.id,
    api_hash: conn.app.hash.clone(),
    except_ids: Vec::new(),
  };
  let result = conn.client.invoke(&request).await.map_err(error)?;
  match result {
    tl::enums::auth::LoginToken::Token(t) => Ok(Exported::Token(
      format!("tg://login?token={}", URL_SAFE_NO_PAD.encode(&t.token)),
      t.expires.into(),
    )),
    tl::enums::auth::LoginToken::Success(s) => {
      authorized(conn, s.authorization).await?;
      Ok(Exported::Done)
    }
    tl::enums::auth::LoginToken::MigrateTo(m) => {
      conn
        .session
        .set_home_dc_id(m.dc_id)
        .await
        .map_err(|e| Error::internal(e.to_string()))?;
      let import = tl::functions::auth::ImportLoginToken { token: m.token };
      match conn.client.invoke(&import).await.map_err(error)? {
        tl::enums::auth::LoginToken::Success(s) => {
          authorized(conn, s.authorization).await?;
          Ok(Exported::Done)
        }
        _ => Err(Error::auth("Telegram did not accept the login token")),
      }
    }
  }
}

/// Whether Telegram announced the scan since the last look.
fn drain_login_updates(conn: &Conn) -> bool {
  let Some(mut updates) = conn.take_updates() else {
    return false;
  };
  let mut seen = false;
  while let Ok(u) = updates.try_recv() {
    if let UpdatesLike::Updates(u) = u {
      let list = match u {
        tl::enums::Updates::UpdateShort(s) => vec![s.update],
        tl::enums::Updates::Updates(u) => u.updates,
        tl::enums::Updates::Combined(u) => u.updates,
        _ => Vec::new(),
      };
      seen |= list
        .iter()
        .any(|u| matches!(u, tl::enums::Update::LoginToken));
    }
  }
  conn.put_updates(updates);
  seen
}

/// Remember the account an authorization belongs to.
async fn authorized(conn: &Conn, auth: tl::enums::auth::Authorization) -> Result<()> {
  match auth {
    tl::enums::auth::Authorization::Authorization(a) => {
      conn
        .session
        .cache_peer(&PeerInfo::from(a.user))
        .await
        .map_err(|e| Error::internal(e.to_string()))?;
      Ok(())
    }
    tl::enums::auth::Authorization::SignUpRequired(_) => Err(Error::auth(
      "this phone number has no Telegram account; sign up in the app first",
    )),
  }
}

/// The two-step verification password, asked on the terminal.
async fn password(conn: &Conn) -> Result<()> {
  let tl::enums::account::Password::Password(p) = conn
    .client
    .invoke(&tl::functions::account::GetPassword {})
    .await
    .map_err(error)?;
  let token = grammers_client::client::PasswordToken::new(p);
  let hint = token
    .hint()
    .map(|h| format!(" (hint: {h})"))
    .unwrap_or_default();
  let password = prompt::secret(&format!("Two-step verification password{hint}: "))?;
  conn
    .client
    .check_password(token, password)
    .await
    .map_err(sign_in_error)?;
  Ok(())
}

/// Write the keys and the account into the session.
async fn finish(ctx: &Ctx, mt: &Mt, conn: &Conn) -> Result<()> {
  let me = conn.client.get_me().await.map_err(error)?;
  conn
    .session
    .cache_peer(&PeerInfo::from(me.raw.clone()))
    .await
    .map_err(|e| Error::internal(e.to_string()))?;
  // A user login replaces a bot token.
  ctx.set_extra(crate::bot::TOKEN, "");
  mt.save(ctx);
  ctx.set_extra(mt::USER, &parse::dialog_id(me.id()).to_string());
  Ok(())
}

/// `login-phone`: the code Telegram sends to the app or by SMS.
pub async fn phone(api: &Api, number: Option<String>) -> Result<User> {
  let (ctx, mt) = (&api.ctx, &api.mt);
  let number = match number {
    Some(n) => n,
    None => prompt::line("Phone number (international format, e.g. +44…): ")?,
  };
  let conn = mt.fresh(ctx)?;
  let token = conn
    .client
    .request_login_code(number.trim(), &conn.app.hash)
    .await
    .map_err(sign_in_error_rpc)?;
  let code = prompt::line("Login code (sent to your Telegram app or by SMS): ")?;
  match conn.client.sign_in(&token, &code).await {
    Ok(_) => {}
    Err(SignInError::PasswordRequired(token)) => {
      let hint = token
        .hint()
        .map(|h| format!(" (hint: {h})"))
        .unwrap_or_default();
      let password = prompt::secret(&format!("Two-step verification password{hint}: "))?;
      conn
        .client
        .check_password(token, password)
        .await
        .map_err(sign_in_error)?;
    }
    Err(e) => return Err(sign_in_error(e)),
  }
  finish(ctx, mt, &conn).await?;
  let me = conn.client.get_me().await.map_err(error)?;
  Ok(parse::user(&me))
}

fn sign_in_error(e: SignInError) -> Error {
  match e {
    SignInError::InvalidCode => Error::auth("the login code is wrong"),
    SignInError::InvalidPassword(_) => Error::auth("the password is wrong"),
    SignInError::SignUpRequired => {
      Error::auth("this phone number has no Telegram account; sign up in the app first")
    }
    SignInError::PasswordRequired(_) => Error::auth("the two-step verification password is needed"),
    SignInError::Other(e) => error(e),
  }
}

fn sign_in_error_rpc(e: grammers_client::InvocationError) -> Error {
  error(e).with_hint("check the number (international format with +)")
}

/// `login --token`: a bot token from @BotFather; it replaces a user account.
pub fn token(api: &Api, token: &str) -> Result<()> {
  if !Bot::valid_token(token) {
    return Err(Error::input(
      "expected a bot token from @BotFather (`123456:ABC…`); user accounts log in with \
       `atp telegram login` (QR code) or `atp telegram login-phone`",
    ));
  }
  api.ctx.set_extra(bot::TOKEN, token);
  api.ctx.set_extra(mt::KEYS, "");
  api.ctx.set_extra(mt::USER, "");
  Ok(())
}
