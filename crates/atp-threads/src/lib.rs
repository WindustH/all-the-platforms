//! Threads for all-the-platforms, through the official Threads API
//! (graph.threads.net) and atp-meta's Graph transport ([`api`]): posts and
//! conversations ([`posts`]), publishing through media containers
//! ([`containers`]), profiles and mentions ([`users`]), keyword search
//! ([`search`]) and insights ([`insights`]). Everything needs a token of
//! the user's own Meta app ([`login`]).

mod api;
mod containers;
mod extra;
mod insights;
mod login;
mod parse;
mod posts;
mod refs;
mod search;
mod users;

/// The commands only this platform has, for [`Platform::run_extra`](atp_core::Platform::run_extra).
pub use extra::Command;

use std::time::Duration;

use atp_core::{
  Action, Cap, Choices, Comment, Ctx, Data, Draft, Insights, Notification, Page, PageReq, Platform,
  PlatformInfo, PlatformKind, Post, Query, Reply, Result, User,
};

use crate::api::Api;

pub struct Threads {
  api: Api,
}

impl Platform for Threads {
  const INFO: PlatformInfo = PlatformInfo {
    id: "threads",
    name: "Threads",
    aliases: &[],
    about: "Threads (official API, your own Meta app): posts, carousels, replies, mentions, search and insights",
    home: "https://www.threads.com",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::TokenLogin,
      Cap::Search,
      Cap::Read,
      Cap::Comments,
      Cap::Replies,
      Cap::User,
      Cap::UserPosts,
      Cap::Notifications,
      Cap::Comment,
      Cap::DeleteComment,
      Cap::Publish,
      Cap::Delete,
      Cap::Download,
      Cap::AccountInsights,
      Cap::PostInsights,
    ],
    choices: Choices {
      search_sort: search::SORTS,
      search_filter: &[],
      hot_category: &[],
      feed_kind: &[],
      comment_sort: &[],
      notification_kind: &[],
    },
    min_interval: Duration::from_millis(500),
    kind: PlatformKind::Social,
    token: "ACCESS_TOKEN",
    env: &[api::ENV_TOKEN],
    guest: "nothing",
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

  fn logged_in(&self) -> bool {
    self.api.logged_in()
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    login::token(&self.api, token).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    search::posts(&self.api, query, page).await
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
    posts::replies(&self.api, post, false, page).await
  }

  async fn replies(&self, _post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    posts::replies(&self.api, comment, true, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    posts::user_posts(&self.api, user, page).await
  }

  async fn notifications(&self, _kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    users::mentions(&self.api, page).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    containers::comment(&self.api, post, reply).await
  }

  async fn delete_comment(&self, _post: &str, comment: &str) -> Result<Action> {
    containers::delete(&self.api, comment).await
  }

  async fn publish(&self, draft: &Draft) -> Result<Action> {
    containers::publish(&self.api, draft).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    containers::delete(&self.api, post).await
  }

  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    match post {
      Some(post) => insights::post(&self.api, post).await,
      None => insights::account(&self.api, days).await,
    }
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
