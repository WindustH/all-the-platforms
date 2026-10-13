//! Zhihu (知乎) for all-the-platforms.
//!
//! Questions, answers, column articles and pins (想法) over the web API
//! (`www.zhihu.com/api/v4`), authenticated by the `z_c0` cookie. Posts are
//! referred to by URL or typed id (`q:`, `a:`, `p:` for pins, `article:`);
//! a bare number is an answer. Users are referred to by `url_token`.

mod api;
mod comments;
mod discover;
mod extra;
mod feeds;
mod insights;
mod library;
mod lists;
mod login;
mod markup;
mod notifications;
mod parse;
mod posts;
mod publish;
mod refs;
mod search;
mod sign;
mod topics;
mod uploads;
mod users;

use std::collections::BTreeMap;
use std::time::Duration;

use api::Api;
use atp_core::{
  Action, Cap, Choices, Collection, Comment, Ctx, Data, Draft, Error, Insights, Notification, Page,
  PageReq, Platform, PlatformInfo, Post, QrStatus, QrTicket, Query, Reply, Result, User,
};
use refs::Target;

pub struct Zhihu {
  api: Api,
}

impl Platform for Zhihu {
  const INFO: PlatformInfo = PlatformInfo {
    id: "zhihu",
    name: "Zhihu",
    aliases: &["zh"],
    about: "Zhihu (知乎)",
    home: "https://www.zhihu.com",
    cookie_domains: &["zhihu.com"],
    required_cookies: &["z_c0"],
    caps: &[
      Cap::QrLogin,
      Cap::Search,
      Cap::SearchUsers,
      Cap::SearchTopics,
      Cap::Hot,
      Cap::Feed,
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
      Cap::History,
      Cap::Notifications,
      Cap::Unread,
      Cap::Like,
      Cap::Favorite,
      Cap::Comment,
      Cap::DeleteComment,
      Cap::Follow,
      Cap::Publish,
      Cap::Delete,
      Cap::Download,
      Cap::AccountInsights,
      Cap::PostInsights,
    ],
    choices: Choices {
      search_sort: &["default", "upvoted", "newest"],
      search_filter: &["answer", "article"],
      feed_kind: &["recommend", "follow"],
      comment_sort: &["score", "ts"],
      notification_kind: notifications::KINDS,
      ..Choices::NONE
    },
    min_interval: Duration::from_millis(600),
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

  async fn prepare_login(&self) -> Result<()> {
    self.api.helper_cookies().await
  }

  async fn qr_start(&self) -> Result<QrTicket> {
    login::qr_start(&self.api).await
  }

  async fn qr_poll(&self, ticket: &QrTicket) -> Result<QrStatus> {
    login::qr_poll(&self.api, ticket).await
  }

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    search::search(&self.api, query, page).await
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    search::search_users(&self.api, query, page).await
  }

  async fn search_topics(&self, query: &Query, page: &PageReq) -> Result<Page<Collection>> {
    search::search_topics(&self.api, query, page).await
  }

  async fn hot(&self, _category: Option<&str>, _page: &PageReq) -> Result<Page<Post>> {
    feeds::hot(&self.api).await
  }

  async fn feed(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    match kind {
      Some("follow") => feeds::follow(&self.api, page).await,
      _ => feeds::feed(&self.api, page).await,
    }
  }

  async fn read(&self, post: &str) -> Result<Post> {
    posts::read(&self.api, &Target::parse(post)?).await
  }

  async fn comments(
    &self,
    post: &str,
    sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    comments::roots(&self.api, &Target::parse(post)?, sort, page).await
  }

  async fn replies(&self, post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    Target::parse(post)?;
    comments::children(&self.api, comment, page).await
  }

  async fn likers(&self, post: &str, page: &PageReq) -> Result<Page<User>> {
    users::likers(&self.api, &Target::parse(post)?, page).await
  }

  async fn user(&self, user: &str) -> Result<User> {
    users::user(&self.api, &refs::user(user)?).await
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    users::answers(&self.api, &refs::user(user)?, page).await
  }

  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::follows(&self.api, &refs::user(user)?, "followers", page).await
  }

  async fn following(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    users::follows(&self.api, &refs::user(user)?, "followees", page).await
  }

  async fn collections(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    let user = user.map(refs::user).transpose()?;
    library::folders(&self.api, user.as_deref(), page).await
  }

  async fn favorites(
    &self,
    user: Option<&str>,
    folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    let folder = match folder {
      Some(f) => f.to_owned(),
      None => {
        let user = user.map(refs::user).transpose()?;
        library::first_folder(&self.api, user.as_deref()).await?
      }
    };
    library::folder_items(&self.api, &folder, page).await
  }

  async fn history(&self, page: &PageReq) -> Result<Page<Post>> {
    library::history(&self.api, page).await
  }

  async fn notifications(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    notifications::notifications(&self.api, kind, page).await
  }

  async fn unread(&self) -> Result<BTreeMap<String, u64>> {
    notifications::unread(&self.api).await
  }

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    posts::like(&self.api, &Target::parse(post)?, undo).await
  }

  async fn favorite(&self, post: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
    library::favorite(&self.api, &Target::parse(post)?, folder, undo).await
  }

  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    let target = Target::parse(post)?;
    comments::comment(&self.api, &target, &reply.text, reply.reply_to.as_deref()).await
  }

  async fn delete_comment(&self, post: &str, comment: &str) -> Result<Action> {
    comments::delete_comment(&self.api, &Target::parse(post)?, comment).await
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    users::follow(&self.api, "members", &refs::user(user)?, undo).await
  }

  /// Publishes a pin (想法); questions and articles have their own commands.
  async fn publish(&self, draft: &Draft) -> Result<Action> {
    if draft.reply_to.is_some() || draft.quote.is_some() || !draft.topics.is_empty() {
      return Err(Error::input(
        "Zhihu pins take a text, --title and --image only; use `ask` / `article` for topics",
      ));
    }
    let title = draft.title.as_deref().unwrap_or_default().trim();
    publish::pin(&self.api, title, &draft.text, &draft.images).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    posts::delete(&self.api, &Target::parse(post)?).await
  }

  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    match post {
      Some(post) => insights::post(&self.api, &Target::parse(post)?, days).await,
      None => insights::account(&self.api, days).await,
    }
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}
