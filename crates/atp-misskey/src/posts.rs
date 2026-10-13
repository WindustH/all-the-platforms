//! Notes: timelines, featured notes, one note, replies, reactions, renotes,
//! favorites, writing and deleting. A chat message where a note is expected
//! goes to [`messages`].

use std::path::PathBuf;

use atp_core::{
  Action, Comment, Draft, Error, Page, PageReq, Post, Reply, Result, User, ValueExt, json,
};

use crate::api::Api;
use crate::messages;
use crate::parse;
use crate::refs::{self, NoteRef};

pub const FEED_KINDS: &[&str] = &["home", "local", "hybrid", "global"];
/// The reaction `like` sends (Misskey's like button).
pub const LIKE: &str = "❤";

/// A note argument → its id on this instance (links to other servers are
/// looked up with `ap/show`, which needs an account).
async fn note_id(api: &Api, arg: &str) -> Result<String> {
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
  api.list(endpoint, json!({}), page, parse::post).await
}

/// Featured notes (`notes/featured`).
pub async fn featured(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  api
    .list("notes/featured", json!({}), page, parse::post)
    .await
}

/// A note, or a chat message given as `<chat>/<message>`.
pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  if let Some(message) = refs::message(arg) {
    return messages::read(api, &message).await;
  }
  let id = note_id(api, arg).await?;
  let v = api.call("notes/show", json!({"noteId": id})).await?;
  Ok(parse::post(&v, &api.instance()))
}

/// Direct replies of a note, as comments.
pub async fn replies(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Comment>> {
  let id = note_id(api, arg).await?;
  let body = json!({"noteId": id});
  api.list("notes/replies", body, page, parse::comment).await
}

/// Who reacted to a note (each reaction in `extra.reaction`).
pub async fn reactions(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  let id = note_id(api, arg).await?;
  let body = json!({"noteId": id});
  api
    .list("notes/reactions", body, page, |row, instance| {
      let mut user = parse::user(row.at("user"), instance);
      if let Some(reaction) = row.str("type") {
        user.extra.insert("reaction".into(), json!(reaction));
      }
      user
    })
    .await
}

pub async fn renotes(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let id = note_id(api, arg).await?;
  let body = json!({"noteId": id});
  api.list("notes/renotes", body, page, parse::post).await
}

pub async fn mentions(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  api.require()?;
  api
    .list("notes/mentions", json!({}), page, parse::post)
    .await
}

/// Your own favorites; Misskey shows nobody else's.
pub async fn favorites(
  api: &Api,
  user: Option<&str>,
  folder: Option<&str>,
  page: &PageReq,
) -> Result<Page<Post>> {
  if user.is_some() || folder.is_some() {
    return Err(
      Error::unsupported("favorites USER / --folder")
        .with_hint("Misskey only shows your own favorites; clips are not supported yet"),
    );
  }
  api.require()?;
  api
    .list("i/favorites", json!({}), page, |row, instance| {
      parse::post(row.at("note"), instance)
    })
    .await
}

// ── writing ─────────────────────────────────────────────────────────────

/// What `notes/create` takes besides the text.
#[derive(Default)]
pub struct NoteOptions {
  pub cw: Option<String>,
  pub visibility: Option<String>,
  pub local_only: bool,
  pub reply_to: Option<String>,
  pub quote: Option<String>,
  pub files: Vec<PathBuf>,
}

pub async fn create(api: &Api, text: &str, options: &NoteOptions) -> Result<Action> {
  api.require()?;
  let mut body = json!({});
  if !text.is_empty() {
    body["text"] = json!(text);
  }
  if let Some(cw) = &options.cw {
    body["cw"] = json!(cw);
  }
  if let Some(visibility) = &options.visibility {
    body["visibility"] = json!(visibility);
  }
  if options.local_only {
    body["localOnly"] = json!(true);
  }
  let action = if let Some(reply_to) = &options.reply_to {
    body["replyId"] = json!(note_id(api, reply_to).await?);
    "reply"
  } else if let Some(quote) = &options.quote {
    body["renoteId"] = json!(note_id(api, quote).await?);
    "quote"
  } else {
    "publish"
  };
  let mut ids = Vec::new();
  for file in &options.files {
    ids.push(api.upload(file).await?);
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
  for topic in &draft.topics {
    text.push_str(&format!(" #{}", topic.trim_start_matches('#')));
  }
  let options = NoteOptions {
    cw: draft.title.clone(),
    reply_to: draft.reply_to.clone(),
    quote: draft.quote.clone(),
    files: draft.images.clone(),
    ..NoteOptions::default()
  };
  create(api, text.trim(), &options).await
}

/// A reply note to `post` (or to the comment it answers).
pub async fn comment(api: &Api, post: &str, reply: &Reply) -> Result<Action> {
  let options = NoteOptions {
    reply_to: Some(reply.reply_to.clone().unwrap_or_else(|| post.to_owned())),
    files: reply.images.clone(),
    ..NoteOptions::default()
  };
  create(api, reply.text.trim(), &options).await
}

/// A note, or a chat message given as `<chat>/<message>`.
pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  match refs::message(arg) {
    Some(message) => messages::delete(api, &message).await,
    None => delete_note(api, arg).await,
  }
}

pub async fn delete_note(api: &Api, arg: &str) -> Result<Action> {
  let id = note_id(api, arg).await?;
  api.write("notes/delete", json!({"noteId": id})).await?;
  Ok(Action::done("delete", &id))
}

pub async fn like(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  react(api, arg, LIKE, undo).await
}

/// React to a note, or to a chat message given as `<chat>/<message>`.
pub async fn react(api: &Api, arg: &str, reaction: &str, undo: bool) -> Result<Action> {
  if let Some(message) = refs::message(arg) {
    return messages::react(api, &message, reaction, undo).await;
  }
  let id = note_id(api, arg).await?;
  if undo {
    api
      .write("notes/reactions/delete", json!({"noteId": id}))
      .await?;
  } else {
    let body = json!({"noteId": id, "reaction": reaction.trim()});
    api.write("notes/reactions/create", body).await?;
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
  let renote = v.str("createdNote.id").unwrap_or_default();
  Ok(
    Action::done("renote", &id)
      .with_id(&renote)
      .with_url(parse::note_url(&api.instance(), &renote)),
  )
}
