//! DingTalk (钉钉) for social-cli.
//!
//! An internal app's robot ([`api`]): it sends to groups and users, recalls
//! and reads back the status of what it sent, and receives messages live
//! through Stream mode ([`live`]); robot webhooks send without any login.
//! Robots have no access to chat history or chat lists.

mod api;
mod extra;
mod groups;
mod live;
mod messages;
mod refs;

use std::time::Duration;

use social_core::output::Stream;
use social_core::{
  Action, Cap, Choices, Ctx, Data, Message, Page, PageReq, Platform, PlatformInfo, Post, Result,
  User, Value,
};

use crate::api::{Api, Creds};

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
    let creds = self.api.require()?;
    self.api.token().await?;
    Ok(User {
      id: creds.robot_code.clone(),
      name: format!("DingTalk app {}", creds.app_key),
      extra: [
        ("app_key".to_owned(), Value::from(creds.app_key)),
        ("robot_code".to_owned(), Value::from(creds.robot_code)),
      ]
      .into_iter()
      .collect(),
      ..User::default()
    })
  }

  fn logged_in(&self) -> bool {
    self.api.creds().is_some()
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    self.api.save(&Creds::parse(token)?);
    Ok(())
  }

  async fn read(&self, post: &str) -> Result<Post> {
    messages::read(&self.api, post).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    messages::user(&self.api, user).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    messages::delete(&self.api, post).await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    messages::send(&self.api, chat, message).await
  }

  async fn members(&self, chat: &str, page: &PageReq) -> Result<Page<User>> {
    groups::members(&self.api, chat, page).await
  }

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    live::listen(&self.api, chats, out).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
