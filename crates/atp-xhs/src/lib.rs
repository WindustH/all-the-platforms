//! Xiaohongshu / RedNote for all-the-platforms.
//!
//! - [`api`]: signed transport and error mapping; [`sign`]: x-s / x-s-common / XYW.
//! - [`refs`]: note and user references; [`parse`]: JSON → models; [`pages`]: note pages.
//! - Endpoints by subject: [`search`], [`feeds`], [`notes`] (reading, publishing),
//!   [`uploads`], [`users`], [`relations`] (following and followers),
//!   [`notifications`], [`interactions`], [`history`], [`boards`],
//!   [`messages`] (private messages), [`events`] (activity center),
//!   [`insights`], [`login`]; platform-only commands in [`extra`].
//! - [`gateway`]: the push connection private messages and live rooms use;
//!   [`live`]: `listen`.

mod api;
mod boards;
mod events;
mod extra;
mod feeds;
mod gateway;
mod history;
mod insights;
mod interactions;
mod live;
mod login;
mod messages;
mod notes;
mod notifications;
mod pages;
mod parse;
mod refs;
mod relations;
mod search;
mod sign;
mod uploads;
mod users;

/// The commands only this platform has, for [`Platform::run_extra`](atp_core::Platform::run_extra).
pub use extra::Command;

use std::cell::Cell;
use std::collections::BTreeMap;
use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Draft, FolderSpec, FollowQuery, Insights,
  Message, Notification, Page, PageReq, Platform, PlatformInfo, PlatformKind, Post, QrStatus,
  QrTicket, Query, Reply, Result, User,
};

use api::Api;

pub struct Xhs {
  api: Api,
  qr_errors: Cell<u32>,
}

impl Platform for Xhs {
  const INFO: PlatformInfo = PlatformInfo {
    id: "xhs",
    name: "Xiaohongshu",
    aliases: &["xiaohongshu", "rednote"],
    about: "Xiaohongshu / RedNote (小红书)",
    home: api::HOME,
    cookie_domains: &["xiaohongshu.com"],
    required_cookies: &["a1", "web_session"],
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
      Cap::User,
      Cap::UserPosts,
      Cap::Followers,
      Cap::Following,
      Cap::Collections,
      Cap::Favorites,
      Cap::Likes,
      Cap::History,
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
      Cap::Chats,
      Cap::Messages,
      Cap::Send,
      Cap::Listen,
      Cap::FollowingSearch,
      Cap::FollowingMutual,
      Cap::FolderCreate,
      Cap::FolderEdit,
      Cap::FolderDescription,
      Cap::FolderPrivacy,
      Cap::FolderDelete,
      Cap::FolderAdd,
      Cap::FolderMove,
    ],
    choices: Choices {
      search_sort: search::SEARCH_SORTS,
      search_filter: search::SEARCH_FILTERS,
      hot_category: feeds::HOT_CATEGORIES,
      feed_kind: feeds::FEED_KINDS,
      comment_sort: &[],
      notification_kind: notifications::KINDS,
    },
    min_interval: Duration::from_secs(1),
    kind: PlatformKind::Social,
    token: "",
    env: &[],
    guest: "read, download",
    guide: include_str!("../GUIDE.md"),
  };

  const FOLLOWING_SORT: &'static [&'static str] = relations::SORTS;

  type Extra = Command;

  fn new(ctx: Ctx) -> Result<Self> {
    Ok(Self {
      api: Api::new(ctx),
      qr_errors: Cell::new(0),
    })
  }

  fn ctx(&self) -> &Ctx {
    &self.api.ctx
  }

  async fn whoami(&self) -> Result<User> {
    users::whoami(&self.api).await
  }

  async fn prepare_login(&self) -> Result<()> {
    login::prepare(&self.api);
    Ok(())
  }

  async fn qr_start(&self) -> Result<QrTicket> {
    self.qr_errors.set(0);
    login::qr_start(&self.api).await
  }

  async fn qr_poll(&self, ticket: &QrTicket) -> Result<QrStatus> {
    login::qr_poll(&self.api, ticket, &self.qr_errors).await
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
    notes::read(&self.api, post).await
  }

  async fn comments(
    &self,
    post: &str,
    _sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    notes::comments(&self.api, post, page).await
  }

  async fn replies(&self, post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    notes::replies(&self.api, post, comment, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    users::user_posts(&self.api, user, page).await
  }

  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    relations::followers(&self.api, user, page).await
  }

  async fn following(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    relations::following(&self.api, user, &FollowQuery::default(), page).await
  }

  async fn following_with(
    &self,
    user: &str,
    query: &FollowQuery,
    page: &PageReq,
  ) -> Result<Page<User>> {
    relations::following(&self.api, user, query, page).await
  }

  async fn collections(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    boards::list(&self.api, user, page).await
  }

  async fn favorites(
    &self,
    user: Option<&str>,
    folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    match folder {
      Some(board) => boards::notes(&self.api, board, page).await,
      None => users::favorites(&self.api, user, page).await,
    }
  }

  async fn history(&self, page: &PageReq) -> Result<Page<Post>> {
    history::page(&self.api, page).await
  }

  async fn likes(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    users::likes(&self.api, user, page).await
  }

  async fn notifications(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    notifications::notifications(&self.api, kind, page).await
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

  async fn favorite_posts(
    &self,
    posts: &[String],
    folder: Option<&str>,
    undo: bool,
  ) -> Result<Vec<Action>> {
    interactions::favorite_notes(&self.api, posts, folder, undo).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    interactions::comment(&self.api, post, reply).await
  }

  async fn delete_comment(&self, post: &str, comment: &str) -> Result<Action> {
    interactions::delete_comment(&self.api, post, comment).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    interactions::follow(&self.api, user, undo).await
  }

  async fn follow_users(&self, users: &[String], undo: bool) -> Result<Vec<Action>> {
    relations::follow_users(&self.api, users, undo).await
  }

  async fn publish(&self, draft: &Draft) -> Result<Action> {
    notes::publish(&self.api, draft).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    notes::delete(&self.api, post).await
  }

  async fn folder_create(&self, folder: &FolderSpec) -> Result<Action> {
    boards::create(&self.api, folder).await
  }

  async fn folder_edit(&self, folder: &str, change: &FolderSpec) -> Result<Action> {
    boards::edit(&self.api, folder, change).await
  }

  async fn folder_delete(&self, folders: &[String]) -> Result<Vec<Action>> {
    boards::delete(&self.api, folders).await
  }

  async fn folder_add(&self, folder: &str, posts: &[String]) -> Result<Vec<Action>> {
    boards::add(&self.api, folder, posts).await
  }

  async fn folder_move(
    &self,
    from: &str,
    to: &str,
    posts: &[String],
    copy: bool,
  ) -> Result<Vec<Action>> {
    boards::shift(&self.api, from, to, posts, copy).await
  }

  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    match post {
      None => insights::account(&self.api, days).await,
      Some(post) => insights::note(&self.api, post, days).await,
    }
  }

  async fn chats(&self, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    if parent.is_some() {
      return Err(atp_core::Error::unsupported("chats PARENT"));
    }
    messages::chats(&self.api, page).await
  }

  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    messages::messages(&self.api, chat, page).await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    messages::send(&self.api, chat, message).await
  }

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    live::listen(&self.api, chats, out).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
