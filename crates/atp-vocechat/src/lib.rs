//! VoceChat for all-the-platforms: a self-hosted chat server, as a user (password
//! login, token renewed as it expires) or as a bot (API key: sending and
//! lookups only) ([`api`], [`login`]). Channels and direct chats are
//! collections ([`chats`]), messages are posts with `<chat>/<mid>` ids
//! ([`messages`]), `listen` reads the user event stream ([`live`]).

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

use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Collection, Ctx, Data, Media, Message, Page, PageReq, Platform,
  PlatformInfo, Post, Result, User,
};

use crate::api::Api;

pub struct VoceChat {
  api: Api,
}

impl Platform for VoceChat {
  const INFO: PlatformInfo = PlatformInfo {
    id: "vocechat",
    name: "VoceChat",
    aliases: &["voce"],
    about: "VoceChat (self-hosted): channels, direct chats, files and live events",
    home: "https://voce.chat",
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

  /// `SERVER|BOT_API_KEY` or `SERVER|EMAIL|PASSWORD`.
  async fn login_token(&self, token: &str) -> Result<()> {
    login::login(&self.api, token).await
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

  async fn media(&self, post: &str, _audio_only: bool) -> Result<(Post, Vec<Media>)> {
    messages::media(&self.api, post).await
  }

  async fn chats(&self, parent: Option<&str>, _page: &PageReq) -> Result<Page<Collection>> {
    chats::list(&self.api, parent).await
  }

  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    messages::history(&self.api, chat, page).await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    messages::send(&self.api, chat, message, false).await
  }

  async fn members(&self, chat: &str, _page: &PageReq) -> Result<Page<User>> {
    chats::members(&self.api, chat).await
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
