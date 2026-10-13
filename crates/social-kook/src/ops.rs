//! Servers, channels, direct chats, messages and writes.

use std::path::Path;

use social_core::http::Part;
use social_core::{
  Action, Collection, Error, Message, Page, PageReq, Post, Result, User, Value, ValueExt, json,
};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, Chat};

pub async fn whoami(api: &Api) -> Result<User> {
  Ok(parse::user(&api.get("/user/me", &[]).await?))
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let id = refs::user(arg)?;
  Ok(parse::user(
    &api.get("/user/view", &[("user_id", id)]).await?,
  ))
}

/// The items of a numbered page and the next page number.
fn paged(v: &Value, page: u64) -> (Vec<Value>, Option<u64>) {
  let items = v.list("items").to_vec();
  let total = v.u64("meta.page_total").unwrap_or(page);
  (items, (page < total).then_some(page + 1))
}

/// Without a parent: servers, then direct chats. With one: its channels.
pub async fn chats(api: &Api, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
  let size = page.size_within(50).to_string();
  if let Some(parent) = parent {
    let guild = guild_of(api, &refs::chat(parent)?).await?;
    let n = page.number_or(1);
    let v = api
      .get(
        "/channel/list",
        &[
          ("guild_id", guild),
          ("page", n.to_string()),
          ("page_size", size),
        ],
      )
      .await?;
    let (items, next) = paged(&v, n);
    return Ok(Page::new(
      items.iter().map(parse::channel).collect(),
      next.map(|n| n.to_string()),
    ));
  }
  let cursor = page.cursor.clone().unwrap_or_else(|| "g:1".into());
  let (part, n) = cursor
    .split_once(':')
    .and_then(|(p, n)| Some((p.to_owned(), n.parse::<u64>().ok()?)))
    .ok_or_else(|| Error::input(format!("bad cursor {cursor}")))?;
  let query = [("page", n.to_string()), ("page_size", size)];
  if part == "g" {
    let v = api.get("/guild/list", &query).await?;
    let (items, next) = paged(&v, n);
    let next = Some(next.map_or_else(|| "d:1".to_owned(), |n| format!("g:{n}")));
    return Ok(Page::new(items.iter().map(parse::guild).collect(), next));
  }
  let v = api.get("/user-chat/list", &query).await?;
  let (items, next) = paged(&v, n);
  Ok(Page::new(
    items.iter().map(parse::direct).collect(),
    next.map(|n| format!("d:{n}")),
  ))
}

/// The server a chat belongs to.
async fn guild_of(api: &Api, chat: &Chat) -> Result<String> {
  match chat {
    Chat::Guild(g) => Ok(g.clone()),
    Chat::Channel(c) => {
      let v = api
        .get("/channel/view", &[("target_id", c.clone())])
        .await?;
      v.str("guild_id")
        .ok_or_else(|| Error::not_found(format!("channel {c} has no server")))
    }
    Chat::Direct(_) => Err(Error::input("direct chats have no server")),
  }
}

pub async fn members(api: &Api, chat: &str, page: &PageReq) -> Result<Page<User>> {
  let guild = guild_of(api, &refs::chat(chat)?).await?;
  let n = page.number_or(1);
  let v = api
    .get(
      "/guild/user-list",
      &[
        ("guild_id", guild),
        ("page", n.to_string()),
        ("page_size", page.size_within(50).to_string()),
      ],
    )
    .await?;
  let (items, next) = paged(&v, n);
  Ok(Page::new(
    items.iter().map(parse::user).collect(),
    next.map(|n| n.to_string()),
  ))
}

async fn chat_name(api: &Api, chat: &Chat) -> Option<String> {
  match chat {
    Chat::Channel(c) => api
      .get("/channel/view", &[("target_id", c.clone())])
      .await
      .ok()
      .and_then(|v| v.str("name"))
      .map(|n| format!("#{n}")),
    _ => None,
  }
}

/// Newest first, paging back from the oldest message of each page.
pub async fn messages(api: &Api, chat: &str, page: &PageReq) -> Result<Page<Post>> {
  let chat = refs::chat(chat)?;
  let size = page.size_within(100);
  let (path, target) = match &chat {
    Chat::Channel(c) => ("/message/list", c.clone()),
    Chat::Direct(u) => ("/direct-message/list", u.clone()),
    Chat::Guild(_) => {
      return Err(Error::input(
        "a server has channels, not messages; list them with `chats guild:<id>`",
      ));
    }
  };
  let mut query = vec![("target_id", target), ("page_size", size.to_string())];
  if let Some(before) = &page.cursor {
    query.push(("msg_id", before.clone()));
    query.push(("flag", "before".into()));
  }
  let v = api.get(path, &query).await?;
  let name = chat_name(api, &chat).await;
  let mut rows = v.list("items").to_vec();
  rows.sort_by_key(|m| std::cmp::Reverse(m.i64("create_at").unwrap_or_default()));
  let next = (rows.len() >= size)
    .then(|| rows.last().and_then(|m| m.str("id")))
    .flatten();
  Ok(Page::new(
    rows
      .iter()
      .map(|m| parse::message(m, &chat, name.as_deref()))
      .collect(),
    next,
  ))
}

