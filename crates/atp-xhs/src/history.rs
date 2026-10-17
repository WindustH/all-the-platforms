//! Browsing history (浏览记录): the notes you opened, newest first, and
//! removing them. The web app's history page calls `common/dual_feed` with
//! `scene=browse_history`; entries are flat notes with `browse_time`.

use atp_core::bulk;
use atp_core::text::from_millis;
use atp_core::{Action, Page, PageReq, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, SOURCE_FEED};

const LIST: &str = "/api/sns/web/v1/common/dual_feed";
const DELETE: &str = "/api/sns/web/v2/note/browsing_history/delete";
const CLEAR: &str = "/api/sns/web/v2/note/browsing_history/clear";
const COUNT: &str = "/api/sns/web/v2/note/browsing_history/count";
/// The history tab of notes (the other tab holds live rooms).
const NOTES: &str = "note";

/// One page; the cursor is `<cursor_score>|<note_index>`.
pub async fn page(api: &Api, req: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let cursor = req.cursor.as_deref().unwrap_or_default();
  let (score, index) = cursor.split_once('|').unwrap_or((cursor, "0"));
  let index: u64 = index.parse().unwrap_or(0);
  let index_s = index.to_string();
  let params = [
    ("scene", "browse_history"),
    ("cursor_score", score),
    ("note_index", index_s.as_str()),
    ("image_formats", "jpg,webp,avif"),
  ];
  let data = api.get(LIST, &params).await?;
  let rows = data.list("data");
  refs::remember(
    &api.ctx,
    rows
      .iter()
      .filter_map(|r| Some((r.str("id")?, r.str("xsec_token")?)))
      .collect::<Vec<_>>()
      .iter()
      .map(|(id, t)| (id.as_str(), t.as_str(), SOURCE_FEED)),
  );
  let items: Vec<Post> = rows.iter().filter_map(entry).collect();
  let next = data
    .str("next_cursor")
    .filter(|_| data.bool("has_no_more") != Some(true) && !items.is_empty())
    .map(|s| format!("{s}|{}", index + items.len() as u64));
  Ok(Page::new(items, next))
}

/// A history entry as a note: full cards pass through; flat entries name
/// their images `images_list` and carry `liked_count` at the top.
fn entry(row: &Value) -> Option<Post> {
  let read_at = row.i64("browse_time").and_then(from_millis);
  let item = if row.at("note_card").is_object() {
    row.clone()
  } else {
    let mut card = row.clone();
    if let Some(obj) = card.as_object_mut() {
      if let Some(images) = obj.remove("images_list") {
        obj.insert("image_list".into(), images);
      }
      let interact = json!({"liked": row.at("liked"), "liked_count": row.at("liked_count")});
      obj.insert("interact_info".into(), interact);
    }
    card
  };
  let mut post = parse::note(&item, SOURCE_FEED)?;
  if let Some(t) = read_at {
    post
      .extra
      .insert("read_at".into(), Value::from(t.to_string()));
  }
  Some(post)
}

/// Remove notes from the history, all in one call; an Action per note.
pub async fn delete(api: &Api, notes: &[String]) -> Result<Vec<Action>> {
  api.require_login()?;
  let mut ids = Vec::new();
  for arg in notes {
    ids.push(refs::note_ref(api, arg).await?.id);
  }
  bulk::batches(
    "history-delete",
    &ids,
    ids.len(),
    String::clone,
    async |chunk| {
      api
        .post(DELETE, &json!({"item_ids": chunk, "type": NOTES}))
        .await?;
      Ok(
        chunk
          .iter()
          .map(|id| Action::done("history-delete", id))
          .collect(),
      )
    },
  )
  .await
}

/// Empty the history of notes.
pub async fn clear(api: &Api) -> Result<Action> {
  api.require_login()?;
  api.post(CLEAR, &json!({"type": NOTES})).await?;
  Ok(Action::done("history-clear", "browsing history"))
}

/// How many notes the history holds.
pub async fn count(api: &Api) -> Result<Value> {
  api.require_login()?;
  api.get(COUNT, &[("content_type", NOTES)]).await
}
