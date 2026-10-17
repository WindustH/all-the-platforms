//! Ctrip (携程) for all-the-platforms: a slow, read-only reader.
//!
//! Sights and hotels are posts (`kind` `sight` / `hotel`, prices in
//! `price`), their reviews are comments (sights with sub-scores); community
//! notes are posts and their authors users; rankings are collections;
//! trains, transfer plans and the days of a flight fare calendar are posts
//! with fares. Everything here is open to visitors except `rooms`, which
//! wants a login (`cticket`).

mod api;
mod cities;
mod comments;
mod extra;
mod fares;
mod feeds;
mod hotels;
mod notes;
mod parse;
mod posts;
mod rankings;
mod refs;
mod search;
mod sights;
mod trains;
mod users;

/// The commands only this platform has, for [`Platform::run_extra`](atp_core::Platform::run_extra).
pub use extra::Command;

use std::time::Duration;

use atp_core::{
  Cap, Choices, Comment, Ctx, Data, Page, PageReq, Platform, PlatformInfo, PlatformKind, Post,
  Query, Result, User,
};

use crate::api::Api;

pub struct Ctrip {
  api: Api,
}

impl Platform for Ctrip {
  const INFO: PlatformInfo = PlatformInfo {
    id: "ctrip",
    name: "Ctrip",
    aliases: &["xiecheng"],
    about: "Ctrip (携程): sights, hotels, reviews, rankings, trains, flight fares and travel notes",
    home: "https://www.ctrip.com",
    cookie_domains: &["ctrip.com"],
    required_cookies: api::LOGIN_COOKIES,
    caps: &[
      Cap::Search,
      Cap::Hot,
      Cap::Feed,
      Cap::Read,
      Cap::Comments,
      Cap::User,
      Cap::UserPosts,
      Cap::Download,
    ],
    choices: Choices {
      hot_category: rankings::CATEGORY_NAMES,
      feed_kind: feeds::KINDS,
      comment_sort: comments::SORTS,
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(2500),
    kind: PlatformKind::Local,
    token: "",
    env: &[],
    guest: "everything except rooms (sights, hotels, reviews, trains, fares, notes)",
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

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    search::search(&self.api, query, page).await
  }

  async fn hot(&self, category: Option<&str>, _page: &PageReq) -> Result<Page<Post>> {
    rankings::hot(&self.api, category).await
  }

  async fn feed(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    feeds::feed(&self.api, kind, page).await
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
    comments::list(&self.api, post, sort, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    users::posts(&self.api, user, page).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
