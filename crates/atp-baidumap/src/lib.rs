//! Baidu Map (百度地图) for all-the-platforms, over the official Web
//! Service API with the user's own AK.
//!
//! Places ([`places`], place search 2.0): search, nearby, details and
//! suggestions; addresses ([`geocoding`]); routes and route matrices
//! ([`routes`]); weather, IP location, live traffic and static maps.
//! Points travel in BD-09, or in GCJ-02 (`coord_type` / `ret_coordtype`)
//! when `--coord` asks for another system. An AK in SN mode signs every
//! request ([`sign`]).

mod api;
mod extra;
mod geocoding;
mod maps;
mod parse;
mod places;
mod refs;
mod routes;
mod sign;
mod traffic;
mod users;
mod weather;

/// The commands only this platform has, for [`Platform::run_extra`](atp_core::Platform::run_extra).
pub use extra::Command;

use std::time::Duration;

use atp_core::geo::System;
use atp_core::{
  Cap, Choices, Ctx, Data, Page, PageReq, Platform, PlatformInfo, PlatformKind, Post, Query,
  Result, User,
};

use crate::api::Api;

pub struct BaiduMap {
  api: Api,
}

impl Platform for BaiduMap {
  const INFO: PlatformInfo = PlatformInfo {
    id: "bmap",
    name: "Baidu Map",
    aliases: &["baidumap"],
    about: "Baidu Map (百度地图): places, geocoding, routes, weather, traffic and maps via the Web Service API",
    home: "https://map.baidu.com",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[Cap::TokenLogin, Cap::Search, Cap::Read],
    choices: Choices {
      search_filter: places::FILTERS,
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(350),
    kind: PlatformKind::Local,
    token: "AK[:SK]",
    env: &["BAIDU_MAP_AK", "BAIDU_MAP_SK"],
    guest: "nothing",
    guide: include_str!("../GUIDE.md"),
  };

  const COORD: Option<System> = Some(System::Bd09);

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

  fn logged_in(&self) -> bool {
    self.api.logged_in()
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    self.api.save(token)
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    places::search(&self.api, query, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    places::read(&self.api, post).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
