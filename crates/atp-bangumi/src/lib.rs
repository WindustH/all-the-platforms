//! Bangumi (番组计划) for all-the-platforms, through its official API
//! ([`api`]): subjects, characters, persons, episodes, the airing calendar
//! and users without an account ([`subjects`]); with a personal access
//! token, your collection and progress ([`users`]).

mod api;
mod extra;
mod parse;
mod refs;
mod subjects;
mod users;

use std::time::Duration;

use atp_core::{
  Cap, Choices, Ctx, Data, Page, PageReq, Platform, PlatformInfo, PlatformKind, Post, Query,
  Result, User,
};

use crate::api::Api;

pub struct Bangumi {
  api: Api,
}

impl Platform for Bangumi {
  const INFO: PlatformInfo = PlatformInfo {
    id: "bangumi",
    name: "Bangumi",
    aliases: &["bgm"],
    about: "Bangumi (番组计划): anime, books, music, games, the airing calendar and your collection",
    home: "https://bgm.tv",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[Cap::TokenLogin, Cap::Search, Cap::Hot, Cap::Read, Cap::User],
    choices: Choices {
      search_sort: subjects::SORTS,
      search_filter: subjects::FILTERS,
      hot_category: subjects::DAYS,
      feed_kind: &[],
      comment_sort: &[],
      notification_kind: &[],
    },
    min_interval: Duration::from_millis(500),
    kind: PlatformKind::Social,
    token: "TOKEN",
    env: &[api::ENV_TOKEN],
    guest: "search, hot (calendar), read, user, characters, persons, relations, episodes, browse, library",
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
    self.api.token().is_some()
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    users::login(&self.api, token)
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    subjects::search(&self.api, query, page).await
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    subjects::calendar(&self.api, category, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    subjects::read(&self.api, post).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
