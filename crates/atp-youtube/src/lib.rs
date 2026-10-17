//! YouTube for all-the-platforms.
//!
//! Talks to InnerTube, the API behind youtube.com (`/youtubei/v1/...`), as
//! the web client, anonymously or with the cookies of a browser session
//! (signed with `SAPISIDHASH`); streams and captions come from the Android
//! VR client (see [`api`]). Posts are videos, Shorts and community posts;
//! users are channels (`UC…` ids, @handles or links); collections are
//! playlists. Live chats are the chat side: `listen` follows them and
//! `send` writes to them ([`live`]); YouTube Music is in [`music`].

mod account;
mod api;
mod captions;
mod channels;
mod comments;
mod edits;
mod extra;
mod insights;
mod library;
mod live;
mod music;
mod notifications;
mod oauth;
mod parse;
mod playlists;
mod proto;
mod refs;
mod rss;
mod search;
mod sign;
mod streams;
mod videos;

/// The commands only this platform has, for [`Platform::run_extra`](atp_core::Platform::run_extra).
pub use extra::Command;

use std::collections::BTreeMap;
use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Edit, Error, FolderSpec, Insights, Media,
  Message, Notification, Page, PageReq, Platform, PlatformInfo, PlatformKind, Post, Query, Reply,
  Result, User,
};

use crate::api::Api;
use crate::channels::Tab;

pub struct YouTube {
  api: Api,
}

impl Platform for YouTube {
  const INFO: PlatformInfo = PlatformInfo {
    id: "youtube",
    name: "YouTube",
    aliases: &["yt"],
    about: "YouTube: videos, Shorts, channels, comments, playlists and transcripts",
    home: "https://www.youtube.com",
    cookie_domains: &["youtube.com"],
    required_cookies: &["SAPISID"],
    caps: &[
      Cap::Search,
      Cap::SearchUsers,
      Cap::SearchTopics,
      Cap::Hot,
      Cap::Feed,
      Cap::Read,
      Cap::Comments,
      Cap::Replies,
      Cap::User,
      Cap::UserPosts,
      Cap::Following,
      Cap::Collections,
      Cap::Favorites,
      Cap::Likes,
      Cap::History,
      Cap::Notifications,
      Cap::Unread,
      Cap::Like,
      Cap::Favorite,
      Cap::Comment,
      Cap::DeleteComment,
      Cap::Follow,
      Cap::Edit,
      Cap::EditTitle,
      Cap::EditTopics,
      Cap::EditCover,
      Cap::EditVisibility,
      Cap::Download,
      Cap::AccountInsights,
      Cap::PostInsights,
      Cap::Send,
      Cap::Listen,
      Cap::FolderCreate,
      Cap::FolderEdit,
      Cap::FolderDescription,
      Cap::FolderPrivacy,
      Cap::FolderDelete,
      Cap::FolderAdd,
      Cap::FolderRemove,
      Cap::FolderMove,
      Cap::WatchLater,
      Cap::LaterAdd,
      Cap::HistoryDelete,
      Cap::HistoryClear,
    ],
    choices: Choices {
      search_sort: search::SORTS,
      search_filter: search::FILTERS,
      hot_category: videos::HOT_CATEGORIES,
      feed_kind: library::FEED_KINDS,
      comment_sort: comments::SORTS,
      notification_kind: &[],
    },
    min_interval: Duration::from_millis(1000),
    kind: PlatformKind::Video,
    token: "",
    env: &[
      "YOUTUBE_CLIENT_ID",
      "YOUTUBE_CLIENT_SECRET",
      "YOUTUBE_REFRESH_TOKEN",
    ],
    guest: "search, hot, read, comments, user, user-posts, transcript, download",
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
    account::whoami(&self.api).await
  }

  fn logged_in(&self) -> bool {
    self.api.logged_in()
  }

  /// Some cookie exports lack `SAPISID`; `__Secure-3PAPISID` holds the same secret.
  async fn prepare_login(&self) -> Result<()> {
    let http = &self.api.ctx.http;
    if !http.has_cookie("SAPISID")
      && let Some(v) = http.cookie("__Secure-3PAPISID")
    {
      http.set_cookie("SAPISID", &v);
    }
    account::reset(&self.api);
    Ok(())
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    search::videos(&self.api, query, page).await
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    search::channels(&self.api, query, page).await
  }

