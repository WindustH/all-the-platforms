//! Google Maps for all-the-platforms, over the official Google Maps
//! Platform APIs with the user's own key.
//!
//! Places ([`places`], Places API New): text and nearby search, details,
//! autocomplete and photos; addresses ([`geocoding`]); routes and distance
//! matrices ([`routes`], Routes API); time zones, elevation and static
//! maps. Points inside mainland China go to Google in GCJ-02 and come back
//! tagged so; elsewhere they are WGS-84.

mod api;
mod elevations;
mod extra;
mod geocoding;
mod maps;
mod parse;
mod places;
mod refs;
mod routes;
mod timezones;
mod users;

use std::time::Duration;

use atp_core::geo::System;
use atp_core::{
  Cap, Choices, Ctx, Data, Media, Page, PageReq, Platform, PlatformInfo, PlatformKind, Post, Query,
  Result, User,
};

use crate::api::Api;

pub struct GoogleMaps {
  api: Api,
}

impl Platform for GoogleMaps {
  const INFO: PlatformInfo = PlatformInfo {
    id: "gmap",
    name: "Google Maps",
    aliases: &["googlemaps", "gmaps"],
    about: "Google Maps: places, geocoding, routes, time zones, elevation and maps via the Google Maps Platform APIs",
    home: "https://www.google.com/maps",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[Cap::TokenLogin, Cap::Search, Cap::Read, Cap::Download],
    choices: Choices {
      search_filter: places::FILTERS,
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(100),
    kind: PlatformKind::Local,
    token: "KEY",
    env: &["GOOGLE_MAPS_API_KEY"],
    guest: "nothing",
    guide: include_str!("../GUIDE.md"),
  };

  const COORD: Option<System> = Some(System::Wgs84);

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

  async fn media(&self, post: &str, _audio_only: bool) -> Result<(Post, Vec<Media>)> {
    places::photos(&self.api, post).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
