//! Feishu / Lark for all-the-platforms.
//!
//! An app's bot through the Open Platform with a tenant token ([`api`]):
//! the chats it is in, their history (with the 获取群组中所有消息 scope),
//! sending text and files, replies, edits, recalls, reactions, pins, and
//! live events over the long connection ([`live`]). Chats are collections
//! (`oc_…`), messages are posts (`om_…`, globally unique ids).

mod api;
mod chats;
mod extra;
mod frame;
mod live;
mod login;
mod messages;
mod parse;
mod pins;
mod reactions;
mod refs;
mod uploads;
mod users;

use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Collection, Ctx, Data, Media, Message, Page, PageReq, Platform,
  PlatformInfo, PlatformKind, Post, Result, User,
};

use crate::api::Api;

pub struct Feishu {
  api: Api,
}

impl Platform for Feishu {
  const INFO: PlatformInfo = PlatformInfo {
    id: "feishu",
    name: "Feishu / Lark",
    aliases: &["lark", "fs"],
    about: "Feishu / Lark (飞书): an app bot's chats, messages, files, reactions and live events",
    home: "https://www.feishu.cn",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::TokenLogin,
      Cap::Read,
      Cap::User,
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
    choices: Choices::NONE,
    min_interval: Duration::from_millis(250),
    kind: PlatformKind::Chat,
    token: "APP_ID:APP_SECRET",
    env: &[
      "FEISHU_APP_ID",
      "FEISHU_APP_SECRET",
      "FEISHU_BASE_URL",
      "LARK_APP_ID",
      "LARK_APP_SECRET",
    ],
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
    login::login_token(&self.api, token)
  }

  async fn read(&self, post: &str) -> Result<Post> {
    messages::read(&self.api, post).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, user).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    messages::delete(&self.api, post).await
  }

  async fn media(&self, post: &str, _audio_only: bool) -> Result<(Post, Vec<Media>)> {
    messages::media(&self.api, post).await
  }

  async fn chats(&self, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    chats::list(&self.api, parent, page).await
  }

  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    messages::list(&self.api, chat, page).await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    messages::send(&self.api, chat, message).await
  }

  async fn members(&self, chat: &str, page: &PageReq) -> Result<Page<User>> {
    chats::members(&self.api, chat, page).await
  }

  async fn react(&self, post: &str, emoji: &str, undo: bool) -> Result<Action> {
    reactions::react(&self.api, post, emoji, undo).await
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
