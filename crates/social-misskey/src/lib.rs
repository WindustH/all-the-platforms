//! Misskey for social-cli (any instance; misskey.io by default).
//!
//! Notes, users, reactions, renotes and hashtags through the HTTP API
//! ([`api`]), chat (direct chats and rooms) through the chat layer
//! ([`chat`]), live events through the streaming socket ([`stream`]), and
//! MiAuth for logging in ([`login`]). Public timelines, notes and users are
//! readable without an account where the instance allows it.

mod api;
mod chat;
mod extra;
mod login;
mod notes;
mod parse;
mod refs;
mod stream;
mod users;

use std::collections::BTreeMap;
use std::time::Duration;

use social_core::output::Stream;
use social_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Draft, Error, Message, Notification, Page,
  PageReq, Platform, PlatformInfo, Post, QrStatus, QrTicket, Query, Reply, Result, User, ValueExt,
  json,
};

use crate::api::Api;
use crate::notes::NoteOpts;

pub struct Misskey {
  api: Api,
}

impl Platform for Misskey {
  const INFO: PlatformInfo = PlatformInfo {
    id: "misskey",
    name: "Misskey",
    aliases: &["mk"],
    about: "Misskey (any instance): notes, timelines, reactions, users, chat and live streams",
    home: "https://misskey.io",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::QrLogin,
      Cap::TokenLogin,
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
      Cap::Chats,
      Cap::Messages,
      Cap::Send,
      Cap::Members,
      Cap::React,
      Cap::Listen,
    ],
    choices: Choices {
      search_sort: &[],
      search_filter: notes::SEARCH_FILTERS,
      hot_category: &[],
      feed_kind: notes::FEED_KINDS,
      comment_sort: &[],
      notification_kind: users::NOTIFICATION_KINDS,
    },
    min_interval: Duration::from_millis(500),
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

  /// `INSTANCE|TOKEN`, or a bare token for the instance in use.
  async fn login_token(&self, token: &str) -> Result<()> {
    let (instance, token) = match token.split_once('|') {
      Some((i, t)) => (api::host(i), t.trim().to_owned()),
      None => (self.api.instance(), token.trim().to_owned()),
    };
    if instance.is_empty() || token.is_empty() {
      return Err(Error::input(
        "expected `INSTANCE|TOKEN`, e.g. `misskey.io|abc123`",
      ));
    }
    let v = self
      .api
      .ctx
      .http
      .post(format!("https://{instance}/api/i"))
      .no_cookies()
      .json(&json!({"i": token}))
      .send()
      .await?;
    if !v.status.is_success() {
      let body: social_core::Value = serde_json::from_slice(&v.body).unwrap_or_default();
      return Err(Error::auth(format!(
        "{instance} did not accept the token ({})",
        body
          .str("error.message")
          .unwrap_or_else(|| format!("HTTP {}", v.status))
      )));
    }
    self.api.save(&instance, &token);
    Ok(())
  }

  async fn qr_start(&self) -> Result<QrTicket> {
    Ok(login::start(&self.api))
  }

  async fn qr_poll(&self, ticket: &QrTicket) -> Result<QrStatus> {
    login::poll(&self.api, ticket).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    notes::search(&self.api, query, page).await
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    users::search(&self.api, query, page).await
  }

  async fn search_topics(&self, query: &Query, page: &PageReq) -> Result<Page<Collection>> {
    notes::search_tags(&self.api, query, page).await
  }

  async fn hot(&self, _category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    notes::featured(&self.api, page).await
  }

  async fn feed(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    notes::timeline(&self.api, kind, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    match refs::chat_message(post) {
      Some(m) => chat::read(&self.api, &m).await,
      None => notes::read(&self.api, post).await,
    }
  }

  async fn comments(
    &self,
    post: &str,
    _sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    notes::replies(&self.api, post, page).await
  }

  async fn replies(&self, _post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    notes::replies(&self.api, comment, page).await
  }

  async fn likers(&self, post: &str, page: &PageReq) -> Result<Page<User>> {
    notes::reactions(&self.api, post, page).await
  }

  async fn reposts(&self, post: &str, page: &PageReq) -> Result<Page<Post>> {
    notes::renotes(&self.api, post, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    users::posts(&self.api, user, page).await
  }

  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::relations(&self.api, user, true, page).await
  }

  async fn following(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::relations(&self.api, user, false, page).await
  }

  async fn favorites(
    &self,
    user: Option<&str>,
    folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    if user.is_some() || folder.is_some() {
      return Err(
        Error::unsupported("favorites USER / --folder")
          .with_hint("Misskey only shows your own favorites; clips are not supported yet"),
      );
    }
    notes::favorites(&self.api, page).await
  }

  async fn notifications(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    users::notifications(&self.api, kind, page).await
  }

  async fn unread(&self) -> Result<BTreeMap<String, u64>> {
    users::unread(&self.api).await
  }

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    self.react(post, notes::LIKE, undo).await
  }

  async fn favorite(&self, post: &str, _folder: Option<&str>, undo: bool) -> Result<Action> {
    notes::favorite(&self.api, post, undo).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    let opts = NoteOpts {
      reply_to: Some(reply.reply_to.clone().unwrap_or_else(|| post.to_owned())),
      files: reply.images.clone(),
      ..NoteOpts::default()
    };
    notes::create(&self.api, reply.text.trim(), &opts).await
  }

  async fn delete_comment(&self, _post: &str, comment: &str) -> Result<Action> {
    notes::delete(&self.api, comment).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    users::follow(&self.api, user, undo).await
  }

  async fn publish(&self, draft: &Draft) -> Result<Action> {
    notes::publish(&self.api, draft).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    match refs::chat_message(post) {
      Some(m) => chat::delete(&self.api, &m).await,
      None => notes::delete(&self.api, post).await,
    }
  }

  async fn chats(&self, parent: Option<&str>, _page: &PageReq) -> Result<Page<Collection>> {
    chat::list(&self.api, parent).await
  }

  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    chat::messages(&self.api, chat, page).await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    chat::send(&self.api, chat, message).await
  }

  async fn members(&self, chat: &str, page: &PageReq) -> Result<Page<User>> {
    chat::members(&self.api, chat, page).await
  }

  async fn react(&self, post: &str, emoji: &str, undo: bool) -> Result<Action> {
    match refs::chat_message(post) {
      Some(m) => chat::react(&self.api, &m, emoji, undo).await,
      None => notes::react(&self.api, post, emoji, undo).await,
    }
  }

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    stream::listen(&self.api, chats, out).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
