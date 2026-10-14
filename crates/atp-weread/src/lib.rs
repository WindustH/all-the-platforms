//! WeRead (微信读书) for all-the-platforms, through the web reader's own
//! routes on weread.qq.com ([`api`]): store search, rankings and
//! categories without an account; with a login, books, public reviews,
//! your shelf, reading progress, and your highlights and thoughts, which
//! export as Markdown. Book contents are never fetched.

mod api;
mod books;
mod extra;
mod ids;
mod login;
mod notes;
mod parse;
mod refs;
mod shelf;
mod users;

use std::time::Duration;

use atp_core::{
  Cap, Choices, Comment, Ctx, Data, Page, PageReq, Platform, PlatformInfo, PlatformKind, Post,
  QrStatus, QrTicket, Query, Result, User,
};

use crate::api::Api;

pub struct WeRead {
  api: Api,
}

impl Platform for WeRead {
  const INFO: PlatformInfo = PlatformInfo {
    id: "weread",
    name: "WeRead",
    aliases: &["wr"],
    about: "WeRead (微信读书): store, rankings, your shelf, progress, highlights and thoughts",
    home: "https://weread.qq.com",
    cookie_domains: &["weread.qq.com"],
    required_cookies: api::LOGIN_COOKIES,
    caps: &[
      Cap::QrLogin,
      Cap::Search,
      Cap::Hot,
      Cap::Read,
      Cap::Comments,
      Cap::User,
      Cap::Favorites,
    ],
    choices: Choices {
      hot_category: books::RANKING_NAMES,
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(800),
    kind: PlatformKind::Social,
    token: "",
    env: &[],
    guest: "search, hot, category",
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
    books::search(&self.api, query, page).await
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    books::hot(&self.api, category, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    books::read(&self.api, post).await
  }

  async fn comments(
    &self,
    post: &str,
    _sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    books::reviews(&self.api, post, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn favorites(
    &self,
    user: Option<&str>,
    folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    shelf::shelf(&self.api, user, folder, page).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
