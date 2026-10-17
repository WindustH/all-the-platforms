//! Who may follow you: removing followers, blocking, and the follow requests
//! of a private account. Writes go one user at a time: the `friendships/…`
//! calls of the API the app and the web share (`remove_follower`,
//! `pending`, `approve`, `ignore`, with the user id in `user_id`) and the
//! web's own `/web/friendships/<id>/block/` and `…/unblock/`.

use atp_core::bulk;
use atp_core::{Action, Page, PageReq, Result, User, ValueExt};

use crate::api::{Api, with_cursor};
use crate::parse;

/// Make users stop following you, one call each.
pub async fn remove_followers(api: &Api, inputs: &[String]) -> Result<Vec<Action>> {
  api.require_session("removing followers")?;
  bulk::each("remove-follower", inputs, async |input| {
    let id = api.user_id(input).await?;
    api
      .post(
        &format!("friendships/remove_follower/{id}/"),
        &[("user_id", id.clone())],
      )
      .await?;
    Ok(Action::done("remove-follower", input).with_id(id))
  })
  .await
}

/// Block users, or unblock them with `undo`, one call each.
pub async fn block(api: &Api, inputs: &[String], undo: bool) -> Result<Vec<Action>> {
  api.require_session("blocking")?;
  let verb = if undo { "unblock" } else { "block" };
  bulk::each(verb, inputs, async |input| {
    let id = api.user_id(input).await?;
    api
      .post(&format!("/web/friendships/{id}/{verb}/"), &[])
      .await?;
    Ok(Action::done(verb, input).with_id(id))
  })
  .await
}

/// Accounts asking to follow your private account.
pub async fn requests(api: &Api, req: &PageReq) -> Result<Page<User>> {
  api.require_session("follow requests")?;
  let query = with_cursor(Vec::new(), "max_id", req.cursor.as_ref());
  let v = api.get("friendships/pending/", &query).await?;
  let users = v.list("users").iter().map(parse::user).collect();
  Ok(Page::new(users, v.str("next_max_id")))
}

/// Accept follow requests (`approve`), or decline them (`ignore`), one
/// call each.
pub async fn answer(api: &Api, inputs: &[String], accept: bool) -> Result<Vec<Action>> {
  api.require_session("follow requests")?;
  let (verb, name) = if accept {
    ("approve", "accept-follower")
  } else {
    ("ignore", "deny-follower")
  };
  bulk::each(name, inputs, async |input| {
    let id = api.user_id(input).await?;
    api
      .post(
        &format!("friendships/{verb}/{id}/"),
        &[("user_id", id.clone())],
      )
      .await?;
    Ok(Action::done(name, input).with_id(id))
  })
  .await
}
