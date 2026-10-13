//! Sina Weibo (微博) for social-cli.
//!
//! - [`api`]: weibo.com / m.weibo.cn transport, visitor cookies, error mapping.
//! - [`refs`]: post (mid / bid) and user references; [`parse`]: JSON → models.
//! - Endpoints by domain: [`discover`], [`posts`], [`people`], [`inbox`],
//!   [`write`], [`insights`], [`login`]; platform-only commands in [`extra`].

mod api;
mod discover;
mod extra;
mod inbox;
mod insights;
mod login;
mod parse;
mod people;
mod posts;
mod refs;
mod write;

use std::collections::BTreeMap;
use std::time::Duration;

use social_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Draft, Error, Insights, Notification, Page,
  PageReq, Platform, PlatformInfo, Post, QrStatus, QrTicket, Query, Reply, Result, User,
};

use api::Client;

pub struct Weibo {
  client: Client,
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
      hot_category: discover::HOT_CATEGORIES,
      feed_kind: discover::FEED_KINDS,
      comment_sort: &["hot", "time"],
      notification_kind: inbox::KINDS,
    },
    min_interval: Duration::from_secs(1),
    guide: include_str!("../GUIDE.md"),
  };

  type Extra = extra::Command;

  fn new(ctx: Ctx) -> Result<Self> {
    Ok(Self {
      client: Client::new(ctx),
    })
  }

  fn ctx(&self) -> &Ctx {
    &self.client.ctx
  }

  fn logged_in(&self) -> bool {
    self.client.logged_in()
  }

  async fn whoami(&self) -> Result<User> {
    people::whoami(&self.client).await
  }

  async fn qr_start(&self) -> Result<QrTicket> {
    login::qr_start(&self.client).await
  }

  async fn qr_poll(&self, ticket: &QrTicket) -> Result<QrStatus> {
    login::qr_poll(&self.client, ticket).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    discover::search(&self.client, query, page).await
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    discover::search_users(&self.client, query, page).await
  }

  async fn search_topics(&self, query: &Query, page: &PageReq) -> Result<Page<Collection>> {
    discover::search_topics(&self.client, query, page).await
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    discover::hot(&self.client, category, page).await
  }

  async fn feed(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    discover::feed(&self.client, kind, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    posts::read(&self.client, post).await
  }

  async fn comments(
    &self,
    post: &str,
    sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    posts::comments(&self.client, post, sort, page).await
  }

  async fn replies(&self, _post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    posts::replies(&self.client, comment, page).await
  }

  async fn likers(&self, post: &str, page: &PageReq) -> Result<Page<User>> {
    posts::likers(&self.client, post, page).await
  }

  async fn reposts(&self, post: &str, page: &PageReq) -> Result<Page<Post>> {
    posts::reposts(&self.client, post, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    people::user(&self.client, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    people::user_posts(&self.client, user, page).await
  }

  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    people::followers(&self.client, user, page).await
  }

  async fn following(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    people::following(&self.client, user, page).await
  }

  /// Favorite folders; only the logged-in account's are visible.
  async fn collections(&self, user: Option<&str>, _page: &PageReq) -> Result<Page<Collection>> {
    own_only(&self.client, user, "collections").await?;
    people::folders(&self.client).await
  }

  async fn favorites(
    &self,
    user: Option<&str>,
    folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    own_only(&self.client, user, "favorites").await?;
    people::favorites(&self.client, folder, page).await
  }

  async fn likes(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    people::likes(&self.client, user, page).await
  }

  async fn notifications(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    inbox::list(&self.client, kind, page).await
  }

  async fn unread(&self) -> Result<BTreeMap<String, u64>> {
    inbox::unread(&self.client).await
  }

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    write::like(&self.client, post, undo).await
  }

  async fn favorite(&self, post: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
    write::favorite(&self.client, post, folder, undo).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    write::comment(&self.client, post, reply).await
  }

  async fn delete_comment(&self, _post: &str, comment: &str) -> Result<Action> {
    write::delete_comment(&self.client, comment).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    write::follow(&self.client, user, undo).await
  }

  async fn publish(&self, draft: &Draft) -> Result<Action> {
    write::publish(&self.client, draft).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    write::delete(&self.client, post).await
  }

  /// Weibo's data center has no usable web API; see [`insights`].
  async fn insights(&self, post: Option<&str>, _days: u32) -> Result<Insights> {
    match post {
      Some(post) => insights::post(&self.client, post).await,
      None => insights::account(&self.client).await,
    }
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.client, command).await
  }
}

/// Favorites and their folders are private: only the logged-in account's.
async fn own_only(c: &Client, user: Option<&str>, what: &str) -> Result<()> {
  let Some(user) = user else { return Ok(()) };
  let me = people::own_uid(c).await?;
  if people::uid(c, user).await? == me {
    Ok(())
  } else {
    Err(Error::new(
      social_core::ErrorCode::PermissionDenied,
      format!("Weibo shows {what} of the logged-in account only"),
    ))
  }
}
