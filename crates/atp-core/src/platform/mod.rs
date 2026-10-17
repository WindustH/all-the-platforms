//! The contract between the shared command set and one platform.
//!
//! A platform describes itself with a static [`PlatformInfo`] and implements
//! the [`Platform`] operations it supports; every other operation keeps its
//! default body, which reports `unsupported_operation`. Commands that only make
//! sense on one platform live in the platform's own `Extra` subcommand enum.

mod cap;
mod info;
mod request;

use crate::ctx::Ctx;
use crate::download::Link;
use crate::error::{Error, Result};
use crate::model::{
  Action, Collection, Comment, Data, FileEntry, Insights, Media, Notification, Page, Post, User,
};
use crate::output::Stream;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub use self::cap::*;
pub use self::info::*;
pub use self::request::*;

macro_rules! unsupported {
  ($op:literal) => {
    Err(Error::unsupported($op))
  };
}

/// Operations of one platform. Post and user arguments arrive already
/// resolved from `#N`; they may be ids or URLs in the platform's own formats.
#[allow(async_fn_in_trait, unused_variables)]
pub trait Platform: Sized {
  const INFO: PlatformInfo;

  /// Map platforms: the coordinate system points are given and printed in
  /// unless `--coord` names another. `None` (the default) adds no
  /// `--coord`.
  const COORD: Option<crate::geo::System> = None;

