//! The user account over MTProto, through grammers.
//!
//! The connection opens on first use: grammers' sender pool runs as a task
//! of the current-thread runtime and the [`Client`] talks to it. The session
//! ([`MtSession`]) lives in memory; [`Mt::save`] writes its authorization keys
//! into the session extras (`mtproto`, private like cookies) and its peer
//! cache into the private cache, so later runs can address chats by id.
//!
//! The app identity comes from `TELEGRAM_API_ID` / `TELEGRAM_API_HASH`
//! (<https://my.telegram.org>); without them it is Telegram Desktop's public
//! pair, as tg-cli does.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use grammers_client::session::types::{PeerId, PeerInfo, PeerRef};
use grammers_client::{Client, InvocationError, SenderPool};
use grammers_mtsender::ConnectionParams;
use grammers_session::updates::UpdatesLike;
use social_core::{Ctx, Error, ErrorCode, Result};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::refs::Chat;
use crate::session::{Keys, MtSession};

/// Session extras: the keys, and the id of the logged-in user.
pub const KEYS: &str = "mtproto";
pub const USER: &str = "user_id";
const PEERS: &str = "peers";
const PEERS_TTL: Duration = Duration::from_secs(10 * 365 * 24 * 3600);

/// Telegram Desktop's public app identity (also used by tg-cli).
const DESKTOP_ID: i32 = 2040;
const DESKTOP_HASH: &str = "b18441a1ff607e10a989891a5462e627";

pub struct App {
  pub id: i32,
  pub hash: String,
}

pub fn app() -> Result<App> {
  let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
  match (var("TELEGRAM_API_ID"), var("TELEGRAM_API_HASH")) {
    (Some(id), Some(hash)) => Ok(App {
      id: id
        .trim()
        .parse()
        .map_err(|_| Error::input("TELEGRAM_API_ID must be a number"))?,
      hash: hash.trim().to_owned(),
    }),
    (None, None) => Ok(App {
      id: DESKTOP_ID,
      hash: DESKTOP_HASH.to_owned(),
    }),
    _ => Err(Error::input(
      "set both TELEGRAM_API_ID and TELEGRAM_API_HASH (from https://my.telegram.org), or neither",
    )),
  }
}

/// An open connection.
pub struct Conn {
  pub client: Client,
  pub session: Arc<MtSession>,
  pub app: App,
  updates: RefCell<Option<UnboundedReceiver<UpdatesLike>>>,
}

impl Conn {
  /// The updates Telegram pushes on this connection (taken once).
  pub fn take_updates(&self) -> Option<UnboundedReceiver<UpdatesLike>> {
    self.updates.borrow_mut().take()
  }

  pub fn put_updates(&self, updates: UnboundedReceiver<UpdatesLike>) {
    *self.updates.borrow_mut() = Some(updates);
  }
}

#[derive(Default)]
pub struct Mt {
  conn: RefCell<Option<Rc<Conn>>>,
  /// The QR login code on screen and when it expires (unix seconds).
  pub qr: RefCell<(String, i64)>,
}

impl Mt {
  /// Whether the session holds a logged-in user account.
  pub fn logged_in(ctx: &Ctx) -> bool {
    let has = |k: &str| ctx.extra(k).is_some_and(|v| !v.is_empty());
    has(KEYS) && has(USER)
  }

  /// The connection, opened with the saved keys (or none, for a login).
  pub fn conn(&self, ctx: &Ctx) -> Result<Rc<Conn>> {
    if let Some(conn) = self.conn.borrow().as_ref() {
      return Ok(Rc::clone(conn));
    }
    let keys = ctx
      .extra(KEYS)
      .filter(|k| !k.is_empty())
      .and_then(|k| serde_json::from_str::<Keys>(&k).ok());
    let peers: Vec<PeerInfo> = ctx.store.cache_get(PEERS, PEERS_TTL).unwrap_or_default();
    self.open(Arc::new(MtSession::new(keys, peers)))
  }

  /// A fresh connection with no keys, replacing any open one (logins).
  pub fn fresh(&self) -> Result<Rc<Conn>> {
    self.open(Arc::new(MtSession::new(None, Vec::new())))
  }

  fn open(&self, session: Arc<MtSession>) -> Result<Rc<Conn>> {
    let app = app()?;
    let params = ConnectionParams {
      device_model: "social-cli".into(),
      app_version: env!("CARGO_PKG_VERSION").into(),
      ..ConnectionParams::default()
    };
    let SenderPool {
      runner,
      handle,
      updates,
    } = SenderPool::with_configuration(Arc::clone(&session), app.id, params);
    tokio::spawn(runner.run());
    let conn = Rc::new(Conn {
      client: Client::new(handle),
      session,
      app,
      updates: RefCell::new(Some(updates)),
    });
    *self.conn.borrow_mut() = Some(Rc::clone(&conn));
    Ok(conn)
  }

