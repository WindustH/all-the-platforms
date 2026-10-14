//! Bilibili for all-the-platforms.
//!
//! Posts are videos (`BV...` / `av...`), dynamics (`t.bilibili.com/<id>`) or
//! articles (`cv…`); users are mids, space links or names. Chats are private
//! conversations (`dm:MID`) and live rooms (`live:ROOM`). The modules below
//! own one domain each; this file only describes the platform and delegates.

mod api;
mod articles;
mod bangumi;
mod comments;
mod device;
mod discover;
mod dynamics;
mod extra;
mod folders;
mod history;
mod insights;
mod library;
mod lists;
mod live;
mod login;
mod media;
mod messages;
mod notifications;
mod paging;
mod parse;
mod proto;
mod refs;
mod relations;
mod rooms;
mod search;
mod sign;
mod users;
mod videos;

use std::collections::BTreeMap;
use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Draft, Error, Insights, Media, Message,
  Notification, Page, PageReq, Platform, PlatformInfo, PlatformKind, Post, QrStatus, QrTicket,
  Query, Reply, Result, User, ValueExt,
};

use crate::api::Api;
use crate::refs::PostRef;

pub struct Bilibili {
  api: Api,
}

impl Bilibili {
  async fn post(&self, arg: &str) -> Result<PostRef> {
    refs::post(&self.api, arg).await
  }

  async fn section(&self, post: &str) -> Result<comments::Section> {
    comments::section(&self.api, &self.post(post).await?).await
  }

  /// Likers and reposts are those of a dynamic; a video's are those of the
  /// dynamic that announced it.
  async fn dynamic_id(&self, post: &str) -> Result<String> {
    match self.post(post).await? {
      PostRef::Dynamic(id) => Ok(id),
      PostRef::Video(v) => dynamics::of_video(&self.api, &v).await,
      PostRef::Article(id) => articles::view(&self.api, id)
        .await?
        .str("dyn_id_str")
        .filter(|d| d != "0")
        .ok_or_else(|| Error::not_found(format!("cv{id} has no dynamic"))),
    }
  }
}

