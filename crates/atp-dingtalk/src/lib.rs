//! DingTalk (钉钉) for all-the-platforms.
//!
//! An internal app's robot ([`api`]): it sends to groups and users, recalls
//! and reads back the status of what it sent, and receives messages live
//! through Stream mode ([`live`]); robot webhooks send without any login.
//! Robots have no access to chat history or chat lists.

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
mod webhooks;

use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Ctx, Data, Message, Page, PageReq, Platform, PlatformInfo, PlatformKind,
  Post, Result, User,
};

use crate::api::Api;

pub struct DingTalk {
  api: Api,
}

impl Platform for DingTalk {
  const INFO: PlatformInfo = PlatformInfo {
    id: "dingtalk",
    name: "DingTalk",
    aliases: &["dd", "ding"],
    about: "DingTalk (钉钉): an app robot's messages to groups and users, robot webhooks and live messages",
    home: "https://www.dingtalk.com",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::TokenLogin,
      Cap::Read,
      Cap::User,
      Cap::Delete,
      Cap::Send,
      Cap::Members,
      Cap::Listen,
    ],
    choices: Choices::NONE,
    min_interval: Duration::from_millis(250),
    kind: PlatformKind::Chat,
    token: "APPKEY:APPSECRET",
    env: &[
      "DINGTALK_APP_KEY",
      "DINGTALK_APP_SECRET",
      "DINGTALK_CLIENT_ID",
      "DINGTALK_CLIENT_SECRET",
      "DINGTALK_ROBOT_CODE",
      "DINGTALK_WEBHOOK_SECRET",
    ],
    guest: "send to group robot webhooks",
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
    users::whoami(&self.api).await
  }

  fn logged_in(&self) -> bool {
    self.api.logged_in()
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    login::login_token(&self.api, token)
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