/// A direct chat's code, which some direct-message calls take.
async fn chat_code(api: &Api, user: &str) -> Result<String> {
  let v = api
    .post("/user-chat/create", json!({"target_id": user}))
    .await?;
  v.str("code")
    .ok_or_else(|| Error::upstream("KOOK returned no chat code"))
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let r = refs::message(arg)?;
  match r.chat {
    Some(Chat::Direct(user)) => {
      let code = chat_code(api, &user).await?;
      let v = api
        .get(
          "/direct-message/view",
          &[("chat_code", code), ("msg_id", r.id.clone())],
        )
        .await?;
      let item = v.list("items").first().cloned().unwrap_or(v);
      Ok(parse::message(&item, &Chat::Direct(user), None))
    }
    _ => {
      let v = api
        .get("/message/view", &[("msg_id", r.id.clone())])
        .await?;
      let chat = Chat::Channel(v.str("channel_id").unwrap_or_default());
      let name = chat_name(api, &chat).await;
      Ok(parse::message(&v, &chat, name.as_deref()))
    }
  }
}

/// Text goes as KMarkdown (type 9, the kind KOOK lets you edit later);
/// each file is uploaded and sent as its own message.
pub async fn send(api: &Api, chat: &str, message: &Message) -> Result<Action> {
  let chat = refs::chat(chat)?;
  let (path, target) = match &chat {
    Chat::Channel(c) => ("/message/create", c.clone()),
    Chat::Direct(u) => ("/direct-message/create", u.clone()),
    Chat::Guild(_) => {
      return Err(Error::input(
        "send to a channel of the server (see `chats guild:<id>`)",
      ));
    }
  };
  let quote = message
    .reply_to
    .as_deref()
    .map(refs::message)
    .transpose()?
    .map(|r| r.id);
  let mut sent = Vec::new();
  let body = |kind: u64, content: String| {
    let mut b = json!({"type": kind, "target_id": target, "content": content});
    if let Some(q) = &quote {
      b["quote"] = json!(q);
    }
    b
  };
  if !message.text.is_empty() {
    let v = api.post(path, body(9, message.text.clone())).await?;
    sent.extend(v.str("msg_id"));
  }
  for file in &message.files {
    let (kind, url) = upload(api, file).await?;
    let v = api.post(path, body(kind, url)).await?;
    sent.extend(v.str("msg_id"));
  }
  let id = sent
    .first()
    .map(|m| format!("{}/{m}", chat.id()))
    .unwrap_or_default();
  Ok(Action::done("send", chat.id()).with_id(id))
}

/// Upload a file as an asset; its message type and URL.
async fn upload(api: &Api, path: &Path) -> Result<(u64, String)> {
  let data = tokio::fs::read(path)
    .await
    .map_err(|e| Error::input(format!("cannot read {}: {e}", path.display())))?;
  let name = path
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or("file")
    .to_owned();
  let ext = path
    .extension()
    .and_then(|e| e.to_str())
    .unwrap_or_default()
    .to_ascii_lowercase();
  let (kind, mime) = if let Some(img) = social_core::file::sniff(&data) {
    (
      2,
      format!("image/{}", if img == "jpg" { "jpeg" } else { img }),
    )
  } else {
    match ext.as_str() {
      "mp4" | "mov" | "webm" | "mkv" => (3, "video/mp4".to_owned()),
      "mp3" | "wav" | "ogg" | "m4a" | "flac" | "aac" => (8, "audio/mpeg".to_owned()),
      _ => (4, "application/octet-stream".to_owned()),
    }
  };
  let v = api
    .upload(vec![Part::file("file", data, &name, &mime)])
    .await?;
  let url = v
    .str("url")
    .ok_or_else(|| Error::upstream("KOOK returned no asset URL"))?;
  Ok((kind, url))
}

/// The channel or the direct-message variant of a write.
fn route(r: &refs::MessageRef, channel: &'static str, direct: &'static str) -> &'static str {
  if matches!(r.chat, Some(Chat::Direct(_))) {
    direct
  } else {
    channel
  }
}

pub async fn edit(api: &Api, arg: &str, text: &str) -> Result<Action> {
  let r = refs::message(arg)?;
  let path = route(&r, "/message/update", "/direct-message/update");
  api
    .post(path, json!({"msg_id": r.id, "content": text}))
    .await?;
  Ok(Action::done("edit", arg))
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let r = refs::message(arg)?;
  let path = route(&r, "/message/delete", "/direct-message/delete");
  api.post(path, json!({"msg_id": r.id})).await?;
  Ok(Action::done("delete", arg))
}

pub async fn react(api: &Api, arg: &str, emoji: &str, undo: bool) -> Result<Action> {
  let r = refs::message(arg)?;
  let path = if undo {
    route(
      &r,
      "/message/delete-reaction",
      "/direct-message/delete-reaction",
    )
  } else {
    route(&r, "/message/add-reaction", "/direct-message/add-reaction")
  };
  api
    .post(path, json!({"msg_id": r.id, "emoji": emoji}))
    .await?;
  Ok(Action::done(if undo { "unreact" } else { "react" }, arg))
}
