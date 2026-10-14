//! Xianyu (闲鱼, goofish.com) for all-the-platforms.
//!
//! Items are posts (`kind` `item`, price in `extra.price`) read through the
//! web app's mtop gateway, signed with the `_m_h5_tk` token (atp-alibaba).
//! Chats run over DingTalk IMPaaS: a websocket whose pushes are MessagePack.
//! A chat is always about an item, so `send` to an item writes to its
//! seller. Risk control answers with walls (`RGV587`), which surface as
//! `verification_required` or `not_authenticated`.

mod api;
mod chats;
mod extra;
mod library;
mod live;
mod login;
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
  Action, Cap, Choices, Collection, Ctx, Data, Message, Page, PageReq, Platform, PlatformInfo,
  Post, QrStatus, QrTicket, Query, Result, User,
};

use crate::api::Api;

pub struct Xianyu {
  api: Api,
}

impl Platform for Xianyu {
  const INFO: PlatformInfo = PlatformInfo {
    id: "xianyu",
    name: "Xianyu",
    aliases: &["goofish"],
    about: "Xianyu (闲鱼): second-hand items, sellers, favorites and buyer–seller chats",
    home: "https://www.goofish.com",
    cookie_domains: &["goofish.com"],
    required_cookies: api::LOGIN_COOKIES,
    caps: &[
      Cap::QrLogin,
      Cap::Search,
      Cap::Hot,
      Cap::Feed,
      Cap::Read,
      Cap::User,
      Cap::UserPosts,
      Cap::Favorites,
      Cap::Download,
      Cap::Chats,
      Cap::Messages,
      Cap::Send,
      Cap::Listen,
    ],
    choices: Choices {
      search_sort: search::SORTS,
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(1500),
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

  async fn qr_start(&self) -> Result<QrTicket> {
    login::qr_start(&self.api).await
  }

  async fn qr_poll(&self, ticket: &QrTicket) -> Result<QrStatus> {
    login::qr_poll(&self.api, ticket).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    search::search(&self.api, query, page).await
  }

  async fn hot(&self, _category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    search::hot(&self.api, page).await
  }

  async fn feed(&self, _kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    posts::feed(&self.api, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    posts::read(&self.api, post).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    users::posts(&self.api, user, page).await
  }

  async fn favorites(
    &self,
    user: Option<&str>,
    _folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    library::favorites(&self.api, user, page).await
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
    extra::run(&self.api, command).await
  }
}
