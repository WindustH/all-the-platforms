//! LINE for all-the-platforms: the Messaging API of a LINE Official Account, with
//! its channel access token. A bot can push, multicast and broadcast, look
//! up profiles, groups and members, download what users sent, and read its
//! statistics. Incoming messages reach bots only by webhook, so there is no
//! history and no `listen`.

mod account;
mod api;
mod chats;
mod extra;
mod insights;
mod messages;
mod parse;
mod refs;
mod users;

use std::time::Duration;

use atp_core::{
  Action, Cap, Choices, Ctx, Data, Insights, Media, Message, Page, PageReq, Platform, PlatformInfo,
  PlatformKind, Post, Result, User,
};

use crate::api::Api;

pub struct Line {
  api: Api,
}

impl Platform for Line {
  const INFO: PlatformInfo = PlatformInfo {
    id: "line",
    name: "LINE",
    aliases: &[],
    about: "LINE Official Account bots (Messaging API): sending, profiles, members and statistics",
    home: "https://line.me",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::TokenLogin,
      Cap::User,
      Cap::Followers,
      Cap::Send,
      Cap::Members,
      Cap::Download,
      Cap::AccountInsights,
      Cap::PostInsights,
    ],
    choices: Choices::NONE,
    min_interval: Duration::from_millis(100),
    kind: PlatformKind::Chat,
    token: "CHANNEL_ACCESS_TOKEN",
    env: &["LINE_CHANNEL_ACCESS_TOKEN"],
    guest: "nothing",
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
    self.api.save(token);
    Ok(())
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  /// Friends of the bot (the logged-in account only).
  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::followers(&self.api, user, page).await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    messages::send(&self.api, chat, message).await
  }

  async fn members(&self, chat: &str, page: &PageReq) -> Result<Page<User>> {
    users::members(&self.api, chat, page).await
  }

  async fn media(&self, post: &str, _audio_only: bool) -> Result<(Post, Vec<Media>)> {
    messages::content(&self.api, post)
  }

  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    match post {
      Some(request_id) => insights::message(&self.api, request_id).await,
      None => insights::account(&self.api, days).await,
    }
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
