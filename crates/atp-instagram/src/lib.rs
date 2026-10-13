//! Instagram for all-the-platforms.
//!
//! The web API ([`client`]) with a browser session does nearly everything
//! the site does: profiles, posts, comments, likers, followers, hashtags,
//! places, saved posts, the activity inbox, stories and highlights, likes,
//! comments, follows and direct messages. Profiles and single posts are
//! also read anonymously. Insights of business and creator accounts come
//! from the Graph API ([`insights`]), ads from the Marketing API.

mod client;
mod direct;
mod extra;
mod insights;
mod parse;
mod read;
mod refs;
mod write;

use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Error, Insights, Message, Notification,
  Page, PageReq, Platform, PlatformInfo, Post, Query, Reply, Result, User,
};

use crate::client::Client;

pub struct Instagram {
  client: Client,
}

impl Platform for Instagram {
  const INFO: PlatformInfo = PlatformInfo {
    id: "instagram",
    name: "Instagram",
    aliases: &["ig", "insta"],
    about: "Instagram: posts, reels, stories, comments, users, hashtags, direct messages and insights",
    home: "https://www.instagram.com/",
    cookie_domains: &["instagram.com"],
    required_cookies: &[client::SESSION],
    caps: &[
      Cap::TokenLogin,
      Cap::SearchUsers,
      Cap::SearchTopics,
      Cap::Read,
      Cap::Comments,
      Cap::Replies,
      Cap::Likers,
      Cap::User,
      Cap::UserPosts,
      Cap::Followers,
      Cap::Following,
      Cap::Collections,
      Cap::Favorites,
      Cap::Likes,
      Cap::Notifications,
      Cap::Like,
      Cap::Favorite,
      Cap::Comment,
      Cap::DeleteComment,
      Cap::Follow,
      Cap::Delete,
      Cap::Download,
      Cap::AccountInsights,
      Cap::PostInsights,
      Cap::Chats,
      Cap::Messages,
      Cap::Send,
      Cap::React,
      Cap::Listen,
    ],
    choices: Choices::NONE,
    min_interval: Duration::from_millis(2500),
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
    let host = if token.starts_with("IG") {
      atp_meta::graph::INSTAGRAM
    } else {
      atp_meta::graph::FACEBOOK
    };
    let g = atp_meta::Graph::new(&self.client.ctx, host, token);
    g.get("me", &[("fields".into(), "id".into())]).await?;
    self.client.ctx.set_extra(atp_meta::graph::TOKEN_KEY, token);
    Ok(())
  }

  async fn whoami(&self) -> Result<User> {
    if self.client.has_session() {
      return read::whoami(&self.client).await;
    }
    if let Some(token) = self.client.token() {
      let host = if token.starts_with("IG") {
        atp_meta::graph::INSTAGRAM
      } else {
        atp_meta::graph::FACEBOOK
      };
      let g = atp_meta::Graph::new(&self.client.ctx, host, token);
      let v = g.get("me", &[("fields".into(), "id,name".into())]).await?;
      use atp_core::ValueExt;
      return Ok(User {
        id: v.str("id").unwrap_or_default(),
        name: v.str("name").unwrap_or_default(),
        extra: [("source".to_owned(), "graph-api".into())].into(),
        ..User::default()
      });
    }
    Err(Error::auth("not logged in").with_hint(self.client.ctx.login_hint()))
  }

  async fn search_users(&self, query: &Query, _page: &PageReq) -> Result<Page<User>> {
    read::search_users(&self.client, &query.keyword).await
  }

  async fn search_topics(&self, query: &Query, _page: &PageReq) -> Result<Page<Collection>> {
    read::search_topics(&self.client, &query.keyword).await
  }

  async fn read(&self, post: &str) -> Result<Post> {
    read::read(&self.client, post).await
  }

  async fn comments(
    &self,
    post: &str,
    _sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    read::comments(&self.client, post, page).await
  }

  async fn replies(&self, post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    read::replies(&self.client, post, comment, page).await
  }

  async fn likers(&self, post: &str, _page: &PageReq) -> Result<Page<User>> {
    read::likers(&self.client, post).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    read::user(&self.client, user).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    read::user_posts(&self.client, user, page).await
  }

  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    read::follows(&self.client, user, "followers", page).await
  }

  async fn following(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    read::follows(&self.client, user, "following", page).await
  }

  async fn collections(&self, user: Option<&str>, _page: &PageReq) -> Result<Page<Collection>> {
    if user.is_some_and(|u| Some(u.to_owned()) != self.client.my_id()) {
      return Err(Error::unsupported("collections of another account"));
    }
    read::collections(&self.client).await
  }

  async fn favorites(
    &self,
    user: Option<&str>,
    folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    if user.is_some_and(|u| Some(u.to_owned()) != self.client.my_id()) {
      return Err(Error::unsupported("saved posts of another account"));
    }
    read::saved(&self.client, folder, page).await
  }

  async fn likes(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    if user.is_some_and(|u| Some(u.to_owned()) != self.client.my_id()) {
      return Err(Error::unsupported("liked posts of another account"));
    }
    read::liked(&self.client, page).await
  }

  async fn notifications(
    &self,
    _kind: Option<&str>,
    _page: &PageReq,
  ) -> Result<Page<Notification>> {
    read::notifications(&self.client).await
  }

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    write::like(&self.client, post, undo).await
  }

  async fn favorite(&self, post: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
    write::save(&self.client, post, folder, undo).await
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

  async fn delete(&self, post: &str) -> Result<Action> {
    write::delete(&self.client, post).await
  }

  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    insights::insights(&self.client, post, days).await
  }

  async fn chats(&self, _parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    direct::chats(&self.client, page).await
  }

  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    direct::messages(&self.client, chat, page).await
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    direct::send(&self.client, chat, message).await
  }

  async fn react(&self, post: &str, emoji: &str, undo: bool) -> Result<Action> {
    direct::react(&self.client, post, emoji, undo).await
  }

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    direct::listen(&self.client, chats, out).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.client, command).await
  }
}
