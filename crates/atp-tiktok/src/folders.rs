//! Favorites collections (folders of saved videos), filled, emptied and
//! sorted the way the collection page's "Add videos", "Remove" and "Move"
//! do: `/api/collection/modify_items/` with `commitIds` or `deleteIds`, and
//! `/api/collection/move_items/` with `itemIDs`, `fromCollectionID` and
//! `targetCollectionID`; video ids go comma-separated, as signed writes.

use atp_core::bulk;
use atp_core::{Action, Result};

use crate::api::Api;
use crate::refs;

/// Videos sent in one call; how many TikTok takes at once is not known.
const BATCH: usize = 20;

/// The video ids of the arguments, all read before anything is sent.
async fn video_ids(api: &Api, posts: &[String]) -> Result<Vec<String>> {
  let mut ids = Vec::with_capacity(posts.len());
  for post in posts {
    ids.push(refs::post_ref(api, post).await?.id);
  }
  Ok(ids)
}

/// Put videos into a collection, or take them out with `undo`, 20 per call.
pub async fn file(api: &Api, folder: &str, posts: &[String], undo: bool) -> Result<Vec<Action>> {
  api.require_login()?;
  let collection = refs::collection_id(folder)?;
  let ids = video_ids(api, posts).await?;
  let (key, name) = if undo {
    ("deleteIds", "folder-remove")
  } else {
    ("commitIds", "folder-add")
  };
  bulk::batches(name, &ids, BATCH, String::clone, async |chunk| {
    api.write_pause().await;
    let params = [("collectionId", collection.clone()), (key, chunk.join(","))];
    api.post("/api/collection/modify_items/", &params).await?;
    Ok(
      chunk
        .iter()
        .map(|id| Action::done(name, id).with_message(format!("collection {collection}")))
        .collect(),
    )
  })
  .await
}

/// Move videos from one collection to another, 20 per call.
pub async fn move_items(api: &Api, from: &str, to: &str, posts: &[String]) -> Result<Vec<Action>> {
  api.require_login()?;
  let (from, to) = (refs::collection_id(from)?, refs::collection_id(to)?);
  let ids = video_ids(api, posts).await?;
  bulk::batches("folder-move", &ids, BATCH, String::clone, async |chunk| {
    api.write_pause().await;
    let params = [
      ("itemIDs", chunk.join(",")),
      ("fromCollectionID", from.clone()),
      ("targetCollectionID", to.clone()),
    ];
    api.post("/api/collection/move_items/", &params).await?;
    Ok(
      chunk
        .iter()
        .map(|id| Action::done("folder-move", id).with_message(format!("{from} → {to}")))
        .collect(),
    )
  })
  .await
}
