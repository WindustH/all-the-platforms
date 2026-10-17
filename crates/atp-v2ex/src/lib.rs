//! V2EX for all-the-platforms, through its official API ([`api`]): API 1.0
//! without an account (hot and latest topics, topics, replies, members,
//! nodes) and API 2.0 with a personal access token (the account,
//! notifications, paged replies and node topics).
//!
//! Topics are posts and replies comments ([`topics`]); members, the
//! account, notifications and nodes are in [`users`].

mod api;
mod extra;
mod parse;
mod refs;
mod topics;
mod users;

/// The commands only this platform has, for [`Platform::run_extra`](atp_core::Platform::run_extra).
pub use extra::Command;

use std::time::Duration;

use atp_core::{
  Cap, Choices, Comment, Ctx, Data, Notification, Page, PageReq, Platform, PlatformInfo,
  PlatformKind, Post, Result, User,
};

use crate::api::Api;

pub struct V2ex {
  api: Api,
}

impl Platform for V2ex {
  const INFO: PlatformInfo = PlatformInfo {
    id: "v2ex",
    name: "V2EX",
    aliases: &[],
    about: "V2EX: hot and latest topics, replies, members, nodes and notifications",
    home: "https://www.v2ex.com",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::TokenLogin,
      Cap::Hot,
      Cap::Read,
      Cap::Comments,
      Cap::User,
      Cap::UserPosts,
      Cap::Notifications,
    ],
    choices: Choices {
      search_sort: &[],
      search_filter: &[],
      hot_category: topics::HOT_CATEGORIES,
      feed_kind: &[],
      comment_sort: &[],
      notification_kind: &[],
    },
    min_interval: Duration::from_millis(1200),
    kind: PlatformKind::Developer,
    token: "TOKEN",
    env: &[api::ENV_TOKEN],
    guest: "hot, read, comments, user, user-posts, node, topics, nodes",
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
    self.api.token().is_some()
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    users::login(&self.api, token)
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    topics::hot(&self.api, category, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    topics::read(&self.api, post).await
  }

  async fn comments(
    &self,
    post: &str,
    _sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    topics::replies(&self.api, post, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    topics::member_topics(&self.api, user, page).await
  }

  async fn notifications(&self, _kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    users::notifications(&self.api, page).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
