//! Notes: timelines, featured notes, one note, replies, reactions, renotes,
//! search, writing and deleting.

use std::path::PathBuf;

use social_core::file::sniff;
use social_core::{
  Action, Collection, Comment, Draft, Error, Page, PageReq, Post, Query, Result, User, Value,
  ValueExt, json,
};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, NoteRef};

pub const FEED_KINDS: &[&str] = &["home", "local", "hybrid", "global"];
pub const SEARCH_FILTERS: &[&str] = &["local"];
/// The reaction `like` sends (Misskey's like button).
pub const LIKE: &str = "❤";

/// A note argument → its id on this instance (links to other servers are
/// looked up with `ap/show`, which needs an account).
pub async fn note_id(api: &Api, arg: &str) -> Result<String> {
  match refs::note(arg, &api.instance())? {
    NoteRef::Id(id) => Ok(id),
    NoteRef::Remote(uri) => {
      let v = api.call_auth("ap/show", json!({"uri": uri})).await?;
      if v.str("type").as_deref() != Some("Note") {
        return Err(Error::not_found(format!("{uri} is not a note")));
      }
      v.str("object.id")
        .ok_or_else(|| Error::not_found(format!("{uri}: no such note")))
    }
  }
}

/// `limit` + `untilId` paging over an array of items whose `id` is the cursor.
pub fn paged(rows: &[Value], size: usize) -> Option<String> {
  (rows.len() >= size)
    .then(|| rows.last().and_then(|r| r.str("id")))
    .flatten()
}

fn page_body(page: &PageReq, max: usize) -> (Value, usize) {
  let size = page.size_within(max);
  let mut body = json!({"limit": size});
  if let Some(c) = &page.cursor {
    body["untilId"] = json!(c);
  }
  (body, size)
}

async fn notes(api: &Api, endpoint: &str, mut body: Value, page: &PageReq) -> Result<Page<Post>> {
  let (paging, size) = page_body(page, 100);
  if let (Some(b), Some(p)) = (body.as_object_mut(), paging.as_object()) {
    b.extend(p.clone());
  }
  let v = api.call(endpoint, body).await?;
  let rows = v.as_array().cloned().unwrap_or_default();
  let instance = api.instance();
  let items = rows.iter().map(|n| parse::note(n, &instance)).collect();
  Ok(Page::new(items, paged(&rows, size)))
}

pub async fn timeline(api: &Api, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let kind = kind.unwrap_or(if api.logged_in() { "home" } else { "local" });
  let endpoint = match kind {
    "home" => {
      api.require()?;
      "notes/timeline"
    }
    "local" => "notes/local-timeline",
    "hybrid" => "notes/hybrid-timeline",
    "global" => "notes/global-timeline",
    other => return Err(Error::input(format!("unknown timeline: {other}"))),
  };
  notes(api, endpoint, json!({}), page).await
}

/// Featured notes (`notes/featured`).
pub async fn featured(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  notes(api, "notes/featured", json!({}), page).await
}

pub async fn show(api: &Api, id: &str) -> Result<Value> {
  api.call("notes/show", json!({"noteId": id})).await
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let id = note_id(api, arg).await?;
  Ok(parse::note(&show(api, &id).await?, &api.instance()))
}

