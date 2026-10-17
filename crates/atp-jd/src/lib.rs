//! JD (京东) for all-the-platforms.
//!
//! Items are posts (`kind` `item`, SKUs as ids) with their price in
//! `price`; reviews are comments; shops are users and their items the
//! users' posts. Calls go through JD's color gateway `api.m.jd.com`;
//! search, item pages, reviews and shop items are signed with h5st 5.3 (a
//! pure port in `sign`), search words are not. JD's risk control answers
//! unknown clients with checks (code 605, the risk handler), which surface
//! as `verification_required`, or with empty 403s.

mod api;
mod comments;
mod extra;
mod library;
mod parse;
mod posts;
mod refs;
mod search;
mod sign;
mod users;

/// The commands only this platform has, for [`Platform::run_extra`](atp_core::Platform::run_extra).
pub use extra::Command;

use std::time::Duration;

use atp_core::{
  Cap, Choices, Comment, Ctx, Data, Page, PageReq, Platform, PlatformInfo, PlatformKind, Post,
  Query, Result, User,
};

use crate::api::Api;

pub struct Jd {
  api: Api,
}

impl Platform for Jd {
  const INFO: PlatformInfo = PlatformInfo {
    id: "jd",
    name: "JD",
    aliases: &["jingdong"],
    about: "JD (京东): items with prices, reviews, shops, search and hot words",
    home: "https://www.jd.com",
    cookie_domains: &["jd.com"],
    required_cookies: api::LOGIN_COOKIES,
    caps: &[
      Cap::Search,
      Cap::Hot,
      Cap::Read,
      Cap::Comments,
      Cap::User,
      Cap::UserPosts,
      Cap::Favorites,
      Cap::History,
      Cap::Download,
    ],
    choices: Choices {
      search_sort: search::SORTS,
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(3000),
    kind: PlatformKind::Shopping,
    token: "",
    env: &[],
    guest: "hot, suggest, related; the rest usually meets a risk check",
    guide: include_str!("../GUIDE.md"),
  };

  type Extra = Command;

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

  async fn hot(&self, _category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    search::hot(&self.api, page).await
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

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    users::posts(&self.api, user, page).await
  }

  async fn favorites(
    &self,
    _user: Option<&str>,
    _folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    library::favorites(&self.api, page).await
  }

  async fn history(&self, page: &PageReq) -> Result<Page<Post>> {
    library::history(&self.api, page).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
