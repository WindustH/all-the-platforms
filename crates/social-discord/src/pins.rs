//! Pinned messages: listing a channel's pins and pinning or unpinning, on
//! the current endpoints with a fallback to the older ones.

use social_core::{Action, ErrorCode, Page, PageReq, Post, Result, ValueExt, json};

use crate::api::Api;
use crate::messages::channel;
use crate::{parse, refs};

/// Pinned messages of a channel, newest pin first.
pub async fn list(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let (id, info) = channel(api, arg).await?;
  let mut query = vec![("limit", page.size_within(50).to_string())];
  if let Some(before) = &page.cursor {
    query.push(("before", before.clone()));
  }
  let v = match api
    .get(&format!("/channels/{id}/messages/pins"), &query)
    .await
  {
    Ok(v) => v,
    // Older API: every pin at once.
    Err(e) if e.code == ErrorCode::NotFound => {
      let all = api.get(&format!("/channels/{id}/pins"), &[]).await?;
      let items = all
        .as_array()
        .map(|rows| {
          rows
            .iter()
            .map(|m| parse::message(m, Some(&info.guild), Some(&info.name)))
            .collect()
        })
        .unwrap_or_default();
      return Ok(Page::last(items));
    }
    Err(e) => return Err(e),
  };
  let rows = v.list("items");
  let next = v
    .bool("has_more")
    .unwrap_or(false)
    .then(|| rows.last().and_then(|p| p.str("pinned_at")))
    .flatten();
  let items = rows
    .iter()
    .map(|p| {
      let mut post = parse::message(p.at("message"), Some(&info.guild), Some(&info.name));
      if let Some(at) = p.str("pinned_at") {
        post.extra.insert("pinned_at".into(), json!(at));
      }
      post
    })
    .collect();
  Ok(Page::new(items, next))
}

pub async fn pin(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let r = refs::message(arg)?;
  let new = format!("/channels/{}/messages/pins/{}", r.channel, r.message);
  let old = format!("/channels/{}/pins/{}", r.channel, r.message);
  let call = async |path: &str| {
    if undo {
      api.delete(path).await
    } else {
      api.put(path).await
    }
  };
  match call(&new).await {
    Err(e) if e.code == ErrorCode::NotFound => {
      call(&old).await?;
    }
    other => {
      other?;
    }
  }
  Ok(Action::done(if undo { "unpin" } else { "pin" }, r.id()))
}
