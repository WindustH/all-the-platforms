//! VoceChat for social-cli: a self-hosted chat server, as a user (password
//! login, token renewed as it expires) or as a bot (API key: sending and
//! lookups only). Channels and direct chats are collections, messages are
//! posts with `<chat>/<mid>` ids, `listen` reads the user event stream.

mod api;
mod chat;
mod events;
mod parse;
mod refs;

use std::time::Duration;

use social_core::output::Stream;
use social_core::{
  Action, Cap, Choices, Collection, Ctx, Data, Error, Media, Message, Page, PageReq, Platform,
  PlatformInfo, Post, Result, User, Value, ValueExt, json,
};

use crate::api::Api;
use crate::chat::Directory;

pub struct VoceChat {
  api: Api,
  dir: Directory,
}

/// VoceChat-only commands.
#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Send a message written in Markdown
  Markdown {
    chat: String,
    /// Text; `-` reads it from stdin
    text: String,
    #[arg(long, value_name = "MESSAGE")]
    reply_to: Option<String>,
  },
}

impl Platform for VoceChat {
  const INFO: PlatformInfo = PlatformInfo {
    id: "vocechat",
    name: "VoceChat",
    aliases: &["voce"],
    about: "VoceChat (self-hosted): channels, direct chats, files and live events",
    home: "https://voce.chat",
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
    min_interval: Duration::from_millis(200),
    guide: include_str!("../GUIDE.md"),
  };

  type Extra = Command;

  fn new(ctx: Ctx) -> Result<Self> {
    Ok(Self {
      api: Api::new(ctx),
      dir: Directory::default(),
    })
  }

  fn ctx(&self) -> &Ctx {
    &self.api.ctx
  }

  async fn whoami(&self) -> Result<User> {
    chat::whoami(&self.api).await
  }

  fn logged_in(&self) -> bool {
    self.api.logged_in()
  }

  /// `SERVER|BOT_API_KEY` or `SERVER|EMAIL|PASSWORD`.
  async fn login_token(&self, token: &str) -> Result<()> {
    let parts: Vec<&str> = token.splitn(3, '|').map(str::trim).collect();
    let api = &self.api;
    match parts.as_slice() {
      [server, email, password] => {
        let server = api::normalize(server);
        let resp = api
          .ctx
          .http
          .post(format!("{server}/api/token/login"))
          .no_cookies()
          .json(&json!({
            "credential": {"type": "password", "email": email, "password": password},
            "device": "social-cli",
          }))
          .send()
          .await?;
        let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
        match (v.str("token"), v.str("refresh_token")) {
          (Some(t), Some(r)) if resp.status.is_success() => api.save_user(&server, &t, &r),
          _ => {
            return Err(Error::auth(format!(
              "{server} refused the login (HTTP {})",
              resp.status
            )));
          }
        }
      }
      [server, key] => {
        let server = api::normalize(server);
        let resp = api
          .ctx
          .http
          .get(format!("{server}/api/bot"))
          .no_cookies()
          .header("x-api-key", *key)
          .send()
          .await?;
        if !resp.status.is_success() {
          return Err(Error::auth(format!(
            "{server} did not accept the bot API key (HTTP {})",
            resp.status
          )));
        }
        api.save_bot(&server, key);
      }
      _ => {
        return Err(Error::input(
          "expected `SERVER|BOT_API_KEY` or `SERVER|EMAIL|PASSWORD`",
        ));
      }
    }
    Ok(())
  }

  async fn read(&self, post: &str) -> Result<Post> {
    chat::read(&self.api, &self.dir, post).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    chat::user(&self.api, user).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    chat::delete(&self.api, post).await
  }

  async fn media(&self, post: &str, _audio_only: bool) -> Result<(Post, Vec<Media>)> {
    chat::media(&self.api, &self.dir, post).await
  }

  async fn chats(&self, parent: Option<&str>, _page: &PageReq) -> Result<Page<Collection>> {
    if parent.is_some() {
      return Err(Error::input("VoceChat channels have nothing inside them"));
    }
    chat::list(&self.api, &self.dir).await
  }

  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    chat::history(&self.api, &self.dir, chat, page).await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    chat::send(&self.api, chat, message, false).await
  }

  async fn members(&self, chat: &str, _page: &PageReq) -> Result<Page<User>> {
    chat::members(&self.api, &self.dir, chat).await
  }

  async fn react(&self, post: &str, emoji: &str, undo: bool) -> Result<Action> {
    chat::react(&self.api, post, emoji, undo).await
  }

  async fn edit(&self, post: &str, text: &str) -> Result<Action> {
    chat::edit(&self.api, post, text).await
  }

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    events::listen(&self.api, &self.dir, chats, out).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    let ctx = &self.api.ctx;
    Ok(match command {
      Command::Markdown {
        chat,
        text,
        reply_to,
      } => {
        let message = Message {
          text: social_core::cli::read_text(Some(text))?,
          reply_to: reply_to.map(|r| ctx.post_ref(&r)).transpose()?,
          files: Vec::new(),
        };
        Data::Action(chat::send(&self.api, &ctx.collection_ref(&chat)?, &message, true).await?)
      }
    })
  }
}
