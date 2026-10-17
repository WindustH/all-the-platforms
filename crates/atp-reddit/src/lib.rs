//! Reddit for all-the-platforms.
//!
//! Reads Reddit's JSON listings (`www.reddit.com/<path>.json`) anonymously or
//! with the cookies of a browser session, and writes through the classic API
//! with the session's modhash; OAuth "script" app credentials in the
//! environment switch both to `oauth.reddit.com` (see [`api`]). Posts are
//! `t3` things, comments `t1`; collections are subreddits. Comment threads
//! are loaded completely ([`comments`]); analytics are in [`insights`].

mod api;
mod comments;
mod extra;
mod insights;
mod library;
mod listings;
mod live;
mod messages;
mod moderation;
mod multis;
mod notifications;
mod oauth;
mod parse;
mod posts;
mod refs;
mod subreddits;
mod uploads;
mod users;
mod videos;
mod write;

/// The commands only this platform has, for [`Platform::run_extra`](atp_core::Platform::run_extra).
pub use extra::Command;

use std::collections::BTreeMap;
use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Draft, Edit, Error, Insights, Media,
  Message, Notification, Page, PageReq, Platform, PlatformInfo, PlatformKind, Post, Query, Reply,
  Result, User,
};

use crate::api::Api;

pub struct Reddit {
  api: Api,
  threads: comments::Threads,
}

impl Platform for Reddit {
  const INFO: PlatformInfo = PlatformInfo {
    id: "reddit",
    name: "Reddit",
    aliases: &["rd"],
    about: "Reddit: posts, comments, subreddits, users and the inbox",
    home: "https://www.reddit.com",
    cookie_domains: &["reddit.com"],
    required_cookies: &[api::SESSION_COOKIE],
    caps: &[
      Cap::Search,
      Cap::SearchUsers,
      Cap::SearchTopics,
      Cap::Hot,
      Cap::Feed,
      Cap::Read,
      Cap::Comments,
      Cap::Replies,
      Cap::Reposts,
      Cap::User,
      Cap::UserPosts,
      Cap::Collections,
      Cap::Favorites,
      Cap::Likes,
      Cap::Notifications,
      Cap::Unread,
      Cap::Like,
      Cap::Favorite,
      Cap::Comment,
      Cap::DeleteComment,
      Cap::Follow,
      Cap::Publish,
      Cap::Delete,
      Cap::Edit,
      Cap::Download,
      Cap::Chats,
      Cap::Messages,
      Cap::Send,
      Cap::Listen,
      Cap::AccountInsights,
      Cap::PostInsights,
      Cap::Block,
      Cap::Blocked,
    ],
    choices: Choices {
      search_sort: posts::SEARCH_SORTS,
      search_filter: posts::TIME_RANGES,
      hot_category: &["popular", "all"],
      feed_kind: posts::FEED_KINDS,
      comment_sort: comments::SORTS,
      notification_kind: notifications::KINDS,
    },
    min_interval: Duration::from_millis(1000),
    kind: PlatformKind::Social,
    token: "",
    env: &[
      "REDDIT_CLIENT_ID",
      "REDDIT_CLIENT_SECRET",
      "REDDIT_USERNAME",
      "REDDIT_PASSWORD",
    ],
    guest: "search, hot, feed, read, comments, user, user-posts, download, sub",
    guide: include_str!("../GUIDE.md"),
  };

  type Extra = Command;

  fn new(ctx: Ctx) -> Result<Self> {
    Ok(Self {
      api: Api::new(ctx),
      threads: comments::Threads::default(),
    })
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

  async fn prepare_login(&self) -> Result<()> {
    self.api.reset_account();
    Ok(())
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    posts::search(&self.api, query, page).await
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    users::search(&self.api, query, page).await
  }

  async fn search_topics(&self, query: &Query, page: &PageReq) -> Result<Page<Collection>> {
    subreddits::search(&self.api, query, page).await
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    posts::hot(&self.api, category, page).await
  }

  async fn feed(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    posts::feed(&self.api, kind, page).await
  }

  async fn edit_post(&self, post: &str, edit: &Edit) -> Result<Action> {
    write::edit(&self.api, post, edit).await
  }

  async fn chats(&self, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    if parent.is_some() {
      return Err(Error::input("Reddit message threads have no parent"));
    }
    messages::chats(&self.api, page).await
  }

  async fn messages(&self, chat: &str, _page: &PageReq) -> Result<Page<Post>> {
    messages::messages(&self.api, chat).await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    messages::send(&self.api, chat, message).await
  }

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    live::listen(&self.api, chats, out).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    posts::read(&self.api, post).await
  }

  async fn comments(
    &self,
    post: &str,
    sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    comments::list(&self.api, &self.threads, post, sort, page).await
  }

  async fn replies(&self, post: &str, comment: &str, _page: &PageReq) -> Result<Page<Comment>> {
    comments::replies(&self.api, &self.threads, post, comment).await
  }

  async fn reposts(&self, post: &str, page: &PageReq) -> Result<Page<Post>> {
    posts::crossposts(&self.api, post, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    posts::user_posts(&self.api, user, page).await
  }

  async fn collections(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    subreddits::subscriptions(&self.api, user, page).await
  }

  async fn favorites(
    &self,
    user: Option<&str>,
    folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    if folder.is_some() {
      return Err(Error::input("Reddit has no folders for saved posts"));
    }
    posts::own(&self.api, user, "saved", page).await
  }

  async fn likes(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    posts::own(&self.api, user, "upvoted", page).await
  }

  async fn notifications(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    notifications::list(&self.api, kind, page).await
  }

  async fn unread(&self) -> Result<BTreeMap<String, u64>> {
    notifications::unread(&self.api).await
  }

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    let (dir, name) = if undo { (0, "unlike") } else { (1, "like") };
    write::vote(&self.api, post, dir, name).await
  }

  async fn favorite(&self, post: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
    write::save(&self.api, post, folder, undo).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    write::comment(&self.api, post, &reply.text, reply.reply_to.as_deref()).await
  }

  async fn delete_comment(&self, _post: &str, comment: &str) -> Result<Action> {
    write::delete_comment(&self.api, comment).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    subreddits::subscribe(&self.api, user, undo).await
  }

  /// Several subreddits and profiles go 50 per `/api/subscribe` call.
  async fn follow_users(&self, users: &[String], undo: bool) -> Result<Vec<Action>> {
    subreddits::subscribe_all(&self.api, users, undo).await
  }

  async fn block(&self, users: &[String], undo: bool) -> Result<Vec<Action>> {
    users::block_all(&self.api, users, undo).await
  }

  async fn blocked(&self, _page: &PageReq) -> Result<Page<User>> {
    users::blocked(&self.api).await
  }

  fn follow_ref(&self, arg: &str) -> Result<String> {
    refs::follow_target(&self.api.ctx, arg)
  }

  async fn publish(&self, draft: &Draft) -> Result<Action> {
    write::publish(&self.api, draft).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    write::delete_post(&self.api, post).await
  }

  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    match post {
      Some(post) => insights::post(&self.api, post).await,
      None => insights::account(&self.api, days).await,
    }
  }

  async fn media(&self, post: &str, _audio_only: bool) -> Result<(Post, Vec<Media>)> {
    posts::media(&self.api, post).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
