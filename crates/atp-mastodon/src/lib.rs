//! Mastodon for all-the-platforms (any instance; mastodon.social by
//! default), through its REST API ([`api`]): statuses, timelines and
//! trends ([`posts`]), accounts and notifications ([`users`]), search
//! ([`search`]), and logging in with a token or the instance's OAuth
//! ([`login`]). Trends, statuses, accounts and hashtags are readable
//! without an account where the instance allows it.

mod api;
mod extra;
mod login;
mod parse;
mod posts;
mod refs;
mod search;
mod users;

use std::time::Duration;

use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Draft, Notification, Page, PageReq,
  Platform, PlatformInfo, PlatformKind, Post, Query, Reply, Result, User,
};

use crate::api::Api;

pub struct Mastodon {
  api: Api,
}

impl Platform for Mastodon {
  const INFO: PlatformInfo = PlatformInfo {
    id: "mastodon",
    name: "Mastodon",
    aliases: &["masto"],
    about: "Mastodon (any instance): statuses, timelines, trends, boosts, bookmarks and notifications",
    home: "https://mastodon.social",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
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
      Cap::User,
      Cap::UserPosts,
      Cap::Followers,
      Cap::Following,
      Cap::Favorites,
      Cap::Likes,
      Cap::Notifications,
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
      search_sort: &[],
      search_filter: &[],
      hot_category: posts::HOT_CATEGORIES,
      feed_kind: posts::FEED_KINDS,
      comment_sort: &[],
      notification_kind: users::NOTIFICATION_KINDS,
    },
    min_interval: Duration::from_millis(500),
    kind: PlatformKind::Social,
    token: "INSTANCE|TOKEN",
    env: &[api::ENV_INSTANCE, api::ENV_TOKEN, login::ENV_PORT],
    guest: "hot (trending statuses and links), trends, tag, read, comments, user, user-posts, followers, search -t user / topic",
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

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    search::posts(&self.api, query, page).await
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    search::users(&self.api, query, page).await
  }

  async fn search_topics(&self, query: &Query, page: &PageReq) -> Result<Page<Collection>> {
    search::topics(&self.api, query, page).await
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    posts::hot(&self.api, category, page).await
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
    posts::descendants(&self.api, post, false, page).await
  }

  async fn replies(&self, _post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    posts::descendants(&self.api, comment, true, page).await
  }

  async fn likers(&self, post: &str, page: &PageReq) -> Result<Page<User>> {
    posts::likers(&self.api, post, page).await
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
    _folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    posts::saved(&self.api, user, true, page).await
  }

  async fn likes(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    posts::saved(&self.api, user, false, page).await
  }

  async fn notifications(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    users::notifications(&self.api, kind, page).await
  }

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    posts::toggle(&self.api, post, "favourite", "unfavourite", undo).await
  }

  async fn favorite(&self, post: &str, _folder: Option<&str>, undo: bool) -> Result<Action> {
    posts::toggle(&self.api, post, "bookmark", "unbookmark", undo).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    posts::comment(&self.api, post, reply).await
  }

  async fn delete_comment(&self, _post: &str, comment: &str) -> Result<Action> {
    posts::delete(&self.api, comment).await
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

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
