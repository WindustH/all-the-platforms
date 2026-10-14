//! Accounts: yours, other public profiles (`profile_lookup`), and the
//! posts that mention you.

use atp_core::{Notification, Page, PageReq, Result, User};
use atp_meta::graph::{fields, params};

use crate::api::Api;
use crate::parse::{self, POST_FIELDS};
use crate::posts;
use crate::refs::{self, UserRef};

const ME_FIELDS: &str =
  "id,username,name,threads_profile_picture_url,threads_biography,is_verified";
const PROFILE_FIELDS: &str = "username,name,profile_picture_url,biography,is_verified,\
follower_count,likes_count,quotes_count,replies_count,reposts_count,views_count";

pub async fn whoami(api: &Api) -> Result<User> {
  let v = api.graph().await?.get("me", &fields(ME_FIELDS)).await?;
  Ok(parse::me(&v))
}

/// Your profile, or another public one (needs the
/// `threads_profile_discovery` permission).
pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let name = match refs::user(arg)? {
    UserRef::Me => return whoami(api).await,
    UserRef::Name(name) => name,
  };
  if name == api.username().await? {
    return whoami(api).await;
  }
  let mut query = params(&[("username", &name)]);
  query.extend(fields(PROFILE_FIELDS));
  let v = api
    .graph()
    .await?
    .get("profile_lookup", &query)
    .await
    .map_err(posts::discovery_hint)?;
  Ok(parse::profile(&v))
}

/// Posts that mention you (`threads_manage_mentions`).
pub async fn mentions(api: &Api, req: &PageReq) -> Result<Page<Notification>> {
  let graph = api.graph().await?;
  let (rows, next) = graph
    .page("me/mentions", &fields(POST_FIELDS), req, 100)
    .await
    .map_err(|e| {
      if e.code == atp_core::ErrorCode::PermissionDenied {
        e.with_hint("mentions need the threads_manage_mentions permission on your app and token")
      } else {
        e
      }
    })?;
  Ok(Page::new(rows.iter().map(parse::mention).collect(), next))
}
