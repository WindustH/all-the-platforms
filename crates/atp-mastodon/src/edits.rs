//! Editing a status (`PUT /api/v1/statuses/:id`, Mastodon 3.5 and later).
//!
//! The update takes the whole status: media, a poll, the content warning
//! and the sensitive mark left out of the form are removed
//! (`UpdateStatusService`). So every edit sends them as they are: the
//! media ids, sensitivity, language and poll from `GET
//! /api/v1/statuses/:id`, the warning from `…/source`. Who can see a
//! status cannot change.

use std::time::{SystemTime, UNIX_EPOCH};

use atp_core::cli::Change;
use atp_core::http::Method;
use atp_core::{Action, Edit, Error, ErrorCode, Result, Value, ValueExt, text};

use crate::api::{Api, Query};
use crate::parse;
use crate::posts::status_id;
use crate::users;

/// Mastodon's shortest poll, in seconds (`PollExpirationValidator`).
const POLL_MIN_SECS: i64 = 300;

/// Change the text of one of your statuses, keeping everything else.
pub async fn edit(api: &Api, arg: &str, edit: &Edit) -> Result<Action> {
  api.require()?;
  let Some(new_text) = edit.text.as_deref() else {
    return Err(Error::input("give the new text"));
  };
  let id = status_id(api, arg).await?;
  supported(api).await?;
  let status = api.get(&format!("v1/statuses/{id}"), Vec::new()).await?;
  if status.str("account.id") != Some(users::my_id(api).await?) {
    return Err(Error::new(
      ErrorCode::PermissionDenied,
      format!("status {id} is not yours; Mastodon edits only your own"),
    ));
  }
  let source = api
    .get(&format!("v1/statuses/{id}/source"), Vec::new())
    .await?;
  let mut form: Query = vec![("status", new_text.to_owned())];
  let mut change = Change::new(format!("Mastodon status {id}:"));
  change.field("text", &source.str("text").unwrap_or_default(), new_text);
  keep_warning(&status, &source, &mut form, &mut change);
  keep_media(&status, &mut form, &mut change);
  keep_poll(&status, &mut form, &mut change)?;
  change.confirm(edit.yes, "change this status on Mastodon")?;
  let v = api
    .write(Method::PUT, &format!("v1/statuses/{id}"), form)
    .await?;
  let mut done = Action::done("edit", &id).with_id(&id);
  if let Some(url) = parse::post(&v, &api.instance()).url {
    done = done.with_url(url);
  }
  Ok(done)
}

/// Refuse on Mastodon before 3.5, which has no edits. Servers with
/// `/api/v2/instance` (Mastodon 4.0 on, and the other servers speaking its
/// API) answer for themselves; before it, `v1/instance` tells the version.
/// Pleroma and Akkoma name a Mastodon version they are `compatible` with,
/// which says nothing about edits.
async fn supported(api: &Api) -> Result<()> {
  match api.get("v2/instance", Vec::new()).await {
    Ok(_) => return Ok(()),
    Err(e) if e.code == ErrorCode::NotFound => {}
    Err(e) => return Err(e),
  }
  let v = api.get("v1/instance", Vec::new()).await?;
  let version = v.str("version").unwrap_or_default();
  if version.contains("compatible") || !older_than_3_5(&version) {
    return Ok(());
  }
  Err(Error::new(
    ErrorCode::UnsupportedOperation,
    format!(
      "{} runs Mastodon {version}, which cannot edit statuses (3.5 and later can)",
      api.instance()
    ),
  ))
}

/// Whether a Mastodon version (`3.4.6`, `3.5.0rc1`) is older than 3.5.
fn older_than_3_5(version: &str) -> bool {
  let mut numbers = version
    .split(|c: char| !c.is_ascii_digit())
    .map(|part| part.parse::<u32>().unwrap_or(0));
  let major = numbers.next().unwrap_or(0);
  let minor = numbers.next().unwrap_or(0);
  (major, minor) < (3, 5)
}

/// The content warning and the sensitive mark, as they are.
fn keep_warning(status: &Value, source: &Value, form: &mut Query, change: &mut Change) {
  let warning = source
    .str("spoiler_text")
    .or_else(|| status.str("spoiler_text"))
    .unwrap_or_default();
  let sensitive = status.bool("sensitive").unwrap_or(false);
  if !warning.is_empty() {
    change.row(
      "warning",
      &format!("{} (kept)", text::truncate(&warning, 50)),
    );
  }
  form.push(("spoiler_text", warning));
  form.push(("sensitive", sensitive.to_string()));
  if let Some(language) = status.str("language") {
    form.push(("language", language));
  }
}

/// The attachments, in their order; without them the edit removes them.
fn keep_media(status: &Value, form: &mut Query, change: &mut Change) {
  let ids: Vec<String> = status
    .list("media_attachments")
    .iter()
    .filter_map(|m| m.str("id"))
    .collect();
  if ids.is_empty() {
    return;
  }
  change.row("media", &format!("{} (kept)", ids.len()));
  form.extend(ids.into_iter().map(|id| ("media_ids[]", id)));
}

/// The poll with its options and choice mode, so its votes stay, and the
/// time it has left; without it the edit deletes the poll. A closed poll
/// would open again, so a status with one is refused.
fn keep_poll(status: &Value, form: &mut Query, change: &mut Change) -> Result<()> {
  let poll = status.at("poll");
  if poll.is_null() {
    return Ok(());
  }
  let left = poll
    .str("expires_at")
    .and_then(|at| text::parse_time(&at))
    .map(|at| at.as_second() - now());
  let Some(left) = left.filter(|secs| *secs > 0 && poll.bool("expired") != Some(true)) else {
    return Err(Error::new(
      ErrorCode::UnsupportedOperation,
      "the status has a closed poll, which an edit would open again",
    ));
  };
  let options = poll.list("options");
  for option in options {
    form.push(("poll[options][]", option.str("title").unwrap_or_default()));
  }
  let multiple = poll.bool("multiple").unwrap_or(false);
  // Totals hidden until the poll ends come without counts.
  let hidden = options.iter().any(|o| o.at("votes_count").is_null());
  let seconds = left.max(POLL_MIN_SECS);
  form.push(("poll[expires_in]", seconds.to_string()));
  form.push(("poll[multiple]", multiple.to_string()));
  form.push(("poll[hide_totals]", hidden.to_string()));
  let closes = text::from_secs(now() + seconds).map_or_else(String::new, text::fmt_time);
  change.row(
    "poll",
    &format!("{} options (kept; closes {closes})", options.len()),
  );
  Ok(())
}

fn now() -> i64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |d| d.as_secs() as i64)
}
