//! KOOK (开黑啦) for all-the-platforms, with a bot token.
//!
//! REST v3 ([`api`]); servers are `guild:<id>` chats, channels and direct
//! chats (`dm:<user>`) hold messages with `CHAT/<message>` ids ([`parse`]);
//! `listen` runs the websocket gateway ([`live`]).

mod api;
mod chats;
mod extra;
mod live;
mod login;
mod messages;
mod parse;
mod refs;
mod uploads;
mod users;

/// The commands only this platform has, for [`Platform::run_extra`](atp_core::Platform::run_extra).
pub use extra::Command;

use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Collection, Ctx, Data, Message, Page, PageReq, Platform, PlatformInfo,
  PlatformKind, Post, Result, User,
};

use crate::api::Api;

pub struct Kook {
  api: Api,
}

impl Platform for Kook {
  const INFO: PlatformInfo = PlatformInfo {
    id: "kook",
    name: "KOOK",
    aliases: &["kaiheila", "khl"],
    about: "KOOK (开黑啦): servers, channels and direct chats, with a bot",
    home: "https://www.kookapp.cn",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::TokenLogin,
      Cap::Read,
      Cap::User,
      Cap::Delete,
      Cap::Download,
      Cap::Chats,
      Cap::Messages,
      Cap::Send,
      Cap::Members,
      Cap::React,
      Cap::Edit,
      Cap::Listen,
    ],
    choices: Choices::NONE,
    min_interval: Duration::from_millis(200),
    kind: PlatformKind::Chat,
    token: "BOT_TOKEN",
    env: &["KOOK_TOKEN"],
    guest: "nothing",
    guide: include_str!("../GUIDE.md"),
  };

  type Extra = Command;

  fn new(ctx: Ctx) -> Result<Self> {
    Ok(Self { api: Api::new(ctx) })
  }

  fn ctx(&self) -> &Ctx {
    &self.api.ctx
  }

  async fn whoami(&self) -> Result<User> {
    users::whoami(&self.api).await
  }

  fn logged_in(&self) -> bool {
    self.api.logged_in()
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    login::login_token(&self.api, token).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    messages::read(&self.api, post).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    messages::delete(&self.api, post).await
  }

  async fn chats(&self, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    chats::list(&self.api, parent, page).await
  }

  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    messages::list(&self.api, chat, page).await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    messages::send(&self.api, chat, message).await
  }

  async fn members(&self, chat: &str, page: &PageReq) -> Result<Page<User>> {
    chats::members(&self.api, chat, page).await
  }

  async fn react(&self, post: &str, emoji: &str, undo: bool) -> Result<Action> {
    messages::react(&self.api, post, emoji, undo).await
  }

  async fn edit(&self, post: &str, text: &str) -> Result<Action> {
    messages::edit(&self.api, post, text).await
  }

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    live::listen(&self.api, chats, out).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
