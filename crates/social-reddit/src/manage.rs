//! Managing posts and comments: author flags (NSFW, spoiler, reply
//! notifications), moderator actions (lock, sticky, approve, remove), post
//! flair, blocking users and the read state of inbox items.

use social_core::{Action, Error, Result, ValueExt};

use crate::api::Api;
use crate::{posts, refs};

/// A switch Reddit flips with one endpoint per direction (or a `state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Flag {
  /// Mark NSFW (authors and moderators)
  Nsfw,
  /// Mark as a spoiler (authors and moderators)
  Spoiler,
  /// Inbox replies to your post or comment
  Replies,
  /// Lock comments (moderators)
  Lock,
  /// Pin to the top of the subreddit (moderators; posts)
  Sticky,
}

/// Set a flag on, or off with `undo`.
pub async fn flag(api: &Api, arg: &str, flag: Flag, undo: bool) -> Result<Action> {
  let id = refs::thing(&api.ctx, arg).await?;
  let mut form = vec![("id", id.clone())];
  let path = match flag {
    Flag::Nsfw => {
      if undo {
        "/api/unmarknsfw"
      } else {
        "/api/marknsfw"
      }
    }
    Flag::Spoiler => {
      if undo {
        "/api/unspoiler"
      } else {
        "/api/spoiler"
      }
    }
    Flag::Lock => {
      if undo {
        "/api/unlock"
      } else {
        "/api/lock"
      }
    }
    Flag::Replies => {
      form.push(("state", (!undo).to_string()));
      "/api/sendreplies"
    }
    Flag::Sticky => {
      if !id.starts_with("t3_") {
        return Err(Error::input("only posts can be stickied"));
      }
      form.push(("state", (!undo).to_string()));
      "/api/set_subreddit_sticky"
    }
  };
  api.post(path, form).await?;
  let name = format!(
    "{}{}",
    if undo { "un" } else { "" },
    format!("{flag:?}").to_lowercase()
  );
  Ok(Action::done(name, id))
}

/// Approve a post or comment in a subreddit you moderate.
pub async fn approve(api: &Api, arg: &str) -> Result<Action> {
  let id = refs::thing(&api.ctx, arg).await?;
  api.post("/api/approve", vec![("id", id.clone())]).await?;
  Ok(Action::done("approve", id))
}

/// Remove a post or comment as a moderator (`spam` trains the filter).
pub async fn remove(api: &Api, arg: &str, spam: bool) -> Result<Action> {
  let id = refs::thing(&api.ctx, arg).await?;
  let form = vec![("id", id.clone()), ("spam", spam.to_string())];
  api.post("/api/remove", form).await?;
  Ok(Action::done("remove", id))
}

/// Set a post's flair: a template id from `flairs SUB` (with optional text
/// where the template allows editing), or free text where the subreddit allows.
pub async fn flair(
  api: &Api,
  arg: &str,
  template: Option<&str>,
  text: Option<&str>,
) -> Result<Action> {
  if template.is_none() && text.is_none() {
    return Err(Error::input("give a flair template (--template) or a text"));
  }
  let post = posts::fetch(api, arg).await?;
  let sub = post
    .extra
    .get("subreddit")
    .and_then(|s| s.as_str())
    .ok_or_else(|| Error::upstream("the post has no subreddit"))?
    .to_owned();
  let mut form = vec![("link", format!("t3_{}", post.id))];
  if let Some(t) = template {
    form.push(("flair_template_id", t.to_owned()));
  }
  if let Some(t) = text {
    form.push(("text", t.to_owned()));
  }
  api.post(&format!("/r/{sub}/api/selectflair"), form).await?;
  let mut action = Action::done("flair", format!("t3_{}", post.id));
  if let Some(u) = post.url {
    action = action.with_url(u);
  }
  Ok(action)
}

/// Block a user, or unblock with `undo`.
pub async fn block(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let name = refs::user(arg)?;
  if undo {
    let me = api.me().await?;
    let container = format!("t2_{}", me.str("id").unwrap_or_default());
    let form = vec![
      ("container", container),
      ("name", name.clone()),
      ("type", "enemy".into()),
    ];
    api.post("/r/all/api/unfriend", form).await?;
    return Ok(Action::done("unblock", format!("u/{name}")));
  }
  api
    .post("/api/block_user", vec![("name", name.clone())])
    .await?;
  Ok(Action::done("block", format!("u/{name}")))
}

/// Mark inbox items (`t1_…` / `t4_…` ids from `notifications`) read, or
/// unread with `unread`.
pub async fn mark(api: &Api, ids: &[String], unread: bool) -> Result<Action> {
  for id in ids {
    if !(id.starts_with("t1_") || id.starts_with("t4_")) {
      return Err(Error::input(format!(
        "not an inbox item id: {id} (expected t1_… or t4_…)"
      )));
    }
  }
  let (path, name) = if unread {
    ("/api/unread_message", "mark-unread")
  } else {
    ("/api/read_message", "mark-read")
  };
  api.post(path, vec![("id", ids.join(","))]).await?;
  Ok(Action::done(name, ids.join(",")))
}
