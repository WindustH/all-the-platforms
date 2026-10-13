//! Sina Weibo (微博) for social-cli.
//!
//! - [`api`]: weibo.com / m.weibo.cn transport, visitor cookies, error mapping.
//! - [`refs`]: post (mid / bid) and user references; [`parse`]: JSON → models.
//! - Endpoints by subject: [`search`], [`feeds`], [`posts`] (reading and
//!   writing), [`interactions`], [`users`], [`notifications`], [`insights`],
//!   [`login`]; platform-only commands in [`extra`].

mod api;
mod extra;
mod feeds;
mod insights;
mod interactions;
mod login;
mod notifications;
mod parse;
mod posts;
mod refs;
mod search;
mod users;

use std::collections::BTreeMap;
use std::time::Duration;

use social_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Draft, Error, Insights, Notification, Page,
  PageReq, Platform, PlatformInfo, Post, QrStatus, QrTicket, Query, Reply, Result, User,
};

use api::Api;

pub struct Weibo {
  api: Api,
}

impl Platform for Weibo {
  const INFO: PlatformInfo = PlatformInfo {
    id: "weibo",
    name: "Weibo",
    aliases: &["wb", "sina"],
    about: "Sina Weibo (微博)",
    home: api::HOME,
    cookie_domains: &["weibo.com"],
    required_cookies: &[api::LOGIN_COOKIE],
    caps: &[
      Cap::QrLogin,
      Cap::Search,
      Cap::SearchUsers,
      Cap::SearchTopics,
      Cap::Hot,
      Cap::Feed,
      Cap::Read,
      Cap::Comments,
      Cap::Replies,
      Cap::Likers,
      Cap::Reposts,
      Cap::User,
      Cap::UserPosts,
      Cap::Followers,
      Cap::Following,
      Cap::Collections,
      Cap::Favorites,
      Cap::Likes,
      Cap::Notifications,
      Cap::Unread,
      Cap::Like,
      Cap::Favorite,
      Cap::Comment,
      Cap::DeleteComment,
      Cap::Follow,
      Cap::Publish,
      Cap::Delete,
      Cap::Download,
      Cap::AccountInsights,
      Cap::PostInsights,
    ],
    choices: Choices {
      search_sort: &["general", "latest", "hot"],
      search_filter: &["video", "image"],
      hot_category: feeds::HOT_CATEGORIES,
      feed_kind: feeds::FEED_KINDS,
      comment_sort: &["hot", "time"],
      notification_kind: notifications::KINDS,
    },
    min_interval: Duration::from_secs(1),
    guide: include_str!("../GUIDE.md"),
  };

  type Extra = extra::Command;

  fn new(ctx: Ctx) -> Result<Self> {
    Ok(Self { api: Api::new(ctx) })
  }

  fn ctx(&self) -> &Ctx {
    &self.api.ctx
  }

  fn logged_in(&self) -> bool {
    self.api.logged_in()
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
    search::search(&self.api, query, page).await
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    search::search_users(&self.api, query, page).await
  }

  async fn search_topics(&self, query: &Query, page: &PageReq) -> Result<Page<Collection>> {
    search::search_topics(&self.api, query, page).await
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
    sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    posts::comments(&self.api, post, sort, page).await
  }

  async fn replies(&self, _post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    posts::replies(&self.api, comment, page).await
  }

  async fn likers(&self, post: &str, page: &PageReq) -> Result<Page<User>> {
    posts::likers(&self.api, post, page).await
  }

  async fn reposts(&self, post: &str, page: &PageReq) -> Result<Page<Post>> {
    posts::reposts(&self.api, post, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    users::user_posts(&self.api, user, page).await
  }

  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::followers(&self.api, user, page).await
  }

  async fn following(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::following(&self.api, user, page).await
  }

  /// Favorite folders; only the logged-in account's are visible.
  async fn collections(&self, user: Option<&str>, _page: &PageReq) -> Result<Page<Collection>> {
    own_only(&self.api, user, "collections").await?;
    users::folders(&self.api).await
  }

  async fn favorites(
    &self,
    user: Option<&str>,
    folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    own_only(&self.api, user, "favorites").await?;
    users::favorites(&self.api, folder, page).await
  }

  async fn likes(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    users::likes(&self.api, user, page).await
  }

  async fn notifications(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    notifications::list(&self.api, kind, page).await
  }

  async fn unread(&self) -> Result<BTreeMap<String, u64>> {
    notifications::unread(&self.api).await
  }

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    interactions::like(&self.api, post, undo).await
  }

  async fn favorite(&self, post: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
    interactions::favorite(&self.api, post, folder, undo).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    interactions::comment(&self.api, post, reply).await
  }

  async fn delete_comment(&self, _post: &str, comment: &str) -> Result<Action> {
    interactions::delete_comment(&self.api, comment).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    interactions::follow(&self.api, user, undo).await
  }

  async fn publish(&self, draft: &Draft) -> Result<Action> {
    posts::publish(&self.api, draft).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    posts::delete(&self.api, post).await
  }

  /// Weibo's data center has no usable web API; see [`insights`].
  async fn insights(&self, post: Option<&str>, _days: u32) -> Result<Insights> {
    match post {
      Some(post) => insights::post(&self.api, post).await,
      None => insights::account(&self.api).await,
    }
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}

/// Favorites and their folders are private: only the logged-in account's.
async fn own_only(api: &Api, user: Option<&str>, what: &str) -> Result<()> {
  let Some(user) = user else { return Ok(()) };
  let me = users::own_uid(api).await?;
  if users::uid(api, user).await? == me {
    Ok(())
  } else {
    Err(Error::new(
      social_core::ErrorCode::PermissionDenied,
      format!("Weibo shows {what} of the logged-in account only"),
    ))
  }
}
