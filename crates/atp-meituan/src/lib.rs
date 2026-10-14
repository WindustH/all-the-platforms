//! Meituan (美团) for all-the-platforms.
//!
//! Deals are posts (`kind` `deal`) with their price in `price`. They come
//! from the official 美团联盟 (CPS) open API, signed with the person's own
//! media app key and secret ([`cps`]): search, rankings (`hot`), details
//! (`read`) and the account's orders (`insights`). Meituan's own pages
//! want `mtgsig`, which atp does not make; the reviews they show for 到店
//! shops come from Dianping's backend, so `comments` reads Dianping's
//! review list with the session's Dianping login ([`atp_mtsec::reviews`]).
//! City and area lookups are open to visitors.

mod api;
mod comments;
mod cps;
mod deals;
mod extra;
mod insights;
mod parse;
mod sign;
mod users;

use std::time::Duration;

use atp_core::{
  Cap, Choices, Comment, Ctx, Data, Error, Insights, Page, PageReq, Platform, PlatformInfo,
  PlatformKind, Post, Query, Result, User,
};

use crate::api::Api;

pub struct Meituan {
  api: Api,
}

impl Platform for Meituan {
  const INFO: PlatformInfo = PlatformInfo {
    id: "meituan",
    name: "Meituan",
    aliases: &["mt"],
    about: "Meituan (美团): deals and rankings via the 美团联盟 API, shop reviews, city lookups",
    home: "https://www.meituan.com",
    cookie_domains: &["meituan.com", "dianping.com"],
    required_cookies: &[],
    caps: &[
      Cap::TokenLogin,
      Cap::Search,
      Cap::Hot,
      Cap::Read,
      Cap::Comments,
      Cap::Download,
      Cap::AccountInsights,
    ],
    choices: Choices {
      search_sort: deals::SORTS,
      hot_category: deals::HOT,
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(2000),
    kind: PlatformKind::Local,
    token: "APP_KEY:APP_SECRET",
    env: &["MEITUAN_APP_KEY", "MEITUAN_APP_SECRET"],
    guest: "locate, areas",
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
    self.api.cps.has(&self.api.ctx) || self.api.has_dianping()
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    cps::Cps::save(&self.api.ctx, token)
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    deals::search(&self.api, query, page).await
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    deals::hot(&self.api, category, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    deals::read(&self.api, post).await
  }

  async fn comments(
    &self,
    post: &str,
    _sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    comments::list(&self.api, post, page).await
  }

  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    match post {
      Some(_) => Err(Error::unsupported("insights POST")),
      None => insights::account(&self.api, days).await,
    }
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
