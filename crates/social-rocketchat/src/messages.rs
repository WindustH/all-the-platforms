//! Messages: room history, one message, threads (as comments), sending
//! with files and in threads, editing, deleting, reactions, stars, pins,
//! and search within a room.

use std::collections::BTreeMap;
use std::path::Path;

use social_core::file::sniff;
use social_core::http::Part;
use social_core::text::emoji_shortcode;
use social_core::{
  Action, Comment, Error, ErrorCode, Media, Message, Page, PageReq, Post, Query, Result, User,
  Value, ValueExt, json,
};

use crate::api::Api;
use crate::parse;
use crate::refs;
use crate::rooms;

/// Messages as posts, their rooms known first.
pub async fn convert(api: &Api, rows: &[Value]) -> Result<Vec<Post>> {
  let mut rids: Vec<String> = rows.iter().filter_map(|m| m.str("rid")).collect();
  rids.sort();
  rids.dedup();
  for rid in rids {
    let _ = rooms::info(api, &rid).await;
  }
  rooms::render(api, |view| {
    rows.iter().map(|m| parse::message(m, view)).collect()
  })
}

/// A room's messages, newest first, paged by `latest=`.
pub async fn list(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let rid = rooms::id(api, arg).await?;
  let room = rooms::info(api, &rid).await?;
  let count = page.size_within(100);
  let mut query = vec![("roomId", rid), ("count", count.to_string())];
  if let Some(latest) = &page.cursor {
    query.push(("latest", latest.clone()));
  }
  let v = api
    .get(&format!("{}.history", rooms::family(&room.t)), &query)
    .await?;
  let rows = v.list("messages").to_vec();
  let next = (rows.len() >= count)
    .then(|| rows.last().and_then(|m| m.str("ts")))
    .flatten();
  Ok(Page::new(convert(api, &rows).await?, next))
}

async fn fetch(api: &Api, arg: &str) -> Result<Value> {
  let id = refs::message(arg)?;
  let v = api.get("chat.getMessage", &[("msgId", id)]).await?;
  Ok(v.at("message").clone())
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let v = fetch(api, arg).await?;
  Ok(convert(api, &[v]).await?.remove(0))
}

pub async fn media(api: &Api, arg: &str) -> Result<(Post, Vec<Media>)> {
  let post = read(api, arg).await?;
  if post.media.is_empty() {
    return Err(Error::not_found("the message has no files"));
  }
  let media = post.media.clone();
  Ok((post, media))
}

fn root_of(v: &Value) -> String {
  v.str("tmid").or_else(|| v.str("_id")).unwrap_or_default()
}

