//! Accounts and follows. Since February 2026 Spotify shows development-mode
//! apps only the logged-in account: no other profiles (`GET /users/{id}`)
//! and no other users' playlists, so USER arguments must name you.

use atp_core::{Action, Error, ErrorCode, Page, PageReq, Post, Result, User, ValueExt};

use crate::api::Api;
use crate::refs::{self, Kind};
use crate::{parse, playlists};

/// Followed artists come this many a page at most.
const PAGE: usize = 50;

/// The logged-in account.
pub async fn whoami(api: &Api) -> Result<User> {
  parse::user(&api.me().await?).ok_or_else(|| Error::upstream("Spotify sent no account"))
}

/// Fail unless `arg` names the logged-in account: `me`, its id or its link.
pub async fn mine(api: &Api, arg: &str) -> Result<()> {
  let Some(id) = refs::user(arg)? else {
    return Ok(());
  };
  if id == api.my_id().await? {
    return Ok(());
  }
  Err(Error::new(
    ErrorCode::UnsupportedOperation,
    format!(
      "Spotify shows apps in development mode only your own account, not {id} (other \
       profiles and playlists were removed in February 2026)"
    ),
  ))
}

/// `user USER`: your own profile.
pub async fn profile(api: &Api, arg: &str) -> Result<User> {
  mine(api, arg).await?;
  whoami(api).await
}

/// `user-posts`: the playlists you made, public and private.
pub async fn playlists(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  mine(api, arg).await?;
  playlists::mine(api, true, page).await
}

/// The artists you follow (Spotify lists followed artists only).
pub async fn following(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  mine(api, arg).await?;
  let mut query = vec![
    ("type", "artist".to_owned()),
    ("limit", page.size_within(PAGE).to_string()),
  ];
  if let Some(after) = &page.cursor {
    query.push(("after", after.clone()));
  }
  let v = api.get("/me/following", &query).await?;
  let found = v.at("artists");
  let items = found
    .list("items")
    .iter()
    .filter_map(parse::artist_user)
    .collect();
  let next = found.str("next").and_then(|_| found.str("cursors.after"));
  Ok(Page::new(items, next))
}

/// Follow (or unfollow) an artist, a user or a playlist; a bare id is an
/// artist.
pub async fn follow(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let kinds = [Kind::Artist, Kind::User, Kind::Playlist];
  let item = refs::expect(api, arg, Kind::Artist, &kinds).await?;
  api.library(&[item.uri()], undo).await?;
  let action = if undo { "unfollow" } else { "follow" };
  Ok(Action::done(action, item.uri()).with_url(item.url()))
}
