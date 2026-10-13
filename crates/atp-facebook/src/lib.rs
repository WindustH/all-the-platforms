//! Facebook for all-the-platforms.
//!
//! Two ways in, by what they reach:
//! - **facebook.com** ([`web`], [`read`]): what the site ships to a browser —
//!   profiles and Pages, posts with reactions and comments, groups — read
//!   anonymously where Facebook allows it; with a browser session
//!   (`c_user` and `xs`) also timelines past the newest post, the home
//!   feed, search, notifications and friends. Paging replays the GraphQL
//!   operation the page itself ran, changing only the cursor.
//! - **the Graph API** ([`pages`], `ads`): with an access token, the Pages it
//!   manages — posts, publishing, comments, insights, Messenger — and the
//!   Marketing API.

mod client;
mod extra;
mod pages;
mod parse;
mod read;
mod refs;
mod relay;
mod web;

use std::time::Duration;

use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Draft, Error, Insights, Message,
  Notification, Page, PageReq, Platform, PlatformInfo, Post, Query, Reply, Result, User,
};

use crate::client::Client;

pub struct Facebook {
  client: Client,
}

/// A Graph API id (`123`, `123_456`) rather than a link or handle.
fn is_graph_id(arg: &str) -> bool {
  arg.contains('_') && arg.bytes().all(|b| b.is_ascii_digit() || b == b'_')
}

impl Platform for Facebook {
  const INFO: PlatformInfo = PlatformInfo {
    id: "facebook",
    name: "Facebook",
    aliases: &["fb"],
    about: "Facebook: profiles, Pages, posts, groups, Page publishing and insights, Messenger, ads",
    home: "https://www.facebook.com/",
    cookie_domains: &["facebook.com"],
    required_cookies: &client::SESSION,
    caps: &[
      Cap::TokenLogin,
      Cap::Search,
      Cap::SearchUsers,
      Cap::Feed,
      Cap::Read,
      Cap::Comments,
      Cap::Replies,
      Cap::User,
      Cap::UserPosts,
      Cap::Notifications,
      Cap::Comment,
      Cap::CommentImages,
      Cap::DeleteComment,
      Cap::Publish,
      Cap::Delete,
      Cap::Download,
      Cap::AccountInsights,
      Cap::PostInsights,
      Cap::Chats,
      Cap::Messages,
      Cap::Send,
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

  fn logged_in(&self) -> bool {
    self.client.has_session() || self.client.token().is_some()
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    let g = atp_meta::Graph::new(&self.client.ctx, atp_meta::graph::FACEBOOK, token);
    g.get("me", &[("fields".into(), "id".into())]).await?;
    self.client.ctx.set_extra(atp_meta::graph::TOKEN_KEY, token);
    Ok(())
  }

  async fn whoami(&self) -> Result<User> {
    if self.client.has_session() {
      let me = self.client.ctx.http.cookie("c_user").unwrap_or_default();
      return read::user(&self.client, &me).await;
    }
    if self.client.token().is_some() {
      return pages::me(&self.client).await;
    }
    Err(Error::auth("not logged in").with_hint(self.client.ctx.login_hint()))
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    read::search(&self.client, &query.keyword, page).await
  }

  async fn search_users(&self, query: &Query, _page: &PageReq) -> Result<Page<User>> {
    read::search_users(&self.client, &query.keyword).await
  }

  async fn feed(&self, _kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    read::feed(&self.client, page).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    if is_graph_id(post) {
      return pages::read(&self.client, post).await;
    }
    read::read(&self.client, post).await
  }

  async fn comments(
    &self,
    post: &str,
    _sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    if is_graph_id(post) {
      return pages::comments(&self.client, post, page).await;
    }
    read::comments(&self.client, post, page).await
  }

  async fn replies(&self, _post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    if is_graph_id(comment) {
      return pages::replies(&self.client, comment, page).await;
    }
    Err(
      Error::unsupported("replies of a facebook.com comment").with_hint(
        "replies are listed for Page posts read through the Graph API (comment ids like 123_456)",
      ),
    )
  }

  async fn user(&self, user: &str) -> Result<User> {
    read::user(&self.client, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    read::user_posts(&self.client, user, page).await
  }

  async fn notifications(
    &self,
    _kind: Option<&str>,
    _page: &PageReq,
  ) -> Result<Page<Notification>> {
    read::notifications(&self.client).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    pages::comment_on(&self.client, post, reply).await
  }

  async fn delete_comment(&self, post: &str, comment: &str) -> Result<Action> {
    pages::delete_comment(&self.client, post, comment).await
  }

  async fn publish(&self, draft: &Draft) -> Result<Action> {
    pages::publish(&self.client, draft).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    pages::delete(&self.client, post).await
  }

  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    pages::insights(&self.client, post, days).await
  }

  async fn chats(&self, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    pages::conversations(&self.client, parent, page).await
  }

  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    pages::messages(&self.client, chat, page).await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    pages::send(&self.client, chat, message).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.client, command).await
  }
}
