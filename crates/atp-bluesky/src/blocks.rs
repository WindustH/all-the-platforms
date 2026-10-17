//! Blocks and mutes. A block is a public `app.bsky.graph.block` record in
//! your repository (unblocking deletes the one your view of the profile
//! names, `viewer.blocking`); `getBlocks` lists them. Mutes are private:
//! `muteActor` / `unmuteActor`, listed by `getMutes`.

use atp_core::bulk;
use atp_core::{Action, Error, Page, PageReq, Result, User, ValueExt, json};

use crate::api::{self, Api};
use crate::{parse, users};

const BLOCK: &str = "app.bsky.graph.block";

/// Block users, or unblock them with `undo`, one at a time.
pub async fn block(api: &Api, inputs: &[String], undo: bool) -> Result<Vec<Action>> {
  api.did()?;
  let name = if undo { "unblock" } else { "block" };
  bulk::each(name, inputs, async |input| {
    let actor = users::actor(api, input)?;
    let profile = api
      .get_auth("app.bsky.actor.getProfile", vec![("actor", actor)])
      .await?;
    let did = profile.str("did").unwrap_or_default();
    match (undo, profile.str("viewer.blocking")) {
      (true, Some(record)) => api.delete_record(&record).await?,
      (true, None) => return Err(Error::input(format!("you do not block {did}"))),
      (false, Some(_)) => return Ok(Action::done(name, &did).with_message("already blocked")),
      (false, None) => {
        let record = json!({"$type": BLOCK, "subject": did, "createdAt": api::now()});
        api.create_record(BLOCK, record).await?;
      }
    }
    Ok(Action::done(name, &did))
  })
  .await
}

/// The accounts you block.
pub async fn blocked(api: &Api, page: &PageReq) -> Result<Page<User>> {
  api.did()?;
  api
    .list(
      "app.bsky.graph.getBlocks",
      Vec::new(),
      "blocks",
      page,
      |v| Some(parse::user(v)),
    )
    .await
}

/// Mute users (their posts leave your feeds; only you know), or unmute them
/// with `undo`, one at a time.
pub async fn mute(api: &Api, inputs: &[String], undo: bool) -> Result<Vec<Action>> {
  api.did()?;
  let (nsid, name) = if undo {
    ("app.bsky.graph.unmuteActor", "unmute")
  } else {
    ("app.bsky.graph.muteActor", "mute")
  };
  bulk::each(name, inputs, async |input| {
    let actor = users::actor(api, input)?;
    api.call(nsid, json!({"actor": actor})).await?;
    Ok(Action::done(name, &actor))
  })
  .await
}

/// The accounts you mute fully (not the ones muted for reposts or quotes
/// only).
pub async fn muted(api: &Api, page: &PageReq) -> Result<Page<User>> {
  api.did()?;
  api
    .list("app.bsky.graph.getMutes", Vec::new(), "mutes", page, |v| {
      Some(parse::user(v))
    })
    .await
}