/// Direct replies of a note, as comments.
pub async fn replies(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Comment>> {
  let id = note_id(api, arg).await?;
  let (mut body, size) = page_body(page, 100);
  body["noteId"] = json!(id);
  let v = api.call("notes/replies", body).await?;
  let rows = v.as_array().cloned().unwrap_or_default();
  let instance = api.instance();
  let items = rows.iter().map(|n| parse::comment(n, &instance)).collect();
  Ok(Page::new(items, paged(&rows, size)))
}

/// Who reacted to a note (each reaction in `extra.reaction`).
pub async fn reactions(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  let id = note_id(api, arg).await?;
  let (mut body, size) = page_body(page, 100);
  body["noteId"] = json!(id);
  let v = api.call("notes/reactions", body).await?;
  let rows = v.as_array().cloned().unwrap_or_default();
  let instance = api.instance();
  let items = rows
    .iter()
    .map(|r| {
      let mut u = parse::user(r.at("user"), &instance);
      if let Some(t) = r.str("type") {
        u.extra.insert("reaction".into(), json!(t));
      }
      u
    })
    .collect();
  Ok(Page::new(items, paged(&rows, size)))
}

pub async fn renotes(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let id = note_id(api, arg).await?;
  notes(api, "notes/renotes", json!({"noteId": id}), page).await
}

pub async fn search(api: &Api, q: &Query, page: &PageReq) -> Result<Page<Post>> {
  let mut body = json!({"query": q.keyword});
  if q.filter.as_deref() == Some("local") {
    body["host"] = json!(".");
  }
  if let Some(tag) = q.keyword.strip_prefix('#').filter(|t| !t.contains(' ')) {
    return notes(api, "notes/search-by-tag", json!({"tag": tag}), page).await;
  }
  notes(api, "notes/search", body, page).await
}

pub async fn by_tag(api: &Api, tag: &str, page: &PageReq) -> Result<Page<Post>> {
  let tag = tag.trim_start_matches('#');
  notes(api, "notes/search-by-tag", json!({"tag": tag}), page).await
}

pub async fn mentions(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  api.require()?;
  notes(api, "notes/mentions", json!({}), page).await
}

/// Hashtags in use now (`hashtags/trend`).
pub async fn trends(api: &Api) -> Result<Page<Collection>> {
  let v = api.call("hashtags/trend", json!({})).await?;
  let items = v.as_array().map(|r| r.iter().map(parse::hashtag).collect());
  Ok(Page::last(items.unwrap_or_default()))
}

pub async fn search_tags(api: &Api, q: &Query, page: &PageReq) -> Result<Page<Collection>> {
  let size = page.size_within(100);
  let offset: usize = page
    .cursor
    .as_deref()
    .and_then(|c| c.parse().ok())
    .unwrap_or(0);
  let v = api
    .call(
      "hashtags/search",
      json!({"query": q.keyword.trim_start_matches('#'), "limit": size, "offset": offset}),
    )
    .await?;
  let rows = v.as_array().cloned().unwrap_or_default();
  let next = (rows.len() >= size).then(|| (offset + rows.len()).to_string());
  Ok(Page::new(rows.iter().map(parse::hashtag).collect(), next))
}

pub async fn favorites(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  let (body, size) = page_body(page, 100);
  let v = api.call_auth("i/favorites", body).await?;
  let rows = v.as_array().cloned().unwrap_or_default();
  let instance = api.instance();
  let items = rows
    .iter()
    .map(|f| parse::note(f.at("note"), &instance))
    .collect();
  Ok(Page::new(items, paged(&rows, size)))
}

// ── writing ─────────────────────────────────────────────────────────────

/// What `notes/create` takes besides the text.
#[derive(Default)]
pub struct NoteOpts {
  pub cw: Option<String>,
  pub visibility: Option<String>,
  pub local_only: bool,
  pub reply_to: Option<String>,
  pub quote: Option<String>,
  pub files: Vec<PathBuf>,
}

pub async fn create(api: &Api, text: &str, opts: &NoteOpts) -> Result<Action> {
  api.require()?;
  let mut body = json!({});
  if !text.is_empty() {
    body["text"] = json!(text);
  }
  if let Some(cw) = &opts.cw {
    body["cw"] = json!(cw);
  }
  if let Some(v) = &opts.visibility {
    body["visibility"] = json!(v);
  }
  if opts.local_only {
    body["localOnly"] = json!(true);
  }
  let action = if let Some(r) = &opts.reply_to {
    body["replyId"] = json!(note_id(api, r).await?);
    "reply"
  } else if let Some(q) = &opts.quote {
    body["renoteId"] = json!(note_id(api, q).await?);
    "quote"
  } else {
    "publish"
  };
  let mut ids = Vec::new();
  for file in &opts.files {
    ids.push(upload(api, file).await?);
  }
  if !ids.is_empty() {
    body["fileIds"] = json!(ids);
  }
  let v = api.write("notes/create", body).await?;
  let id = v.str("createdNote.id").unwrap_or_default();
  Ok(
    Action::done(action, &id)
      .with_id(&id)
      .with_url(parse::note_url(&api.instance(), &id)),
  )
}

/// `post`: the title becomes the content warning, topics hashtags.
pub async fn publish(api: &Api, draft: &Draft) -> Result<Action> {
  let mut text = draft.text.clone();
  for t in &draft.topics {
    text.push_str(&format!(" #{}", t.trim_start_matches('#')));
  }
  let opts = NoteOpts {
    cw: draft.title.clone(),
    reply_to: draft.reply_to.clone(),
    quote: draft.quote.clone(),
    files: draft.images.clone(),
    ..NoteOpts::default()
  };
  create(api, text.trim(), &opts).await
}

pub async fn upload(api: &Api, path: &std::path::Path) -> Result<String> {
  let data = tokio::fs::read(path)
    .await
    .map_err(|e| Error::input(format!("cannot read {}: {e}", path.display())))?;
  let name = path
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or("file")
    .to_owned();
  let mime = match sniff(&data) {
    Some("jpg") => "image/jpeg",
    Some("png") => "image/png",
    Some("gif") => "image/gif",
    Some("webp") => "image/webp",
    Some("mp4") => "video/mp4",
    _ => "application/octet-stream",
  };
  api.upload(&name, data, mime).await
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let id = note_id(api, arg).await?;
  api.write("notes/delete", json!({"noteId": id})).await?;
  Ok(Action::done("delete", &id))
}

pub async fn react(api: &Api, arg: &str, reaction: &str, undo: bool) -> Result<Action> {
  let id = note_id(api, arg).await?;
  if undo {
    api
      .write("notes/reactions/delete", json!({"noteId": id}))
      .await?;
  } else {
    api
      .write(
        "notes/reactions/create",
        json!({"noteId": id, "reaction": reaction.trim()}),
      )
      .await?;
  }
  Ok(
    Action::done(if undo { "unreact" } else { "react" }, &id)
      .with_url(parse::note_url(&api.instance(), &id))
      .with_message(reaction.trim()),
  )
}

pub async fn favorite(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let id = note_id(api, arg).await?;
  let endpoint = if undo {
    "notes/favorites/delete"
  } else {
    "notes/favorites/create"
  };
  api.write(endpoint, json!({"noteId": id})).await?;
  Ok(Action::done(
    if undo { "unfavorite" } else { "favorite" },
    &id,
  ))
}

pub async fn renote(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let id = note_id(api, arg).await?;
  if undo {
    api.write("notes/unrenote", json!({"noteId": id})).await?;
    return Ok(Action::done("unrenote", &id));
  }
  let v = api.write("notes/create", json!({"renoteId": id})).await?;
  let new = v.str("createdNote.id").unwrap_or_default();
  Ok(
    Action::done("renote", &id)
      .with_id(&new)
      .with_url(parse::note_url(&api.instance(), &new)),
  )
}
