//! Stack Overflow and the other Stack Exchange sites for all-the-platforms,
//! through the official API v2.3 ([`api`]).
//!
//! Questions and answers are posts ([`posts`]); the answers of a question
//! and the comments under posts are comments; users and their posts are in
//! [`users`]; search, tags and sites in [`search`] and [`extra`]. Reading
//! needs no account; an app key raises the daily quota, and an access token
//! of the user's own app ([`login`]) unlocks the inbox and the writes in
//! [`write`].

mod api;
mod extra;
mod login;
mod parse;
mod posts;
mod refs;
mod search;
mod users;
mod write;

use std::time::Duration;

use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Notification, Page, PageReq, Platform,
  PlatformInfo, PlatformKind, Post, Query, Reply, Result, User,
};

use crate::api::Api;

pub struct StackExchange {
  api: Api,
}

impl Platform for StackExchange {
  const INFO: PlatformInfo = PlatformInfo {
    id: "so",
    name: "Stack Overflow",
    aliases: &["stackoverflow", "stackexchange", "se"],
    about: "Stack Overflow and every Stack Exchange site: questions, answers, comments, users, tags",
    home: "https://stackoverflow.com",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::TokenLogin,
      Cap::Search,
      Cap::SearchUsers,
      Cap::SearchTopics,
      Cap::Hot,
      Cap::Read,
      Cap::Comments,
      Cap::Replies,
      Cap::User,
      Cap::UserPosts,
      Cap::Notifications,
      Cap::Like,
      Cap::Favorite,
      Cap::Comment,
      Cap::DeleteComment,
    ],
    choices: Choices {
      search_sort: search::SORTS,
      search_filter: search::FILTERS,
      hot_category: posts::HOT_CATEGORIES,
      feed_kind: &[],
      comment_sort: posts::COMMENT_SORTS,
      notification_kind: &[],
    },
    min_interval: Duration::from_millis(1000),
    kind: PlatformKind::Developer,
    token: "KEY[:ACCESS_TOKEN]",
    env: &[api::ENV_KEY, api::ENV_TOKEN, api::ENV_SITE, login::ENV_PORT],
    guest: "search, hot, read, comments, replies, user, user-posts, tags, tag (300 requests a day per IP)",
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
    self.api.key().is_some()
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    login::token(&self.api, token).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    search::questions(&self.api, query, page).await
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    search::users(&self.api, query, page).await
  }

  async fn search_topics(&self, query: &Query, page: &PageReq) -> Result<Page<Collection>> {
    search::tags(&self.api, Some(&query.keyword), page).await
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    posts::hot(&self.api, category, page).await
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
    posts::comments(&self.api, post, sort, page).await
  }

  async fn replies(&self, post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    posts::replies(&self.api, post, comment, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    users::posts(&self.api, user, page).await
  }

  async fn notifications(&self, _kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    users::notifications(&self.api, page).await
  }

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    write::vote(&self.api, post, true, undo).await
  }

  async fn favorite(&self, post: &str, _folder: Option<&str>, undo: bool) -> Result<Action> {
    write::bookmark(&self.api, post, undo).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    write::comment(&self.api, post, reply).await
  }

  async fn delete_comment(&self, post: &str, comment: &str) -> Result<Action> {
    write::delete_comment(&self.api, post, comment).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
