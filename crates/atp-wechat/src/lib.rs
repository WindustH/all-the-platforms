//! Personal WeChat (微信) for all-the-platforms, through WeChat's own ClawBot bot
//! (the iLink bot API, [`api`]).
//!
//! Scanning a QR code with WeChat ([`login`]) attaches a bot to the account:
//! people (the owner first) message it and it answers. There is no history,
//! contact list or group access: messages arrive once through long polling
//! ([`live`]), and replies need the `context_token` of the conversation.
//! So everything received or sent is recorded on this machine ([`local`]);
//! `chats`, `messages`, `read` and `download` work from that record.
//! Chats are the other person's id; messages are `CHAT/MESSAGE`.

mod api;
mod archive;
mod chats;
mod crypto;
mod extra;
mod live;
mod login;
mod media;
mod messages;
mod parse;
mod refs;
mod uploads;

/// The commands only this platform has, for [`Platform::run_extra`](atp_core::Platform::run_extra).
pub use extra::Command;

use std::path::{Path, PathBuf};
use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Collection, Ctx, Data, Error, Media, Message, Page, PageReq, Platform,
  PlatformInfo, PlatformKind, Post, QrStatus, QrTicket, Result, User,
};

use crate::api::Api;

pub struct Wechat {
  api: Api,
  login: login::Login,
}

impl Platform for Wechat {
  const INFO: PlatformInfo = PlatformInfo {
    id: "wechat",
    name: "WeChat",
    aliases: &["weixin", "wx"],
    about: "WeChat (微信) through its official ClawBot: receive and send messages as a bot on your account",
    home: "https://weixin.qq.com",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::QrLogin,
      Cap::Read,
      Cap::Download,
      Cap::Chats,
      Cap::Messages,
      Cap::Send,
      Cap::Listen,
    ],
    choices: Choices::NONE,
    min_interval: Duration::from_millis(300),
    kind: PlatformKind::Chat,
    token: "",
    env: &[],
    guest: "nothing",
    guide: include_str!("../GUIDE.md"),
  };

  type Extra = Command;

  fn new(ctx: Ctx) -> Result<Self> {
    Ok(Self {
      api: Api::new(ctx),
      login: login::Login::default(),
    })
  }

  fn ctx(&self) -> &Ctx {
    &self.api.ctx
  }

  fn logged_in(&self) -> bool {
    self.api.account().is_some()
  }

  async fn whoami(&self) -> Result<User> {
    login::whoami(&self.api).await
  }

  async fn qr_start(&self) -> Result<QrTicket> {
    login::start(&self.api, &self.login).await
  }

  async fn qr_poll(&self, _ticket: &QrTicket) -> Result<QrStatus> {
    login::poll(&self.api, &self.login).await
  }

  async fn chats(&self, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    if parent.is_some() {
      return Err(Error::input("WeChat chats have no channels inside them"));
    }
    let account = self.api.require_account()?;
    Ok(chats::chats(&self.api, &account, page))
  }

  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    let account = self.api.require_account()?;
    chats::messages(&self.api, &account, chat, page)
  }

  async fn read(&self, post: &str) -> Result<Post> {
    let account = self.api.require_account()?;
    chats::read(&self.api, &account, post)
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    let account = self.api.require_account()?;
    messages::send(&self.api, &account, &refs::chat(chat)?, message).await
  }

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    let account = self.api.require_account()?;
    let chats = chats
      .iter()
      .map(|c| refs::chat(c))
      .collect::<Result<Vec<_>>>()?;
    live::listen(&self.api, &account, &chats, out).await
  }

  async fn save_media(&self, media: &Media, dir: &Path, name: &str) -> Option<Result<PathBuf>> {
    if !media.url.starts_with(parse::SCHEME) {
      return None;
    }
    let account = match self.api.require_account() {
      Ok(a) => a,
      Err(e) => return Some(Err(e)),
    };
    Some(media::save(&self.api, &account, media, dir, name).await)
  }

  async fn run_extra(&self, command: extra::Command) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
