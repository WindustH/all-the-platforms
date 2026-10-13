//! Comments of the web client. A thread lists `commentViewModel`s holding
//! keys only; text, author and counters arrive as entity payloads in
//! `frameworkUpdates.entityBatchUpdate.mutations`, keyed by those keys
//! (`commentEntityPayload`, `engagementToolbarStateEntityPayload`,
//! `engagementToolbarSurfaceEntityPayload`), as YouTube.js `CommentView` maps them.

use std::collections::HashMap;

use atp_core::{Comment, User, Value, ValueExt};

use super::{ago, count, put, text};
use crate::refs::{channel_url, comment_url};

/// The entity payloads of a response, by entity key.
pub struct Entities<'a>(HashMap<String, &'a Value>);

impl<'a> Entities<'a> {
  pub fn of(v: &'a Value) -> Self {
    let map = v
      .list("frameworkUpdates.entityBatchUpdate.mutations")
      .iter()
      .filter_map(|m| {
        let payload = m.at("payload").as_object()?.values().next()?;
        Some((m.str("entityKey").or_else(|| payload.str("key"))?, payload))
      })
      .collect();
    Self(map)
  }

  pub fn get(&self, key: Option<String>) -> Option<&'a Value> {
    self.0.get(&key?).copied()
  }
}

/// A comment from its view model and the response's entities.
pub fn comment_view(view: &Value, entities: &Entities, video: &str) -> Option<Comment> {
  let entity = entities.get(view.str("commentKey"))?;
  let props = entity.at("properties");
  let id = props.str("commentId").or_else(|| view.str("commentId"))?;
  let toolbar = entity.at("toolbar");
  let text = text(props.at("content")).unwrap_or_default();
  let top = props.u64("replyLevel").unwrap_or(0) == 0;
  let published = props.str("publishedTime");
  let mut c = Comment {
    created_at: published.as_deref().and_then(ago),
    likes: toolbar
      .str("likeCountA11y")
      .and_then(|t| count(&t))
      .or_else(|| toolbar.str("likeCountNotliked").and_then(|t| count(&t)))
      .or(Some(0)),
    reply_count: top.then(|| {
      toolbar
        .str("replyCount")
        .and_then(|t| count(&t))
        .unwrap_or(0)
    }),
    // Replies name whom they answer with a leading @handle.
    reply_to: (!top).then(|| mentioned(&text)).flatten(),
    raw: Some(entity.clone()),
    author: comment_author(entity.at("author")),
    text,
    ..Comment::default()
  };
  put(&mut c.extra, "url", comment_url(video, &id));
  comment_flags(&mut c, view, entity, entities, published);
  c.id = id;
  Some(c)
}

/// The channel that wrote a comment (its `author` entity).
fn comment_author(author: &Value) -> Option<User> {
  let channel = author.str("channelId")?;
  let name = author.str("displayName").unwrap_or_else(|| channel.clone());
  Some(User {
    handle: name.strip_prefix('@').map(str::to_owned),
    url: Some(channel_url(&channel)),
    avatar: author.str("avatarThumbnailUrl"),
    verified: author.bool("isVerified") == Some(true),
    name,
    id: channel,
    ..User::default()
  })
}

/// The @handle a reply starts with.
fn mentioned(text: &str) -> Option<String> {
  text
    .split_whitespace()
    .next()
    .filter(|w| w.starts_with('@'))
    .map(str::to_owned)
}

/// Edited, pinned, by the creator, hearted, and your own rating.
fn comment_flags(
  c: &mut Comment,
  view: &Value,
  entity: &Value,
  entities: &Entities,
  published: Option<String>,
) {
  let x = &mut c.extra;
  put(x, "published", published.clone());
  if published.is_some_and(|p| p.contains("edited")) {
    put(x, "edited", true);
  }
  if view.str("pinnedText").is_some() {
    put(x, "pinned", true);
  }
  if entity.bool("author.isCreator") == Some(true) {
    put(x, "by_creator", true);
  }
  if let Some(state) = entities.get(view.str("toolbarStateKey")) {
    if state.str("heartState").as_deref() == Some("TOOLBAR_HEART_STATE_HEARTED") {
      put(x, "hearted", true);
    }
    match state.str("likeState").as_deref() {
      Some("TOOLBAR_LIKE_STATE_LIKED") => put(x, "liked", true),
      Some("TOOLBAR_LIKE_STATE_DISLIKED") => put(x, "disliked", true),
      _ => {}
    }
  }
}

/// Exact number of comments from a comments page header (`1,598 Comments`).
pub fn header_count(v: &Value) -> Option<u64> {
  super::first(v, "commentsHeaderRenderer")
    .and_then(|h| text(h.at("countText")))
    .and_then(|t| count(&t))
}
