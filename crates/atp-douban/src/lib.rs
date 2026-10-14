//! Douban (豆瓣) for all-the-platforms: films, shows, books and albums
//! with their ratings, short comments (短评) and reviews, Douban's lists
//! (榜单), groups (小组) with their topics and replies, users and what they
//! marked, through the JSON API m.douban.com's pages call ([`api`]). With
//! a login: your own marks, and marking a subject (想看 / 看过) with stars.

mod api;
mod comments;
mod extra;
mod groups;
mod login;
mod parse;
mod refs;
mod subjects;
mod users;

use std::time::Duration;

use atp_core::{
  Cap, Choices, Comment, Ctx, Data, Page, PageReq, Platform, PlatformInfo, PlatformKind, Post,
  QrStatus, QrTicket, Query, Result, User,
};

use crate::api::Api;

pub struct Douban {
  api: Api,
}

impl Platform for Douban {
  const INFO: PlatformInfo = PlatformInfo {
    id: "douban",
    name: "Douban",
    aliases: &["db"],
    about: "Douban (豆瓣): films, books and music with ratings, short comments and reviews, lists, groups",
    home: "https://www.douban.com",
    cookie_domains: &["douban.com"],
    required_cookies: api::LOGIN_COOKIES,
    caps: &[
      Cap::QrLogin,
      Cap::Search,
      Cap::Hot,
      Cap::Read,
      Cap::Comments,
      Cap::User,
      Cap::Download,
    ],
    choices: Choices {
      search_filter: subjects::FILTER_NAMES,
      hot_category: subjects::CHART_NAMES,
      comment_sort: comments::SORTS,
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(1500),
    kind: PlatformKind::Social,
    token: "",
    env: &[],
    guest: "search, hot, read, comments, user, download, reviews, group, list, library USER",
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
    subjects::search(&self.api, query, page).await
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    subjects::hot(&self.api, category, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    subjects::read(&self.api, post).await
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

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
