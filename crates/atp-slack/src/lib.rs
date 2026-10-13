//! Slack for all-the-platforms.
//!
//! The Web API with a bot or user token ([`api`]): conversations are
//! collections ([`chats`]), messages are posts with `CHANNEL/TS` ids and
//! permalinks, a message's thread is its comment list ([`messages`]);
//! search and mentions need a user token ([`search`]); `listen` runs over
//! Socket Mode or polls ([`live`]).

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

use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Media, Message, Notification, Page,
  PageReq, Platform, PlatformInfo, Post, Query, Reply, Result, User,
};

use crate::api::Api;

pub struct Slack {
  api: Api,
}

impl Platform for Slack {
  const INFO: PlatformInfo = PlatformInfo {
    id: "slack",
    name: "Slack",
    aliases: &[],
    about: "Slack: channels, direct messages, threads, search and live events",
    home: "https://slack.com",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::TokenLogin,
      Cap::Search,
      Cap::SearchUsers,
      Cap::Read,
      Cap::Comments,
      Cap::User,
      Cap::Notifications,
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
    choices: Choices {
      search_sort: search::SORTS,
      ..Choices::NONE
    },
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

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    users::search(&self.api, &query.keyword, page).await
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

  async fn notifications(&self, _kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    search::mentions(&self.api, page).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    messages::reply(&self.api, post, &reply.text, &reply.images).await
  }

  async fn delete_comment(&self, post: &str, comment: &str) -> Result<Action> {
    messages::delete_comment(&self.api, post, comment).await
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
