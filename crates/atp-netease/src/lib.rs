//! NetEase Cloud Music (网易云音乐) for all-the-platforms: what a person
//! does in the app, through the web player's `weapi` and the desktop
//! client's `eapi` calls (both encrypted in [`sign`]).
//!
//! Posts are songs, albums, playlists, artists, podcast shows and episodes
//! (ids, `kind:ID` or music.163.com links); comments are those of songs,
//! albums, playlists and episodes; users are accounts. Catalog audio is
//! never fetched: the only audio atp touches is what you uploaded to your
//! own cloud drive. This file only describes the platform and delegates.

mod albums;
mod api;
mod cloud;
mod comments;
mod discover;
mod extra;
mod library;
mod login;
mod lyrics;
mod parse;
mod playlists;
mod podcasts;
mod refs;
mod search;
mod sign;
mod songs;
mod uploads;
mod users;

use std::time::Duration;

use atp_core::{
  Action, Cap, Choices, Comment, Ctx, Data, Error, Page, PageReq, Platform, PlatformInfo, Post,
  QrStatus, QrTicket, Query, Reply, Result, User,
};

use crate::api::Api;
use crate::refs::PostRef;

pub struct Netease {
  api: Api,
}

impl Netease {
  async fn post(&self, arg: &str) -> Result<PostRef> {
    refs::post(&self.api, arg).await
  }

  async fn thread(&self, post: &str) -> Result<String> {
    self.post(post).await?.thread()
  }
}

impl Platform for Netease {
  const INFO: PlatformInfo = PlatformInfo {
    id: "ncm",
    name: "NetEase Cloud Music",
    aliases: &["netease", "163music"],
    about: "NetEase Cloud Music (网易云音乐): songs, lyrics, playlists, charts, comments, your cloud drive",
    home: "https://music.163.com",
    cookie_domains: &["music.163.com"],
    required_cookies: api::LOGIN_COOKIES,
    caps: &[
      Cap::QrLogin,
      Cap::Search,
      Cap::SearchUsers,
      Cap::Hot,
      Cap::Feed,
      Cap::Read,
      Cap::Comments,
      Cap::Replies,
      Cap::User,
      Cap::UserPosts,
      Cap::Followers,
      Cap::Following,
      Cap::Favorites,
      Cap::Likes,
      Cap::History,
      Cap::Like,
      Cap::Favorite,
      Cap::Comment,
      Cap::DeleteComment,
      Cap::Follow,
      Cap::Download,
    ],
    choices: Choices {
      search_filter: search::FILTERS,
      hot_category: discover::CHARTS,
      comment_sort: comments::SORTS,
      ..Choices::NONE
    },
    min_interval: Duration::from_secs(1),
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

  async fn qr_start(&self) -> Result<QrTicket> {
    login::qr_start(&self.api).await
  }

  async fn qr_poll(&self, ticket: &QrTicket) -> Result<QrStatus> {
    login::qr_poll(&self.api, ticket).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    search::posts(&self.api, query, page).await
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    search::users(&self.api, query, page).await
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    discover::hot(&self.api, category, page).await
  }

  async fn feed(&self, _kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    discover::daily(&self.api, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    let api = &self.api;
    match self.post(post).await? {
      PostRef::Song(id) => songs::read(api, id).await,
      PostRef::Album(id) => albums::read(api, id).await,
      PostRef::Playlist(id) => playlists::read(api, id).await,
      PostRef::Artist(id) => albums::artist(api, id).await,
      PostRef::Episode(id) => podcasts::episode(api, id).await,
      PostRef::Podcast(id) => podcasts::show(api, id).await,
    }
  }

  async fn comments(
    &self,
    post: &str,
    sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    comments::list(&self.api, &self.thread(post).await?, sort, page).await
  }

  async fn replies(&self, post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    let thread = self.thread(post).await?;
    let comment = refs::number(comment, "comment")?;
    comments::replies(&self.api, &thread, comment, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::profile(&self.api, user).await
  }

  /// The playlists a user made.
  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    let uid = users::id(&self.api, user).await?;
    users::playlists(&self.api, &uid, false, page).await
  }

  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::followers(&self.api, user, page).await
  }

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

  async fn likes(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    let uid = users::id_or_me(&self.api, user).await?;
    library::liked(&self.api, &uid, page).await
  }

  async fn history(&self, page: &PageReq) -> Result<Page<Post>> {
    library::history(&self.api, page).await
  }

  /// 红心: songs only.
  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    match self.post(post).await? {
      PostRef::Song(id) => songs::like(&self.api, id, undo).await,
      _ => Err(Error::input(
        "like (红心) takes a song; playlists, albums, artists and podcasts take favorite",
      )),
    }
  }

  async fn favorite(&self, post: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
    if folder.is_some() {
      return Err(Error::input(
        "favorite takes no folder; add songs to a playlist with `playlist add`",
      ));
    }
    let api = &self.api;
    match self.post(post).await? {
      PostRef::Playlist(id) => playlists::favorite(api, id, undo).await,
      PostRef::Album(id) => albums::favorite(api, id, undo).await,
      PostRef::Artist(id) => albums::favorite_artist(api, id, undo).await,
      PostRef::Podcast(id) => podcasts::favorite(api, id, undo).await,
      PostRef::Song(_) => Err(Error::input(
        "songs are liked (红心) with like, or added to a playlist with `playlist add`",
      )),
      PostRef::Episode(_) => Err(Error::input(
        "episodes cannot be collected; subscribe to their podcast with favorite",
      )),
    }
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    self.api.require_login()?;
    let thread = self.thread(post).await?;
    let parent = reply
      .reply_to
      .as_deref()
      .map(|c| refs::number(c, "comment"))
      .transpose()?;
    comments::add(&self.api, &thread, &reply.text, parent).await
  }

  async fn delete_comment(&self, post: &str, comment: &str) -> Result<Action> {
    self.api.require_login()?;
    let thread = self.thread(post).await?;
    comments::delete(&self.api, &thread, refs::number(comment, "comment")?).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    users::follow(&self.api, user, undo).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
