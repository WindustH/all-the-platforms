//! Spotify for all-the-platforms, through the official Web API and the
//! user's own app: Authorization Code with PKCE in the browser ([`login`]),
//! tokens renewed with the session ([`api`]).
//!
//! Posts are tracks, albums, artists, playlists, shows and episodes (links,
//! `spotify:` URIs, `kind:ID` or ids); users are Spotify accounts, and the
//! artists you follow. Only endpoints Spotify still serves to
//! development-mode apps are used; nothing touches audio or DRM. This file
//! only describes the platform and delegates.

mod albums;
mod api;
mod artists;
mod extra;
mod insights;
mod library;
mod login;
mod parse;
mod player;
mod playlists;
mod refs;
mod search;
mod shows;
mod tracks;
mod users;

use std::time::Duration;

use atp_core::{
  Action, Cap, Choices, Ctx, Data, Error, Insights, Page, PageReq, Platform, PlatformInfo,
  PlatformKind, Post, Query, Result, User,
};

use crate::api::Api;
use crate::refs::Kind;

pub struct Spotify {
  api: Api,
}

impl Platform for Spotify {
  const INFO: PlatformInfo = PlatformInfo {
    id: "spotify",
    name: "Spotify",
    aliases: &["sp"],
    about: "Spotify through the official Web API and your own app: search, library, playlists, \
            top tracks and artists, playback control",
    home: "https://open.spotify.com",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::TokenLogin,
      Cap::Search,
      Cap::Read,
      Cap::User,
      Cap::UserPosts,
      Cap::Following,
      Cap::Favorites,
      Cap::Likes,
      Cap::History,
      Cap::Like,
      Cap::Favorite,
      Cap::Follow,
      Cap::AccountInsights,
      Cap::Download,
    ],
    choices: Choices {
      search_filter: search::FILTERS,
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(300),
    kind: PlatformKind::Music,
    token: "CLIENT_ID",
    env: &[api::ENV_CLIENT_ID, login::ENV_PORT],
    guest: "nothing",
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
    login::token(&self.api, token).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    search::posts(&self.api, query, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    let api = &self.api;
    let item = refs::item(api, post, Kind::Track).await?;
    match item.kind {
      Kind::Track => tracks::read(api, &item.id).await,
      Kind::Album => albums::read(api, &item.id).await,
      Kind::Artist => artists::read(api, &item.id).await,
      Kind::Playlist => playlists::read(api, &item.id).await,
      Kind::Show => shows::read(api, &item.id).await,
      Kind::Episode => shows::episode(api, &item.id).await,
      Kind::User => Err(Error::input("accounts are shown with `user`")),
    }
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::profile(&self.api, user).await
  }

  /// The playlists you made.
  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    users::playlists(&self.api, user, page).await
  }

  /// The artists you follow.
  async fn following(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::following(&self.api, user, page).await
  }

  async fn favorites(
    &self,
    user: Option<&str>,
    folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    library::favorites(&self.api, user, folder, page).await
  }

  /// Liked Songs.
  async fn likes(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    library::liked(&self.api, user, page).await
  }

  async fn history(&self, page: &PageReq) -> Result<Page<Post>> {
    library::history(&self.api, page).await
  }

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    library::like(&self.api, post, undo).await
  }

  async fn favorite(&self, post: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
    if folder.is_some() {
      return Err(Error::input(
        "favorite takes no folder; add tracks to a playlist with `playlist add`",
      ));
    }
    library::favorite(&self.api, post, undo).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    users::follow(&self.api, user, undo).await
  }

  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    insights::top(&self.api, post, days).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
