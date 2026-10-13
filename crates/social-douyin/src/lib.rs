//! Douyin (抖音) for social-cli.
//!
//! Posts are awemes: videos (`douyin.com/video/<id>`) and image posts
//! (`/note/<id>`); users are `sec_uid`s. Every web API call is signed with
//! `a_bogus` (see [`sign`]). Writes are not offered: the web checks them
//! with ticket-guard headers bound to keys the browser created at login.

mod api;
mod aweme;
mod comment;
mod device;
mod discover;
mod extra;
mod library;
mod live;
mod notice;
mod parse;
mod refs;
mod sign;
mod user;

use std::time::Duration;

use social_core::{
  Cap, Choices, Collection, Comment, Ctx, Data, Media, Notification, Page, PageReq, Platform,
  PlatformInfo, Post, Query, Result, User,
};

pub use extra::Extra;

pub struct Douyin {
  ctx: Ctx,
}

impl Platform for Douyin {
  const INFO: PlatformInfo = PlatformInfo {
    id: "douyin",
    name: "Douyin",
    aliases: &["dy"],
    about: "Douyin (抖音): videos, image posts, comments, users and hot boards",
    home: "https://www.douyin.com",
    cookie_domains: &["douyin.com"],
    required_cookies: api::LOGIN_COOKIES,
    caps: &[
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
      Cap::Download,
    ],
    choices: Choices {
      search_sort: discover::SEARCH_SORTS,
      search_filter: discover::SEARCH_FILTERS,
      hot_category: discover::HOT_CATEGORIES,
      feed_kind: discover::FEED_KINDS,
      comment_sort: &[],
      notification_kind: notice::KINDS,
    },
    min_interval: Duration::from_millis(800),
    guide: include_str!("../GUIDE.md"),
  };

  type Extra = Extra;

  fn new(mut ctx: Ctx) -> Result<Self> {
    api::configure(&mut ctx.http);
    Ok(Self { ctx })
  }

  fn ctx(&self) -> &Ctx {
    &self.ctx
  }

  async fn whoami(&self) -> Result<User> {
    user::whoami(&self.ctx).await
  }

  async fn prepare_login(&self) -> Result<()> {
    device::ensure(&self.ctx).await;
    Ok(())
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    discover::search(&self.ctx, query, page).await
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    discover::search_users(&self.ctx, query, page).await
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    discover::hot(&self.ctx, category, page).await
  }

  async fn feed(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    discover::feed(&self.ctx, kind, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    aweme::read(&self.ctx, post).await
  }

  async fn comments(
    &self,
    post: &str,
    _sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    comment::list(&self.ctx, post, page).await
  }

  async fn replies(&self, post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    comment::replies(&self.ctx, post, comment, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    user::profile(&self.ctx, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    user::posts(&self.ctx, user, page).await
  }

  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    user::followers(&self.ctx, user, page).await
  }

  async fn following(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    user::following(&self.ctx, user, page).await
  }

  async fn collections(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    if user.is_some() {
      return Err(social_core::Error::unsupported(
        "collections of other users",
      ));
    }
    library::folders(&self.ctx, page).await
  }

  async fn favorites(
    &self,
    user: Option<&str>,
    folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    if user.is_some() {
      return Err(social_core::Error::unsupported("favorites of other users"));
    }
    library::favorites(&self.ctx, folder, page).await
  }

  async fn likes(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    user::likes(&self.ctx, user, page).await
  }

  async fn history(&self, page: &PageReq) -> Result<Page<Post>> {
    library::history(&self.ctx, page).await
  }

  async fn notifications(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    notice::list(&self.ctx, kind, page).await
  }

  async fn media(&self, post: &str, audio_only: bool) -> Result<(Post, Vec<Media>)> {
    aweme::media(&self.ctx, post, audio_only).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.ctx, command).await
  }
}