/// The replies of a message's thread, oldest first.
pub async fn thread(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Comment>> {
  let v = fetch(api, arg).await?;
  let offset = page.number_or(0);
  let count = page.size_within(100);
  let list = api
    .get(
      "chat.getThreadMessages",
      &[
        ("tmid", root_of(&v)),
        ("offset", offset.to_string()),
        ("count", count.to_string()),
      ],
    )
    .await?;
  let mut rows = list.list("messages").to_vec();
  rows.sort_by_key(|m| {
    parse::time(m.at("ts"))
      .map(|t| t.as_millisecond())
      .unwrap_or_default()
  });
  let _ = rooms::info(api, &v.str("rid").unwrap_or_default()).await;
  let items = rooms::render(api, |view| {
    rows.iter().map(|m| parse::comment(m, view)).collect()
  })?;
  let total = list.u64("total").unwrap_or_default();
  let seen = offset + rows.len() as u64;
  Ok(Page::new(items, (seen < total).then(|| seen.to_string())))
}

// ── writing ─────────────────────────────────────────────────────────────

pub async fn send(api: &Api, arg: &str, message: &Message) -> Result<Action> {
  let rid = rooms::id(api, arg).await?;
  let tmid = match message.reply_to.as_deref() {
    Some(r) => Some(root_of(&fetch(api, r).await?)),
    None => None,
  };
  create(api, &rid, &message.text, tmid.as_deref(), &message.files).await
}

/// `comment`: a reply in the message's thread.
pub async fn reply(
  api: &Api,
  arg: &str,
  text: &str,
  files: &[std::path::PathBuf],
) -> Result<Action> {
  let v = fetch(api, arg).await?;
  let rid = v.str("rid").unwrap_or_default();
  create(api, &rid, text, Some(&root_of(&v)), files).await
}

async fn create(
  api: &Api,
  rid: &str,
  text: &str,
  tmid: Option<&str>,
  files: &[std::path::PathBuf],
) -> Result<Action> {
  let mut sent = None;
  for (i, path) in files.iter().enumerate() {
    // The text goes with the first file.
    let caption = if i == 0 { text } else { "" };
    sent = Some(upload(api, rid, path, caption, tmid).await?);
  }
  let m = match sent {
    Some(m) if !files.is_empty() => m,
    _ => {
      let mut msg = json!({"rid": rid, "msg": text});
      if let Some(t) = tmid {
        msg["tmid"] = json!(t);
      }
      api
        .post("chat.sendMessage", json!({ "message": msg }))
        .await?
        .at("message")
        .clone()
    }
  };
  let id = m.str("_id").unwrap_or_default();
  let mut action = Action::done("send", rid);
  if !id.is_empty() {
    action = action.with_id(&id);
    if let Some(url) = convert(api, &[m]).await?.pop().and_then(|p| p.url) {
      action = action.with_url(url);
    }
  }
  Ok(action)
}

/// Upload a file: `rooms.media` + `rooms.mediaConfirm` (Rocket.Chat 6.8+),
/// or `rooms.upload` on older servers. Returns the message posted.
async fn upload(
  api: &Api,
  rid: &str,
  path: &Path,
  text: &str,
  tmid: Option<&str>,
) -> Result<Value> {
  let data = tokio::fs::read(path)
    .await
    .map_err(|e| Error::input(format!("cannot read {}: {e}", path.display())))?;
  let mut name = path
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or("file")
    .to_owned();
  if Path::new(&name).extension().is_none()
    && let Some(ext) = sniff(&data)
  {
    name = format!("{name}.{ext}");
  }
  let file = Part::file("file", data, &name, "application/octet-stream");
  match api
    .upload(&format!("rooms.media/{rid}"), vec![file.clone()])
    .await
  {
    Ok(v) => {
      let id = v
        .str("file._id")
        .ok_or_else(|| Error::upstream("the upload returned no file id"))?;
      let mut body = json!({"msg": text});
      if let Some(t) = tmid {
        body["tmid"] = json!(t);
      }
      let v = api
        .post(&format!("rooms.mediaConfirm/{rid}/{id}"), body)
        .await?;
      Ok(v.at("message").clone())
    }
    Err(e) if e.code == ErrorCode::NotFound => {
      let mut parts = vec![file, Part::text("msg", text)];
      if let Some(t) = tmid {
        parts.push(Part::text("tmid", t));
      }
      let v = api.upload(&format!("rooms.upload/{rid}"), parts).await?;
      Ok(v.at("message").clone())
    }
    Err(e) => Err(e),
  }
}

pub async fn edit(api: &Api, arg: &str, text: &str) -> Result<Action> {
  let v = fetch(api, arg).await?;
  let id = v.str("_id").unwrap_or_default();
  api
    .post(
      "chat.update",
      json!({"roomId": v.str("rid"), "msgId": id, "text": text}),
    )
    .await?;
  Ok(Action::done("edit", id))
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let v = fetch(api, arg).await?;
  let id = v.str("_id").unwrap_or_default();
  api
    .post("chat.delete", json!({"roomId": v.str("rid"), "msgId": id}))
    .await?;
  Ok(Action::done("delete", id))
}

pub async fn react(api: &Api, arg: &str, emoji: &str, undo: bool) -> Result<Action> {
  let id = refs::message(arg)?;
  let name = emoji_shortcode(emoji);
  api
    .post(
      "chat.react",
      json!({"messageId": id, "emoji": format!(":{name}:"), "shouldReact": !undo}),
    )
    .await?;
  let action = if undo { "unreact" } else { "react" };
  Ok(Action::done(action, id).with_message(format!(":{name}:")))
}

/// Star a message (`favorite`), or unstar it.
pub async fn star(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let id = refs::message(arg)?;
  let method = if undo {
    "chat.unStarMessage"
  } else {
    "chat.starMessage"
  };
  api.post(method, json!({"messageId": id})).await?;
  Ok(Action::done(if undo { "unstar" } else { "star" }, id))
}

pub async fn pins(api: &Api, arg: &str) -> Result<Page<Post>> {
  let rid = rooms::id(api, arg).await?;
  let v = api
    .get("chat.getPinnedMessages", &[("roomId", rid)])
    .await?;
  Ok(Page::last(convert(api, v.list("messages")).await?))
}

pub async fn pin(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let id = refs::message(arg)?;
  let method = if undo {
    "chat.unPinMessage"
  } else {
    "chat.pinMessage"
  };
  api.post(method, json!({"messageId": id})).await?;
  Ok(Action::done(if undo { "unpin" } else { "pin" }, id))
}

/// Search one room: `room:NAME` (or `in:NAME`) in the query names it.
pub async fn search(api: &Api, q: &Query, page: &PageReq) -> Result<Page<Post>> {
  let mut room = None;
  let words: Vec<&str> = q
    .keyword
    .split_whitespace()
    .filter(
      |w| match w.strip_prefix("room:").or_else(|| w.strip_prefix("in:")) {
        Some(r) => {
          room = Some(r.to_owned());
          false
        }
        None => true,
      },
    )
    .collect();
  let Some(room) = room else {
    return Err(
      Error::input("Rocket.Chat searches one room at a time")
        .with_hint("add `room:NAME` to the query, e.g. `social rc search 'deploy room:general'`"),
    );
  };
  let rid = rooms::id(api, &room).await?;
  let offset = page.number_or(0);
  let count = page.size_within(100);
  let v = api
    .get(
      "chat.search",
      &[
        ("roomId", rid),
        ("searchText", words.join(" ")),
        ("offset", offset.to_string()),
        ("count", count.to_string()),
      ],
    )
    .await?;
  let rows = v.list("messages").to_vec();
  let next = (rows.len() >= count).then(|| (offset + rows.len() as u64).to_string());
  Ok(Page::new(convert(api, &rows).await?, next))
}

pub async fn search_users(api: &Api, term: &str) -> Result<Page<User>> {
  let v = api
    .get(
      "spotlight",
      &[("query", format!("@{}", term.trim_start_matches('@')))],
    )
    .await?;
  let server = api.require()?.server;
  Ok(Page::last(
    v.list("users")
      .iter()
      .map(|u| parse::user(u, &server))
      .collect(),
  ))
}

/// Unread messages per room.
pub async fn unread(api: &Api) -> Result<BTreeMap<String, u64>> {
  let v = api.get("subscriptions.get", &[]).await?;
  let rows = v.list("update").to_vec();
  rooms::remember(api, &rows);
  let known = api.rooms.borrow();
  Ok(
    rows
      .iter()
      .filter_map(|s| {
        let n = s.u64("unread").filter(|n| *n > 0)?;
        let rid = s.str("rid")?;
        Some((known.get(&rid).map(|r| r.name.clone()).unwrap_or(rid), n))
      })
      .collect(),
  )
}
