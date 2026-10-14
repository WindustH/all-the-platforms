//! Tencent Map (腾讯地图) for all-the-platforms, over the official
//! WebService API with the user's own key.
//!
//! Places ([`places`]): search, nearby and recommendations, details and
//! suggestions; addresses ([`geocoding`]); routes with decompressed
//! polylines and distance matrices ([`routes`]); districts, weather, IP
//! location and static maps. Tencent works in GCJ-02; a key with an SK
//! signs every request ([`sign`]).

mod api;
mod districts;
mod extra;
mod geocoding;
mod maps;
mod parse;
mod places;
mod polyline;
mod refs;
mod routes;
mod sign;
mod users;
mod weather;

use std::time::Duration;

use atp_core::geo::System;
use atp_core::{
  Cap, Choices, Ctx, Data, Page, PageReq, Platform, PlatformInfo, PlatformKind, Post, Query,
  Result, User,
};

use crate::api::Api;

pub struct TencentMap {
  api: Api,
}

impl Platform for TencentMap {
  const INFO: PlatformInfo = PlatformInfo {
    id: "qqmap",
    name: "Tencent Map",
    aliases: &["tencentmap"],
    about: "Tencent Map (腾讯地图): places, geocoding, routes, districts, weather and maps via the WebService API",
    home: "https://map.qq.com",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[Cap::TokenLogin, Cap::Search, Cap::Read],
    choices: Choices {
      search_filter: places::FILTERS,
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(220),
    kind: PlatformKind::Local,
    token: "KEY[:SK]",
    env: &["TENCENT_MAP_KEY", "TENCENT_MAP_SK"],
    guest: "nothing",
    guide: include_str!("../GUIDE.md"),
  };

  const COORD: Option<System> = Some(System::Gcj02);

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
    places::search(&self.api, query, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    places::read(&self.api, post).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
