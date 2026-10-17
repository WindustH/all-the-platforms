//! Juejin (掘金) for all-the-platforms, through the JSON API of
//! api.juejin.cn that the site calls ([`api`]): articles with their whole
//! Markdown as text, pins (沸点), the hot ranking and home feeds, search,
//! comments with their replies, users, their articles, followers and
//! followings, without an account. With a login: 点赞, collecting into
//! collection sets, comments and replies, following, and the daily
//! check-in.

mod api;
mod comments;
mod extra;
mod feeds;
mod library;
mod markdown;
mod parse;
mod posts;
mod refs;
mod search;
mod users;

/// The commands only this platform has, for [`Platform::run_extra`](atp_core::Platform::run_extra).
pub use extra::Command;

use std::time::Duration;

use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Page, PageReq, Platform, PlatformInfo,
  PlatformKind, Post, Query, Reply, Result, User,
};

use crate::api::Api;

pub struct Juejin {
  api: Api,
}

impl Platform for Juejin {
  const INFO: PlatformInfo = PlatformInfo {
    id: "juejin",
    name: "Juejin",
    aliases: &["jj"],
    about: "Juejin (掘金): articles, pins, hot ranking, comments, users, check-in",
    home: "https://juejin.cn",
    cookie_domains: &["juejin.cn"],
    required_cookies: api::LOGIN_COOKIES,
    caps: &[
      Cap::Search,
      Cap::SearchUsers,
      Cap::Hot,
      Cap::Feed,
      Cap::Read,
      Cap::Comments,
      Cap::Replies,
      Cap::User,
      Cap::UserPosts,
      Cap::Followers,
      Cap::Following,
      Cap::Collections,
      Cap::Like,
      Cap::Favorite,
      Cap::Comment,
      Cap::Follow,
      Cap::Download,
    ],
    choices: Choices {
      search_sort: search::SORTS,
      hot_category: feeds::CATEGORY_NAMES,
      feed_kind: feeds::FEEDS,
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(600),
    kind: PlatformKind::Developer,
    token: "",
    env: &[],
    guest: "search, hot, feed, read, comments, replies, user, user-posts, followers, following, collections USER, download, pins",
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
    search::articles(&self.api, query, page).await
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    search::users(&self.api, query, page).await
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    feeds::hot(&self.api, category, page).await
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

  async fn replies(&self, post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    comments::replies(&self.api, post, comment, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    posts::articles(&self.api, user, page).await
  }

  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::follows(&self.api, user, "followers", page).await
  }

  async fn following(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::follows(&self.api, user, "followees", page).await
  }

  async fn collections(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    library::collections(&self.api, user, page).await
  }

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    posts::like(&self.api, post, undo).await
  }

  async fn favorite(&self, post: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
    library::favorite(&self.api, post, folder, undo).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    comments::publish(&self.api, post, &reply.text, reply.reply_to.as_deref()).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    users::follow(&self.api, user, undo).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
