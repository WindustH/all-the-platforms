//! Dewu / POIZON (得物) for all-the-platforms. Read-only, without a user
//! login.
//!
//! Products are posts (`kind` `item`, `spuId`s as ids) with their lowest
//! price in `price` and their sizes in `extra.sizes`; community posts
//! (`trendId`s) are posts of kind `video` or `post`. Three routes:
//!
//! - the PC site www.dewu.com, whose calls to `app.dewu.com` carry a plain
//!   md5 `sign` (`sign::web`): home page tabs (`hot`, `tabs`, `tab`), the
//!   community hot list (`feed`) and product pages with per-size buy
//!   prices (`read`). The device headers the site's `pceptor.js` adds are
//!   not made; where Dewu insists on them (tab products, product pages),
//!   calls fail with `verification_required` or `upstream_error` and the
//!   page to open;
//! - community share pages on `m.dewu.com` (`read` and `download` of
//!   posts: the clean video or the images), no signature;
//! - the official POIZON Open Platform (`login --token
//!   APP_KEY:APP_SECRET`): `search` by article number, `prices`, `sales`.

mod api;
mod extra;
mod market;
mod open;
mod parse;
mod posts;
mod products;
mod refs;
mod sign;
mod users;

use std::time::Duration;

use atp_core::{
  Cap, Choices, Ctx, Data, Media, Page, PageReq, Platform, PlatformInfo, PlatformKind, Post, Query,
  Result, User,
};

use crate::api::Api;

pub struct Dewu {
  api: Api,
}

impl Platform for Dewu {
  const INFO: PlatformInfo = PlatformInfo {
    id: "dewu",
    name: "Dewu",
    aliases: &["poizon", "dw"],
    about: "Dewu / POIZON (得物): products with per-size prices, community posts and their videos, official market data",
    home: "https://www.dewu.com",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::TokenLogin,
      Cap::Search,
      Cap::Hot,
      Cap::Feed,
      Cap::Read,
      Cap::Download,
    ],
    choices: Choices::NONE,
    min_interval: Duration::from_millis(3000),
    kind: PlatformKind::Shopping,
    token: "APP_KEY:APP_SECRET",
    env: &["DEWU_APP_KEY", "DEWU_APP_SECRET"],
    guest: "everything but search, prices and sales",
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
    self.api.open.has(&self.api.ctx)
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    users::login_token(&self.api, token)
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    market::search(&self.api, query, page).await
  }

  async fn hot(&self, _category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    products::hot(&self.api, page).await
  }

  async fn feed(&self, _kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    posts::feed(&self.api, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    posts::read(&self.api, post).await
  }

  async fn media(&self, post: &str, _audio_only: bool) -> Result<(Post, Vec<Media>)> {
    posts::media(&self.api, post).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
