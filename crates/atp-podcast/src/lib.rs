//! Podcasts for all-the-platforms: Apple's public directory (search,
//! charts, show lookups) and Podcast Index with the user's own key
//! ([`api`], [`shows`]), each show's own feed for its episodes and show
//! notes, and the episode files the publisher put in the feed
//! ([`episodes`]).

mod api;
mod episodes;
mod extra;
mod login;
mod parse;
mod refs;
mod shows;

/// The commands only this platform has, for [`Platform::run_extra`](atp_core::Platform::run_extra).
pub use extra::Command;

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::time::Duration;

use atp_core::{
  Cap, Choices, Ctx, Data, Media, Page, PageReq, Platform, PlatformInfo, PlatformKind, Post, Query,
  Result, User,
};

use crate::api::Api;

pub struct Podcast {
  api: Api,
  /// `download --audio-only`: video episodes go to the shared downloader,
  /// which keeps their sound.
  audio_only: Cell<bool>,
}

impl Platform for Podcast {
  const INFO: PlatformInfo = PlatformInfo {
    id: "podcast",
    name: "Podcasts",
    aliases: &["pod"],
    about: "Podcasts: Apple's directory and charts, show feeds, episodes with show notes, downloads",
    home: "https://podcasts.apple.com",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::TokenLogin,
      Cap::Search,
      Cap::Hot,
      Cap::Read,
      Cap::Download,
    ],
    choices: Choices {
      search_sort: &[],
      search_filter: shows::FILTERS,
      hot_category: shows::CHARTS,
      feed_kind: &[],
      comment_sort: &[],
      notification_kind: &[],
    },
    min_interval: Duration::from_millis(1000),
    kind: PlatformKind::Music,
    token: "KEY:SECRET",
    env: &[api::ENV_KEY, api::ENV_SECRET, api::ENV_COUNTRY],
    guest: "search, hot (Apple charts), read, episodes, download",
    guide: include_str!("../GUIDE.md"),
  };

  type Extra = Command;

  fn new(ctx: Ctx) -> Result<Self> {
    Ok(Self {
      api: Api::new(ctx),
      audio_only: Cell::new(false),
    })
  }

  fn ctx(&self) -> &Ctx {
    &self.api.ctx
  }

  async fn whoami(&self) -> Result<User> {
    login::whoami(&self.api).await
  }

  fn logged_in(&self) -> bool {
    self.api.index_keys().is_some()
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    login::token(&self.api, token)
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    shows::search(&self.api, query, page).await
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    shows::hot(&self.api, category, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    episodes::read_any(&self.api, post).await
  }

  async fn media(&self, post: &str, audio_only: bool) -> Result<(Post, Vec<Media>)> {
    self.audio_only.set(audio_only);
    episodes::media(&self.api, post).await
  }

  async fn save_media(&self, media: &Media, dir: &Path, name: &str) -> Option<Result<PathBuf>> {
    episodes::save(&self.api, media, self.audio_only.get(), dir, name).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
