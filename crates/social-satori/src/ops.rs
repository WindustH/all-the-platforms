//! Satori resources as the shared commands.

use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use social_core::{
  Action, Collection, Error, Message, Page, PageReq, Post, Result, User, Value, ValueExt, json,
};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, Chat};

pub async fn whoami(api: &Api) -> Result<User> {
  let v = api.call("login.get", json!({})).await?;
  let mut u = parse::user(v.at("user"));
  for key in ["platform", "status", "adapter"] {
    if let Some(val) = v.str(key) {
      u.extra.insert(key.into(), json!(val));
    }
  }
  if let Some(f) = v.as_object().and_then(|o| o.get("features")) {
    u.extra.insert("features".into(), f.clone());
  }
  Ok(u)
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let id = arg.trim().trim_start_matches("user:");
  Ok(parse::user(
    &api.call("user.get", json!({"user_id": id})).await?,
  ))
}

/// `{data, next}` (and `prev` for two-way lists).
fn list(v: &Value) -> (Vec<Value>, Option<String>) {
  (v.list("data").to_vec(), v.str("next"))
}

pub async fn chats(api: &Api, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
  let mut body = json!({});
  if let Some(n) = &page.cursor {
    body["next"] = json!(n);
  }
  match parent.map(refs::chat).transpose()? {
    None => {
      let (rows, next) = list(&api.call("guild.list", body).await?);
      Ok(Page::new(rows.iter().map(parse::guild).collect(), next))
    }
    Some(Chat::Guild(g)) => {
      body["guild_id"] = json!(g);
      let (rows, next) = list(&api.call("channel.list", body).await?);
      Ok(Page::new(rows.iter().map(parse::channel).collect(), next))
    }
    Some(_) => Err(Error::input(
      "give a server (guild:<id>) to list its channels",
    )),
  }
}

pub async fn members(api: &Api, chat: &str, page: &PageReq) -> Result<Page<User>> {
  let guild = match refs::chat(chat)? {
    Chat::Guild(g) => g,
    Chat::Channel(c) => api
      .call("channel.get", json!({"channel_id": c}))
      .await?
      .str("guild_id")
      .ok_or_else(|| Error::input("give a server (guild:<id>)"))?,
    Chat::User(_) => return Err(Error::input("give a server (guild:<id>)")),
  };
  let mut body = json!({"guild_id": guild});
  if let Some(n) = &page.cursor {
    body["next"] = json!(n);
  }
  let (rows, next) = list(&api.call("guild.member.list", body).await?);
  Ok(Page::new(rows.iter().map(parse::member).collect(), next))
}

/// The channel id of a chat argument; direct chats are opened first.
async fn channel(api: &Api, chat: &str) -> Result<String> {
  match refs::chat(chat)? {
    Chat::Channel(c) => Ok(c),
    Chat::User(u) => api
      .call("user.channel.create", json!({"user_id": u}))
      .await?
      .str("id")
      .ok_or_else(|| Error::upstream("the server returned no direct channel")),
    Chat::Guild(_) => Err(Error::input(
      "a server has channels; list them with `chats guild:<id>`",
    )),
  }
}

async fn channel_name(api: &Api, id: &str) -> Option<String> {
  api
    .call("channel.get", json!({"channel_id": id}))
    .await
    .ok()
    .and_then(|v| v.str("name"))
}

/// Newest first, going back with the token for earlier messages.
pub async fn messages(api: &Api, chat: &str, page: &PageReq) -> Result<Page<Post>> {
  let id = channel(api, chat).await?;
  let mut body = json!({
    "channel_id": id,
    "direction": "before",
    "limit": page.size_within(100),
    "order": "desc",
  });
  if let Some(n) = &page.cursor {
    body["next"] = json!(n);
  }
  let v = api.call("message.list", body).await?;
  let name = channel_name(api, &id).await;
  let mut rows = v.list("data").to_vec();
  rows.sort_by_key(|m| std::cmp::Reverse(m.i64("created_at").unwrap_or_default()));
  // Two-way lists name the earlier side `prev`; one-way lists only `next`.
  let older = v.str("prev").or_else(|| v.str("next"));
  Ok(Page::new(
    rows
      .iter()
      .map(|m| parse::message(m, &id, name.as_deref()))
      .collect(),
    older,
  ))
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let r = refs::message(arg)?;
  let v = api
    .call(
      "message.get",
      json!({"channel_id": r.channel, "message_id": r.id}),
    )
    .await?;
  let name = channel_name(api, &r.channel).await;
  Ok(parse::message(&v, &r.channel, name.as_deref()))
}

pub async fn send(api: &Api, chat: &str, message: &Message) -> Result<Action> {
  let id = channel(api, chat).await?;
  let mut content = String::new();
  if let Some(r) = &message.reply_to {
    let r = refs::message(r)?;
    content.push_str(&format!("<quote id=\"{}\"/>", parse::escape(&r.id)));
  }
  content.push_str(&parse::escape(&message.text));
  for file in &message.files {
    content.push_str(&element(file).await?);
  }
  let v = api
    .call(
      "message.create",
      json!({"channel_id": id, "content": content}),
    )
    .await?;
  let first = v
    .as_array()
    .and_then(|a| a.first())
    .and_then(|m| m.str("id"));
  let mut action = Action::done("send", &id);
  if let Some(m) = first {
    action = action.with_id(format!("{id}/{m}"));
  }
  Ok(action)
}

/// A local file as an inline (`data:`) resource element.
async fn element(path: &Path) -> Result<String> {
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
  let (tag, mime) = if let Some(img) = social_core::file::sniff(&data) {
    (
      "img",
      format!("image/{}", if img == "jpg" { "jpeg" } else { img }),
    )
  } else {
    match ext.as_str() {
      "mp4" | "mov" | "webm" | "mkv" => ("video", "video/mp4".to_owned()),
      "mp3" | "wav" | "ogg" | "m4a" | "flac" | "amr" | "silk" => ("audio", "audio/mpeg".to_owned()),
      _ => ("file", "application/octet-stream".to_owned()),
    }
  };
  Ok(format!(
    "<{tag} src=\"data:{mime};base64,{}\" title=\"{}\"/>",
    STANDARD.encode(&data),
    parse::escape(&name)
  ))
}

pub async fn edit(api: &Api, arg: &str, text: &str) -> Result<Action> {
  let r = refs::message(arg)?;
  api
    .call(
      "message.update",
      json!({"channel_id": r.channel, "message_id": r.id, "content": parse::escape(text)}),
    )
    .await?;
  Ok(Action::done("edit", arg))
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let r = refs::message(arg)?;
  api
    .call(
      "message.delete",
      json!({"channel_id": r.channel, "message_id": r.id}),
    )
    .await?;
  Ok(Action::done("delete", arg))
}

pub async fn react(api: &Api, arg: &str, emoji: &str, undo: bool) -> Result<Action> {
  let r = refs::message(arg)?;
  let method = if undo {
    "reaction.delete"
  } else {
    "reaction.create"
  };
  api
    .call(
      method,
      json!({"channel_id": r.channel, "message_id": r.id, "emoji": emoji}),
    )
    .await?;
  Ok(Action::done(if undo { "unreact" } else { "react" }, arg))
}
