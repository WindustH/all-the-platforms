//! WeChat Official Accounts (微信公众号) for social-cli.
//!
//! Three ways in, by what they can reach:
//! - public pages, anonymously: articles ([`page`]) and albums ([`album`]);
//! - the official API with the account's AppID and AppSecret;
//! - the mp.weixin.qq.com backend with a QR-code login.

mod album;
mod client;
mod extra;
mod js;
mod page;
mod refs;

use std::time::Duration;

use social_core::{Cap, Choices, Ctx, Data, Error, Platform, PlatformInfo, Post, Result, User};

use crate::client::Client;

pub struct Mp {
  client: Client,
}

impl Platform for Mp {
  const INFO: PlatformInfo = PlatformInfo {
    id: "mp",
    name: "WeChat Official Accounts",
    aliases: &["gzh", "wechat-mp", "weixin-mp"],
    about: "WeChat Official Accounts (微信公众号): articles, albums, drafts, publishing, comments and analytics",
    home: "https://mp.weixin.qq.com/",
    cookie_domains: &["mp.weixin.qq.com"],
    required_cookies: &[],
    caps: &[Cap::Read, Cap::Download],
    choices: Choices::NONE,
    min_interval: Duration::from_millis(1500),
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
    Err(Error::auth("not logged in"))
  }

  async fn read(&self, post: &str) -> Result<Post> {
    page::read(&self.client.ctx, post).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.client, command).await
  }
}
