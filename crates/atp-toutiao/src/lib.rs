//! Toutiao (今日头条) for all-the-platforms.
//!
//! Posts are group ids: articles, videos and 微头条 (`kind` `thread`).
//! Reads go through the mobile pages (the desktop pages hide their data
//! behind a script challenge); the recommend feed and user feeds are signed
//! with ByteDance's `a_bogus` from atp-bytedance; users are their `MS4w…`
//! tokens. Search answers suspicious clients with a slide captcha, which
//! surfaces as `verification_required`.

mod api;
mod comments;
mod feeds;
mod insights;
mod interactions;
mod login;
mod parse;
mod posts;
mod refs;
mod search;
mod users;

use std::time::Duration;

use atp_core::{
  Action, Cap, Choices, Comment, Ctx, Data, Insights, NoExtra, Page, PageReq, Platform,
  PlatformInfo, PlatformKind, Post, QrStatus, QrTicket, Query, Result, User,
};

use crate::api::Api;

pub struct Toutiao {
  api: Api,
}

impl Platform for Toutiao {
  const INFO: PlatformInfo = PlatformInfo {
    id: "toutiao",
    name: "Toutiao",
    aliases: &["jrtt"],
    about: "Toutiao (今日头条): articles, videos, 微头条, comments, users and the hot board",
    home: "https://www.toutiao.com",
    cookie_domains: &["toutiao.com"],
    required_cookies: api::LOGIN_COOKIES,
    caps: &[
      Cap::QrLogin,
      Cap::Search,
      Cap::Hot,
      Cap::Feed,
      Cap::Read,
      Cap::Comments,
      Cap::Replies,
      Cap::User,
      Cap::UserPosts,
      Cap::Following,
      Cap::Like,
      Cap::Follow,
      Cap::Download,
      Cap::AccountInsights,
      Cap::PostInsights,
    ],
    choices: Choices {
      feed_kind: feeds::FEED_KINDS,
      search_filter: search::FILTERS,
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(1000),
    kind: PlatformKind::Social,
    token: "",
    env: &[],
    guest: "hot, feed, search, read, comments, user, user-posts, download",
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

  async fn qr_start(&self) -> Result<QrTicket> {
    login::qr_start(&self.api).await
  }

  async fn qr_poll(&self, ticket: &QrTicket) -> Result<QrStatus> {
    login::qr_poll(&self.api, ticket).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    search::search(&self.api, query, page).await
  }

  async fn hot(&self, _category: Option<&str>, _page: &PageReq) -> Result<Page<Post>> {
    feeds::hot(&self.api).await
  }

  async fn feed(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    feeds::feed(&self.api, kind, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    posts::read(&self.api, post).await
  }

  async fn comments(
    &self,
    post: &str,
    _sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    comments::list(&self.api, post, page).await
  }

  async fn replies(&self, _post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    comments::replies(&self.api, comment, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    users::posts(&self.api, user, page).await
  }

  async fn following(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::following(&self.api, user, page).await
  }

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    interactions::like(&self.api, post, undo).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    interactions::follow(&self.api, user, undo).await
  }

  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    match post {
      Some(post) => insights::post(&self.api, post).await,
      None => insights::account(&self.api, days).await,
    }
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    match command {}
  }
}
