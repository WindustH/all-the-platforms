//! WeCom (企业微信) for all-the-platforms.
//!
//! A corp app ([`api`]): messages to users, departments, tags, app group
//! chats and 微信客服 customers, recalls, 微信客服 conversations, group chat
//! and department members; group robot webhooks without any login; and the
//! AI bot's long connection for live messages ([`live`]).

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

pub struct WeCom {
  api: Api,
}

impl Platform for WeCom {
  const INFO: PlatformInfo = PlatformInfo {
    id: "wecom",
    name: "WeCom",
    aliases: &["wxwork", "qywx"],
    about: "WeCom (企业微信): app messages, group chats, 微信客服, robot webhooks and an AI bot's live messages",
    home: "https://work.weixin.qq.com",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::TokenLogin,
      Cap::User,
      Cap::Delete,
      Cap::Chats,
      Cap::Messages,
      Cap::Send,
      Cap::Members,
      Cap::Listen,
    ],
    choices: Choices::NONE,
    min_interval: Duration::from_millis(250),
    kind: PlatformKind::Chat,
    token: "CORPID:SECRET:AGENTID",
    env: &[
      "WECOM_CORP_ID",
      "WECOM_SECRET",
      "WECOM_AGENT_ID",
      "WECOM_BOT_ID",
      "WECOM_BOT_SECRET",
    ],
    guest: "send to group robot webhooks",
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
    login::login_token(&self.api, token)
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

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    live::listen(&self.api, chats, out).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
