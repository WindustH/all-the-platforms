//! Twitter / X for all-the-platforms.
//!
//! Talks to the web client's GraphQL API with the session cookies
//! (`auth_token`, `ct0`), or with a guest token when logged out (profiles,
//! single tweets and user timelines). Collections are the user's
//! lists; bookmark folders have their own `folders` command. Analytics come
//! from the web client's analytics pages (`insights`).

mod account;
mod api;
mod comments;
mod communities;
mod extra;
mod graphql;
mod insights;
mod lists;
mod messages;
mod notifications;
mod parse;
mod posts;
mod refs;
mod sign;
mod spaces;
mod tabs;
mod timeline;
mod trends;
mod uploads;
mod users;
mod webapp;
mod write;

use std::time::Duration;

use std::collections::BTreeMap;

use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Draft, Insights, Message, Notification,
  Page, PageReq, Platform, PlatformInfo, PlatformKind, Post, Query, Reply, Result, User,
};

use crate::api::Api;

pub struct Twitter {
  api: Api,
}

impl Platform for Twitter {
  const INFO: PlatformInfo = PlatformInfo {
    id: "twitter",
    name: "Twitter / X",
    aliases: &["x", "tw"],
    about: "Twitter / X: tweets, replies, timelines, users, lists, communities, Spaces, direct messages and analytics",
    home: "https://x.com",
    cookie_domains: &["x.com", "twitter.com"],
    required_cookies: &["auth_token", "ct0"],
    caps: &[
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
      Cap::CommentImages,
      Cap::DeleteComment,
      Cap::Follow,
      Cap::Publish,
      Cap::Delete,
      Cap::Edit,
      Cap::Download,
      Cap::AccountInsights,
      Cap::PostInsights,
      Cap::Chats,
      Cap::Messages,
      Cap::Send,
      Cap::Members,
      Cap::React,
      Cap::Listen,
    ],
    choices: Choices {
      search_sort: &["top", "latest", "media"],
      search_filter: &["media", "images", "videos", "links"],
      hot_category: &["trending", "for-you", "news", "sports", "entertainment"],
      feed_kind: &["for-you", "following"],
      comment_sort: &["relevance", "recency", "likes"],
      notification_kind: &["all", "verified", "mentions"],
    },
    min_interval: Duration::from_millis(1500),
    kind: PlatformKind::Social,
    token: "",
    env: &[],
    guest: "user, read, user-posts, download, insights of a tweet",
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

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    posts::search(&self.api, query, page).await
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    users::search(&self.api, query, page).await
  }

  /// Lists (X's search has no topics; its Lists tab is the closest).
  async fn search_topics(&self, query: &Query, page: &PageReq) -> Result<Page<Collection>> {
    users::search_lists(&self.api, query, page).await
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    trends::hot(&self.api, category, page).await
  }

  async fn feed(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    posts::feed(&self.api, kind, page).await
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
    comments::comments(&self.api, post, sort, page).await
  }

  async fn replies(&self, _post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    comments::replies(&self.api, comment, page).await
  }

  /// Only the author sees the likers (likes are private since June 2024).
  async fn likers(&self, post: &str, page: &PageReq) -> Result<Page<User>> {
    users::likers(&self.api, post, page).await
  }

  /// Quotes; retweets have no post of their own (`retweeters` lists their accounts).
  async fn reposts(&self, post: &str, page: &PageReq) -> Result<Page<Post>> {
    posts::quotes(&self.api, post, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    posts::user_posts(&self.api, user, page).await
  }

  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::followers(&self.api, user, page).await
  }

  async fn following(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::following(&self.api, user, page).await
  }

  async fn collections(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    users::lists(&self.api, user, page).await
  }

  async fn favorites(
    &self,
    user: Option<&str>,
    folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    posts::bookmarks(&self.api, user, folder, page).await
  }

  async fn likes(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    posts::likes(&self.api, user, page).await
  }

  async fn notifications(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    notifications::list(&self.api, kind, page).await
  }

  async fn unread(&self) -> Result<BTreeMap<String, u64>> {
    account::unread(&self.api).await
  }

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    write::like(&self.api, post, undo).await
  }

  /// With `folder`: file the bookmark into that folder (or take it out).
  async fn favorite(&self, post: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
    match folder {
      Some(folder) => account::file(&self.api, post, folder, undo).await,
      None => write::bookmark(&self.api, post, None, undo).await,
    }
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    write::reply(&self.api, post, reply).await
  }

  async fn delete_comment(&self, _post: &str, comment: &str) -> Result<Action> {
    write::delete(&self.api, comment, "delete-comment").await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    write::follow(&self.api, user, undo).await
  }

  async fn publish(&self, draft: &Draft) -> Result<Action> {
    write::publish(&self.api, draft).await
  }

  /// A post, or a direct message (`CONVERSATION/MESSAGE`).
  async fn delete(&self, post: &str) -> Result<Action> {
    if messages::is_dm_ref(post) {
      messages::delete(&self.api, post).await
    } else {
      write::delete(&self.api, post, "delete").await
    }
  }

  async fn edit(&self, post: &str, text: &str) -> Result<Action> {
    write::edit(&self.api, post, text).await
  }

  /// Direct-message conversations.
  async fn chats(&self, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    messages::chats(&self.api, parent, page).await
  }

  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    messages::messages(&self.api, chat, page).await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    messages::send(&self.api, chat, message).await
  }

  async fn members(&self, chat: &str, _page: &PageReq) -> Result<Page<User>> {
    messages::members(&self.api, chat).await
  }

  async fn react(&self, post: &str, emoji: &str, undo: bool) -> Result<Action> {
    messages::react(&self.api, post, emoji, undo).await
  }

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    messages::listen(&self.api, chats, out).await
  }

  /// Full analytics need X Premium; see `insights` for what is left without it.
  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    match post {
      Some(post) => insights::post(&self.api, post, days).await,
      None => insights::account(&self.api, days).await,
    }
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
