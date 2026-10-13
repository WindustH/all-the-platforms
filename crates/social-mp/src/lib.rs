//! WeChat Official Accounts (微信公众号) for social-cli.
//!
//! Three ways in, by what they can reach:
//! - public pages, anonymously: articles ([`articles`]) and albums
//!   ([`albums`]);
//! - the official API ([`official`]) with the account's AppID and AppSecret:
//!   drafts and material for every account type; publishing, comments,
//!   analytics and followers for verified organization accounts;
//! - the mp.weixin.qq.com backend ([`backend`]) with a QR-code login: drafts,
//!   publishing, your published list with its counts, other accounts'
//!   articles, comments and analytics, for every account type.
//!
//! Each operation picks a route that can do it ([`api`]); the guide lists
//! which.

mod accounts;
mod albums;
mod api;
mod articles;
mod backend;
mod body;
mod comments;
mod drafts;
mod extra;
mod insights;
mod js;
mod login;
mod material;
mod official;
mod publish;
mod published;
mod refs;
mod safety;
mod tools;
mod uploads;
mod users;

use std::time::Duration;

use social_core::{
  Action, Cap, Choices, Comment, Ctx, Data, Draft, Error, Insights, Page, PageReq, Platform,
  PlatformInfo, Post, QrStatus, QrTicket, Query, Reply, Result, User,
};

use crate::accounts::Account;
use crate::api::Api;
use crate::drafts::Compose;

pub struct Mp {
  api: Api,
}

impl Platform for Mp {
  const INFO: PlatformInfo = PlatformInfo {
    id: "mp",
    name: "WeChat Official Accounts",
    aliases: &["gzh", "wechat-mp", "weixin-mp"],
    about: "WeChat Official Accounts (微信公众号): articles, albums, drafts, publishing, comments and analytics",
    home: "https://mp.weixin.qq.com/",
    cookie_domains: &["mp.weixin.qq.com"],
    required_cookies: &[backend::SESSION_COOKIE],
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
    self.api.official().set_credentials(token)
  }

  async fn qr_start(&self) -> Result<QrTicket> {
    login::qr_start(&self.api).await
  }

  async fn qr_poll(&self, ticket: &QrTicket) -> Result<QrStatus> {
    login::qr_poll(&self.api, ticket).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    let mut post = articles::read(&self.api.ctx, post).await?;
    if self.api.backend().has() {
      published::add_own_counts(&self.api, &mut post).await;
    }
    Ok(post)
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    accounts::search_page(&self.api, &query.keyword, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    accounts::user(&self.api, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    if !accounts::is_own(&self.api, user) {
      self.api.backend().require()?;
    }
    let account = accounts::account(&self.api, user).await?;
    published::page(&self.api, &account, page).await
  }

  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    match accounts::account(&self.api, user).await? {
      Account::Own => users::followers(&self.api, page).await,
      Account::Other { .. } => Err(Error::unsupported("followers of other accounts")),
    }
  }

  async fn comments(
    &self,
    post: &str,
    _sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    comments::list(&self.api, post, page).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    comments::reply(&self.api, post, &reply.text, reply.reply_to.as_deref()).await
  }

  async fn delete_comment(&self, post: &str, comment: &str) -> Result<Action> {
    comments::delete(&self.api, post, comment).await
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
    drafts::create(&self.api, &compose).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    publish::delete(&self.api, post).await
  }

  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    match post {
      Some(p) => insights::post(&self.api, p).await,
      None => insights::account(&self.api, days).await,
    }
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
