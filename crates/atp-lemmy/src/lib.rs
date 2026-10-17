//! Lemmy for all-the-platforms (any instance; lemmy.world by default),
//! through the HTTP API v3 ([`api`]): posts and listings ([`posts`]),
//! editing posts ([`edits`]), comment threads ([`comments`]), communities ([`communities`]), users,
//! replies and mentions ([`users`]), search ([`search`]), and logging in
//! with a token or a password ([`login`]).

mod api;
mod comments;
mod communities;
mod edits;
mod extra;
mod login;
mod parse;
mod posts;
mod refs;
mod search;
mod users;

/// The commands only this platform has, for [`Platform::run_extra`](atp_core::Platform::run_extra).
pub use extra::Command;

use std::collections::BTreeMap;
use std::time::Duration;

use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Draft, Edit, Notification, Page, PageReq,
  Platform, PlatformInfo, PlatformKind, Post, Query, Reply, Result, User,
};

use crate::api::Api;

pub struct Lemmy {
  api: Api,
}

impl Platform for Lemmy {
  const INFO: PlatformInfo = PlatformInfo {
    id: "lemmy",
    name: "Lemmy",
    aliases: &[],
    about: "Lemmy (any instance): communities, posts, comment threads, votes and replies",
    home: "https://lemmy.world",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::TokenLogin,
      Cap::Search,
      Cap::SearchUsers,
      Cap::SearchTopics,
      Cap::Hot,
      Cap::Feed,
      Cap::Read,
      Cap::Comments,
      Cap::Replies,
      Cap::User,
      Cap::UserPosts,
      Cap::Favorites,
      Cap::Notifications,
      Cap::Unread,
      Cap::Like,
      Cap::Favorite,
      Cap::Comment,
      Cap::DeleteComment,
      Cap::Publish,
      Cap::Delete,
      Cap::Edit,
      Cap::EditTitle,
    ],
    choices: Choices {
      search_sort: search::SORTS,
      search_filter: search::FILTERS,
      hot_category: posts::SORT_NAMES,
      feed_kind: posts::FEEDS,
      comment_sort: comments::SORTS,
      notification_kind: users::NOTIFICATION_KINDS,
    },
    min_interval: Duration::from_millis(500),
    kind: PlatformKind::Social,
    token: "INSTANCE|USER|PASSWORD",
    env: &[api::ENV_INSTANCE, api::ENV_TOKEN, api::ENV_TOTP],
    guest: "hot, feed (local, all), search, read, comments, user, user-posts, community, communities",
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
    login::token(&self.api, token).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    search::posts(&self.api, query, page).await
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    search::users(&self.api, query, page).await
  }

  async fn search_topics(&self, query: &Query, page: &PageReq) -> Result<Page<Collection>> {
    search::communities(&self.api, query, page).await
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    posts::hot(&self.api, category, page).await
  }

  async fn feed(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    posts::feed(&self.api, kind, page).await
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
    comments::list(&self.api, post, sort, page).await
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

  async fn favorites(
    &self,
    user: Option<&str>,
    folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    posts::saved(&self.api, user, folder, page).await
  }

  async fn notifications(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    users::notifications(&self.api, kind, page).await
  }

  async fn unread(&self) -> Result<BTreeMap<String, u64>> {
    users::unread(&self.api).await
  }

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    posts::vote(&self.api, post, if undo { 0 } else { 1 }).await
  }

  async fn favorite(&self, post: &str, _folder: Option<&str>, undo: bool) -> Result<Action> {
    posts::save(&self.api, post, undo).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    comments::comment(&self.api, post, reply).await
  }

  async fn delete_comment(&self, _post: &str, comment: &str) -> Result<Action> {
    comments::delete(&self.api, comment).await
  }

  async fn publish(&self, draft: &Draft) -> Result<Action> {
    posts::publish(&self.api, draft).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    posts::delete(&self.api, post).await
  }

  async fn edit_post(&self, post: &str, edit: &Edit) -> Result<Action> {
    edits::edit(&self.api, post, edit).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
