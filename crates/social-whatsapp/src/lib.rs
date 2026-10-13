//! WhatsApp for social-cli, in two modes:
//!
//! - a **personal account** as a linked device ([`personal`], whatsapp-rust):
//!   QR-code or pairing-code login, chats, messages, groups, sending,
//!   reactions, edits, deletions and live events. WhatsApp keeps no history
//!   on its servers, so what the device receives is kept in a local archive
//!   ([`archive`]) that `chats`, `messages`, `read` and `search` read;
//! - the **Business Cloud API** ([`cloud`], `login --token
//!   TOKEN:PHONE_NUMBER_ID`): sending text, media and templates, reactions,
//!   read receipts, media, the business profile and messaging analytics.
//!
//! Chats are collections whose `id` is the JID; messages are posts whose
//! `id` is `CHAT/MESSAGE` ([`refs`], [`parse`]).

mod api;
mod archive;
mod chats;
mod cloud;
mod extra;
mod files;
mod groups;
mod link;
mod live;
mod login;
mod media;
mod messages;
mod parse;
mod personal;
mod proxy;
mod refs;
mod store;
mod users;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use social_core::output::Stream;
use social_core::{
  Action, Cap, Choices, Collection, Ctx, Data, Error, Insights, Media, Message, Page, PageReq,
  Platform, PlatformInfo, Post, QrStatus, QrTicket, Query, Result, User,
};

use crate::api::Api;

pub struct WhatsApp {
  api: Api,
}

impl Platform for WhatsApp {
  const INFO: PlatformInfo = PlatformInfo {
    id: "whatsapp",
    name: "WhatsApp",
    aliases: &["wa"],
    about: "WhatsApp: chats, groups and messages of a linked account, or the Business Cloud API",
    home: "https://web.whatsapp.com",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::QrLogin,
      Cap::TokenLogin,
      Cap::Search,
      Cap::Read,
      Cap::User,
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
    choices: Choices::NONE,
    min_interval: Duration::from_millis(300),
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
    !matches!(self.api.mode(), api::Mode::None)
  }

  async fn whoami(&self) -> Result<User> {
    users::me(&self.api).await
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    login::token(&self.api, token)
  }

  async fn qr_start(&self) -> Result<QrTicket> {
    login::qr_start(&self.api.personal).await
  }

  async fn qr_poll(&self, _ticket: &QrTicket) -> Result<QrStatus> {
    login::qr_poll(&self.api).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    let words = &query.keyword;
    let search = async |link: &mut link::Link| chats::search(link, words, page);
    self.api.on_device("search", search).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    let read = async |link: &mut link::Link| chats::read(link, post);
    self.api.on_device("read", read).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    let lookup = async |link: &mut link::Link| users::user(link, user).await;
    self.api.on_device("user", lookup).await
  }

  async fn unread(&self) -> Result<BTreeMap<String, u64>> {
    let unread = async |link: &mut link::Link| Ok(chats::unread(link));
    self.api.on_device("unread", unread).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    let follow = async |link: &mut link::Link| groups::follow(link, user, undo).await;
    self.api.on_device("follow (join a group)", follow).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    let delete = async |link: &mut link::Link| messages::delete(link, post).await;
    self.api.on_device("delete", delete).await
  }

  async fn chats(&self, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    if parent.is_some() {
      return Err(Error::input(
        "WhatsApp chats have nothing inside them to list; use `messages CHAT` or `members GROUP`",
      ));
    }
    let chats = async |link: &mut link::Link| chats::chats(link, page).await;
    self.api.on_device("chats", chats).await
  }

  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    let messages = async |link: &mut link::Link| chats::messages(link, chat, page).await;
    self.api.on_device("messages", messages).await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    messages::send(&self.api, chat, message).await
  }

  async fn members(&self, chat: &str, page: &PageReq) -> Result<Page<User>> {
    let members = async |link: &mut link::Link| groups::members(link, chat, page).await;
    self.api.on_device("members", members).await
  }

  async fn react(&self, post: &str, emoji: &str, undo: bool) -> Result<Action> {
    messages::react(&self.api, post, emoji, undo).await
  }

  async fn edit(&self, post: &str, text: &str) -> Result<Action> {
    let edit = async |link: &mut link::Link| messages::edit(link, post, text).await;
    self.api.on_device("edit", edit).await
  }

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    let listen = async |link: &mut link::Link| live::listen(link, chats, out).await;
    self.api.on_device("listen", listen).await
  }

  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    if post.is_some() {
      return Err(Error::unsupported("insights for one message"));
    }
    if matches!(self.api.mode(), api::Mode::None) {
      return Err(api::not_logged_in());
    }
    self
      .api
      .cloud("insights")?
      .insights(&self.api.ctx, days)
      .await
  }

  async fn media(&self, post: &str, _audio_only: bool) -> Result<(Post, Vec<Media>)> {
    media::media(&self.api, post).await
  }

  async fn save_media(&self, media: &Media, dir: &Path, name: &str) -> Option<Result<PathBuf>> {
    let url = media.url.strip_prefix(media::SCHEME)?;
    Some(media::save(&self.api.personal, url, dir, name).await)
  }

  async fn run_extra(&self, command: extra::Command) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
