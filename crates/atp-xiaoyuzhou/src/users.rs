//! Users: a user's page (profile, counts, the shows they host), and the
//! posts of a show or user for `user-posts`: a show's episodes, or the
//! shows a user hosts.

use atp_core::{Error, ErrorCode, Page, PageReq, Post, Result, User, ValueExt};

use crate::api::Api;
use crate::refs::{self, Item};
use crate::{parse, shows};

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  match refs::resolve(api, arg).await? {
    Item::User(id) | Item::Bare(id) => profile(api, &id).await,
    Item::Show(_) => Err(Error::input(
      "that is a show; `read` it (its hosts are in extra.podcasters)",
    )),
    Item::Episode(_) => Err(Error::input("that is an episode; use `read`")),
  }
}

async fn profile(api: &Api, id: &str) -> Result<User> {
  let props = api.page(&format!("/user/{id}")).await?;
  let profile = props.at("profile");
  if profile.str("user.uid").is_none() {
    return Err(Error::not_found(format!("Xiaoyuzhou has no user {id}")));
  }
  Ok(parse::profile(profile))
}

/// A show's episodes, or the shows a user hosts; a bare id is tried as a
/// show, then as a user.
pub async fn posts(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  match refs::resolve(api, arg).await? {
    Item::Show(id) => shows::episodes(api, &id, page).await,
    Item::User(id) => hosted(api, &id).await,
    Item::Episode(_) => Err(Error::input("that is an episode; give its show")),
    Item::Bare(id) => match shows::episodes(api, &id, page).await {
      Err(e) if e.code == ErrorCode::NotFound => hosted(api, &id).await,
      other => other,
    },
  }
}

/// The shows a user hosts.
async fn hosted(api: &Api, id: &str) -> Result<Page<Post>> {
  let props = api.page(&format!("/user/{id}")).await?;
  let shows = props.list("profile.ownedPodcasts");
  Ok(Page::last(shows.iter().map(parse::show).collect()))
}
