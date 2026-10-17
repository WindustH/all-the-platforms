//! Commands on several things at once (`follow A B C`, `folder-add F P…`):
//! one [`Action`] per thing, in the order given, whether the platform
//! sends them in one batch or one by one.

use crate::error::{Error, ErrorCode, Result};
use crate::model::Action;
use crate::platform::Platform;

/// Errors the next things would meet too: a run of several stops there.
fn stops(code: ErrorCode) -> bool {
  matches!(
    code,
    ErrorCode::NotAuthenticated
      | ErrorCode::VerificationRequired
      | ErrorCode::IpBlocked
      | ErrorCode::RateLimited
      | ErrorCode::SignatureError
  )
}

/// Run `op` on each item in turn (the HTTP client paces the requests),
/// an Action each. An item that fails gets a failed Action named `action`
/// and the next one goes on; an error the rest would meet too (a login, a
/// human check, a rate limit) ends the run and marks the rest skipped.
/// With one item, or when the first one meets such an error, the error
/// itself comes back, as from a command on one thing.
pub async fn each(
  action: &str,
  items: &[String],
  mut op: impl AsyncFnMut(&str) -> Result<Action>,
) -> Result<Vec<Action>> {
  if let [item] = items {
    return Ok(vec![op(item).await?]);
  }
  let mut done: Vec<Action> = Vec::with_capacity(items.len());
  let mut stopped: Option<Error> = None;
  for item in items {
    if let Some(e) = &stopped {
      done.push(skipped(action, item, e));
      continue;
    }
    match op(item).await {
      Ok(a) => done.push(a),
      Err(e) if stops(e.code) && done.is_empty() => return Err(e),
      Err(e) => {
        done.push(Action::failed(action, item.as_str(), &e));
        stopped = stops(e.code).then_some(e);
      }
    }
  }
  Ok(done)
}

/// Send items `size` at a time through `op`, which acts on a whole batch
/// and gives an Action per item of it; `target` names an item in failed
/// Actions. A batch that fails marks its items failed and the next batch
/// goes on, as [`each`] does with single items; when everything is one
/// batch, or the first batch meets an error the rest would meet too, the
/// error itself comes back.
pub async fn batches<T>(
  action: &str,
  items: &[T],
  size: usize,
  target: impl Fn(&T) -> String,
  mut op: impl AsyncFnMut(&[T]) -> Result<Vec<Action>>,
) -> Result<Vec<Action>> {
  let mut done: Vec<Action> = Vec::with_capacity(items.len());
  let mut stopped: Option<Error> = None;
  for chunk in items.chunks(size.max(1)) {
    if let Some(e) = &stopped {
      done.extend(chunk.iter().map(|t| skipped(action, &target(t), e)));
      continue;
    }
    match op(chunk).await {
      Ok(list) => done.extend(list),
      Err(e) if done.is_empty() && (stops(e.code) || chunk.len() == items.len()) => {
        return Err(e);
      }
      Err(e) => {
        done.extend(chunk.iter().map(|t| Action::failed(action, target(t), &e)));
        stopped = stops(e.code).then_some(e);
      }
    }
  }
  Ok(done)
}

/// An item not tried after the run stopped at `error`.
pub fn skipped(action: &str, item: &str, error: &Error) -> Action {
  Action {
    message: Some(format!("not tried after: {}", error.message)),
    ..Action::failed(action, item, error)
  }
}

/// The default `folder-move`: add the posts to `to`, then remove the ones
/// added from `from` (unless `copy`). A post added but not removed is a
/// failed Action saying it is in both folders.
pub async fn move_by_adding<P: Platform>(
  p: &P,
  from: &str,
  to: &str,
  posts: &[String],
  copy: bool,
) -> Result<Vec<Action>> {
  let name = if copy { "folder-copy" } else { "folder-move" };
  let added = p.folder_add(to, posts).await?;
  if copy {
    return Ok(added.into_iter().map(|a| renamed(a, name)).collect());
  }
  let moved: Vec<String> = posts
    .iter()
    .zip(&added)
    .filter(|(_, a)| a.ok)
    .map(|(post, _)| post.clone())
    .collect();
  let removed = match moved.as_slice() {
    [] => Vec::new(),
    _ => p
      .folder_remove(from, &moved)
      .await
      .unwrap_or_else(|e| moved.iter().map(|m| Action::failed(name, m, &e)).collect()),
  };
  let mut removed = removed.into_iter();
  let both = |a: Action, why: Option<String>| Action {
    ok: false,
    message: Some(format!(
      "added to {to} but still in {from}: {}",
      why.unwrap_or_default()
    )),
    error: a.error.or(Some(ErrorCode::UpstreamError)),
    ..renamed(a, name)
  };
  let merged = added.into_iter().map(|a| {
    if !a.ok {
      return renamed(a, name);
    }
    match removed.next() {
      Some(r) if r.ok => renamed(a, name).with_message(format!("{from} → {to}")),
      Some(r) => both(
        Action {
          error: r.error,
          ..a
        },
        r.message,
      ),
      None => both(a, None),
    }
  });
  Ok(merged.collect())
}

fn renamed(a: Action, name: &str) -> Action {
  Action {
    action: name.to_owned(),
    ..a
  }
}
