//! WhatsApp for social-cli, in two modes:
//!
//! - a **personal account** as a linked device ([`personal`], whatsapp-rust):
//!   QR-code or pairing-code login, chats, messages, groups, sending,
//!   reactions, edits, deletions and live events. WhatsApp keeps no history
//!   on its servers, so what the device receives is kept in a local archive
//!   ([`archive`]) that `chats`, `messages`, `read` and `search` read;
//! - the **Business Cloud API** ([`cloud`], `login --token
//!   TOKEN:PHONE_NUMBER_ID`): sending text, media and templates, reactions,
//!   read receipts, media, the business profile and messaging analytics.
//!
//! Chats are collections whose `id` is the JID; messages are posts whose
//! `id` is `CHAT/MESSAGE` ([`refs`], [`convert`]).

mod archive;
mod cloud;
mod convert;
mod extra;
mod link;
mod personal;
mod proxy;
mod refs;
mod store;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use social_core::output::Stream;
use social_core::{
  Action, Cap, Choices, Collection, Ctx, Data, Error, Insights, Media, Message, Page, PageReq,
  Platform, PlatformInfo, Post, QrStatus, QrTicket, Query, Result, User,
};

use crate::cloud::Cloud;
use crate::personal::Personal;

pub struct WhatsApp {
  ctx: Ctx,
  personal: Personal,
}

enum Mode {
  Personal,
  Cloud(Cloud),
  None,
}

impl WhatsApp {
  /// A linked device wins; else saved or environment Cloud API credentials.
  fn mode(&self) -> Mode {
    if self.personal.linked(&self.ctx) {
      return Mode::Personal;
    }
    match Cloud::from_ctx(&self.ctx) {
      Some(c) => Mode::Cloud(c),
      None => Mode::None,
    }
  }

  fn not_logged_in(&self) -> Error {
    Error::auth("not logged in to WhatsApp").with_hint(
      "link your account with `social whatsapp login` (QR code) or `social whatsapp login-code +NUMBER`; for the Business Cloud API use `social whatsapp login --token TOKEN:PHONE_NUMBER_ID`",
    )
  }

  fn cloud_cannot(&self, what: &str) -> Error {
    Error::unsupported(what).with_hint(
      "the Cloud API only sends: incoming messages go to a webhook, and there is no chat list or history; link a personal account with `social whatsapp login` for this",
    )
  }

  fn personal_cannot(&self, what: &str) -> Error {
    Error::unsupported(what)
      .with_hint("this is a Business Cloud API feature: `social whatsapp login --token TOKEN:PHONE_NUMBER_ID[:WABA_ID]`")
  }

  /// Run on the linked device, or fail with what each mode allows.
  async fn personal<T>(
    &self,
    what: &str,
    f: impl AsyncFnOnce(&mut link::Link) -> Result<T>,
  ) -> Result<T> {
    match self.mode() {
      Mode::Personal => self.personal.with(&self.ctx, f).await,
      Mode::Cloud(_) => Err(self.cloud_cannot(what)),
      Mode::None => Err(self.not_logged_in()),
    }
  }
}

impl Platform for WhatsApp {
  const INFO: PlatformInfo = PlatformInfo {
    id: "whatsapp",
    name: "WhatsApp",
    aliases: &["wa"],
    about: "WhatsApp: chats, groups and messages of a linked account, or the Business Cloud API",
    home: "https://web.whatsapp.com",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::QrLogin,
      Cap::TokenLogin,
      Cap::Search,
      Cap::Read,
      Cap::User,
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
    choices: Choices::NONE,
    min_interval: Duration::from_millis(300),
    guide: include_str!("../GUIDE.md"),
  };

  type Extra = extra::Command;

  fn new(ctx: Ctx) -> Result<Self> {
    Ok(Self {
      personal: Personal::new(&ctx),
      ctx,
    })
  }

  fn ctx(&self) -> &Ctx {
    &self.ctx
  }

  fn logged_in(&self) -> bool {
    !matches!(self.mode(), Mode::None)
  }

  async fn whoami(&self) -> Result<User> {
    match self.mode() {
      Mode::Personal => {
        self
          .personal
          .with(&self.ctx, async |l| Ok(personal::whoami(l)))
          .await
      }
      Mode::Cloud(c) => c.whoami(&self.ctx).await,
      Mode::None => Err(self.not_logged_in()),
    }
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    let (token, phone, waba) = Cloud::parse_token(token)?;
    self.ctx.set_extra(cloud::TOKEN, &token);
    self.ctx.set_extra(cloud::PHONE, &phone);
    self
      .ctx
      .set_extra(cloud::WABA, waba.as_deref().unwrap_or(""));
    self.ctx.set_extra(personal::DEVICE, "");
    Ok(())
  }

