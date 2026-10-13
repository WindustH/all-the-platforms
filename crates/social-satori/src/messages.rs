//! Messages: a channel's history, one message, sending (text, quotes,
//! inline files), editing, deleting and reactions.

use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use social_core::chat;
use social_core::file::Upload;
use social_core::{Action, MediaKind, Message, Page, PageReq, Post, Result, ValueExt, json};

use crate::api::Api;
use crate::{chats, parse, refs};

/// Newest first, going back with the token for earlier messages.
pub async fn list(api: &Api, chat: &str, page: &PageReq) -> Result<Page<Post>> {
  let id = chats::channel_id(api, chat).await?;
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
  let name = chats::channel_name(api, &id).await;
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
  let name = chats::channel_name(api, &r.channel).await;
  Ok(parse::message(&v, &r.channel, name.as_deref()))
}

pub async fn send(api: &Api, chat: &str, message: &Message) -> Result<Action> {
  let id = chats::channel_id(api, chat).await?;
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
    action = action.with_id(chat::message_id(&id, &m));
  }
  Ok(action)
}

/// A local file as an inline (`data:`) resource element.
async fn element(path: &Path) -> Result<String> {
  let file = Upload::read(path).await?;
  // QQ voice notes (`.silk`) go as audio, as the platforms behind expect.
  let silk = file.name.to_ascii_lowercase().ends_with(".silk");
  let (tag, mime) = match file.kind() {
    _ if silk => ("audio", "audio/mpeg"),
    MediaKind::Image | MediaKind::Gif => ("img", file.mime),
    MediaKind::Video => ("video", file.mime),
    MediaKind::Audio => ("audio", file.mime),
    MediaKind::File => ("file", file.mime),
  };
  Ok(format!(
    "<{tag} src=\"data:{mime};base64,{}\" title=\"{}\"/>",
    STANDARD.encode(&file.data),
    parse::escape(&file.name)
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
