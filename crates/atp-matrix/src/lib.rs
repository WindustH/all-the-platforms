//! Matrix for all-the-platforms: any homeserver, through the client-server API
//! ([`api`]). Rooms are collections ([`chats`]), timeline events are posts
//! with `ROOM/$event` ids ([`messages`]), `listen` long-polls `/sync`
//! ([`live`]). End-to-end encrypted rooms are not decrypted: their messages
//! show as `encrypted` events.

mod api;
mod chats;
mod extra;
mod live;
mod messages;
mod parse;
mod refs;
mod users;

/// The commands only this platform has, for [`Platform::run_extra`](atp_core::Platform::run_extra).
pub use extra::Command;

use std::collections::BTreeMap;
use std::time::Duration;

use atp_core::http::Method;
use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Collection, Ctx, Data, Error, Media, Message, Notification, Page, PageReq,
  Platform, PlatformInfo, PlatformKind, Post, Query, Reply, Result, User, ValueExt, json,
};

use crate::api::Api;

pub struct Matrix {
  api: Api,
}

impl Platform for Matrix {
  const INFO: PlatformInfo = PlatformInfo {
    id: "matrix",
    name: "Matrix",
    aliases: &["mx"],
    about: "Matrix (any homeserver): rooms, spaces, messages, reactions and live sync",
    home: "https://matrix.org",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::TokenLogin,
      Cap::Search,
      Cap::SearchTopics,
      Cap::Read,
      Cap::User,
      Cap::Notifications,
      Cap::Unread,
      Cap::Comment,
      Cap::CommentImages,
      Cap::Follow,
      Cap::Delete,
      Cap::Download,
      Cap::Chats,
      Cap::Messages,
      Cap::Send,
      Cap::Members,
      Cap::React,
      Cap::Edit,
      Cap::Listen,
    ],
    choices: Choices {
      search_sort: &["recent", "rank"],
      search_filter: &[],
      hot_category: &[],
      feed_kind: &[],
      comment_sort: &[],
      notification_kind: &["all", "highlight"],
    },
    min_interval: Duration::from_millis(300),
    kind: PlatformKind::Chat,
    token: "HOMESERVER|ACCESS_TOKEN",
    env: &["MATRIX_HOMESERVER", "MATRIX_ACCESS_TOKEN"],
    guest: "public room directory: rooms, search -t topic; user",
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
    users::whoami(&self.api).await
  }

  fn logged_in(&self) -> bool {
    self.api.logged_in()
  }

  /// `HOMESERVER|ACCESS_TOKEN`, `HOMESERVER|@user:server|PASSWORD`, or a
  /// bare access token for the homeserver in use.
  async fn login_token(&self, token: &str) -> Result<()> {
    let parts: Vec<&str> = token.splitn(3, '|').map(str::trim).collect();
    let api = &self.api;
    match parts.as_slice() {
      [server, user, password] => {
        let base = api::discover(api, server).await;
        let mut body = json!({
          "type": "m.login.password",
          "identifier": {"type": "m.id.user", "user": user},
          "password": password,
          "initial_device_display_name": "all-the-platforms",
        });
        if let Some(device) = api.ctx.extra("device_id") {
          body["device_id"] = json!(device);
        }
        let v = api
          .call_with(
            None,
            Method::POST,
            &format!("{base}{}/login", api::CLIENT),
            &[],
            Some(&body),
            None,
          )
          .await?;
        let token = v
          .str("access_token")
          .ok_or_else(|| Error::upstream("the login answered without an access token"))?;
        api.save(
          &base,
          &token,
          &v.str("user_id").unwrap_or_default(),
          v.str("device_id").as_deref(),
        );
      }
      [server, token] => {
        let base = api::discover(api, server).await;
        api.save(&base, token, "", None);
      }
      [token] => {
        let base = api.base().await;
        api.save(&base, token, "", None);
      }
      _ => return Err(Error::input("expected `HOMESERVER|TOKEN`")),
    }
    api.forget_base();
    // Fills in the user id and checks the token.
    users::whoami(api).await?;
    Ok(())
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    messages::search(&self.api, query, page).await
  }

  async fn search_topics(&self, query: &Query, page: &PageReq) -> Result<Page<Collection>> {
    chats::directory(&self.api, Some(query), None, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    messages::read(&self.api, post).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn notifications(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    users::notifications(&self.api, kind, page).await
  }

  async fn unread(&self) -> Result<BTreeMap<String, u64>> {
    chats::unread(&self.api).await
  }

  /// Answer a message in its room.
  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    let target = reply.reply_to.as_deref().unwrap_or(post);
    let r = refs::event(target)?;
    let room = match &r.room {
      refs::Room::Id(i) | refs::Room::Alias(i) => i.clone(),
    };
    let message = Message {
      text: reply.text.clone(),
      reply_to: Some(target.to_owned()),
      files: reply.images.clone(),
    };
    messages::send(&self.api, &room, &message).await
  }

  /// Join a room (`--undo` leaves it).
  async fn follow(&self, room: &str, undo: bool) -> Result<Action> {
    chats::join(&self.api, room, undo).await
  }

  /// `#N` of `follow` comes from the last room list.
  fn follow_ref(&self, arg: &str) -> Result<String> {
    self.ctx().collection_ref(arg)
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    messages::delete(&self.api, post).await
  }

  async fn media(&self, post: &str, _audio_only: bool) -> Result<(Post, Vec<Media>)> {
    messages::media(&self.api, post).await
  }

  async fn chats(&self, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    match parent {
      Some(space) => chats::space(&self.api, space, page).await,
      None => chats::list(&self.api).await,
    }
  }

  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    messages::history(&self.api, chat, page).await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    messages::send(&self.api, chat, message).await
  }

  async fn members(&self, chat: &str, _page: &PageReq) -> Result<Page<User>> {
    chats::members(&self.api, chat).await
  }

  async fn react(&self, post: &str, emoji: &str, undo: bool) -> Result<Action> {
    messages::react(&self.api, post, emoji, undo).await
  }

  async fn edit(&self, post: &str, text: &str) -> Result<Action> {
    messages::edit(&self.api, post, text).await
  }

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    live::listen(&self.api, chats, out).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
