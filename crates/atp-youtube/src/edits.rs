//! Editing your videos through the official YouTube Data API, with the
//! OAuth grant of `atp youtube oauth` (scope `youtube.force-ssl`).
//!
//! `videos.update` replaces every writable field of the parts it is sent
//! (a description or tags left out are deleted, a privacy left out goes
//! back to the default), so the video's own `snippet` and `status`, as
//! `videos.list` gives them to the owner, go back with the changes; only
//! the parts that change are sent. The thumbnail goes through
//! `thumbnails.set`.

use std::path::Path;

use atp_core::cli::Change;
use atp_core::file::Upload;
use atp_core::http::Method;
use atp_core::{Action, Edit, Error, ErrorCode, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::oauth::{self, Credentials, DATA, SETUP_HINT};
use crate::refs;

const LABEL: &str = "YouTube Data API";
const THUMBNAIL_URL: &str = "https://www.googleapis.com/upload/youtube/v3/thumbnails/set";
/// The largest thumbnail YouTube takes.
const THUMBNAIL_MAX: usize = 2 * 1024 * 1024;

/// Change one of your videos: its title, description (TEXT), tags,
/// privacy and thumbnail.
pub async fn edit(api: &Api, arg: &str, edit: &Edit) -> Result<Action> {
  let creds = Credentials::from_env().ok_or_else(|| {
    Error::auth("editing videos needs the OAuth grant of the YouTube Data API")
      .with_hint(SETUP_HINT)
  })?;
  let id = refs::video(arg)?;
  if creds.can_write(&api.ctx).await? == Some(false) {
    return Err(oauth::scope_error());
  }
  let video = own_video(api, &creds, &id).await?;
  let mut change = Change::new(format!("YouTube video {id}:"));
  let (parts, body) = merged(&video, edit, &mut change)?;
  let thumbnail = match &edit.cover {
    Some(path) => Some(thumbnail(path, &mut change).await?),
    None => None,
  };
  change.confirm(edit.yes, "change this video on YouTube")?;
  if !parts.is_empty() {
    let req = api
      .ctx
      .http
      .request(Method::PUT, format!("{DATA}/videos"))
      .query("part", parts.join(","))
      .json(&body);
    creds.send(&api.ctx, req, LABEL).await?;
  }
  if let Some(file) = thumbnail {
    let req = api
      .ctx
      .http
      .post(THUMBNAIL_URL)
      .query("videoId", &id)
      .bytes(file.data, file.mime);
    creds.send(&api.ctx, req, LABEL).await.map_err(no_custom)?;
  }
  Ok(
    Action::done("edit", &id)
      .with_id(&id)
      .with_url(refs::video_url(&id)),
  )
}

/// The video's `snippet` and `status`, when it belongs to the channel of
/// the grant.
async fn own_video(api: &Api, creds: &Credentials, id: &str) -> Result<Value> {
  let req = api
    .ctx
    .http
    .get(format!("{DATA}/videos"))
    .query("part", "snippet,status")
    .query("id", id);
  let v = creds.send(&api.ctx, req, LABEL).await?;
  let Some(video) = v.list("items").first() else {
    return Err(Error::not_found(format!("no YouTube video {id}")));
  };
  let req = api
    .ctx
    .http
    .get(format!("{DATA}/channels"))
    .query("part", "id")
    .query("mine", "true");
  let mine = creds.send(&api.ctx, req, LABEL).await?;
  let channel = mine.str("items.0.id");
  if channel.is_none() || video.str("snippet.channelId") != channel {
    return Err(Error::new(
      ErrorCode::PermissionDenied,
      format!("video {id} is not on the channel of the OAuth grant"),
    ));
  }
  Ok(video.clone())
}

/// The parts to send and the request body: the video's own `snippet` and
/// `status` with the changes, each row added to `change`.
fn merged(video: &Value, edit: &Edit, change: &mut Change) -> Result<(Vec<&'static str>, Value)> {
  let mut snippet = video.at("snippet").clone();
  let mut status = video.at("status").clone();
  let mut parts = Vec::new();
  let title = snippet.str("title").unwrap_or_default();
  if let Some(new) = edit.title.as_deref().map(str::trim) {
    if new.is_empty() {
      return Err(Error::input("a YouTube video needs a title"));
    }
    change.field("title", &title, new);
    snippet["title"] = json!(new);
  }
  if let Some(text) = &edit.text {
    let old = snippet.str("description").unwrap_or_default();
    change.field("description", &old, text);
    snippet["description"] = json!(text);
  }
  if let Some(topics) = &edit.topics {
    let old: Vec<String> = snippet
      .list("tags")
      .iter()
      .filter_map(|t| t.as_str().map(str::to_owned))
      .collect();
    let new: Vec<String> = topics
      .iter()
      .map(|t| t.trim_start_matches('#').to_owned())
      .collect();
    change.field("tags", &old.join(", "), &new.join(", "));
    snippet["tags"] = json!(new);
  }
  if edit.title.is_some() || edit.text.is_some() || edit.topics.is_some() {
    if snippet.str("categoryId").is_none() {
      return Err(Error::upstream(
        "YouTube sent the video without its category",
      ));
    }
    parts.push("snippet");
  }
  if let Some(new) = &edit.visibility {
    privacy(&mut status, new, change)?;
    parts.push("status");
  }
  let mut body = json!({ "id": video.str("id") });
  if parts.contains(&"snippet") {
    body["snippet"] = snippet;
  }
  if parts.contains(&"status") {
    body["status"] = status;
  }
  Ok((parts, body))
}

/// `public` or `private`; a scheduled publication (`publishAt`) stays
/// only while the video stays private.
fn privacy(status: &mut Value, new: &str, change: &mut Change) -> Result<()> {
  if new == "friends" {
    return Err(Error::unsupported(
      "edit --visibility friends (YouTube videos are public, unlisted or private)",
    ));
  }
  let old = status.str("privacyStatus").unwrap_or_default();
  change.field("visibility", &old, new);
  status["privacyStatus"] = json!(new);
  if new != "private"
    && let Some(fields) = status.as_object_mut()
  {
    fields.remove("publishAt");
  }
  Ok(())
}

/// The new thumbnail, read and checked (JPEG or PNG, 2 MB at most).
async fn thumbnail(path: &Path, change: &mut Change) -> Result<Upload> {
  let file = Upload::read(path).await?;
  if !matches!(file.mime, "image/jpeg" | "image/png") {
    return Err(Error::input("a YouTube thumbnail is a JPEG or PNG image"));
  }
  if file.data.len() > THUMBNAIL_MAX {
    return Err(Error::input("a YouTube thumbnail is 2 MB at most"));
  }
  change.row("thumbnail", &format!("{} (new)", path.display()));
  Ok(file)
}

/// `thumbnails.set` refused for the channel, said plainly.
fn no_custom(e: Error) -> Error {
  if e.code != ErrorCode::PermissionDenied {
    return e;
  }
  e.with_hint("custom thumbnails need a channel verified by phone (youtube.com/verify)")
}
