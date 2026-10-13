//! TikTok for social-cli.
//!
//! Reads pages' rehydration data where it can (videos, profiles, hashtags,
//! sounds) and the web client's signed API for lists (comments, a user's
//! videos, search, followers ...), with the browser session cookies
//! (`sessionid`) when logged in. Ad accounts go through the official
//! Marketing API (`ads`), with an access token.

mod ads;
mod api;
mod extra;
mod insights;
mod login;
mod parse;
mod refs;
mod sign;
mod users;
mod videos;
mod waf;
mod write;

use std::time::Duration;

use social_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Error, Insights, Media, Page, PageReq,
  Platform, PlatformInfo, Post, QrStatus, QrTicket, Query, Reply, Result, User,
};

use crate::api::Api;

pub struct TikTok {
  api: Api,
}

impl Platform for TikTok {
  const INFO: PlatformInfo = PlatformInfo {
    id: "tiktok",
    name: "TikTok",
    aliases: &["tt"],
    about: "TikTok: videos, photo posts, comments, users, sounds, hashtags and ads",
    home: "https://www.tiktok.com/",
    cookie_domains: &["tiktok.com"],
    required_cookies: &["sessionid"],
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
      Cap::User,
      Cap::UserPosts,
      Cap::Followers,
      Cap::Following,
      Cap::Collections,
      Cap::Favorites,
      Cap::Likes,
      Cap::Like,
      Cap::Favorite,
      Cap::Comment,
      Cap::DeleteComment,
      Cap::Follow,
      Cap::Delete,
      Cap::Download,
      Cap::AccountInsights,
      Cap::PostInsights,
    ],
    choices: Choices {
      search_filter: &["general", "video"],
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(1500),
    guide: include_str!("../GUIDE.md"),
  };

  type Extra = extra::Command;

  fn new(ctx: Ctx) -> Result<Self> {
    Ok(Self { api: Api::new(ctx) })
  }

  fn ctx(&self) -> &Ctx {
    &self.api.ctx
  }

  /// A browser session, or a Marketing API token.
  fn logged_in(&self) -> bool {
    self.api.logged_in() || ads::configured(&self.api)
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    if token.chars().any(char::is_whitespace) {
      return Err(Error::input("an access token has no spaces"));
    }
    self.api.ctx.set_extra(ads::TOKEN_KEY, token);
    Ok(())
  }

  async fn qr_start(&self) -> Result<QrTicket> {
    login::qr_start(&self.api).await
  }

  async fn qr_poll(&self, ticket: &QrTicket) -> Result<QrStatus> {
    login::qr_poll(&self.api, ticket).await
  }

  async fn whoami(&self) -> Result<User> {
    if !self.api.logged_in() && ads::configured(&self.api) {
      return ads::Ads::new(&self.api)?.me().await;
    }
    users::whoami(&self.api).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    videos::search(&self.api, query, page).await
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    videos::search_users(&self.api, query, page).await
  }

  async fn search_topics(&self, query: &Query, page: &PageReq) -> Result<Page<Collection>> {
    videos::search_hashtags(&self.api, query, page).await
  }

  async fn hot(&self, _category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    videos::recommended(&self.api, page).await
  }

  async fn feed(&self, _kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    videos::recommended(&self.api, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    videos::read(&self.api, post).await
  }

  async fn comments(
    &self,
    post: &str,
    _sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    videos::comments(&self.api, post, page).await
  }

  async fn replies(&self, post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    videos::replies(&self.api, post, comment, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
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

  async fn collections(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    users::collections(&self.api, user, page).await
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

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    write::like(&self.api, post, undo).await
  }

  async fn favorite(&self, post: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
    write::favorite(&self.api, post, folder, undo).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    write::comment(&self.api, post, reply).await
  }

  async fn delete_comment(&self, post: &str, comment: &str) -> Result<Action> {
    write::delete_comment(&self.api, post, comment).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    write::follow(&self.api, user, undo).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    write::delete(&self.api, post).await
  }

  /// Streams are bound to the `tt_chain_token` cookie the page set.
  async fn media(&self, post: &str, _audio_only: bool) -> Result<(Post, Vec<Media>)> {
    let post = videos::read(&self.api, post).await?;
    let token = self.api.ctx.http.cookie("tt_chain_token");
    let media = post
      .media
      .iter()
      .cloned()
      .map(|mut m| {
        if let Some(t) = &token {
          m.headers
            .push(("cookie".into(), format!("tt_chain_token={t}")));
        }
        m
      })
      .collect();
    Ok((post, media))
  }

  async fn insights(&self, post: Option<&str>, _days: u32) -> Result<Insights> {
    insights::insights(&self.api, post).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