  async fn search_topics(&self, query: &Query, page: &PageReq) -> Result<Page<Collection>> {
    search::playlists(&self.api, query, page).await
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    videos::hot(&self.api, category, page).await
  }

  async fn feed(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    library::feed(&self.api, kind, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    videos::read(&self.api, post).await
  }

  async fn comments(
    &self,
    post: &str,
    sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    comments::list(&self.api, post, sort, page).await
  }

  async fn replies(&self, post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    comments::replies(&self.api, post, comment, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    let user = if account::is_me(user) {
      account::my_channel(&self.api).await?
    } else {
      user.to_owned()
    };
    channels::profile(&self.api, &user, true).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    let l = channels::tab(&self.api, user, Tab::Videos, page).await?;
    Ok(Page::new(l.posts, l.next))
  }

  async fn following(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    library::following(&self.api, user, page).await
  }

  async fn collections(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    library::collections(&self.api, user, page).await
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
    library::likes(&self.api, user, page).await
  }

  async fn history(&self, page: &PageReq) -> Result<Page<Post>> {
    library::history(&self.api, page).await
  }

  async fn notifications(&self, _kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    notifications::list(&self.api, page).await
  }

  async fn unread(&self) -> Result<BTreeMap<String, u64>> {
    notifications::unread(&self.api).await
  }

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    videos::like(&self.api, post, undo).await
  }

  async fn favorite(&self, post: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
    library::favorite(&self.api, post, folder, undo).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    comments::add(&self.api, post, &reply.text, reply.reply_to.as_deref()).await
  }

  async fn delete_comment(&self, post: &str, comment: &str) -> Result<Action> {
    comments::delete(&self.api, post, comment).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    channels::follow(&self.api, user, undo).await
  }

  /// A new playlist (private).
  async fn folder_create(&self, folder: &FolderSpec) -> Result<Action> {
    playlists::create(&self.api, folder).await
  }

  async fn folder_edit(&self, folder: &str, change: &FolderSpec) -> Result<Action> {
    playlists::edit(&self.api, folder, change).await
  }

  async fn folder_delete(&self, folders: &[String]) -> Result<Vec<Action>> {
    playlists::delete(&self.api, folders).await
  }

  async fn folder_add(&self, folder: &str, posts: &[String]) -> Result<Vec<Action>> {
    playlists::file(&self.api, folder, posts, false).await
  }

  async fn folder_remove(&self, folder: &str, posts: &[String]) -> Result<Vec<Action>> {
    playlists::file(&self.api, folder, posts, true).await
  }

  /// The `WL` playlist.
  async fn watch_later(&self, page: &PageReq) -> Result<Page<Post>> {
    library::watch_later(&self.api, page).await
  }

  async fn later_add(&self, posts: &[String]) -> Result<Vec<Action>> {
    playlists::later(&self.api, posts, false).await
  }

  async fn later_remove(&self, posts: &[String]) -> Result<Vec<Action>> {
    playlists::later(&self.api, posts, true).await
  }

  async fn history_delete(&self, posts: &[String]) -> Result<Vec<Action>> {
    library::forget(&self.api, posts).await
  }

  /// The watch history; `search-history-clear` clears the search history.
  async fn history_clear(&self) -> Result<Action> {
    library::clear(&self.api, false).await
  }

  async fn edit_post(&self, post: &str, edit: &Edit) -> Result<Action> {
    edits::edit(&self.api, post, edit).await
  }

  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    insights::insights(&self.api, post, days).await
  }

  async fn media(&self, post: &str, audio_only: bool) -> Result<(Post, Vec<Media>)> {
    streams::media(&self.api, post, audio_only).await
  }

  /// A message to the live chat of a stream.
  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    if !message.files.is_empty() || message.reply_to.is_some() {
      return Err(Error::unsupported(
        "send --attach / --reply-to to a live chat",
      ));
    }
    live::send(&self.api, chat, &message.text).await
  }

  /// Follow the live chats of streams.
  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    live::listen(&self.api, chats, out).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
