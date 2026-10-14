//! Hacker News for all-the-platforms: read-only, without an account.
//!
//! Lists, items and users come from the official Firebase API, search and
//! whole comment trees from Algolia's HN search API ([`api`]). Stories,
//! jobs, polls and comments are posts ([`stories`]); comment threads are in
//! [`comments`], searches and a user's stories and comments in [`search`].

mod api;
mod comments;
mod extra;
mod parse;
mod refs;
mod search;
mod stories;
mod users;

use std::time::Duration;

use atp_core::{
  Cap, Choices, Comment, Ctx, Data, Error, ErrorCode, Page, PageReq, Platform, PlatformInfo,
  PlatformKind, Post, Query, Result, User,
};

use crate::api::Api;

pub struct HackerNews {
  api: Api,
}

impl Platform for HackerNews {
  const INFO: PlatformInfo = PlatformInfo {
    id: "hn",
    name: "Hacker News",
    aliases: &["hackernews"],
    about: "Hacker News: front-page lists, stories, comment threads, users and search",
    home: "https://news.ycombinator.com",
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
      search_filter: search::FILTERS,
      hot_category: stories::HOT_CATEGORIES,
      feed_kind: &[],
      comment_sort: comments::SORTS,
      notification_kind: &[],
    },
    min_interval: Duration::ZERO,
    kind: PlatformKind::Developer,
    token: "",
    env: &[],
    guest: "every command (hot, search, read, comments, replies, user, user-posts, threads)",
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
    Err(Error::new(
      ErrorCode::UnsupportedOperation,
      "ATP reads Hacker News without an account; there is nothing to log in to",
    ))
  }

  fn logged_in(&self) -> bool {
    false
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    search::posts(&self.api, query, page).await
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
    sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    comments::comments(&self.api, post, sort, page).await
  }

  async fn replies(&self, post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    comments::replies(&self.api, post, comment, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    search::stories(&self.api, user, page).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
