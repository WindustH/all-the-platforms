//! Taobao and Tmall (淘宝 / 天猫) for all-the-platforms.
//!
//! Items are posts (`kind` `item`, price in `extra.price`) read through the
//! mtop gateway, signed with the `_m_h5_tk` token (atp-alibaba); reviews
//! are comments; sellers are found by their items. Chats with shops run
//! over DingTalk IMPaaS like Xianyu's. Everything needs a login from a
//! browser (`--browser` or `--cookie`); risk control answers with walls
//! (`RGV587`), which surface as `verification_required`.

mod api;
mod chats;
mod comments;
mod live;
mod messages;
mod parse;
mod posts;
mod refs;
mod search;
mod uploads;
mod users;

use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Message, NoExtra, Page, PageReq, Platform,
  PlatformInfo, Post, Query, Result, User,
};

use crate::api::Api;

pub struct Taobao {
  api: Api,
}

impl Platform for Taobao {
  const INFO: PlatformInfo = PlatformInfo {
    id: "taobao",
    name: "Taobao",
    aliases: &["tmall"],
    about: "Taobao and Tmall (淘宝 / 天猫): items, reviews, sellers and chats with shops",
    home: "https://www.taobao.com",
    cookie_domains: &["taobao.com", "tmall.com"],
    required_cookies: api::LOGIN_COOKIES,
    caps: &[
      Cap::Search,
      Cap::Read,
      Cap::Comments,
      Cap::User,
      Cap::Download,
      Cap::Chats,
      Cap::Messages,
      Cap::Send,
      Cap::Listen,
    ],
    choices: Choices {
      search_sort: search::SORTS,
      comment_sort: comments::SORTS,
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(2000),
    guide: include_str!("../GUIDE.md"),
  };

  type Extra = NoExtra;

  fn new(ctx: Ctx) -> Result<Self> {
    Ok(Self { api: Api::new(ctx) })
  }

  fn ctx(&self) -> &Ctx {
    &self.api.ctx
  }

  async fn whoami(&self) -> Result<User> {
    users::whoami(&self.api).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    search::search(&self.api, query, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    posts::read(&self.api, post).await
  }

  async fn comments(
    &self,
    post: &str,
    sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    comments::list(&self.api, post, sort, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn chats(&self, _parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    chats::list(&self.api, page).await
  }

  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    messages::list(&self.api, chat, page).await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    messages::send(&self.api, chat, message).await
  }

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    live::listen(&self.api, chats, out).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    match command {}
  }
}
