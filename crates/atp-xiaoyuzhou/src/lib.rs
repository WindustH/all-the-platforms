//! Xiaoyuzhou (小宇宙) for all-the-platforms, read-only through the
//! public pages of www.xiaoyuzhoufm.com ([`api`]): shows and the episodes
//! their pages list ([`shows`]), episodes with show notes, hot comments
//! and their audio files ([`episodes`]), and users with the shows they
//! host ([`users`]). There is no login: the app's logged-in API is never
//! used.

mod api;
mod episodes;
mod parse;
mod refs;
mod shows;
mod users;

use std::time::Duration;

use atp_core::{
  Cap, Choices, Comment, Ctx, Data, Error, Media, NoExtra, Page, PageReq, Platform, PlatformInfo,
  PlatformKind, Post, Result, User,
};

use crate::api::Api;

pub struct Xiaoyuzhou {
  api: Api,
}

impl Platform for Xiaoyuzhou {
  const INFO: PlatformInfo = PlatformInfo {
    id: "xiaoyuzhou",
    name: "Xiaoyuzhou",
    aliases: &["xyz"],
    about: "Xiaoyuzhou (小宇宙): podcast shows, episodes with show notes, hot comments, downloads",
    home: api::BASE,
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::Read,
      Cap::Comments,
      Cap::User,
      Cap::UserPosts,
      Cap::Download,
    ],
    choices: Choices::NONE,
    min_interval: Duration::from_millis(1500),
    kind: PlatformKind::Music,
    token: "",
    env: &[],
    guest: "read, comments, user, user-posts, download",
    guide: include_str!("../GUIDE.md"),
  };

  type Extra = NoExtra;

  fn new(ctx: Ctx) -> Result<Self> {
    Ok(Self { api: Api::new(ctx) })
  }

  fn ctx(&self) -> &Ctx {
    &self.api.ctx
  }

  async fn whoami(&self) -> Result<User> {
    Err(Error::unsupported("whoami").with_hint(
      "ATP reads Xiaoyuzhou's public pages only; there is no login (see `atp guide xiaoyuzhou`)",
    ))
  }

  fn logged_in(&self) -> bool {
    false
  }

  async fn read(&self, post: &str) -> Result<Post> {
    episodes::read_any(&self.api, post).await
  }

  async fn comments(
    &self,
    post: &str,
    _sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    episodes::comments(&self.api, post, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    users::posts(&self.api, user, page).await
  }

  async fn media(&self, post: &str, _audio_only: bool) -> Result<(Post, Vec<Media>)> {
    episodes::media(&self.api, post).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    match command {}
  }
}
