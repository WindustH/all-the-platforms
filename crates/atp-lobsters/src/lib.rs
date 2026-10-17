//! Lobsters for all-the-platforms: read-only, without an account.
//!
//! Its pages have JSON versions ([`api`]): story listings, stories with
//! their comments, users and tags ([`stories`], [`users`]); search reads
//! the HTML results page ([`search`]).

mod api;
mod extra;
mod parse;
mod refs;
mod search;
mod stories;
mod users;

/// The commands only this platform has, for [`Platform::run_extra`](atp_core::Platform::run_extra).
pub use extra::Command;

use std::time::Duration;

use atp_core::{
  Cap, Choices, Comment, Ctx, Data, Error, ErrorCode, Page, PageReq, Platform, PlatformInfo,
  PlatformKind, Post, Query, Result, User,
};

use crate::api::Api;

pub struct Lobsters {
  api: Api,
}

impl Platform for Lobsters {
  const INFO: PlatformInfo = PlatformInfo {
    id: "lobsters",
    name: "Lobsters",
    aliases: &["lobste.rs"],
    about: "Lobsters: hottest, newest and active stories, comment threads, tags, users and search",
    home: "https://lobste.rs",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::Search,
      Cap::Hot,
      Cap::Read,
      Cap::Comments,
      Cap::Replies,
      Cap::User,
      Cap::UserPosts,
    ],
    choices: Choices {
      search_sort: search::SORTS,
      search_filter: &[],
      hot_category: stories::HOT_CATEGORIES,
      feed_kind: &[],
      comment_sort: &[],
      notification_kind: &[],
    },
    min_interval: Duration::from_millis(500),
    kind: PlatformKind::Developer,
    token: "",
    env: &[],
    guest: "every command (hot, search, read, comments, replies, user, user-posts, tag, tags)",
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
    Err(Error::new(
      ErrorCode::UnsupportedOperation,
      "ATP reads Lobsters without an account; there is nothing to log in to",
    ))
  }

  fn logged_in(&self) -> bool {
    false
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    search::stories(&self.api, query, page).await
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    stories::hot(&self.api, category, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    stories::read(&self.api, post).await
  }

  async fn comments(
    &self,
    post: &str,
    _sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    stories::comments(&self.api, post, page).await
  }

  async fn replies(&self, post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    stories::replies(&self.api, post, comment, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    users::posts(&self.api, user, page).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
