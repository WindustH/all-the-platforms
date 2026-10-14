//! Telegram for all-the-platforms, in three modes:
//!
//! - a **user account** over MTProto ([`mt`], grammers): logged in with a QR
//!   code or the phone number ([`login`]); chats, history, search, comments,
//!   members, sending and live updates ([`mt`], [`live`]);
//! - a **bot** through the Bot API (`login --token`, [`bot`]): sending,
//!   editing, reactions, admins and updates — bots cannot read history;
//! - **no account**: public channels through the t.me web preview ([`web`]).
//!
//! Chats are collections whose `id` is the Bot API id; messages are posts
//! whose `id` is `CHAT/MESSAGE` and whose `url` is the t.me link when the
//! chat has one ([`refs`], [`parse`]). [`api`] picks the route; the feature
//! modules ([`chats`], [`messages`], [`search`], [`users`]) route each
//! command.

mod api;
mod bot;
mod chats;
mod extra;
mod insights;
mod live;
mod login;
mod messages;
mod mt;
mod parse;
mod prompt;
mod proxy;
mod refs;
mod search;
mod uploads;
mod users;
mod web;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Insights, Media, Message, Page, PageReq,
  Platform, PlatformInfo, PlatformKind, Post, QrStatus, QrTicket, Query, Result, User,
};

use crate::api::Api;

pub struct Telegram {
  api: Api,
}

impl Platform for Telegram {
  const INFO: PlatformInfo = PlatformInfo {
    id: "telegram",
    name: "Telegram",
    aliases: &["tg"],
    about: "Telegram: chats, groups, channels and bots",
    home: "https://t.me",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::QrLogin,
      Cap::TokenLogin,
      Cap::Search,
      Cap::SearchUsers,
      Cap::Read,
      Cap::Comments,
      Cap::User,
      Cap::UserPosts,
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
    choices: Choices {
      search_filter: mt::FILTERS,
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(500),
    kind: PlatformKind::Chat,
    token: "BOT_TOKEN",
    env: &["TELEGRAM_BOT_TOKEN", "TELEGRAM_API_ID", "TELEGRAM_API_HASH"],
    guest: "public channels: messages, read, user, search-chat, listen, download",
    guide: include_str!("../GUIDE.md"),
  };

  type Extra = extra::Command;

  fn new(ctx: Ctx) -> Result<Self> {
    Ok(Self { api: Api::new(ctx) })
  }

  fn ctx(&self) -> &Ctx {
    &self.api.ctx
  }

  fn logged_in(&self) -> bool {
    self.api.logged_in()
  }

  async fn whoami(&self) -> Result<User> {
    users::whoami(&self.api).await
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    login::token(&self.api, token)
  }

  async fn qr_start(&self) -> Result<QrTicket> {
    login::qr_start(&self.api).await
  }

  async fn qr_poll(&self, ticket: &QrTicket) -> Result<QrStatus> {
    login::qr_poll(&self.api, ticket).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    search::posts(&self.api, query, page).await
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    search::users(&self.api, query, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    messages::read(&self.api, post).await
  }

  async fn comments(
    &self,
    post: &str,
    _sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    messages::comments(&self.api, post, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    messages::list(&self.api, user, page).await
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

  async fn delete(&self, post: &str) -> Result<Action> {
    messages::delete(&self.api, post).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    chats::join(&self.api, user, undo).await
  }

  async fn unread(&self) -> Result<BTreeMap<String, u64>> {
    chats::unread(&self.api).await
  }

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    live::listen(&self.api, chats, out).await
  }

  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    insights::insights(&self.api, post, days).await
  }

  async fn save_media(&self, media: &Media, dir: &Path, name: &str) -> Option<Result<PathBuf>> {
    messages::save_media(&self.api, media, dir, name).await
  }

  async fn run_extra(&self, command: extra::Command) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
