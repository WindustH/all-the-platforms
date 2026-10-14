//! Instagram for all-the-platforms.
//!
//! The web API ([`api`]) with a browser session does nearly everything
//! the site does: profiles, posts, comments, likers, followers, hashtags,
//! places, saved posts, the activity inbox, stories and highlights, likes,
//! comments, follows and direct messages. Profiles and single posts are
//! also read anonymously. Insights of business and creator accounts come
//! from the Graph API ([`insights`]), ads from the Marketing API.

mod api;
mod comments;
mod extra;
mod insights;
mod library;
mod live;
mod messages;
mod notifications;
mod parse;
mod posts;
mod refs;
mod search;
mod stories;
mod topics;
mod users;

use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Error, Insights, Message, Notification,
  Page, PageReq, Platform, PlatformInfo, PlatformKind, Post, Query, Reply, Result, User,
};
use atp_meta::graph::{self, host_for};

use crate::api::Api;

pub struct Instagram {
  api: Api,
}

impl Platform for Instagram {
  const INFO: PlatformInfo = PlatformInfo {
    id: "instagram",
    name: "Instagram",
    aliases: &["ig", "insta"],
    about: "Instagram: posts, reels, stories, comments, users, hashtags, direct messages and insights",
    home: "https://www.instagram.com/",
    cookie_domains: &["instagram.com"],
    required_cookies: &[api::SESSION],
    caps: &[
      Cap::TokenLogin,
      Cap::SearchUsers,
      Cap::SearchTopics,
      Cap::Read,
      Cap::Comments,
      Cap::Replies,
      Cap::Likers,
      Cap::User,
      Cap::UserPosts,
      Cap::Followers,
      Cap::Following,
      Cap::Collections,
      Cap::Favorites,
      Cap::Likes,
      Cap::Notifications,
      Cap::Like,
      Cap::Favorite,
      Cap::Comment,
      Cap::DeleteComment,
      Cap::Follow,
      Cap::Delete,
      Cap::Download,
      Cap::AccountInsights,
      Cap::PostInsights,
      Cap::Chats,
      Cap::Messages,
      Cap::Send,
      Cap::React,
      Cap::Listen,
    ],
    choices: Choices::NONE,
    min_interval: Duration::from_millis(2500),
    kind: PlatformKind::Social,
    token: "ACCESS_TOKEN",
    env: &[
      "INSTAGRAM_ACCESS_TOKEN",
      "META_ACCESS_TOKEN",
      "INSTAGRAM_USER_ID",
      "META_AD_ACCOUNT_ID",
      "META_GRAPH_VERSION",
    ],
    guest: "public accounts: user, read, first comments, latest user-posts",
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
    graph::login(&self.api.ctx, host_for(token), token).await
  }

  async fn whoami(&self) -> Result<User> {
    if self.api.has_session() {
      return users::whoami(&self.api).await;
    }
    match self.api.token() {
      Some(token) => {
        let graph = atp_meta::Graph::new(&self.api.ctx, host_for(&token), token);
        graph::me(&graph, "id,name").await
      }
      None => Err(Error::auth("not logged in").with_hint(self.api.ctx.login_hint())),
    }
  }

  async fn search_users(&self, query: &Query, _page: &PageReq) -> Result<Page<User>> {
    search::users(&self.api, &query.keyword).await
  }

  async fn search_topics(&self, query: &Query, _page: &PageReq) -> Result<Page<Collection>> {
    search::topics(&self.api, &query.keyword).await
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

  async fn replies(&self, post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    comments::replies(&self.api, post, comment, page).await
  }

  async fn likers(&self, post: &str, _page: &PageReq) -> Result<Page<User>> {
    users::likers(&self.api, post).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    posts::user_posts(&self.api, user, page).await
  }

  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::follows(&self.api, user, "followers", page).await
  }

  async fn following(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::follows(&self.api, user, "following", page).await
  }

  async fn collections(&self, user: Option<&str>, _page: &PageReq) -> Result<Page<Collection>> {
    library::collections(&self.api, user).await
  }

  async fn favorites(
    &self,
    user: Option<&str>,
    folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    library::saved(&self.api, user, folder, page).await
  }

  async fn likes(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    library::liked(&self.api, user, page).await
  }

  async fn notifications(
    &self,
    _kind: Option<&str>,
    _page: &PageReq,
  ) -> Result<Page<Notification>> {
    notifications::list(&self.api).await
  }

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    posts::like(&self.api, post, undo).await
  }

  async fn favorite(&self, post: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
    posts::save(&self.api, post, folder, undo).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    comments::comment(&self.api, post, reply).await
  }

  async fn delete_comment(&self, post: &str, comment: &str) -> Result<Action> {
    comments::delete(&self.api, post, comment).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    users::follow(&self.api, user, undo).await
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

  async fn chats(&self, _parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    messages::chats(&self.api, page).await
  }

  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    messages::messages(&self.api, chat, page).await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    messages::send(&self.api, chat, message).await
  }

  async fn react(&self, post: &str, emoji: &str, undo: bool) -> Result<Action> {
    messages::react(&self.api, post, emoji, undo).await
  }

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    live::listen(&self.api, chats, out).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