  async fn qr_start(&self) -> Result<QrTicket> {
    self.personal.qr_start().await
  }

  async fn qr_poll(&self, _ticket: &QrTicket) -> Result<QrStatus> {
    let status = self.personal.qr_poll(&self.ctx).await?;
    if status == QrStatus::Confirmed {
      for key in [cloud::TOKEN, cloud::PHONE, cloud::WABA] {
        self.ctx.set_extra(key, "");
      }
    }
    Ok(status)
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    let words = query.keyword.clone();
    self
      .personal("search", async |l| personal::search(l, &words, page))
      .await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    self
      .personal("read", async |l| personal::read(l, post))
      .await
  }

  async fn user(&self, user: &str) -> Result<User> {
    self
      .personal("user", async |l| personal::user(l, user).await)
      .await
  }

  async fn unread(&self) -> Result<BTreeMap<String, u64>> {
    self
      .personal("unread", async |l| Ok(personal::unread(l)))
      .await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    self
      .personal("follow (join a group)", async |l| {
        personal::follow(l, user, undo).await
      })
      .await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    self
      .personal("delete", async |l| personal::delete(l, post).await)
      .await
  }

  async fn chats(&self, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    if parent.is_some() {
      return Err(Error::input(
        "WhatsApp chats have nothing inside them to list; use `messages CHAT` or `members GROUP`",
      ));
    }
    self
      .personal("chats", async |l| personal::chats(l, page).await)
      .await
  }

  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    self
      .personal("messages", async |l| {
        personal::messages(l, chat, page).await
      })
      .await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    match self.mode() {
      Mode::Personal => {
        self
          .personal
          .with(&self.ctx, async |l| personal::send(l, chat, message).await)
          .await
      }
      Mode::Cloud(c) => c.send(&self.ctx, chat, message).await,
      Mode::None => Err(self.not_logged_in()),
    }
  }

  async fn members(&self, chat: &str, page: &PageReq) -> Result<Page<User>> {
    self
      .personal("members", async |l| personal::members(l, chat, page).await)
      .await
  }

  async fn react(&self, post: &str, emoji: &str, undo: bool) -> Result<Action> {
    match self.mode() {
      Mode::Personal => {
        self
          .personal
          .with(&self.ctx, async |l| {
            personal::react(l, post, emoji, undo).await
          })
          .await
      }
      Mode::Cloud(c) => c.react(&self.ctx, post, emoji, undo).await,
      Mode::None => Err(self.not_logged_in()),
    }
  }

  async fn edit(&self, post: &str, text: &str) -> Result<Action> {
    self
      .personal("edit", async |l| personal::edit(l, post, text).await)
      .await
  }

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    self
      .personal("listen", async |l| personal::listen(l, chats, out).await)
      .await
  }

  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    if post.is_some() {
      return Err(Error::unsupported("insights for one message"));
    }
    match self.mode() {
      Mode::Cloud(c) => c.insights(&self.ctx, days).await,
      Mode::Personal => Err(self.personal_cannot("insights")),
      Mode::None => Err(self.not_logged_in()),
    }
  }

  /// Cloud API: a media id from a webhook; personal: an archived message.
  async fn media(&self, post: &str, _audio_only: bool) -> Result<(Post, Vec<Media>)> {
    if let Mode::Cloud(c) = self.mode() {
      let media = c.media(&self.ctx, post).await?;
      let p = Post {
        id: post.to_owned(),
        kind: "media".into(),
        ..Post::default()
      };
      return Ok((p, vec![media]));
    }
    let p = self.read(post).await?;
    let media = p.media.clone();
    if media.is_empty() {
      return Err(Error::not_found("this message has no media"));
    }
    Ok((p, media))
  }

  async fn save_media(&self, media: &Media, dir: &Path, name: &str) -> Option<Result<PathBuf>> {
    let url = media.url.strip_prefix("wa-media:")?;
    Some(self.personal.save_media(url, dir, name).await)
  }

  async fn run_extra(&self, command: extra::Command) -> Result<Data> {
    extra::run(self, command).await
  }
}
