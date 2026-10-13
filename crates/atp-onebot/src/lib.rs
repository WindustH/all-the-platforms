//! OneBot v11 for all-the-platforms: QQ through an implementation the user runs
//! (NapCat, Lagrange.OneBot, LLOneBot, go-cqhttp ...).
//!
//! Actions go over HTTP or the forward websocket ([`api`]); groups and
//! friends are chats (`group:<n>`, `user:<n>`), messages are posts with
//! `CHAT/<message_id>` ids ([`parse`]); history uses the common
//! `get_*_msg_history` extensions and `listen` the event websocket ([`live`]).

mod api;
mod extra;
mod live;
mod ops;
mod parse;
mod refs;

use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Collection, Ctx, Data, Message, Page, PageReq, Platform, PlatformInfo,
  Post, Result, User, json,
};

use crate::api::{Api, Endpoint};

pub struct OneBot {
  api: Api,
}

impl Platform for OneBot {
  const INFO: PlatformInfo = PlatformInfo {
    id: "onebot",
    name: "OneBot",
    aliases: &["qq-onebot", "ob"],
    about: "OneBot v11: QQ groups and friends through NapCat, Lagrange or LLOneBot",
    home: "https://onebot.dev",
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
      Cap::Listen,
    ],
    choices: Choices::NONE,
    min_interval: Duration::ZERO,
    guide: include_str!("../GUIDE.md"),
  };

  type Extra = extra::Command;

  fn new(ctx: Ctx) -> Result<Self> {
    Ok(Self { api: Api::new(ctx) })
  }

  fn ctx(&self) -> &Ctx {
    &self.api.ctx
  }

  async fn whoami(&self) -> Result<User> {
    ops::whoami(&self.api).await
  }

  fn logged_in(&self) -> bool {
    self.api.logged_in()
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    let endpoint = Endpoint::parse(token)?;
    self
      .api
      .call_at(&endpoint, "get_login_info", json!({}))
      .await?;
    self.api.save(&endpoint);
    Ok(())
  }

  async fn read(&self, post: &str) -> Result<Post> {
    ops::read(&self.api, post).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    ops::user(&self.api, user).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    ops::delete(&self.api, post).await
  }

  async fn chats(&self, _parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    ops::chats(&self.api, page).await
  }

  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    ops::messages(&self.api, chat, page).await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    ops::send(&self.api, chat, message).await
  }

  async fn members(&self, chat: &str, page: &PageReq) -> Result<Page<User>> {
    ops::members(&self.api, chat, page).await
  }

  async fn react(&self, post: &str, emoji: &str, undo: bool) -> Result<Action> {
    ops::react(&self.api, post, emoji, undo).await
  }

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    live::listen(&self.api, chats, out).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
