//! Rocket.Chat for all-the-platforms.
//!
//! A server's REST API v1 with a user id and auth token ([`api`],
//! [`login`]): rooms are collections ([`chats`]), messages are posts with
//! `?msg=` links and a message's thread is its comment list ([`messages`]);
//! search runs within a room ([`search`]); `listen` uses the realtime DDP
//! websocket ([`live`]).

mod api;
mod chats;
mod extra;
mod live;
mod login;
mod messages;
mod parse;
mod refs;
mod search;
mod users;

use std::collections::BTreeMap;
use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Media, Message, Page, PageReq, Platform,
  PlatformInfo, Post, Query, Reply, Result, User,
};

use crate::api::Api;

pub struct RocketChat {
  api: Api,
}

impl Platform for RocketChat {
  const INFO: PlatformInfo = PlatformInfo {
    id: "rocketchat",
    name: "Rocket.Chat",
    aliases: &["rc"],
    about: "Rocket.Chat: channels, groups, direct messages, threads and live events",
    home: "https://rocket.chat",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::TokenLogin,
      Cap::Search,
      Cap::SearchUsers,
      Cap::Read,
      Cap::Comments,
      Cap::User,
      Cap::Unread,
      Cap::Favorite,
      Cap::Comment,
      Cap::CommentImages,
      Cap::DeleteComment,
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

  async fn login_token(&self, token: &str) -> Result<()> {
    login::login(&self.api, token).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    search::messages(&self.api, query, page).await
  }

  async fn search_users(&self, query: &Query, _page: &PageReq) -> Result<Page<User>> {
    search::users(&self.api, &query.keyword).await
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
    messages::thread(&self.api, post, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn unread(&self) -> Result<BTreeMap<String, u64>> {
    chats::unread(&self.api).await
  }

  async fn favorite(&self, post: &str, _folder: Option<&str>, undo: bool) -> Result<Action> {
    messages::star(&self.api, post, undo).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    messages::reply(&self.api, post, &reply.text, &reply.images).await
  }

  async fn delete_comment(&self, _post: &str, comment: &str) -> Result<Action> {
    messages::delete(&self.api, comment).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    messages::delete(&self.api, post).await
  }

  async fn media(&self, post: &str, _audio_only: bool) -> Result<(Post, Vec<Media>)> {
    messages::media(&self.api, post).await
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
