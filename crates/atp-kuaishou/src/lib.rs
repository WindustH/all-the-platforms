//! Kuaishou (快手) for all-the-platforms.
//!
//! Posts are photos (作品): videos or image sets, `kuaishou.com/short-video/<id>`;
//! users are eids (`3x…`, `/profile/<eid>`); live rooms are
//! `live.kuaishou.com/u/<id>`. The web signs its calls (see [`sign`]) and
//! puts detail pages and comments behind a slider captcha for clients it
//! does not trust; a captcha surfaces as `verification_required`, and
//! `read` falls back to the mobile share page.

mod api;
mod comments;
mod device;
mod extra;
mod feeds;
mod gateway;
mod insights;
mod interactions;
mod live;
mod login;
mod notifications;
mod parse;
mod posts;
mod refs;
mod rooms;
mod search;
mod sign;
mod users;

/// The commands only this platform has, for [`Platform::run_extra`](atp_core::Platform::run_extra).
pub use extra::Command;

use std::collections::BTreeMap;
use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Comment, Ctx, Data, Insights, Media, Page, PageReq, Platform, PlatformInfo,
  PlatformKind, Post, QrStatus, QrTicket, Query, Reply, Result, User,
};

use crate::api::Api;

pub struct Kuaishou {
  api: Api,
}

impl Platform for Kuaishou {
  const INFO: PlatformInfo = PlatformInfo {
    id: "kuaishou",
    name: "Kuaishou",
    aliases: &["ks"],
    about: "Kuaishou (快手): videos, image sets, comments, users, hot board and live rooms",
    home: "https://www.kuaishou.com",
    cookie_domains: &["kuaishou.com"],
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
      Cap::Favorites,
      Cap::Likes,
      Cap::History,
      Cap::Unread,
      Cap::Like,
      Cap::Favorite,
      Cap::Comment,
      Cap::Follow,
      Cap::Download,
      Cap::AccountInsights,
      Cap::PostInsights,
      Cap::Listen,
    ],
    choices: Choices {
      feed_kind: feeds::FEED_KINDS,
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(1500),
    kind: PlatformKind::Video,
    token: "",
    env: &[],
    guest: "hot, feed, read, comments, live, rooms, gifts, download",
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

  async fn prepare_login(&self) -> Result<()> {
    device::ensure(&self.api).await;
    Ok(())
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

  async fn hot(&self, _category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    feeds::hot(&self.api, page).await
  }

  async fn feed(&self, _kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    feeds::feed(&self.api, page).await
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

  async fn favorites(
    &self,
    user: Option<&str>,
    folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    users::favorites(&self.api, user, folder, page).await
  }

  async fn likes(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    users::likes(&self.api, user, page).await
  }

  async fn history(&self, page: &PageReq) -> Result<Page<Post>> {
    users::history(&self.api, page).await
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

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    interactions::follow(&self.api, user, undo).await
  }

  async fn insights(&self, post: Option<&str>, _days: u32) -> Result<Insights> {
    match post {
      Some(post) => insights::post(&self.api, post).await,
      None => insights::account(&self.api).await,
    }
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
