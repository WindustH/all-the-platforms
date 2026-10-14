//! Baidu Tieba (百度贴吧) for all-the-platforms.
//!
//! - [`api`]: the signed client API and the mobile web, error mapping and the
//!   logged-in account; [`refs`]: thread, user, forum and topic references;
//!   [`login`]: QR login through Baidu passport.
//! - [`parse`]: JSON → models (threads are posts, floors comments).
//! - By subject: [`forums`] (threads of a forum, details, follows, sign-in),
//!   [`threads`] (reading, posting, deleting), [`topics`], [`search`],
//!   [`interactions`], [`uploads`], [`users`], [`notifications`];
//!   platform-only commands in [`extra`].

mod api;
mod extra;
mod forums;
mod interactions;
mod login;
mod notifications;
mod parse;
mod refs;
mod search;
mod threads;
mod topics;
mod uploads;
mod users;

use std::collections::BTreeMap;
use std::time::Duration;

use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Draft, Error, Notification, Page, PageReq,
  Platform, PlatformInfo, PlatformKind, Post, QrStatus, QrTicket, Query, Reply, Result, User,
};

use api::Api;

pub struct Tieba {
  api: Api,
}

impl Platform for Tieba {
  const INFO: PlatformInfo = PlatformInfo {
    id: "tieba",
    name: "Tieba",
    aliases: &["tb", "baidu-tieba"],
    about: "Baidu Tieba (百度贴吧): forums, threads, floors and replies",
    home: api::WEB,
    cookie_domains: &["baidu.com", "tieba.baidu.com"],
    required_cookies: &[api::SESSION_COOKIE],
    caps: &[
      Cap::QrLogin,
      Cap::Search,
      Cap::SearchUsers,
      Cap::SearchTopics,
      Cap::Hot,
      Cap::Read,
      Cap::Comments,
      Cap::Replies,
      Cap::User,
      Cap::UserPosts,
      Cap::Followers,
      Cap::Following,
      Cap::Collections,
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
    ],
    choices: Choices {
      search_sort: search::SEARCH_SORTS,
      search_filter: search::SEARCH_FILTERS,
      hot_category: &[],
      feed_kind: &[],
      comment_sort: threads::COMMENT_SORTS,
      notification_kind: notifications::KINDS,
    },
    min_interval: Duration::from_secs(1),
    kind: PlatformKind::Social,
    token: "",
    env: &[],
    guest: "search, hot, forum, topic, read, comments, replies, user, user-posts",
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

  async fn qr_start(&self) -> Result<QrTicket> {
    login::qr_start(&self.api).await
  }

  async fn qr_poll(&self, ticket: &QrTicket) -> Result<QrStatus> {
    login::qr_poll(&self.api, ticket).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    search::search(&self.api, query, None, page).await
  }

  async fn search_users(&self, query: &Query, _page: &PageReq) -> Result<Page<User>> {
    search::search_users(&self.api, query).await
  }

  async fn search_topics(&self, query: &Query, _page: &PageReq) -> Result<Page<Collection>> {
    search::search_forums(&self.api, query).await
  }

  async fn hot(&self, _category: Option<&str>, _page: &PageReq) -> Result<Page<Post>> {
    topics::hot(&self.api).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    if post.contains("topic_id=") {
      return Err(
        Error::input("that is a hot topic, not a thread")
          .with_hint("list its threads with `atp tieba topic TOPIC`"),
      );
    }
    threads::read(&self.api, post).await
  }

  async fn comments(
    &self,
    post: &str,
    sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    threads::comments(&self.api, post, sort, page).await
  }

  async fn replies(&self, post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    threads::replies(&self.api, post, comment, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    users::posts(&self.api, user, true, page).await
  }

  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::relations(&self.api, user, true, page).await
  }

  async fn following(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::relations(&self.api, user, false, page).await
  }

  async fn collections(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    users::forums(&self.api, user, page).await
  }

  async fn favorites(
    &self,
    user: Option<&str>,
    _folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    if user.is_some() {
      return Err(
        Error::unsupported("favorites USER").with_hint("only your own saved threads are listed"),
      );
    }
    notifications::favorites(&self.api, page).await
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

  async fn favorite(&self, post: &str, _folder: Option<&str>, undo: bool) -> Result<Action> {
    interactions::favorite(&self.api, post, undo).await
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

  async fn publish(&self, draft: &Draft) -> Result<Action> {
    threads::publish(&self.api, draft).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    threads::delete(&self.api, post).await
  }

  async fn run_extra(&self, command: extra::Command) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
