//! Dianping (大众点评) for all-the-platforms: a small, slow reader.
//!
//! Shops are posts (`kind` `shop`, the spend per person in `price`);
//! reviews are comments; discovery notes are posts (`kind` `note`);
//! members are users. Visitors get a shop's mobile page (by numeric id)
//! and the discovery feed; search, reviews and member pages want a login.
//! Every request goes through [`atp_mtsec::guard`]; the gateway's checks
//! and refusals surface as `verification_required` with the page to open.

mod api;
mod comments;
mod extra;
mod feeds;
mod login;
mod parse;
mod posts;
mod refs;
mod search;
mod users;

use std::time::Duration;

use atp_core::{
  Cap, Choices, Comment, Ctx, Data, Page, PageReq, Platform, PlatformInfo, PlatformKind, Post,
  QrStatus, QrTicket, Query, Result, User,
};

use crate::api::Api;

pub struct Dianping {
  api: Api,
}

impl Platform for Dianping {
  const INFO: PlatformInfo = PlatformInfo {
    id: "dianping",
    name: "Dianping",
    aliases: &["dp"],
    about: "Dianping (大众点评): shops, reviews, search and the discovery feed",
    home: "https://www.dianping.com",
    cookie_domains: &["dianping.com"],
    required_cookies: api::LOGIN_COOKIES,
    caps: &[
      Cap::QrLogin,
      Cap::Search,
      Cap::Feed,
      Cap::Read,
      Cap::Comments,
      Cap::User,
      Cap::Download,
    ],
    choices: Choices {
      search_filter: api::CHANNEL_NAMES,
      feed_kind: api::CHANNEL_NAMES,
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(3000),
    kind: PlatformKind::Local,
    token: "",
    env: &[],
    guest: "read, download (shops by numeric id, notes), feed, cities, locate",
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

  async fn qr_start(&self) -> Result<QrTicket> {
    login::qr_start(&self.api).await
  }

  async fn qr_poll(&self, ticket: &QrTicket) -> Result<QrStatus> {
    login::qr_poll(&self.api, ticket).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    search::search(&self.api, query, page).await
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
    _sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    comments::list(&self.api, post, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
