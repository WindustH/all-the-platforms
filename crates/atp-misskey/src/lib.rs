//! Misskey for all-the-platforms (any instance; misskey.io by default).
//!
//! Notes ([`posts`]), users, reactions, renotes and hashtags through the
//! HTTP API ([`api`]), chat (direct chats and rooms: [`chats`],
//! [`messages`]), live events through the streaming socket ([`live`]), and
//! tokens or MiAuth for logging in ([`login`]). Public timelines, notes and
//! users are readable without an account where the instance allows it.

mod api;
mod chats;
mod extra;
mod live;
mod login;
mod messages;
mod parse;
mod posts;
mod refs;
mod search;
mod users;

use std::collections::BTreeMap;
use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Draft, Message, Notification, Page,
  PageReq, Platform, PlatformInfo, Post, QrStatus, QrTicket, Query, Reply, Result, User,
};

use crate::api::Api;

pub struct Misskey {
  api: Api,
}

impl Platform for Misskey {
  const INFO: PlatformInfo = PlatformInfo {
    id: "misskey",
    name: "Misskey",
    aliases: &["mk"],
    about: "Misskey (any instance): notes, timelines, reactions, users, chat and live streams",
    home: "https://misskey.io",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::QrLogin,
      Cap::TokenLogin,
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
      Cap::Favorites,
      Cap::Notifications,
      Cap::Unread,
      Cap::Like,
      Cap::Favorite,
      Cap::Comment,
      Cap::CommentImages,
      Cap::DeleteComment,
      Cap::Follow,
      Cap::Publish,
      Cap::Delete,
      Cap::Download,
      Cap::Chats,
      Cap::Messages,
      Cap::Send,
      Cap::Members,
      Cap::React,
      Cap::Listen,
    ],
    choices: Choices {
      search_sort: &[],
      search_filter: search::FILTERS,
      hot_category: &[],
      feed_kind: posts::FEED_KINDS,
      comment_sort: &[],
      notification_kind: users::NOTIFICATION_KINDS,
    },
    min_interval: Duration::from_millis(500),
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
    self.api.logged_in()
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    login::token(&self.api, token).await
  }

  async fn qr_start(&self) -> Result<QrTicket> {
    Ok(login::start(&self.api))
  }

  async fn qr_poll(&self, ticket: &QrTicket) -> Result<QrStatus> {
    login::poll(&self.api, ticket).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    search::posts(&self.api, query, page).await
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    search::users(&self.api, query, page).await
  }

  async fn search_topics(&self, query: &Query, page: &PageReq) -> Result<Page<Collection>> {
    search::topics(&self.api, query, page).await
  }

  async fn hot(&self, _category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    posts::featured(&self.api, page).await
  }

  async fn feed(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    posts::timeline(&self.api, kind, page).await
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
    posts::replies(&self.api, post, page).await
  }

  async fn replies(&self, _post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    posts::replies(&self.api, comment, page).await
  }

  async fn likers(&self, post: &str, page: &PageReq) -> Result<Page<User>> {
    posts::reactions(&self.api, post, page).await
  }

  async fn reposts(&self, post: &str, page: &PageReq) -> Result<Page<Post>> {
    posts::renotes(&self.api, post, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    users::posts(&self.api, user, page).await
  }

  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::relations(&self.api, user, true, page).await
  }

  async fn following(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::relations(&self.api, user, false, page).await
  }

  async fn favorites(
    &self,
    user: Option<&str>,
    folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    posts::favorites(&self.api, user, folder, page).await
  }

  async fn notifications(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    users::notifications(&self.api, kind, page).await
  }

  async fn unread(&self) -> Result<BTreeMap<String, u64>> {
    users::unread(&self.api).await
  }

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    posts::like(&self.api, post, undo).await
  }

  async fn favorite(&self, post: &str, _folder: Option<&str>, undo: bool) -> Result<Action> {
    posts::favorite(&self.api, post, undo).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    posts::comment(&self.api, post, reply).await
  }

  async fn delete_comment(&self, _post: &str, comment: &str) -> Result<Action> {
    posts::delete_note(&self.api, comment).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    users::follow(&self.api, user, undo).await
  }

  async fn publish(&self, draft: &Draft) -> Result<Action> {
    posts::publish(&self.api, draft).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    posts::delete(&self.api, post).await
  }

  async fn chats(&self, parent: Option<&str>, _page: &PageReq) -> Result<Page<Collection>> {
    chats::list(&self.api, parent).await
  }

  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    messages::list(&self.api, chat, page).await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    messages::send(&self.api, chat, message).await
  }

  async fn members(&self, chat: &str, page: &PageReq) -> Result<Page<User>> {
    chats::members(&self.api, chat, page).await
  }

  async fn react(&self, post: &str, emoji: &str, undo: bool) -> Result<Action> {
    posts::react(&self.api, post, emoji, undo).await
  }

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    live::listen(&self.api, chats, out).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
