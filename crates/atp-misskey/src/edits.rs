//! Editing a note, on the servers that have it. Upstream Misskey has no
//! edits (2023.9.2 brought them, 2023.10.0 took them out); its forks
//! differ, so the server's software is read from nodeinfo first:
//!
//! - Sharkey: `notes/edit` with `editId` and the note's whole
//!   `notes/create` body. The note is rebuilt from it, so the files, the
//!   poll (resent exactly, so its votes stay), the quote, the reply, the
//!   channel and the reaction setting go along as they are.
//! - CherryPick: `notes/update` for roles allowed to edit (`canEditNote`),
//!   with the files, poll, event, right-click setting and scheduled
//!   deletion resent as they are.

use std::time::{SystemTime, UNIX_EPOCH};

use atp_core::cli::Change;
use atp_core::{Action, Edit, Error, ErrorCode, Result, Value, ValueExt, json, text};

use crate::api::Api;
use crate::{parse, posts, refs, users};

/// The forks that edit notes, by nodeinfo `software.name`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fork {
  Sharkey,
  CherryPick,
}

/// Change the text of one of your notes.
pub async fn edit(api: &Api, arg: &str, edit: &Edit) -> Result<Action> {
  api.require()?;
  let Some(new_text) = edit.text.as_deref() else {
    return Err(Error::input("give the new text"));
  };
  if refs::message(arg).is_some() {
    return Err(Error::unsupported("edit of chat messages"));
  }
  let fork = fork(api).await?;
  let id = posts::note_id(api, arg).await?;
  let note = api.call("notes/show", json!({ "noteId": id })).await?;
  if note.str("userId") != Some(users::my_id(api).await?) {
    return Err(Error::new(
      ErrorCode::PermissionDenied,
      format!("note {id} is not yours; only your own notes can change"),
    ));
  }
  let mut change = Change::new(format!("Misskey note {id}:"));
  change.field("text", &note.str("text").unwrap_or_default(), new_text);
  kept(&note, &mut change);
  let (endpoint, body) = match fork {
    Fork::Sharkey => ("notes/edit", sharkey(&note, &id, new_text)?),
    Fork::CherryPick => ("notes/update", cherrypick(&note, &id, new_text)?),
  };
  change.confirm(edit.yes, "change this note")?;
  api.write(endpoint, body).await.map_err(role)?;
  Ok(
    Action::done("edit", &id)
      .with_id(&id)
      .with_url(parse::note_url(&api.instance(), &id)),
  )
}

/// The fork the instance runs, from its nodeinfo; others are refused.
async fn fork(api: &Api) -> Result<Fork> {
  let index = get(api, &format!("{}/.well-known/nodeinfo", api.base())).await?;
  let href = index
    .list("links")
    .iter()
    .filter_map(|l| l.str("href"))
    .next_back()
    .ok_or_else(|| Error::upstream("the instance's nodeinfo names no document"))?;
  let info = get(api, &href).await?;
  let name = info.str("software.name").unwrap_or_default();
  match name.to_lowercase().as_str() {
    "sharkey" => Ok(Fork::Sharkey),
    "cherrypick" => Ok(Fork::CherryPick),
    software => Err(Error::new(
      ErrorCode::UnsupportedOperation,
      format!(
        "{} runs {}, which cannot edit notes (Sharkey and CherryPick can; Misskey itself has no edits)",
        api.instance(),
        if software.is_empty() {
          "unknown software"
        } else {
          software
        },
      ),
    )),
  }
}

/// CherryPick's refusal for roles that may not edit, said plainly.
fn role(e: Error) -> Error {
  if !e.message.contains("ROLE_PERMISSION_DENIED") {
    return e;
  }
  Error::new(ErrorCode::PermissionDenied, e.message)
    .with_hint("the instance lets only some roles edit notes (canEditNote); ask its admins")
}

async fn get(api: &Api, url: &str) -> Result<Value> {
  let resp = api.ctx.http.get(url).no_cookies().send().await?;
  if !resp.status.is_success() {
    return Err(atp_core::http::status_error(resp.status, &resp.text()));
  }
  resp.value()
}

