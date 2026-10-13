//! LINE for social-cli: the Messaging API of a LINE Official Account, with
//! its channel access token. A bot can push, multicast and broadcast, look
//! up profiles, groups and members, download what users sent, and read its
//! statistics. Incoming messages reach bots only by webhook, so there is no
//! history and no `listen`.

mod api;
mod bot;
mod stats;

use std::time::Duration;

use social_core::{
  Action, Cap, Choices, Ctx, Data, Error, Insights, Media, Message, Page, PageReq, Platform,
  PlatformInfo, Post, Result, User,
};

use crate::api::Api;

pub struct Line {
  api: Api,
}

/// LINE-only commands.
#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Send an image by its public HTTPS URL
  Image {
    /// User (U…), group (C…), room (R…), `U…,U…` or `all`
    chat: String,
    url: String,
    /// Smaller preview image URL (default: the image itself)
    #[arg(long)]
    preview: Option<String>,
  },
  /// Send a sticker (package and sticker ids from LINE's sticker list)
  Sticker {
    chat: String,
    package: String,
    sticker: String,
  },
  /// A group's name, picture and member count
  Group { group: String },
  /// Leave a group or room
  Leave { chat: String },
  /// Monthly message quota, usage and the webhook URL
  Quota,
  /// Rich menus and the default one
  Richmenus,
}

impl Platform for Line {
  const INFO: PlatformInfo = PlatformInfo {
    id: "line",
    name: "LINE",
    aliases: &[],
    about: "LINE Official Account bots (Messaging API): sending, profiles, members and statistics",
    home: "https://line.me",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::TokenLogin,
      Cap::User,
      Cap::Followers,
      Cap::Send,
      Cap::Members,
      Cap::Download,
      Cap::AccountInsights,
      Cap::PostInsights,
    ],
    choices: Choices::NONE,
    min_interval: Duration::from_millis(100),
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
    bot::whoami(&self.api).await
  }

  fn logged_in(&self) -> bool {
    self.api.logged_in()
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    self.api.save(token.trim());
    Ok(())
  }

  async fn user(&self, user: &str) -> Result<User> {
    bot::user(&self.api, user).await
  }

  /// Friends of the bot (the logged-in account only).
  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    if !matches!(user, "" | "me") {
      let me = bot::whoami(&self.api).await?;
      if user != me.id {
        return Err(
          Error::unsupported("followers of another account")
            .with_hint("LINE lists the friends of your own bot only"),
        );
      }
    }
    bot::followers(&self.api, page).await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    bot::send(&self.api, chat, message).await
  }

  async fn members(&self, chat: &str, page: &PageReq) -> Result<Page<User>> {
    bot::members(&self.api, chat, page).await
  }

  async fn media(&self, post: &str, _audio_only: bool) -> Result<(Post, Vec<Media>)> {
    bot::content(&self.api, post)
  }

  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    match post {
      Some(request_id) => stats::message(&self.api, request_id).await,
      None => stats::account(&self.api, days).await,
    }
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    let api = &self.api;
    Ok(match command {
      Command::Image { chat, url, preview } => {
        Data::Action(bot::image(api, &chat, &url, preview.as_deref()).await?)
      }
      Command::Sticker {
        chat,
        package,
        sticker,
      } => Data::Action(bot::sticker(api, &chat, &package, &sticker).await?),
      Command::Group { group } => Data::User(Box::new(bot::user(api, &group).await?)),
      Command::Leave { chat } => Data::Action(bot::leave(api, &chat).await?),
      Command::Quota => Data::Value(bot::quota(api).await?),
      Command::Richmenus => Data::Value(bot::rich_menus(api).await?),
    })
  }
}
