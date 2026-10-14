//! Amap (高德地图) for all-the-platforms, over the official Web Service API.
//!
//! Places ([`pois`]): keyword and nearby search, details and photos, input
//! tips; addresses and coordinates ([`geocoding`]); routes and distances
//! ([`routes`]); districts, weather, live traffic and static maps. A
//! Web Service key (optionally with its signing secret) is the login.

mod api;
mod districts;
mod extra;
mod geocoding;
mod maps;
mod parse;
mod pois;
mod refs;
mod routes;
mod sign;
mod traffic;
mod users;
mod weather;

use std::time::Duration;

use atp_core::{
  Cap, Choices, Ctx, Data, Page, PageReq, Platform, PlatformInfo, Post, Query, Result, User,
};

use crate::api::Api;

pub struct Amap {
  api: Api,
}

impl Platform for Amap {
  const INFO: PlatformInfo = PlatformInfo {
    id: "amap",
    name: "Amap",
    aliases: &["gaode"],
    about: "Amap (高德地图): places, geocoding, routes, weather, traffic and maps via the Web Service API",
    home: "https://www.amap.com",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[Cap::TokenLogin, Cap::Search, Cap::Read, Cap::Download],
    choices: Choices {
      search_filter: pois::FILTERS,
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(150),
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
    self.api.save(token)
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    pois::search(&self.api, query, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    pois::read(&self.api, post).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
