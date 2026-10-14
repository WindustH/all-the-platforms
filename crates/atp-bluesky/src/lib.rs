//! Bluesky for all-the-platforms, through AT Protocol XRPC ([`api`]):
//! feeds, posts and threads ([`posts`]), accounts and notifications
//! ([`users`]), search and trends ([`search`]), and everything written to
//! your repository ([`records`], with rich-text [`facets`]). Public reads
//! go to the public AppView; logging in with an app password
//! ([`login`]) moves everything to your PDS ([`identity`] finds it).

mod api;
mod extra;
mod facets;
mod identity;
mod login;
mod parse;
mod posts;
mod records;
mod refs;
mod search;
mod users;

use std::collections::BTreeMap;
use std::time::Duration;

use atp_core::{
  Action, Cap, Choices, Comment, Ctx, Data, Draft, Media, Notification, Page, PageReq, Platform,
  PlatformInfo, PlatformKind, Post, Query, Reply, Result, User,
};

use crate::api::Api;

pub struct Bluesky {
  api: Api,
}

impl Platform for Bluesky {
  const INFO: PlatformInfo = PlatformInfo {
    id: "bluesky",
    name: "Bluesky",
    aliases: &["bsky"],
    about: "Bluesky: posts, threads, feeds, trends, likes, reposts, bookmarks and notifications",
    home: "https://bsky.app",
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
      Cap::Likers,
      Cap::Reposts,
      Cap::User,
      Cap::UserPosts,
      Cap::Followers,
      Cap::Following,
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
      Cap::Download,
    ],
    choices: Choices {
      search_sort: search::SORTS,
      search_filter: &[],
      hot_category: &[],
      feed_kind: posts::FEED_KINDS,
      comment_sort: &[],
      notification_kind: users::NOTIFICATION_KINDS,
    },
    min_interval: Duration::from_millis(300),
    kind: PlatformKind::Social,
    token: "HANDLE|APP_PASSWORD",
    env: &[],
    guest: "hot (Discover), feeds, trends, read, comments, likers, reposts, user, user-posts, followers, search -t user",
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

  async fn hot(&self, _category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    posts::generator(&self.api, posts::DISCOVER, page).await
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
    posts::thread(&self.api, post, page).await
  }

  async fn replies(&self, _post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    posts::thread(&self.api, comment, page).await
  }

  async fn likers(&self, post: &str, page: &PageReq) -> Result<Page<User>> {
    posts::likers(&self.api, post, page).await
  }

  async fn reposts(&self, post: &str, page: &PageReq) -> Result<Page<Post>> {
    posts::quotes(&self.api, post, page).await
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
    posts::bookmarks(&self.api, user, page).await
  }

  async fn likes(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    posts::likes(&self.api, user, page).await
  }

  async fn notifications(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    users::notifications(&self.api, kind, page).await
  }

  async fn unread(&self) -> Result<BTreeMap<String, u64>> {
    users::unread(&self.api).await
  }

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    records::mark(&self.api, post, false, undo).await
  }

  async fn favorite(&self, post: &str, _folder: Option<&str>, undo: bool) -> Result<Action> {
    records::bookmark(&self.api, post, undo).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    records::comment(&self.api, post, reply).await
  }

  async fn delete_comment(&self, _post: &str, comment: &str) -> Result<Action> {
    records::delete(&self.api, comment).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    records::follow(&self.api, user, undo).await
  }

  async fn publish(&self, draft: &Draft) -> Result<Action> {
    records::publish(&self.api, draft).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    records::delete(&self.api, post).await
  }

  async fn media(&self, post: &str, _audio_only: bool) -> Result<(Post, Vec<Media>)> {
    posts::media(&self.api, post).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
