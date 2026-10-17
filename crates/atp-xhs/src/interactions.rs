//! Interactions: like, collect, comment, follow (`InteractionEndpointsMixin`,
//! `SocialEndpointsMixin`). Each pauses briefly first, as bursts get flagged.

use atp_core::{Action, Error, Reply, Result, ValueExt, json};

use crate::api::Api;
use crate::boards;
use crate::refs::{self, NoteRef, user_url};

const COMMENT: &str = "/api/sns/web/v1/comment/post";
const DELETE_COMMENT: &str = "/api/sns/web/v1/comment/delete";

async fn prepare_note(api: &Api, arg: &str) -> Result<NoteRef> {
  api.require_login()?;
  let note = refs::note_ref(api, arg).await?;
  api.write_pause().await;
  Ok(note)
}

pub async fn like(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let note = prepare_note(api, arg).await?;
  let (path, action) = match undo {
    false => ("/api/sns/web/v1/note/like", "like"),
    true => ("/api/sns/web/v1/note/dislike", "unlike"),
  };
  api.post(path, &json!({"note_oid": note.id})).await?;
  Ok(Action::done(action, &note.id).with_url(note.url()))
}

/// Collect a note (or with `undo`, uncollect it). With a board, the
/// collected note goes into it next, as the board picker the web shows
/// after collecting does.
pub async fn favorite(api: &Api, arg: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
  let board = folder.map(refs::board_ref).transpose()?;
  if undo && board.is_some() {
    return Err(Error::input(
      "unfavorite takes a note out of its board too: drop --folder",
    ));
  }
  let note = prepare_note(api, arg).await?;
  let (path, body, action) = match undo {
    false => (
      "/api/sns/web/v1/note/collect",
      json!({"note_id": note.id}),
      "favorite",
    ),
    true => (
      "/api/sns/web/v1/note/uncollect",
      json!({"note_ids": note.id}),
      "unfavorite",
    ),
  };
  api.post(path, &body).await?;
  let done = Action::done(action, &note.id).with_url(note.url());
  let Some(board) = board else {
    return Ok(done);
  };
  api.write_pause().await;
  match boards::place(api, &board, &note).await {
    Ok(()) => Ok(done.with_message(format!("board {board}"))),
    Err(e) => Err(Error {
      message: format!("collected, but not put into board {board}: {}", e.message),
      ..e
    }),
  }
}

/// Comment on a note, or reply to one of its comments.
///
/// Text only: comment images are an app feature, and the web endpoint drops
/// any it is sent.
pub async fn comment(api: &Api, arg: &str, reply: &Reply) -> Result<Action> {
  let note = prepare_note(api, arg).await?;
  let mut body = json!({"note_id": note.id, "content": reply.text, "at_users": []});
  if let Some(target) = &reply.reply_to {
    body["target_comment_id"] = json!(target);
  }
  let data = api.post(COMMENT, &body).await?;
  let name = if reply.reply_to.is_some() {
    "reply"
  } else {
    "comment"
  };
  let mut action = Action::done(name, &note.id).with_url(note.url());
  if let Some(id) = data.str("comment.id") {
    action = action.with_id(id);
  }
  Ok(action)
}

/// Like a comment (or with `undo`, take the like back).
pub async fn like_comment(api: &Api, arg: &str, comment: &str, undo: bool) -> Result<Action> {
  let note = prepare_note(api, arg).await?;
  let path = if undo {
    "/api/sns/web/v1/comment/dislike"
  } else {
    "/api/sns/web/v1/comment/like"
  };
  api
    .post(path, &json!({"note_id": note.id, "comment_id": comment}))
    .await?;
  let name = if undo {
    "unlike-comment"
  } else {
    "like-comment"
  };
  Ok(Action::done(name, comment).with_url(note.url()))
}

pub async fn delete_comment(api: &Api, arg: &str, comment: &str) -> Result<Action> {
  let note = prepare_note(api, arg).await?;
  let body = json!({"note_id": note.id, "comment_id": comment});
  api.post(DELETE_COMMENT, &body).await?;
  Ok(Action::done("delete-comment", comment).with_url(note.url()))
}

pub async fn follow(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  api.require_login()?;
  let id = refs::user_ref(api, arg).await?;
  api.write_pause().await;
  let (path, action) = match undo {
    false => ("/api/sns/web/v1/user/follow", "follow"),
    true => ("/api/sns/web/v1/user/unfollow", "unfollow"),
  };
  api.post(path, &json!({"target_user_id": id})).await?;
  Ok(Action::done(action, &id).with_url(user_url(&id)))
}
