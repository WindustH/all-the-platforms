//! Editing a tweet (X Premium): `CreateTweet` with
//! `edit_options.previous_tweet_id` makes a new version that replaces the
//! tweet. The tweet's `edit_control` says whether it may change
//! (`is_edit_eligible`), until when (`editable_until_msecs`, an hour after
//! posting) and how often (`edits_remaining`, five in all; an edited
//! version keeps them under `edit_control_initial`). The new version is
//! built from scratch, so the media (with their sensitive mark), the
//! quoted tweet and the tweet answered go along as they were.

use std::time::{SystemTime, UNIX_EPOCH};

use atp_core::cli::Change;
use atp_core::{Action, Edit, Error, ErrorCode, Result, Value, ValueExt};

use crate::api::Api;
use crate::parse;
use crate::posts;
use crate::refs::{self, tweet_url};
use crate::write::{self, Links};

/// Change the text of one of your tweets.
pub async fn edit(api: &Api, post: &str, edit: &Edit) -> Result<Action> {
  api.require_login()?;
  let Some(text) = edit.text.as_deref() else {
    return Err(Error::input("give the new text"));
  };
  let id = refs::tweet_id(post)?;
  let result = posts::result(api, &id).await?;
  let tweet = parse::tweet(&result);
  if !tweet.at("legacy").is_object() {
    return Err(Error::not_found(format!("tweet {id} not found")));
  }
  if api.own_id().is_some() && tweet.str("core.user_results.result.rest_id") != api.own_id() {
    return Err(Error::new(
      ErrorCode::PermissionDenied,
      format!("tweet {id} is not yours; X edits only your own"),
    ));
  }
  let left = editable(tweet, &id)?;
  let legacy = tweet.at("legacy");
  let media: Vec<String> = legacy
    .list("extended_entities.media")
    .iter()
    .filter_map(|m| m.str("id_str"))
    .collect();
  let links = Links {
    reply_to: legacy.str("in_reply_to_status_id_str"),
    quote: legacy.str("quoted_status_id_str"),
    edit: Some(id.clone()),
    sensitive: legacy.bool("possibly_sensitive").unwrap_or(false),
  };
  let mut change = Change::new(format!("X post {id}:"));
  let old = parse::post(&result)
    .and_then(|p| p.text)
    .unwrap_or_default();
  change.field("text", &old, text);
  kept(&mut change, media.len(), &links);
  if let Some(left) = left {
    change.row("edits", &format!("{left} left"));
  }
  change.confirm(edit.yes, "change this post on X")?;
  let new = write::create(api, text, &media, &links).await?;
  Ok(
    Action::done("edit", &id)
      .with_id(&new)
      .with_url(tweet_url("i", &new)),
  )
}

/// The edits left (when X says), or why the tweet cannot change now.
fn editable(tweet: &Value, id: &str) -> Result<Option<u64>> {
  let control = match tweet.at("edit_control.edit_control_initial") {
    initial @ Value::Object(_) => initial,
    _ => tweet.at("edit_control"),
  };
  let refuse = |why: String| Err(Error::new(ErrorCode::PermissionDenied, why));
  if let Some(newest) = control
    .list("edit_tweet_ids")
    .last()
    .and_then(Value::as_str)
    && newest != id
  {
    return refuse(format!(
      "tweet {id} has a newer version, {newest}: edit that one"
    ));
  }
  if control.bool("is_edit_eligible") == Some(false) {
    return refuse(format!(
      "X does not let tweet {id} be edited (posts with a poll and promoted posts cannot change)"
    ));
  }
  if let Some(until) = control.i64("editable_until_msecs")
    && until <= now_millis()
  {
    return refuse(format!(
      "the hour in which tweet {id} could be edited is over"
    ));
  }
  let left = control.u64("edits_remaining");
  if left == Some(0) {
    return refuse(format!("tweet {id} has no edits left"));
  }
  Ok(left)
}

/// The rows of what the new version keeps.
fn kept(change: &mut Change, media: usize, links: &Links) {
  if media > 0 {
    let sensitive = if links.sensitive { ", sensitive" } else { "" };
    change.row("media", &format!("{media} (kept{sensitive})"));
  }
  if let Some(quote) = &links.quote {
    change.row("quote", &format!("{quote} (kept)"));
  }
  if let Some(reply_to) = &links.reply_to {
    change.row("reply to", &format!("{reply_to} (kept)"));
  }
}

fn now_millis() -> i64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |d| d.as_millis() as i64)
}