  /// Write what changed back: keys into the session extras, peers into the cache.
  pub fn save(&self, ctx: &Ctx) {
    let Some(conn) = self.conn.borrow().as_ref().map(Rc::clone) else {
      return;
    };
    let (keys, peers) = conn.session.take_changes();
    if keys && let Some(k) = conn.session.keys()
      && let Ok(json) = serde_json::to_string(&k) {
        ctx.set_extra(KEYS, &json);
      }
    if peers {
      ctx.store.cache_put(PEERS, &conn.session.peers());
    }
    if let Some(PeerInfo::User { id, .. }) = conn.session.self_user() {
      ctx.set_extra(USER, &id.to_string());
    }
  }
}

/// A logged-in connection, or `not_authenticated`.
pub fn require(ctx: &Ctx, mt: &Mt) -> Result<Rc<Conn>> {
  if !Mt::logged_in(ctx) {
    return Err(Error::auth("not logged in to a Telegram account").with_hint(ctx.login_hint()));
  }
  mt.conn(ctx)
}

/// Map grammers errors onto social-cli's.
pub fn error(e: InvocationError) -> Error {
  match e {
    InvocationError::Rpc(r) => {
      let name = r.name.as_str();
      let text = format!("Telegram: {name}");
      match r.code {
        401 => Error::auth(text).with_hint("the session ended; run `social telegram login`"),
        420 => Error::new(
          ErrorCode::RateLimited,
          format!("Telegram asks to wait {} s ({name})", r.value.unwrap_or(0)),
        ),
        403 => Error::new(ErrorCode::PermissionDenied, text),
        400
          if [
            "USERNAME_NOT_OCCUPIED",
            "USERNAME_INVALID",
            "PEER_ID_INVALID",
            "CHANNEL_INVALID",
            "CHANNEL_PRIVATE",
            "MSG_ID_INVALID",
            "MESSAGE_ID_INVALID",
            "CHAT_ID_INVALID",
            "USER_ID_INVALID",
          ]
          .contains(&name) =>
        {
          Error::not_found(text)
        }
        400 => Error::input(text),
        _ => Error::upstream(text),
      }
    }
    InvocationError::Io(e) => Error::network(format!("Telegram connection: {e}")),
    other => Error::upstream(format!("Telegram: {other}")),
  }
}

/// Errors of grammers helpers that are boxed.
pub fn boxed(e: Box<dyn std::error::Error + Send + Sync>) -> Error {
  match e.downcast::<InvocationError>() {
    Ok(e) => error(*e),
    Err(e) => Error::upstream(format!("Telegram: {e}")),
  }
}

/// A chat reference as a peer the account may address.
pub async fn peer(conn: &Conn, chat: &Chat) -> Result<PeerRef> {
  let client = &conn.client;
  match chat {
    Chat::Username(u) if u.eq_ignore_ascii_case("me") || u.eq_ignore_ascii_case("self") => {
      cached(conn, PeerId::self_user())
        .await?
        .ok_or_else(|| Error::auth("the logged-in user is unknown; run `social telegram whoami`"))
    }
    Chat::Username(u) => {
      let peer = client
        .resolve_username(u)
        .await
        .map_err(error)?
        .ok_or_else(|| Error::not_found(format!("no Telegram user, group or channel @{u}")))?;
      peer
        .to_ref()
        .await
        .map_err(boxed)?
        .ok_or_else(|| Error::not_found(format!("@{u} cannot be addressed")))
    }
    Chat::Id(id) => {
      let peer_id = PeerId::from_bot_api_dialog_id(*id)
        .ok_or_else(|| Error::input(format!("not a Telegram chat id: {id}")))?;
      if let Some(r) = cached(conn, peer_id).await? {
        return Ok(r);
      }
      // Basic groups need no access hash.
      if *id < 0 && *id > -1_000_000_000_000 {
        return Ok(peer_id.to_ambient_ref());
      }
      Err(
        Error::not_found(format!("chat {id} is not known to this session yet"))
          .with_hint("list it once with `social telegram chats` (or use its @name / link)"),
      )
    }
    Chat::Invite(hash) => invite_peer(conn, hash).await,
  }
}

async fn cached(conn: &Conn, id: PeerId) -> Result<Option<PeerRef>> {
  use grammers_session::Session as _;
  conn
    .session
    .peer_ref(id)
    .await
    .map_err(|e| Error::internal(e.to_string()))
}

/// The chat behind an invite link, when the account is in it already.
async fn invite_peer(conn: &Conn, hash: &str) -> Result<PeerRef> {
  use grammers_client::tl;
  let check = tl::functions::messages::CheckChatInvite {
    hash: hash.to_owned(),
  };
  match conn.client.invoke(&check).await.map_err(error)? {
    tl::enums::ChatInvite::Already(a) => Ok(PeerRef::from(a.chat)),
    tl::enums::ChatInvite::Peek(p) => Ok(PeerRef::from(p.chat)),
    tl::enums::ChatInvite::Invite(i) => Err(Error::input(format!(
      "not a member of \"{}\" yet; join it with `social telegram follow https://t.me/+{hash}`",
      i.title
    ))),
  }
}
