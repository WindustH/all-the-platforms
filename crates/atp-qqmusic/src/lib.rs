//! QQ Music (QQ 音乐) for all-the-platforms: what a person does on
//! y.qq.com and in the app, through the signed module calls of the
//! `u.y.qq.com` gateway (signatures in [`sign`]).
//!
//! Posts are songs, albums, playlists, singers and charts (mids, ids,
//! `kind:ID` or y.qq.com links); comments are those of songs, albums and
//! playlists; users go by the encrypted id QQ Music shows. Catalog audio
//! is never fetched. This file only describes the platform and delegates.

mod albums;
mod api;
mod comments;
mod discover;
mod extra;
mod library;
mod login;
mod lyrics;
mod parse;
mod playlists;
mod refs;
mod search;
mod sign;
mod singers;
mod songs;
mod users;

use std::time::Duration;

use atp_core::{
  Action, Cap, Choices, Comment, Ctx, Data, Error, Page, PageReq, Platform, PlatformInfo,
  PlatformKind, Post, QrStatus, QrTicket, Query, Reply, Result, User,
};

use crate::api::Api;
use crate::refs::PostRef;

pub struct QqMusic {
  api: Api,
}

impl QqMusic {
  async fn post(&self, arg: &str) -> Result<PostRef> {
    refs::post(&self.api, arg).await
  }

  async fn thread(&self, post: &str) -> Result<comments::Thread> {
    comments::thread(&self.api, &self.post(post).await?).await
  }
}

impl Platform for QqMusic {
  const INFO: PlatformInfo = PlatformInfo {
    id: "qqmusic",
    name: "QQ Music",
    aliases: &["qqm"],
    about: "QQ Music (QQ 音乐): songs, lyrics with translations, playlists, charts, comments",
    home: "https://y.qq.com",
    cookie_domains: &["y.qq.com", "qq.com"],
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
    kind: PlatformKind::Music,
    token: "",
    env: &[],
    guest: "search, read, lyrics, hot, toplists, comments, user-posts, favorites and likes of a USER",
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

  async fn prepare_login(&self) -> Result<()> {
    login::prepare(&self.api);
    Ok(())
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

  /// Songs picked for you (猜你喜欢).
  async fn feed(&self, _kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    discover::guess(&self.api, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    let api = &self.api;
    match self.post(post).await? {
      PostRef::Song(key) => songs::read(api, &key).await,
      PostRef::Album(key) => albums::read(api, &key).await,
      PostRef::Playlist(id) => playlists::read(api, id).await,
      PostRef::Singer(mid) => singers::read(api, &mid).await,
      PostRef::Toplist(id) => discover::read(api, id).await,
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

  async fn replies(&self, _post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    comments::replies(&self.api, &refs::comment(comment)?, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::profile(&self.api, user).await
  }

  /// The playlists a user made.
  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    users::playlists(&self.api, user, page).await
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
    let euin = users::euin_or_me(&self.api, user).await?;
    library::liked(&self.api, &euin, page).await
  }

  /// 喜欢: songs only.
  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    match self.post(post).await? {
      PostRef::Song(key) => songs::like(&self.api, &key, undo).await,
      _ => Err(Error::input(
        "like (喜欢) takes a song; playlists and albums take favorite, singers follow",
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
      PostRef::Album(key) => albums::favorite(api, &key, undo).await,
      PostRef::Song(_) => Err(Error::input(
        "songs are liked (喜欢) with like, or added to a playlist with `playlist add`",
      )),
      PostRef::Singer(_) => Err(Error::input("singers are followed: follow singer:MID")),
      PostRef::Toplist(_) => Err(Error::input("charts cannot be collected")),
    }
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    self.api.require_login()?;
    let thread = self.thread(post).await?;
    let parent = reply.reply_to.as_deref().map(refs::comment).transpose()?;
    comments::add(&self.api, &thread, &reply.text, parent.as_deref()).await
  }

  async fn delete_comment(&self, _post: &str, comment: &str) -> Result<Action> {
    self.api.require_login()?;
    comments::delete(&self.api, &refs::comment(comment)?).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    users::follow(&self.api, user, undo).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
