//! RSS, Atom and JSON Feed for all-the-platforms, read directly with no
//! service in between ([`entries`]); a local subscription list with a
//! merged timeline ([`subscriptions`]); RSSHub routes on the user's chosen
//! instance; and, optionally, the user's own Miniflux account
//! ([`miniflux`]).

mod api;
mod entries;
mod extra;
mod index;
mod miniflux;
mod parse;
mod refs;
mod subscriptions;

/// The commands only this platform has, for [`Platform::run_extra`](atp_core::Platform::run_extra).
pub use extra::Command;

use std::time::Duration;

use atp_core::{
  Action, Cap, Choices, Ctx, Data, Page, PageReq, Platform, PlatformInfo, PlatformKind, Post,
  Query, Result, User,
};

use crate::api::Api;
use crate::miniflux::Miniflux;

pub struct Rss {
  api: Api,
}

impl Platform for Rss {
  const INFO: PlatformInfo = PlatformInfo {
    id: "rss",
    name: "RSS",
    aliases: &[],
    about: "RSS, Atom and JSON feeds: subscriptions, a merged timeline, RSSHub routes, Miniflux",
    home: "https://rsshub.app",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::TokenLogin,
      Cap::Search,
      Cap::Feed,
      Cap::Read,
      Cap::User,
      Cap::UserPosts,
      Cap::Favorites,
      Cap::Favorite,
      Cap::Download,
    ],
    choices: Choices {
      search_sort: &[],
      search_filter: &["local"],
      hot_category: &[],
      feed_kind: entries::FEEDS,
      comment_sort: &[],
      notification_kind: &[],
    },
    min_interval: Duration::from_millis(200),
    kind: PlatformKind::Social,
    token: "URL|API_KEY",
    env: &[miniflux::ENV_URL, miniflux::ENV_KEY, api::ENV_HUB],
    guest: "read, user, user-posts, feed (local), search (local), add, remove, list, hub, download",
    guide: include_str!("../GUIDE.md"),
  };

  type Extra = Command;

  fn new(ctx: Ctx) -> Result<Self> {
    Ok(Self { api: Api::new(ctx) })
  }

  fn ctx(&self) -> &Ctx {
    &self.api.ctx
  }

  async fn whoami(&self) -> Result<User> {
    self.api.require_miniflux()?.me().await
  }

  fn logged_in(&self) -> bool {
    self.api.miniflux().is_some()
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    Miniflux::login(&self.api.ctx, token)
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    entries::search(&self.api, query, page).await
  }

  async fn feed(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    entries::feed(&self.api, kind, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    entries::read(&self.api, post).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    entries::source(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    entries::posts(&self.api, user, page).await
  }

  async fn favorites(
    &self,
    user: Option<&str>,
    folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    entries::starred(&self.api, user, folder, page).await
  }

  async fn favorite(&self, post: &str, _folder: Option<&str>, undo: bool) -> Result<Action> {
    entries::star(&self.api, post, undo).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
