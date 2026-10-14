//! Writes through the access token: votes, bookmarks, comments and
//! answers. Each waits a moment first; the sites also limit how often an
//! account may comment or answer, and refuse with `write_failed`.

use atp_core::{Action, Error, Reply, Result, ValueExt};

use crate::api::{Api, Params};
use crate::posts;
use crate::refs::{self, Kind};

fn noun(kind: Kind) -> &'static str {
  match kind {
    Kind::Answer => "answers",
    _ => "questions",
  }
}

/// `upvote` / `downvote`, or their `/undo`.
pub async fn vote(api: &Api, arg: &str, up: bool, undo: bool) -> Result<Action> {
  api.require()?;
  let post = refs::post(arg)?;
  let site = api.site_or(&post.site);
  let kind = posts::kind(api, &post, &site).await?;
  let vote = if up { "upvote" } else { "downvote" };
  let undo_part = if undo { "/undo" } else { "" };
  let method = format!("{}/{}/{vote}{undo_part}", noun(kind), post.id);
  api.post(&method, &site, Vec::new()).await?;
  let action = match (up, undo) {
    (true, false) => "upvote",
    (true, true) => "unvote",
    (false, false) => "downvote",
    (false, true) => "undownvote",
  };
  Ok(Action::done(action, arg))
}

/// Bookmark a question (the API still calls it `favorite`).
pub async fn bookmark(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  api.require()?;
  let post = refs::post(arg)?;
  let site = api.site_or(&post.site);
  if posts::kind(api, &post, &site).await? != Kind::Question {
    return Err(Error::input("only questions can be bookmarked"));
  }
  let undo_part = if undo { "/undo" } else { "" };
  let method = format!("questions/{}/favorite{undo_part}", post.id);
  api.post(&method, &site, Vec::new()).await?;
  Ok(Action::done(
    if undo { "unfavorite" } else { "favorite" },
    arg,
  ))
}

/// A comment under a question or answer. Comments are flat: `--reply-to`
/// mentions the author of that comment (`@Name`) at the start.
pub async fn comment(api: &Api, arg: &str, reply: &Reply) -> Result<Action> {
  api.require()?;
  let post = refs::post(arg)?;
  let site = api.site_or(&post.site);
  let mut body = reply.text.trim().to_owned();
  if let Some(to) = &reply.reply_to {
    let (_, to) = refs::comment(to)?;
    let v = api
      .get(
        &format!("comments/{to}"),
        Some(&site),
        Vec::new(),
        "default",
      )
      .await?;
    let name = v.str("items.0.owner.display_name").unwrap_or_default();
    let name: String = atp_core::text::html_to_text(&name)
      .chars()
      .filter(|c| !c.is_whitespace())
      .collect();
    if !name.is_empty() {
      body = format!("@{name} {body}");
    }
  }
  let form: Params = vec![("body", body), ("preview", "false".into())];
  let v = api
    .post(&format!("posts/{}/comments/add", post.id), &site, form)
    .await?;
  created(
    v.u64("items.0.comment_id"),
    v.str("items.0.link"),
    "comment",
    arg,
  )
}

/// A comment of yours; its site is the post's when the comment is an id.
pub async fn delete_comment(api: &Api, arg: &str, comment: &str) -> Result<Action> {
  api.require()?;
  let (site, id) = refs::comment(comment)?;
  let site = api.site_or(&site.or(refs::post(arg)?.site));
  let method = format!("comments/{id}/delete");
  api.post(&method, &site, Vec::new()).await?;
  Ok(Action::done("delete-comment", comment))
}

/// An answer (Markdown) to a question.
pub async fn answer(api: &Api, arg: &str, body: &str) -> Result<Action> {
  api.require()?;
  let post = refs::post(arg)?;
  if post.kind == Kind::Answer {
    return Err(Error::input("answer a question, not an answer"));
  }
  let site = api.site_or(&post.site);
  let form: Params = vec![("body", body.to_owned()), ("preview", "false".into())];
  let v = api
    .post(&format!("questions/{}/answers/add", post.id), &site, form)
    .await?;
  created(
    v.u64("items.0.answer_id"),
    v.str("items.0.link"),
    "answer",
    arg,
  )
}

fn created(id: Option<u64>, link: Option<String>, action: &str, target: &str) -> Result<Action> {
  let mut done = Action::done(action, target);
  if let Some(id) = id {
    done = done.with_id(id.to_string());
  }
  if let Some(link) = link {
    done = done.with_url(link);
  }
  Ok(done)
}
