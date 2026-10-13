//! QQ official bots (QQ 机器人) for social-cli.
//!
//! AppID + AppSecret buy an access token ([`api`]). Servers (频道) and
//! their sub-channels are listable chats; groups and C2C chats are known by
//! the openids events carry ([`refs`]). There is no history API: messages
//! come through `listen` (the gateway, [`live`]) and single sub-channel
//! lookups, and replies to them are passive messages ([`ops`]).

mod api;
mod extra;
mod live;
mod ops;
mod parse;
mod refs;

use std::time::Duration;

use social_core::output::Stream;
use social_core::{
  Action, Cap, Choices, Collection, Ctx, Data, Message, Page, PageReq, Platform, PlatformInfo,
  Post, Result, User,
};

use crate::api::{Api, Credentials};

pub struct Qq {
  api: Api,
}

impl Platform for Qq {
  const INFO: PlatformInfo = PlatformInfo {
    id: "qq",
    name: "QQ bot",
    aliases: &["qqbot"],
    about: "QQ official bots: 频道 servers and channels, groups, C2C chats and events",
    home: "https://q.qq.com",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::TokenLogin,
      Cap::Read,
      Cap::User,
      Cap::Delete,
      Cap::Download,
      Cap::Chats,
      Cap::Send,
      Cap::Members,
      Cap::React,
      Cap::Listen,
    ],
    choices: Choices::NONE,
    min_interval: Duration::from_millis(200),
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
    let creds = Credentials::parse(token)?;
    self.api.access_token(&creds).await?;
    self.api.save(&creds);
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

  async fn chats(&self, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    ops::chats(&self.api, parent, page).await
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