/// `notes/edit` (Sharkey): the note's own `notes/create` fields with the
/// new text.
fn sharkey(note: &Value, id: &str, new_text: &str) -> Result<Value> {
  let mut body = json!({
    "editId": id,
    "text": new_text,
    "cw": note.at("cw"),
    "visibility": note.at("visibility"),
    "localOnly": note.bool("localOnly").unwrap_or(false),
    "reactionAcceptance": note.at("reactionAcceptance"),
    "replyId": note.at("replyId"),
    "renoteId": note.at("renoteId"),
    "channelId": note.at("channelId"),
  });
  if note.str("visibility").as_deref() == Some("specified") {
    body["visibleUserIds"] = note.at("visibleUserIds").clone();
  }
  add_files_and_poll(note, &mut body)?;
  Ok(body)
}

/// `notes/update` (CherryPick): the text with the files, poll, event,
/// right-click setting and scheduled deletion as they are.
fn cherrypick(note: &Value, id: &str, new_text: &str) -> Result<Value> {
  let mut body = json!({
    "noteId": id,
    "text": new_text,
    "cw": note.at("cw"),
    "disableRightClick": note.bool("disableRightClick").unwrap_or(false),
  });
  add_files_and_poll(note, &mut body)?;
  let event = note.at("event");
  if event.is_object() {
    let end = event.str("end").and_then(|at| millis(&at));
    if end.is_some_and(|end| end <= now_millis()) {
      return Err(Error::unsupported(
        "edit of a note whose event has ended (CherryPick would refuse to keep it)",
      ));
    }
    body["event"] = json!({
      "title": event.at("title"),
      "start": event.str("start").and_then(|at| millis(&at)),
      "end": end,
      "metadata": event.get("metadata").cloned().unwrap_or_else(|| json!({})),
    });
  }
  if let Some(at) = note.str("deleteAt").and_then(|at| millis(&at)) {
    body["scheduledDelete"] = json!({ "deleteAt": at });
  }
  Ok(body)
}

/// The note's files in their order and its poll exactly as it is (the
/// same choices, mode and end leave its votes alone). A closed poll
/// cannot be sent again, so such a note is refused.
fn add_files_and_poll(note: &Value, body: &mut Value) -> Result<()> {
  let files = note.list("fileIds");
  if !files.is_empty() {
    body["fileIds"] = json!(files);
  }
  let poll = note.at("poll");
  if !poll.is_object() {
    return Ok(());
  }
  let expires = poll.str("expiresAt").and_then(|at| millis(&at));
  if expires.is_some_and(|at| at <= now_millis()) {
    return Err(Error::new(
      ErrorCode::UnsupportedOperation,
      "the note has a closed poll, which the server refuses to keep in an edit",
    ));
  }
  let choices: Vec<String> = poll
    .list("choices")
    .iter()
    .filter_map(|c| c.str("text"))
    .collect();
  body["poll"] = json!({
    "choices": choices,
    "multiple": poll.bool("multiple").unwrap_or(false),
    "expiresAt": expires,
  });
  Ok(())
}

/// The rows of what an edit keeps.
fn kept(note: &Value, change: &mut Change) {
  if let Some(cw) = note.str("cw") {
    change.row("warning", &format!("{} (kept)", text::truncate(&cw, 50)));
  }
  let files = note.list("fileIds").len();
  if files > 0 {
    change.row("files", &format!("{files} (kept)"));
  }
  if let Some(choices) = note.at("poll").get("choices").and_then(Value::as_array) {
    change.row("poll", &format!("{} choices (kept)", choices.len()));
  }
  if let Some(quote) = note.str("renoteId") {
    change.row("quote", &format!("{quote} (kept)"));
  }
}

fn millis(at: &str) -> Option<i64> {
  text::parse_time(at).map(|ts| ts.as_millisecond())
}

fn now_millis() -> i64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |d| d.as_millis() as i64)
}
