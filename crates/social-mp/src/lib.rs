//! WeChat Official Accounts (微信公众号) for social-cli.
//!
//! Three ways in, by what they can reach:
//! - public pages, anonymously: articles ([`page`]) and albums ([`album`]);
//! - the official API ([`api`]) with the account's AppID and AppSecret:
//!   drafts and material for every account type; publishing, comments,
//!   analytics and followers for verified organization accounts;
//! - the mp.weixin.qq.com backend ([`web`]) with a QR-code login: drafts,
//!   publishing, your published list with its counts, other accounts'
//!   articles, comments and analytics, for every account type.
//!
//! Each operation picks a route that can do it ([`client`]); the guide lists
//! which.

mod account;
mod album;
mod api;
mod body;
mod client;
mod comments;
mod drafts;
mod extra;
mod js;
mod login;
mod material;
mod page;
mod publish;
mod published;
mod refs;
mod stats;
mod tools;
mod upload;
mod users;
mod web;

use std::time::Duration;

use social_core::{
  Action, Cap, Choices, Comment, Ctx, Data, Draft, Error, Insights, Page, PageReq, Platform,
  PlatformInfo, Post, QrStatus, QrTicket, Query, Reply, Result, User,
};

use crate::client::Client;
use crate::drafts::Compose;
use crate::published::Account;

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
    required_cookies: &[web::SESSION_COOKIE],
    caps: &[
      Cap::QrLogin,
      Cap::TokenLogin,
      Cap::SearchUsers,
      Cap::Read,
      Cap::Comments,
      Cap::User,
      Cap::UserPosts,
      Cap::Followers,
      Cap::Comment,
      Cap::DeleteComment,
      Cap::Publish,
      Cap::Delete,
      Cap::Download,
      Cap::AccountInsights,
      Cap::PostInsights,
    ],
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
    account::whoami(&self.client).await
  }

  fn logged_in(&self) -> bool {
    self.client.logged_in()
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    self.client.set_credentials(token)
  }

  async fn qr_start(&self) -> Result<QrTicket> {
    login::qr_start(&self.client).await
  }

  async fn qr_poll(&self, ticket: &QrTicket) -> Result<QrStatus> {
    login::qr_poll(&self.client, ticket).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    let mut post = page::read(&self.client.ctx, post).await?;
    if self.client.has_web() {
      published::add_own_counts(&self.client, &mut post).await;
    }
    Ok(post)
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    published::search_page(&self.client, &query.keyword, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    published::user(&self.client, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    if !published::is_own(&self.client, user) {
      self.client.require_web()?;
    }
    let account = published::account(&self.client, user).await?;
    published::page(&self.client, &account, page).await
  }

  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    match published::account(&self.client, user).await? {
      Account::Own => users::followers(&self.client, page).await,
      Account::Other { .. } => Err(Error::unsupported("followers of other accounts")),
    }
  }

  async fn comments(
    &self,
    post: &str,
    _sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    comments::list(&self.client, post, page).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    comments::reply(&self.client, post, &reply.text, reply.reply_to.as_deref()).await
  }

  async fn delete_comment(&self, post: &str, comment: &str) -> Result<Action> {
    comments::delete(&self.client, post, comment).await
  }

  /// `post` saves a draft; `social mp publish` publishes it.
  async fn publish(&self, draft: &Draft) -> Result<Action> {
    if draft.reply_to.is_some() || draft.quote.is_some() {
      return Err(Error::unsupported("post --reply-to / --quote"));
    }
    let compose = Compose {
      title: draft.title.clone().unwrap_or_default(),
      text: draft.text.clone(),
      images: draft.images.clone(),
      ..Compose::default()
    };
    drafts::create(&self.client, &compose).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    publish::delete(&self.client, post).await
  }

  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    match post {
      Some(p) => stats::post(&self.client, p).await,
      None => stats::account(&self.client, days).await,
    }
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.client, command).await
  }
}