  /// Values of `following --sort`, the plain order first: `recent` (latest
  /// follow first), `oldest`, `smart` (the platform's own mix), `visited`
  /// (most visited). Empty (the default) hides the option.
  const FOLLOWING_SORT: &'static [&'static str] = &[];

  /// Values of `follow-group --sort`, as for [`Self::FOLLOWING_SORT`].
  const FOLLOW_GROUP_SORT: &'static [&'static str] = &[];

  /// Whether a `verification_required` error may be passed in the person's
  /// browser and the command retried with the cookies it set. A platform
  /// without session cookies (`cookie_domains` empty) has none to take
  /// back, so by default only cookie platforms get the browser; single
  /// errors opt out with [`Error::unclearable`].
  const BROWSER_CHECKS: bool = !Self::INFO.cookie_domains.is_empty();

  /// Platform-only commands, merged next to the shared ones.
  type Extra: clap::Subcommand;

  fn new(ctx: Ctx) -> Result<Self>;

  fn ctx(&self) -> &Ctx;

  // ── account ──────────────────────────────────────────────────────────

  /// The logged-in account; fails with `not_authenticated` when the session is not valid.
  async fn whoami(&self) -> Result<User>;

  /// Whether credentials are present (not whether they still work). Defaults
  /// to the required cookies; platforms with other credentials override it.
  fn logged_in(&self) -> bool {
    let ctx = self.ctx();
    ctx
      .info
      .required_cookies
      .iter()
      .all(|c| ctx.http.has_cookie(c))
  }

  /// Called after cookies were imported (cookie string or browser), before they are
  /// verified and saved. Use it to fetch missing helper cookies.
  async fn prepare_login(&self) -> Result<()> {
    Ok(())
  }

  /// Keep an access token or API credentials (`login --token`) in the
  /// session; [`Platform::token_whoami`] verifies them afterwards.
  async fn login_token(&self, token: &str) -> Result<()> {
    unsupported!("login --token")
  }

  /// The account behind the token `login --token` just kept: what `login`
  /// checks before saving. `whoami` by default; a platform whose `whoami`
  /// prefers a cookie session overrides it, so the token is what gets checked.
  async fn token_whoami(&self) -> Result<User> {
    self.whoami().await
  }

  async fn qr_start(&self) -> Result<QrTicket> {
    unsupported!("login --qrcode")
  }

  async fn qr_poll(&self, ticket: &QrTicket) -> Result<QrStatus> {
    unsupported!("login --qrcode")
  }

  // ── reading ──────────────────────────────────────────────────────────

  async fn search(&self, query: &Query, page: &PageReq) -> Result<Page<Post>> {
    unsupported!("search")
  }

  async fn search_users(&self, query: &Query, page: &PageReq) -> Result<Page<User>> {
    unsupported!("search --type user")
  }

  async fn search_topics(&self, query: &Query, page: &PageReq) -> Result<Page<Collection>> {
    unsupported!("search --type topic")
  }

  async fn hot(&self, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    unsupported!("hot")
  }

  async fn feed(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    unsupported!("feed")
  }

  async fn read(&self, post: &str) -> Result<Post> {
    unsupported!("read")
  }

  async fn comments(
    &self,
    post: &str,
    sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Comment>> {
    unsupported!("comments")
  }

  async fn replies(&self, post: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
    unsupported!("replies")
  }

  /// Accounts that liked a post (where the platform shows them).
  async fn likers(&self, post: &str, page: &PageReq) -> Result<Page<User>> {
    unsupported!("likers")
  }

  /// Reposts, retweets, quotes or crossposts of a post.
  async fn reposts(&self, post: &str, page: &PageReq) -> Result<Page<Post>> {
    unsupported!("reposts")
  }

  async fn user(&self, user: &str) -> Result<User> {
    unsupported!("user")
  }

  async fn user_posts(&self, user: &str, page: &PageReq) -> Result<Page<Post>> {
    unsupported!("user-posts")
  }

  async fn followers(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    unsupported!("followers")
  }

  async fn following(&self, user: &str, page: &PageReq) -> Result<Page<User>> {
    unsupported!("following")
  }

  /// `following` sorted (`--sort`), searched (`--search`) or narrowed to
  /// mutual follows (`--mutual`). Without any of them it is
  /// [`following`](Self::following); a platform that offers them overrides it.
  async fn following_with(
    &self,
    user: &str,
    query: &FollowQuery,
    page: &PageReq,
  ) -> Result<Page<User>> {
    if query.is_empty() {
      return self.following(user, page).await;
    }
    Err(Error::unsupported(query.option()))
  }

  /// Favorites folders, lists and similar containers (`None` = the logged-in account).
  async fn collections(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    unsupported!("collections")
  }

  /// Saved / bookmarked posts, optionally inside one folder.
  async fn favorites(
    &self,
    user: Option<&str>,
    folder: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<Post>> {
    unsupported!("favorites")
  }

  async fn likes(&self, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
    unsupported!("likes")
  }

  async fn history(&self, page: &PageReq) -> Result<Page<Post>> {
    unsupported!("history")
  }

  async fn notifications(&self, kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
    unsupported!("notifications")
  }

  async fn unread(&self) -> Result<BTreeMap<String, u64>> {
    unsupported!("unread")
  }

  // ── writing ──────────────────────────────────────────────────────────

  async fn like(&self, post: &str, undo: bool) -> Result<Action> {
    unsupported!("like")
  }

  async fn favorite(&self, post: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
    unsupported!("favorite")
  }

  /// `like` / `unlike` of several posts: one by one through
  /// [`like`](Self::like) by default; platforms with a batch call
  /// override it. One Action per post, in order (see
  /// [`bulk`](crate::bulk)).
  async fn like_posts(&self, posts: &[String], undo: bool) -> Result<Vec<Action>> {
    let action = if undo { "unlike" } else { "like" };
    crate::bulk::each(action, posts, async |post| self.like(post, undo).await).await
  }

  /// `favorite` / `unfavorite` of several posts, as
  /// [`like_posts`](Self::like_posts) through [`favorite`](Self::favorite).
  async fn favorite_posts(
    &self,
    posts: &[String],
    folder: Option<&str>,
    undo: bool,
  ) -> Result<Vec<Action>> {
    let action = if undo { "unfavorite" } else { "favorite" };
    crate::bulk::each(action, posts, async |post| {
      self.favorite(post, folder, undo).await
    })
    .await
  }

  /// Comment on a post, or reply to one of its comments.
  async fn comment(&self, post: &str, reply: &Reply) -> Result<Action> {
    unsupported!("comment")
  }

  async fn delete_comment(&self, post: &str, comment: &str) -> Result<Action> {
    unsupported!("delete-comment")
  }

  async fn follow(&self, user: &str, undo: bool) -> Result<Action> {
    unsupported!("follow")
  }

  /// The argument of `follow` / `unfollow` with `#N` resolved: from the
  /// last user list by default. Platforms where following joins a chat or
  /// a community, or takes several kinds, pick the list here
  /// ([`Ctx::collection_ref`], [`Ctx::newest_ref`]).
  fn follow_ref(&self, arg: &str) -> Result<String> {
    self.ctx().user_ref(arg)
  }

  async fn publish(&self, draft: &Draft) -> Result<Action> {
    unsupported!("post")
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    unsupported!("delete")
  }

  // ── follows ──────────────────────────────────────────────────────────
  //
  // Commands on several users or posts return one Action per item, in
  // order (see [`bulk`](crate::bulk)); failed items are Actions with `ok`
  // false. A single item fails with its error instead.

  /// `follow` / `unfollow` of several users: one by one through
  /// [`follow`](Self::follow) by default; platforms with a batch call
  /// override it.
  async fn follow_users(&self, users: &[String], undo: bool) -> Result<Vec<Action>> {
    let action = if undo { "unfollow" } else { "follow" };
    crate::bulk::each(action, users, async |user| self.follow(user, undo).await).await
  }

  /// Make users stop following you.
  async fn remove_followers(&self, users: &[String]) -> Result<Vec<Action>> {
    unsupported!("remove-follower")
  }

  /// Block (or with `undo` unblock) users.
  async fn block(&self, users: &[String], undo: bool) -> Result<Vec<Action>> {
    unsupported!("block")
  }

  /// The accounts you block.
  async fn blocked(&self, page: &PageReq) -> Result<Page<User>> {
    unsupported!("blocked")
  }

  /// Your follow groups (关注分组, lists of the people you follow), as
  /// Collections of kind `group`.
  async fn follow_groups(&self, page: &PageReq) -> Result<Page<Collection>> {
    unsupported!("follow-groups")
  }

  /// The people in one of your follow groups; `sort` is one of
  /// [`Self::FOLLOW_GROUP_SORT`].
  async fn follow_group(
    &self,
    group: &str,
    sort: Option<&str>,
    page: &PageReq,
  ) -> Result<Page<User>> {
    unsupported!("follow-group")
  }

  async fn follow_group_create(&self, name: &str) -> Result<Action> {
    unsupported!("follow-group-create")
  }

  async fn follow_group_rename(&self, group: &str, name: &str) -> Result<Action> {
    unsupported!("follow-group-rename")
  }

  async fn follow_group_delete(&self, group: &str) -> Result<Action> {
    unsupported!("follow-group-delete")
  }

  /// Put users into a follow group; their other groups stay.
  async fn follow_group_add(&self, group: &str, users: &[String]) -> Result<Vec<Action>> {
    unsupported!("follow-group-add")
  }

  /// Take users out of a follow group; they stay followed.
  async fn follow_group_remove(&self, group: &str, users: &[String]) -> Result<Vec<Action>> {
    unsupported!("follow-group-remove")
  }

  // ── folders ──────────────────────────────────────────────────────────
  //
  // Favorites folders: the containers `collections` lists and `favorite
  // --folder` saves into (收藏夹, boards, bookmark folders).

  /// Create a folder; `folder.name` is set.
  async fn folder_create(&self, folder: &FolderSpec) -> Result<Action> {
    unsupported!("folder-create")
  }

  /// Rename a folder or change its description, visibility or cover.
  async fn folder_edit(&self, folder: &str, change: &FolderSpec) -> Result<Action> {
    unsupported!("folder-edit")
  }

  async fn folder_delete(&self, folders: &[String]) -> Result<Vec<Action>> {
    unsupported!("folder-delete")
  }

  /// Save posts into a folder.
  async fn folder_add(&self, folder: &str, posts: &[String]) -> Result<Vec<Action>> {
    unsupported!("folder-add")
  }

  /// Take posts out of a folder.
  async fn folder_remove(&self, folder: &str, posts: &[String]) -> Result<Vec<Action>> {
    unsupported!("folder-remove")
  }

  /// Move posts from one folder to another, or with `copy` add them to
  /// `to` and keep them in `from`. By default
  /// [`folder_add`](Self::folder_add) then
  /// [`folder_remove`](Self::folder_remove); platforms with a move call
  /// override it.
  async fn folder_move(
    &self,
    from: &str,
    to: &str,
    posts: &[String],
    copy: bool,
  ) -> Result<Vec<Action>> {
    crate::bulk::move_by_adding(self, from, to, posts, copy).await
  }

  // ── watch later and history ──────────────────────────────────────────
  //
  // The watch-later list (稍后再看, Watch later) and the viewing history
  // `history` lists. Methods on several posts return one Action per post,
  // as those of the follows do.

  /// Your watch-later list.
  async fn watch_later(&self, page: &PageReq) -> Result<Page<Post>> {
    unsupported!("watch-later")
  }

  /// Put posts on the watch-later list.
  async fn later_add(&self, posts: &[String]) -> Result<Vec<Action>> {
    unsupported!("later-add")
  }

  /// Take posts off the watch-later list.
  async fn later_remove(&self, posts: &[String]) -> Result<Vec<Action>> {
    unsupported!("later-remove")
  }

  /// Empty the watch-later list, or with `watched`
  /// ([`Cap::LaterClearWatched`]) drop only the posts already watched.
  async fn later_clear(&self, watched: bool) -> Result<Action> {
    unsupported!("later-clear")
  }

  /// Remove posts from the viewing history.
  async fn history_delete(&self, posts: &[String]) -> Result<Vec<Action>> {
    unsupported!("history-delete")
  }

  /// Erase the whole viewing history.
  async fn history_clear(&self) -> Result<Action> {
    unsupported!("history-clear")
  }

  // ── analytics ────────────────────────────────────────────────────────

  /// Creator-center analytics of the logged-in account (`post` = `None`) or of
  /// one of its posts, over the last `days` days where the platform allows it.
  async fn insights(&self, post: Option<&str>, days: u32) -> Result<Insights> {
    unsupported!("insights")
  }

  // ── chat ─────────────────────────────────────────────────────────────
  //
  // Chats are `Collection`s (`kind`: server, channel, group, dm, thread ...)
  // and messages are `Post`s. A message without a permalink takes
  // `CHAT/MESSAGE` as its `id`, so commands accept it back.

  /// The account's conversations: servers, teams, groups and direct chats,
  /// or the channels inside `parent`.
  async fn chats(&self, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    unsupported!("chats")
  }

  /// Messages of one chat, newest first.
  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    unsupported!("messages")
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    unsupported!("send")
  }

  async fn members(&self, chat: &str, page: &PageReq) -> Result<Page<User>> {
    unsupported!("members")
  }

  /// Add (or with `undo` remove) an emoji reaction to a message or post.
  async fn react(&self, post: &str, emoji: &str, undo: bool) -> Result<Action> {
    unsupported!("react")
  }

  /// Replace the text of one of your messages.
  async fn edit(&self, post: &str, text: &str) -> Result<Action> {
    unsupported!("edit")
  }

  /// Change one of your posts: its text, title, topics, cover or
  /// visibility, as far as its capabilities (`Edit*`) say. By default a
  /// new text alone goes to [`edit`](Self::edit).
  async fn edit_post(&self, post: &str, edit: &Edit) -> Result<Action> {
    match edit.text_only() {
      Some(text) => self.edit(post, text).await,
      None => unsupported!("edit"),
    }
  }

  /// Hand every incoming message (of `chats`, or of all chats when empty) to
  /// `out` as it arrives, until the connection ends or the user interrupts.
  /// Other events (edits, deletions, reactions ...) are `Post`s too, with
  /// their kind in `kind`.
  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    unsupported!("listen")
  }

  // ── media ────────────────────────────────────────────────────────────

  /// Save one media file that is not reachable over HTTP (its `url` in the
  /// platform's own scheme) as `dir/name.<ext>` and return the path; `None`
  /// lets the shared downloader fetch `url`.
  async fn save_media(&self, media: &Media, dir: &Path, name: &str) -> Option<Result<PathBuf>> {
    None
  }

  /// The post and the media files to download. Defaults to the media of `read`.
  async fn media(&self, post: &str, audio_only: bool) -> Result<(Post, Vec<Media>)> {
    let post = self.read(post).await?;
    let media = post.media.clone();
    Ok((post, media))
  }

  // ── drives ───────────────────────────────────────────────────────────
  //
  // A drive (`Cap::Files`) holds files and folders: its `search`, `read`
  // and `download` call these instead of the post methods, and `delete`
  // takes a file. Arguments are its ids, paths or links.

  /// One file or folder.
  async fn file(&self, file: &str) -> Result<FileEntry> {
    unsupported!("read")
  }

  /// The entries of a folder.
  async fn files(&self, folder: &FileEntry, page: &PageReq) -> Result<Page<FileEntry>> {
    unsupported!("ls")
  }

  /// Files and folders whose names match.
  async fn search_files(&self, query: &Query, page: &PageReq) -> Result<Page<FileEntry>> {
    unsupported!("search")
  }

  /// Where to fetch a file's bytes, and the headers its host wants; `renew`
  /// asks for a fresh link after the host refused the last one.
  async fn file_link(&self, file: &FileEntry, renew: bool) -> Result<Link> {
    unsupported!("download")
  }

  // ── platform-only commands ───────────────────────────────────────────

  async fn run_extra(&self, command: Self::Extra) -> Result<Data>;
}

/// `Extra` for a platform without commands of its own.
#[derive(Debug, clap::Subcommand)]
pub enum NoExtra {}
