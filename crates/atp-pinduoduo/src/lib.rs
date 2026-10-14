//! Pinduoduo (拼多多) for all-the-platforms.
//!
//! Goods are posts (`kind` `item`) with their group-buy price in `price`;
//! reviews are comments; shops are users. Two routes: the H5 site
//! `mobile.yangkeduo.com`, whose home page is open to visitors and whose
//! calls carry `anti_content` (a pure port in `sign`), and the official
//! 多多进宝 open API (`login --token CLIENT_ID:CLIENT_SECRET:PID`), which
//! serves search, goods details, rankings and categories. Everything else
//! on the H5 site needs a login (`PDDAccessToken`).

mod api;
mod categories;
mod comments;
mod ddk;
mod extra;
mod feeds;
mod parse;
mod posts;
mod refs;
mod search;
mod sign;
mod users;

use std::time::Duration;

use atp_core::{
  Cap, Choices, Comment, Ctx, Data, Media, Page, PageReq, Platform, PlatformInfo, PlatformKind,
  Post, Query, Result, User,
};

use crate::api::Api;

pub struct Pinduoduo {
  api: Api,
}

impl Platform for Pinduoduo {
  const INFO: PlatformInfo = PlatformInfo {
    id: "pdd",
    name: "Pinduoduo",
    aliases: &["pinduoduo", "yangkeduo"],
    about: "Pinduoduo (拼多多): goods with prices, reviews, shops, search, hot words and the home feed",
    home: "https://mobile.yangkeduo.com",
    cookie_domains: &["yangkeduo.com"],
    required_cookies: api::LOGIN_COOKIES,
    caps: &[
      Cap::TokenLogin,
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
      search_sort: search::SORTS,
      hot_category: feeds::HOT,
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(2000),
    kind: PlatformKind::Shopping,
    token: "CLIENT_ID:CLIENT_SECRET:PID",
    env: &["PDD_CLIENT_ID", "PDD_CLIENT_SECRET", "PDD_PID"],
    guest: "feed, hot, categories, category (home page); share-link images",
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
    self.api.has_session() || self.api.ddk.has(&self.api.ctx)
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    users::login_token(&self.api, token).await
  }

  async fn token_whoami(&self) -> Result<User> {
    users::app(&self.api).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    search::search(&self.api, query, page).await
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    feeds::hot(&self.api, category, page).await
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
    comments::list(&self.api, post, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    users::goods(&self.api, user, page).await
  }

  async fn media(&self, post: &str, _audio_only: bool) -> Result<(Post, Vec<Media>)> {
    posts::media(&self.api, post).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
