//! Relations beyond following: removing followers, blocks, mutes and the
//! follow requests of a locked account (`/api/v1/accounts/:id/…`,
//! `/api/v1/blocks`, `/api/v1/mutes`, `/api/v1/follow_requests`), one
//! account per call.

use atp_core::bulk;
use atp_core::http::Method;
use atp_core::{Action, Page, PageReq, Result, User};

use crate::api::Api;
use crate::{parse, users};

/// `POST <path with the account id>` for each account; `action` names it.
async fn each_account(
  api: &Api,
  inputs: &[String],
  action: &str,
  path: impl Fn(&str) -> String,
) -> Result<Vec<Action>> {
  api.require()?;
  bulk::each(action, inputs, async |input| {
    let id = users::id(api, input).await?;
    api.write(Method::POST, &path(&id), Vec::new()).await?;
    Ok(Action::done(action, &id))
  })
  .await
}

/// Make accounts stop following you (`remove_from_followers`).
pub async fn remove_followers(api: &Api, inputs: &[String]) -> Result<Vec<Action>> {
  each_account(api, inputs, "remove-follower", |id| {
    format!("v1/accounts/{id}/remove_from_followers")
  })
  .await
}

/// Block accounts, or unblock them with `undo`.
pub async fn block(api: &Api, inputs: &[String], undo: bool) -> Result<Vec<Action>> {
  let verb = if undo { "unblock" } else { "block" };
  each_account(api, inputs, verb, |id| format!("v1/accounts/{id}/{verb}")).await
}

/// Mute accounts (their notifications too, with no end), or unmute them
/// with `undo`.
pub async fn mute(api: &Api, inputs: &[String], undo: bool) -> Result<Vec<Action>> {
  let verb = if undo { "unmute" } else { "mute" };
  each_account(api, inputs, verb, |id| format!("v1/accounts/{id}/{verb}")).await
}

/// Accept follow requests (`authorize`), or decline them (`reject`).
pub async fn answer(api: &Api, inputs: &[String], accept: bool) -> Result<Vec<Action>> {
  let (verb, action) = if accept {
    ("authorize", "accept-follower")
  } else {
    ("reject", "deny-follower")
  };
  each_account(api, inputs, action, |id| {
    format!("v1/follow_requests/{id}/{verb}")
  })
  .await
}

/// An account listing of yours: `v1/blocks`, `v1/mutes`,
/// `v1/follow_requests`.
pub async fn accounts(api: &Api, path: &str, page: &PageReq) -> Result<Page<User>> {
  api.require()?;
  api.list(path, Vec::new(), page, parse::user).await
}
