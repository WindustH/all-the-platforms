//! Telegram for social-cli, in three modes:
//!
//! - a **user account** over MTProto ([`mt`], grammers): logged in with a QR
//!   code or the phone number ([`login`]); chats, history, search, comments,
//!   members, sending and live updates ([`user`], [`live`]);
//! - a **bot** through the Bot API (`login --token`, [`bot`]): sending,
//!   editing, reactions, admins and updates — bots cannot read history;
//! - **no account**: public channels through the t.me web preview ([`web`]).
//!
//! Chats are collections whose `id` is the Bot API id; messages are posts
//! whose `id` is `CHAT/MESSAGE` and whose `url` is the t.me link when the
//! chat has one ([`refs`], [`convert`]).

mod bot;
mod convert;
mod extra;
mod live;
mod login;
mod mt;
mod prompt;
mod refs;
mod session;
mod stats;
mod user;
mod web;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use social_core::output::Stream;
use social_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Error, Insights, Media, Message, Page,
  PageReq, Platform, PlatformInfo, Post, QrStatus, QrTicket, Query, Result, User,
};

use crate::bot::Bot;
use crate::mt::{Conn, Mt};
use crate::refs::Chat;

pub struct Telegram {
  ctx: Ctx,
  mt: Mt,
}

/// Who is acting.
enum Mode {
  User(Rc<Conn>),
  Bot(Bot),
  Web,
}

impl Telegram {
  /// A saved bot token, else a saved user account, else `TELEGRAM_BOT_TOKEN`,
  /// else no account.
  fn mode(&self) -> Result<Mode> {
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
  async fn as_user<T>(&self, f: impl AsyncFnOnce(&Conn) -> Result<T>) -> Result<T> {
    let conn = mt::require(&self.ctx, &self.mt)?;
    let out = f(&conn).await;
    self.mt.save(&self.ctx);
    out
  }

  fn needs_account(&self, what: &str) -> Error {
    Error::auth(format!("{what} needs a Telegram account")).with_hint(self.ctx.login_hint())
  }

  fn bots_cannot(&self, what: &str) -> Error {
    Error::unsupported(what).with_hint(
      "the Bot API cannot do this (bots only receive messages as updates); log in a user account with `social telegram login`",
    )
  }

  /// A public channel's posts, for modes without history.
  async fn public_posts(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    match refs::chat(chat)? {
      Chat::Username(u) => web::posts(&self.ctx, &u, None, page.cursor.as_deref()).await,
      _ => Err(self.needs_account("reading this chat")),
    }
  }
}

impl Platform for Telegram {
  const INFO: PlatformInfo = PlatformInfo {
    id: "telegram",
    name: "Telegram",
    aliases: &["tg"],
    about: "Telegram: chats, groups, channels and bots",
    home: "https://t.me",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::QrLogin,
      Cap::TokenLogin,
      Cap::Search,
      Cap::SearchUsers,
      Cap::Read,
      Cap::Comments,
      Cap::User,
      Cap::UserPosts,
      Cap::Unread,
      Cap::Follow,
      Cap::Delete,
      Cap::Download,
      Cap::AccountInsights,
      Cap::Chats,
      Cap::Messages,
      Cap::Send,
      Cap::Members,
      Cap::React,
      Cap::Edit,
      Cap::Listen,
    ],
    choices: Choices {
      search_filter: user::FILTERS,
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(500),
    guide: include_str!("../GUIDE.md"),
  };

  type Extra = extra::Command;

  fn new(ctx: Ctx) -> Result<Self> {
    Ok(Self {
      ctx,
      mt: Mt::default(),
    })
  }

  fn ctx(&self) -> &Ctx {
    &self.ctx
  }

  fn logged_in(&self) -> bool {
    Mt::logged_in(&self.ctx) || Bot::from_ctx(&self.ctx).is_some()
  }

  async fn whoami(&self) -> Result<User> {
    match self.mode()? {
      Mode::User(_) => self.as_user(async |c| user::whoami(c).await).await,
      Mode::Bot(b) => b.whoami(&self.ctx).await,
      Mode::Web => Err(self.needs_account("whoami")),
    }
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    if !Bot::valid_token(token) {
      return Err(Error::input(
        "expected a bot token from @BotFather (`123456:ABC…`); user accounts log in with \
         `social telegram login` (QR code) or `social telegram login-phone`",
      ));
    }
    self.ctx.set_extra(bot::TOKEN, token);
    self.ctx.set_extra(mt::KEYS, "");
    self.ctx.set_extra(mt::USER, "");
    Ok(())
  }

  async fn qr_start(&self) -> Result<QrTicket> {
    login::qr_start(&self.ctx, &self.mt).await
  }

  async fn qr_poll(&self, ticket: &QrTicket) -> Result<QrStatus> {
    login::qr_poll(&self.ctx, &self.mt, ticket).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    match self.mode()? {
      Mode::User(_) => {
        let filter = query.filter.as_deref();
        self
          .as_user(async |c| user::search(c, &query.keyword, filter, page).await)
          .await
      }
      Mode::Bot(_) => Err(self.bots_cannot("search")),
      Mode::Web => Err(
        self
          .needs_account("searching all chats")
          .with_hint("without an account, `social telegram search-chat @channel WORDS` searches a public channel"),
      ),
    }
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    match self.mode()? {
      Mode::User(_) => {
        self
          .as_user(async |c| user::search_users(c, &query.keyword, page).await)
          .await
      }
      Mode::Bot(_) => Err(self.bots_cannot("search --type user")),
      Mode::Web => Err(self.needs_account("searching users")),
    }
  }

