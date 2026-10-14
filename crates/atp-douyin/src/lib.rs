//! Douyin (抖音) for all-the-platforms.
//!
//! Posts are awemes: videos (`douyin.com/video/<id>`) and image posts
//! (`/note/<id>`); users are `sec_uid`s. Every web API call is signed with
//! `a_bogus` (see [`sign`]). Writes carry ticket-guard headers signed with
//! the key ATP registers when it logs in (QR code or texted code), so a
//! session copied from a browser reads but does not write. `listen` follows
//! live rooms over the webcast websocket.

mod api;
mod comments;
mod discover;
mod extra;
mod library;
mod live;
mod login;
mod notifications;
mod parse;
mod posts;
mod refs;
mod rooms;
mod sign;
mod users;

use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Media, Notification, Page, PageReq,
  Platform, PlatformInfo, PlatformKind, Post, QrStatus, QrTicket, Query, Reply, Result, User,
};

use crate::api::Api;

pub struct Douyin {
  api: Api,
}

impl Platform for Douyin {
  const INFO: PlatformInfo = PlatformInfo {
    id: "douyin",
    name: "Douyin",
    aliases: &["dy"],
    about: "Douyin (抖音): videos, image posts, comments, users, hot boards and live danmaku",
    home: "https://www.douyin.com",
    cookie_domains: &["douyin.com"],
    required_cookies: api::LOGIN_COOKIES,
    caps: &[
      Cap::QrLogin,
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
      Cap::Favorites,
      Cap::Likes,
      Cap::History,
      Cap::Notifications,
      Cap::Like,
      Cap::Favorite,
      Cap::Comment,
      Cap::DeleteComment,
      Cap::Follow,
      Cap::Listen,
      Cap::Download,
    ],
    choices: Choices {
      search_sort: discover::SEARCH_SORTS,
      search_filter: discover::SEARCH_FILTERS,
      hot_category: discover::HOT_CATEGORIES,
      feed_kind: discover::FEED_KINDS,
      comment_sort: &[],
      notification_kind: notifications::KINDS,
    },
    min_interval: Duration::from_millis(800),
    kind: PlatformKind::Video,
    token: "",
    env: &[],
    guest: "hot, user, read, download (via the share page), listen",
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

  async fn prepare_login(&self) -> Result<()> {
    atp_bytedance::visitor::ensure(&self.api.ctx).await;
    Ok(())
  }

  async fn qr_start(&self) -> Result<QrTicket> {
    login::qr_start(&self.api).await
  }

  async fn qr_poll(&self, ticket: &QrTicket) -> Result<QrStatus> {
    login::qr_poll(&self.api, ticket).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    discover::search(&self.api, query, page).await
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    discover::search_users(&self.api, query, page).await
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    discover::hot(&self.api, category, page).await
  }

  async fn feed(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    discover::feed(&self.api, kind, page).await
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
    users::profile(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    users::posts(&self.api, user, page).await
  }

  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::followers(&self.api, user, page).await
  }

  async fn following(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::following(&self.api, user, page).await
  }

  async fn collections(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    if user.is_some() {
      return Err(atp_core::Error::unsupported("collections of other users"));
    }
    library::folders(&self.api, page).await
  }

  async fn favorites(
    &self,
    user: Option<&str>,
    folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    if user.is_some() {
      return Err(atp_core::Error::unsupported("favorites of other users"));
    }
    library::favorites(&self.api, folder, page).await
  }

  async fn likes(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    users::likes(&self.api, user, page).await
  }

  async fn history(&self, page: &PageReq) -> Result<Page<Post>> {
    library::history(&self.api, page).await
  }

  async fn notifications(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    notifications::list(&self.api, kind, page).await
  }

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    posts::like(&self.api, post, undo).await
  }

  async fn favorite(&self, post: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
    library::favorite(&self.api, post, folder, undo).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    comments::send(&self.api, post, reply).await
  }

  async fn delete_comment(&self, post: &str, comment: &str) -> Result<Action> {
    comments::delete(&self.api, post, comment).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    users::follow(&self.api, user, undo).await
  }

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    live::listen(&self.api, chats, out).await
  }

  async fn media(&self, post: &str, audio_only: bool) -> Result<(Post, Vec<Media>)> {
    posts::media(&self.api, post, audio_only).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
