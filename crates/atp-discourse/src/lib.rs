//! Discourse for all-the-platforms (any forum; meta.discourse.org by
//! default, `DISCOURSE_URL` for another, such as linux.do), through its
//! JSON API ([`api`]): topic lists, topics and their posts ([`topics`]),
//! search ([`search`]), users, bookmarks and notifications ([`users`]),
//! likes, bookmarks, replies and new topics ([`posts`]), editing posts
//! and topics ([`edits`]), and logging in
//! with a User API Key or an admin's API key ([`login`]).

mod api;
mod edits;
mod extra;
mod login;
mod parse;
mod posts;
mod refs;
mod search;
mod topics;
mod users;

/// The commands only this platform has, for [`Platform::run_extra`](atp_core::Platform::run_extra).
pub use extra::Command;

use std::collections::BTreeMap;
use std::time::Duration;

use atp_core::{
  Action, Cap, Choices, Comment, Ctx, Data, Draft, Edit, Notification, Page, PageReq, Platform,
  PlatformInfo, PlatformKind, Post, Query, Reply, Result, User,
};

use crate::api::Api;

pub struct Discourse {
  api: Api,
}

impl Platform for Discourse {
  const INFO: PlatformInfo = PlatformInfo {
    id: "discourse",
    name: "Discourse",
    aliases: &["linuxdo"],
    about: "Discourse forums (any forum, such as linux.do): topics, posts, categories, search, likes, bookmarks and replies",
    home: "https://meta.discourse.org",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::TokenLogin,
      Cap::Search,
      Cap::SearchUsers,
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
      Cap::CommentImages,
      Cap::DeleteComment,
      Cap::Publish,
      Cap::Delete,
      Cap::Edit,
      Cap::EditTitle,
      Cap::EditTopics,
    ],
    choices: Choices {
      search_sort: search::SORTS,
      search_filter: &[],
      hot_category: &[],
      feed_kind: topics::FEED_KINDS,
      comment_sort: &[],
      notification_kind: users::NOTIFICATION_KINDS,
    },
    min_interval: Duration::from_millis(500),
    kind: PlatformKind::Social,
    token: "FORUM_URL",
    env: &[
      api::ENV_URL,
      api::ENV_KEY,
      api::ENV_USERNAME,
      login::ENV_PORT,
    ],
    guest: "on open forums: feed (latest), hot, top, categories, category, tag, read, comments, replies, search, user, user-posts",
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

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    search::users(&self.api, query, page).await
  }

  async fn hot(&self, _category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    topics::hot(&self.api, page).await
  }

  async fn feed(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    topics::feed(&self.api, kind, page).await
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
    topics::comments(&self.api, post, page).await
  }

  async fn replies(&self, _post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    topics::replies(&self.api, comment, page).await
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
    _folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    users::bookmarks(&self.api, user, page).await
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
    posts::bookmark(&self.api, post, undo).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    posts::comment(&self.api, post, reply).await
  }

  async fn delete_comment(&self, _post: &str, comment: &str) -> Result<Action> {
    posts::delete(&self.api, comment, true).await
  }

  async fn publish(&self, draft: &Draft) -> Result<Action> {
    posts::publish(&self.api, draft).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    posts::delete(&self.api, post, false).await
  }

  async fn edit_post(&self, post: &str, edit: &Edit) -> Result<Action> {
    edits::edit(&self.api, post, edit).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
