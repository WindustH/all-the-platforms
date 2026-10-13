//! Baidu Tieba (百度贴吧) for social-cli.
//!
//! - [`api`]: the signed client API and the mobile web, error mapping and the
//!   logged-in account; [`refs`]: thread, user, forum and topic references;
//!   [`login`]: QR login through Baidu passport.
//! - [`parse`]: JSON → models (threads are posts, floors comments).
//! - By domain: [`forum`] (forums, hot topics, search), [`thread`],
//!   [`people`], [`inbox`], [`write`]; platform-only commands in [`extra`].

mod api;
mod extra;
mod forum;
mod inbox;
mod login;
mod parse;
mod people;
mod refs;
mod thread;
mod write;

use std::collections::BTreeMap;
use std::time::Duration;

use social_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Draft, Error, Notification, Page, PageReq,
  Platform, PlatformInfo, Post, QrStatus, QrTicket, Query, Reply, Result, User,
};

use api::Client;

pub struct Tieba {
  client: Client,
}

impl Platform for Tieba {
  const INFO: PlatformInfo = PlatformInfo {
    id: "tieba",
    name: "Tieba",
    aliases: &["tb", "baidu-tieba"],
    about: "Baidu Tieba (百度贴吧): forums, threads, floors and replies",
    home: api::WEB,
    cookie_domains: &["baidu.com", "tieba.baidu.com"],
    required_cookies: &[api::SESSION_COOKIE],
    caps: &[
      Cap::QrLogin,
      Cap::Search,
      Cap::SearchUsers,
      Cap::SearchTopics,
      Cap::Hot,
      Cap::Read,
      Cap::Comments,
      Cap::Replies,
      Cap::User,
      Cap::UserPosts,
      Cap::Followers,
      Cap::Following,
      Cap::Collections,
      Cap::Favorites,
      Cap::Notifications,
      Cap::Unread,
      Cap::Like,
      Cap::Favorite,
      Cap::Comment,
      Cap::CommentImages,
      Cap::DeleteComment,
      Cap::Follow,
      Cap::Publish,
      Cap::Delete,
      Cap::Download,
    ],
    choices: Choices {
      search_sort: forum::SEARCH_SORTS,
      search_filter: forum::SEARCH_FILTERS,
      hot_category: &[],
      feed_kind: &[],
      comment_sort: thread::COMMENT_SORTS,
      notification_kind: inbox::KINDS,
    },
    min_interval: Duration::from_secs(1),
    guide: include_str!("../GUIDE.md"),
  };

  type Extra = extra::Command;

  fn new(ctx: Ctx) -> Result<Self> {
    Ok(Self {
      client: Client::new(ctx),
    })
  }

  fn ctx(&self) -> &Ctx {
    &self.client.ctx
  }

  async fn whoami(&self) -> Result<User> {
    people::whoami(&self.client).await
  }

  async fn qr_start(&self) -> Result<QrTicket> {
    login::qr_start(&self.client).await
  }

  async fn qr_poll(&self, ticket: &QrTicket) -> Result<QrStatus> {
    login::qr_poll(&self.client, ticket).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    forum::search(&self.client, query, None, page).await
  }

  async fn search_users(&self, query: &Query, _page: &PageReq) -> Result<Page<User>> {
    forum::search_users(&self.client, query).await
  }

  async fn search_topics(&self, query: &Query, _page: &PageReq) -> Result<Page<Collection>> {
    forum::search_forums(&self.client, query).await
  }

  async fn hot(&self, _category: Option<&str>, _page: &PageReq) -> Result<Page<Post>> {
    forum::hot(&self.client).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    if post.contains("topic_id=") {
      return Err(
        Error::input("that is a hot topic, not a thread")
          .with_hint("list its threads with `social tieba topic TOPIC`"),
      );
    }
    thread::read(&self.client, post).await
  }

  async fn comments(
    &self,
    post: &str,
    sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    thread::comments(&self.client, post, sort, page).await
  }

  async fn replies(&self, post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    thread::replies(&self.client, post, comment, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    people::user(&self.client, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    people::posts(&self.client, user, true, page).await
  }

  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    people::people(&self.client, user, true, page).await
  }

  async fn following(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    people::people(&self.client, user, false, page).await
  }

  async fn collections(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    people::forums(&self.client, user, page).await
  }

  async fn favorites(
    &self,
    user: Option<&str>,
    _folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    if user.is_some() {
      return Err(
        Error::unsupported("favorites USER").with_hint("only your own saved threads are listed"),
      );
    }
    inbox::favorites(&self.client, page).await
  }

  async fn notifications(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    inbox::notifications(&self.client, kind, page).await
  }

  async fn unread(&self) -> Result<BTreeMap<String, u64>> {
    inbox::unread(&self.client).await
  }

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    write::like(&self.client, post, undo).await
  }

  async fn favorite(&self, post: &str, _folder: Option<&str>, undo: bool) -> Result<Action> {
    write::favorite(&self.client, post, undo).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    write::comment(&self.client, post, reply).await
  }

  async fn delete_comment(&self, post: &str, comment: &str) -> Result<Action> {
    write::delete_comment(&self.client, post, comment).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    write::follow(&self.client, user, undo).await
  }

  async fn publish(&self, draft: &Draft) -> Result<Action> {
    write::publish(&self.client, draft).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    write::delete(&self.client, post).await
  }

  async fn run_extra(&self, command: extra::Command) -> Result<Data> {
    extra::run(&self.client, command).await
  }
}
