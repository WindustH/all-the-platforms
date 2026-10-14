//! Facebook for all-the-platforms.
//!
//! Two ways in, by what they reach:
//! - **facebook.com** ([`web`], [`users`], [`posts`], [`groups`], …): what the site ships to a browser —
//!   profiles and Pages, posts with reactions and comments, groups — read
//!   anonymously where Facebook allows it; with a browser session
//!   (`c_user` and `xs`) also timelines past the newest post, the home
//!   feed, search, notifications and friends. Paging replays the GraphQL
//!   operation the page itself ran, changing only the cursor.
//! - **the Graph API** ([`pages`], [`insights`], [`messages`], `ads`): with an access token, the Pages it
//!   manages — posts, publishing, comments, insights, Messenger — and the
//!   Marketing API.

mod api;
mod comments;
mod extra;
mod feeds;
mod groups;
mod insights;
mod messages;
mod notifications;
mod pages;
mod parse;
mod posts;
mod refs;
mod relay;
mod users;
mod web;

use std::time::Duration;

use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Draft, Insights, Message, Notification,
  Page, PageReq, Platform, PlatformInfo, PlatformKind, Post, Query, Reply, Result, User,
};

use atp_meta::graph::FACEBOOK;

use crate::api::Api;

pub struct Facebook {
  api: Api,
}

impl Platform for Facebook {
  const INFO: PlatformInfo = PlatformInfo {
    id: "facebook",
    name: "Facebook",
    aliases: &["fb"],
    about: "Facebook: profiles, Pages, posts, groups, Page publishing and insights, Messenger, ads",
    home: "https://www.facebook.com/",
    cookie_domains: &["facebook.com"],
    required_cookies: &api::SESSION,
    caps: &[
      Cap::TokenLogin,
      Cap::Search,
      Cap::SearchUsers,
      Cap::Feed,
      Cap::Read,
      Cap::Comments,
      Cap::Replies,
      Cap::User,
      Cap::UserPosts,
      Cap::Notifications,
      Cap::Comment,
      Cap::CommentImages,
      Cap::DeleteComment,
      Cap::Publish,
      Cap::Delete,
      Cap::Download,
      Cap::AccountInsights,
      Cap::PostInsights,
      Cap::Chats,
      Cap::Messages,
      Cap::Send,
    ],
    choices: Choices::NONE,
    min_interval: Duration::from_millis(1500),
    kind: PlatformKind::Social,
    token: "ACCESS_TOKEN",
    env: &[
      "FACEBOOK_ACCESS_TOKEN",
      "META_ACCESS_TOKEN",
      "FACEBOOK_PAGE_ID",
      "META_AD_ACCOUNT_ID",
      "META_GRAPH_VERSION",
    ],
    guest: "user, user-posts, read, comments, group, group-posts, download",
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
    self.api.has_session() || self.api.token().is_some()
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    atp_meta::graph::login(&self.api.ctx, FACEBOOK, token).await
  }

  async fn token_whoami(&self) -> Result<User> {
    atp_meta::graph::token_owner(&self.api.ctx, "id,name,link").await
  }

  async fn whoami(&self) -> Result<User> {
    users::whoami(&self.api).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    feeds::search(&self.api, &query.keyword, page).await
  }

  async fn search_users(&self, query: &Query, _page: &PageReq) -> Result<Page<User>> {
    users::search(&self.api, &query.keyword).await
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
    comments::comments(&self.api, post, page).await
  }

  async fn replies(&self, _post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    comments::replies(&self.api, comment, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    users::user_posts(&self.api, user, page).await
  }

  async fn notifications(
    &self,
    _kind: Option<&str>,
    _page: &PageReq,
  ) -> Result<Page<Notification>> {
    notifications::list(&self.api).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    comments::comment(&self.api, post, reply).await
  }

  async fn delete_comment(&self, post: &str, comment: &str) -> Result<Action> {
    comments::delete(&self.api, post, comment).await
  }

  async fn publish(&self, draft: &Draft) -> Result<Action> {
    posts::publish(&self.api, draft).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    posts::delete(&self.api, post).await
  }

  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    match post {
      Some(post) => insights::post(&self.api, post).await,
      None => insights::account(&self.api, days).await,
    }
  }

  async fn chats(&self, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    messages::chats(&self.api, parent, page).await
  }

  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    messages::messages(&self.api, chat, page).await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    messages::send(&self.api, chat, message).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