  async fn read(&self, post: &str) -> Result<Post> {
    match self.mode()? {
      Mode::User(_) => self.as_user(async |c| user::read(c, post).await).await,
      Mode::Bot(_) | Mode::Web => {
        let m = refs::message(post)?;
        match &m.chat {
          Chat::Username(u) => web::post(&self.ctx, u, m.id).await,
          _ => Err(self.needs_account("reading this message")),
        }
      }
    }
  }

  async fn comments(
    &self,
    post: &str,
    _sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    match self.mode()? {
      Mode::User(_) => {
        self
          .as_user(async |c| user::comments(c, post, page).await)
          .await
      }
      Mode::Bot(_) => Err(self.bots_cannot("comments")),
      Mode::Web => Err(self.needs_account("reading comments")),
    }
  }

  async fn user(&self, user: &str) -> Result<User> {
    match self.mode()? {
      Mode::User(_) => self.as_user(async |c| user::user(c, user).await).await,
      Mode::Bot(b) => b.user(&self.ctx, user).await,
      Mode::Web => match refs::chat(user)? {
        Chat::Username(u) => web::profile(&self.ctx, &u).await,
        _ => Err(self.needs_account("looking up ids")),
      },
    }
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    self.messages(user, page).await
  }

  async fn chats(&self, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    match self.mode()? {
      Mode::User(_) => {
        self
          .as_user(async |c| user::chats(c, parent, page).await)
          .await
      }
      Mode::Bot(_) => Err(self.bots_cannot("chats")),
      Mode::Web => Err(self.needs_account("listing chats")),
    }
  }

  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    match self.mode()? {
      Mode::User(_) => {
        self
          .as_user(async |c| user::messages(c, chat, page).await)
          .await
      }
      Mode::Bot(_) | Mode::Web => self.public_posts(chat, page).await,
    }
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    match self.mode()? {
      Mode::User(_) => {
        self
          .as_user(async |c| user::send(c, chat, message).await)
          .await
      }
      Mode::Bot(b) => b.send(&self.ctx, chat, message).await,
      Mode::Web => Err(self.needs_account("sending")),
    }
  }

  async fn members(&self, chat: &str, page: &PageReq) -> Result<Page<User>> {
    match self.mode()? {
      Mode::User(_) => {
        self
          .as_user(async |c| user::members(c, chat, page).await)
          .await
      }
      Mode::Bot(b) => b.members(&self.ctx, chat).await,
      Mode::Web => Err(self.needs_account("listing members")),
    }
  }

  async fn react(&self, post: &str, emoji: &str, undo: bool) -> Result<Action> {
    match self.mode()? {
      Mode::User(_) => {
        self
          .as_user(async |c| user::react(c, post, emoji, undo).await)
          .await
      }
      Mode::Bot(b) => b.react(&self.ctx, post, emoji, undo).await,
      Mode::Web => Err(self.needs_account("reacting")),
    }
  }

  async fn edit(&self, post: &str, text: &str) -> Result<Action> {
    match self.mode()? {
      Mode::User(_) => {
        self
          .as_user(async |c| user::edit(c, post, text).await)
          .await
      }
      Mode::Bot(b) => b.edit(&self.ctx, post, text).await,
      Mode::Web => Err(self.needs_account("editing")),
    }
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    match self.mode()? {
      Mode::User(_) => self.as_user(async |c| user::delete(c, post).await).await,
      Mode::Bot(b) => b.delete(&self.ctx, post).await,
      Mode::Web => Err(self.needs_account("deleting")),
    }
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    match self.mode()? {
      Mode::User(_) => {
        self
          .as_user(async |c| user::join(c, user, undo).await)
          .await
      }
      Mode::Bot(_) => Err(self.bots_cannot("follow (join)")),
      Mode::Web => Err(self.needs_account("joining")),
    }
  }

  async fn unread(&self) -> Result<BTreeMap<String, u64>> {
    match self.mode()? {
      Mode::User(_) => self.as_user(async |c| user::unread(c).await).await,
      Mode::Bot(_) => Err(self.bots_cannot("unread")),
      Mode::Web => Err(self.needs_account("unread")),
    }
  }

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    match self.mode()? {
      Mode::User(conn) => {
        let r = live::user(&self.ctx, &self.mt, &conn, chats, out).await;
        self.mt.save(&self.ctx);
        r
      }
      Mode::Bot(b) => b.listen(&self.ctx, chats, out).await,
      Mode::Web => live::web(&self.ctx, chats, out).await,
    }
  }

  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    let Some(channel) = post else {
      return Err(Error::input(
        "name a channel you administer: `social telegram insights @channel`",
      ));
    };
    match self.mode()? {
      Mode::User(_) => {
        self
          .as_user(async |c| stats::channel(c, channel, days).await)
          .await
      }
      _ => Err(self.needs_account("channel statistics")),
    }
  }

  async fn save_media(&self, media: &Media, dir: &Path, name: &str) -> Option<Result<PathBuf>> {
    if media.url.starts_with("tg-bot-file:") {
      let bot = Bot::from_ctx(&self.ctx)?;
      return Some(bot.save(&self.ctx, media, dir, name).await);
    }
    let post = media.url.strip_prefix("tg-media:")?;
    Some(
      self
        .as_user(async |c| user::save_media(c, post, dir, name).await)
        .await,
    )
  }

  async fn run_extra(&self, command: extra::Command) -> Result<Data> {
    extra::run(self, command).await
  }
}