impl Platform for Bilibili {
  const INFO: PlatformInfo = PlatformInfo {
    id: "bili",
    name: "Bilibili",
    aliases: &["bilibili", "b23"],
    about: "Bilibili (哔哩哔哩)",
    home: "https://www.bilibili.com",
    cookie_domains: &["bilibili.com"],
    required_cookies: api::LOGIN_COOKIES,
    caps: &[
      Cap::QrLogin,
      Cap::Search,
      Cap::SearchUsers,
      Cap::SearchTopics,
      Cap::Hot,
      Cap::Feed,
      Cap::Read,
      Cap::Comments,
      Cap::Replies,
      Cap::Likers,
      Cap::Reposts,
      Cap::User,
      Cap::UserPosts,
      Cap::Followers,
      Cap::Following,
      Cap::Collections,
      Cap::Favorites,
      Cap::History,
      Cap::Notifications,
      Cap::Unread,
      Cap::Like,
      Cap::Favorite,
      Cap::Comment,
      Cap::DeleteComment,
      Cap::Follow,
      Cap::Publish,
      Cap::Delete,
      Cap::Download,
      Cap::AccountInsights,
      Cap::PostInsights,
      Cap::Chats,
      Cap::Messages,
      Cap::Send,
      Cap::Listen,
    ],
    choices: Choices {
      search_sort: &["totalrank", "click", "pubdate", "dm", "stow", "scores"],
      search_filter: search::FILTERS,
      hot_category: videos::HOT_CATEGORIES,
      feed_kind: &["all", "video"],
      comment_sort: &["hot", "time"],
      notification_kind: &["reply", "at", "like"],
    },
    min_interval: Duration::from_millis(300),
    kind: PlatformKind::Video,
    token: "",
    env: &[],
    guest: "search, hot, read, comments, user, user-posts, live, listen, download",
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
    device::ensure(&self.api).await;
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

  async fn search_topics(&self, query: &Query, page: &PageReq) -> Result<Page<Collection>> {
    search::topics(&self.api, query, page).await
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    videos::search_users(&self.api, query, page).await
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    videos::hot(&self.api, category, page).await
  }

  async fn feed(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    dynamics::feed(&self.api, kind, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    match self.post(post).await? {
      PostRef::Video(v) => videos::read(&self.api, &v).await,
      PostRef::Dynamic(id) => dynamics::read(&self.api, &id).await,
      PostRef::Article(id) => articles::read(&self.api, id).await,
    }
  }

  async fn comments(
    &self,
    post: &str,
    sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    let section = self.section(post).await?;
    comments::list(&self.api, &section, sort, page).await
  }

  async fn replies(&self, post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    let section = self.section(post).await?;
    let (root, _) = refs::reply_target(comment)?;
    comments::thread(&self.api, &section, &root, page).await
  }

  async fn likers(&self, post: &str, page: &PageReq) -> Result<Page<User>> {
    let id = self.dynamic_id(post).await?;
    dynamics::likers(&self.api, &id, page).await
  }

  async fn reposts(&self, post: &str, page: &PageReq) -> Result<Page<Post>> {
    let id = self.dynamic_id(post).await?;
    dynamics::reposts(&self.api, &id, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::profile(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    let mid = users::mid(&self.api, user).await?;
    videos::uploads(&self.api, &mid, page).await
  }

  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::followers(&self.api, user, page).await
  }

  async fn following(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::following(&self.api, user, page).await
  }

  async fn collections(&self, user: Option<&str>, _page: &PageReq) -> Result<Page<Collection>> {
    library::collections(&self.api, user).await
  }

  async fn favorites(
    &self,
    user: Option<&str>,
    folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    library::favorites(&self.api, user, folder, page).await
  }

  async fn history(&self, page: &PageReq) -> Result<Page<Post>> {
    library::history(&self.api, page).await
  }

  async fn notifications(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    notifications::notifications(&self.api, kind, page).await
  }

  async fn unread(&self) -> Result<BTreeMap<String, u64>> {
    let mut counts = notifications::unread(&self.api).await?;
    // Private messages have their own counters (best effort).
    if let Ok(dm) = messages::unread(&self.api).await {
      counts.extend(dm);
    }
    Ok(counts)
  }

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    match self.post(post).await? {
      PostRef::Video(v) => videos::like(&self.api, &v, undo).await,
      PostRef::Dynamic(id) => dynamics::like(&self.api, &id, undo).await,
      PostRef::Article(id) => articles::like(&self.api, id, undo).await,
    }
  }

  async fn favorite(&self, post: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
    match self.post(post).await? {
      PostRef::Video(v) => library::favorite(&self.api, &v, folder, undo).await,
      PostRef::Dynamic(id) => dynamics::favorite(&self.api, &id, undo).await,
      PostRef::Article(id) => articles::favorite(&self.api, id, undo).await,
    }
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    self.api.require_login()?;
    let section = self.section(post).await?;
    comments::add(&self.api, &section, &reply.text, reply.reply_to.as_deref()).await
  }

  async fn delete_comment(&self, post: &str, comment: &str) -> Result<Action> {
    self.api.require_login()?;
    let section = self.section(post).await?;
    comments::delete(&self.api, &section, comment).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    users::follow(&self.api, user, undo).await
  }

  async fn publish(&self, draft: &Draft) -> Result<Action> {
    let quote = match draft.quote.as_deref() {
      None => None,
      Some(q) => match self.post(q).await? {
        PostRef::Dynamic(id) => Some(id),
        _ => return Err(Error::input("--quote takes a dynamic")),
      },
    };
    dynamics::publish(&self.api, draft, quote.as_deref()).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    match self.post(post).await? {
      PostRef::Dynamic(id) => dynamics::delete(&self.api, &id).await,
      PostRef::Video(_) => Err(Error::unsupported("delete (videos)")),
      PostRef::Article(_) => Err(Error::unsupported("delete (articles)")),
    }
  }

  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    match post {
      None => insights::account(&self.api, days).await,
      Some(p) => insights::post(&self.api, &self.post(p).await?, days).await,
    }
  }

  async fn media(&self, post: &str, audio_only: bool) -> Result<(Post, Vec<Media>)> {
    match self.post(post).await? {
      PostRef::Video(v) => media::media(&self.api, &v, audio_only).await,
      PostRef::Dynamic(id) => media::dynamic_media(&self.api, &id, audio_only).await,
      PostRef::Article(id) => {
        let post = articles::read(&self.api, id).await?;
        let media = post.media.clone();
        Ok((post, media))
      }
    }
  }

  async fn chats(&self, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    if parent.is_some() {
      return Err(Error::unsupported("chats PARENT (Bilibili chats are flat)"));
    }
    messages::chats(&self.api, page).await
  }

  /// A private conversation, or the latest danmaku of a live room.
  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    if refs::room(chat).is_some() {
      return rooms::recent(&self.api, chat).await;
    }
    messages::messages(&self.api, chat, page).await
  }

  /// A private message, or danmaku to a live room (`live:ROOM`).
  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    if refs::room(chat).is_some() {
      if !message.files.is_empty() || message.reply_to.is_some() {
        return Err(Error::unsupported(
          "send --attach / --reply-to (live rooms)",
        ));
      }
      return rooms::send(&self.api, chat, &message.text).await;
    }
    messages::send(&self.api, chat, message).await
  }

  /// Live danmaku, super chats, gifts and memberships of live rooms.
  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    live::listen(&self.api, chats, out).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
