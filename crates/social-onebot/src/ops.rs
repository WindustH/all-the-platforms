//! Chats, members, messages and writes as OneBot actions.

use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use social_core::{
  Action, Collection, Error, Message, Page, PageReq, Post, Result, User, Value, ValueExt, json,
};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, Chat, num};

pub async fn whoami(api: &Api) -> Result<User> {
  let v = api.call("get_login_info", json!({})).await?;
  Ok(parse::user(&v))
}

/// Groups, then friends. The lists come whole; paging is local.
pub async fn chats(api: &Api, page: &PageReq) -> Result<Page<Collection>> {
  let offset: usize = page.number_or(0) as usize;
  let groups = api.call("get_group_list", json!({})).await?;
  let friends = api.call("get_friend_list", json!({})).await?;
  let all: Vec<Collection> = list(&groups)
    .iter()
    .map(parse::group)
    .chain(list(&friends).iter().map(parse::friend))
    .collect();
  Ok(slice(all, offset, page.size_within(200)))
}

pub async fn members(api: &Api, chat: &str, page: &PageReq) -> Result<Page<User>> {
  let Chat::Group(group) = refs::chat(chat)? else {
    return Err(Error::input(
      "members are listed for groups (group:<number>)",
    ));
  };
  let v = api
    .call("get_group_member_list", json!({"group_id": num(&group)}))
    .await?;
  let users: Vec<User> = list(&v).iter().map(parse::user).collect();
  Ok(slice(
    users,
    page.number_or(0) as usize,
    page.size_within(500),
  ))
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let id = refs::user(arg)?;
  let v = api
    .call("get_stranger_info", json!({"user_id": num(&id)}))
    .await?;
  Ok(parse::user(&v))
}

/// History through the common extensions (`get_group_msg_history`,
/// `get_friend_msg_history`): pages go back from a message's sequence.
pub async fn messages(api: &Api, chat: &str, page: &PageReq) -> Result<Page<Post>> {
  let chat = refs::chat(chat)?;
  let count = page.size_within(50);
  let mut params = match &chat {
    Chat::Group(g) => json!({"group_id": num(g), "count": count, "reverseOrder": false}),
    Chat::Private(u) => json!({"user_id": num(u), "count": count, "reverseOrder": false}),
  };
  if let Some(seq) = &page.cursor {
    params["message_seq"] = num(seq);
  }
  let action = match chat {
    Chat::Group(_) => "get_group_msg_history",
    Chat::Private(_) => "get_friend_msg_history",
  };
  let v = api.call(action, params).await?;
  let rows = v.list("messages");
  let mut posts: Vec<Post> = rows
    .iter()
    .map(|m| parse::message(m, Some(&chat)))
    .collect();
  // Oldest first upstream; newest first here. The anchor message comes back
  // with the page it starts, so a page of just the anchor is the end.
  posts.reverse();
  let oldest = rows
    .first()
    .and_then(|m| m.first_str(&["message_seq", "real_seq", "message_id"]));
  let next = match (&page.cursor, oldest) {
    (Some(c), Some(o)) if *c == o => None,
    (_, o) if rows.len() >= 2 => o,
    _ => None,
  };
  Ok(Page::new(posts, next))
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let r = refs::message(arg)?;
  let v = api
    .call("get_msg", json!({"message_id": num(&r.id)}))
    .await?;
  Ok(parse::message(&v, r.chat.as_ref()))
}

pub async fn send(api: &Api, chat: &str, message: &Message) -> Result<Action> {
  let chat = refs::chat(chat)?;
  let mut segments = Vec::new();
  if let Some(r) = &message.reply_to {
    let r = refs::message(r)?;
    segments.push(json!({"type": "reply", "data": {"id": r.id}}));
  }
  if !message.text.is_empty() {
    segments.push(json!({"type": "text", "data": {"text": message.text}}));
  }
  for path in &message.files {
    segments.push(file_segment(path).await?);
  }
  let (action, params) = match &chat {
    Chat::Group(g) => (
      "send_group_msg",
      json!({"group_id": num(g), "message": segments}),
    ),
    Chat::Private(u) => (
      "send_private_msg",
      json!({"user_id": num(u), "message": segments}),
    ),
  };
  let v = api.call(action, params).await?;
  let id = v.str("message_id").unwrap_or_default();
  Ok(Action::done("send", chat.id()).with_id(format!("{}/{id}", chat.id())))
}

/// A local file as a segment, sent inline (`base64://`) so the
/// implementation need not see the path.
async fn file_segment(path: &Path) -> Result<Value> {
  let data = tokio::fs::read(path)
    .await
    .map_err(|e| Error::input(format!("cannot read {}: {e}", path.display())))?;
  let name = path
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or("file")
    .to_owned();
  let file = format!("base64://{}", STANDARD.encode(&data));
  let ext = path
    .extension()
    .and_then(|e| e.to_str())
    .unwrap_or_default()
    .to_ascii_lowercase();
  let kind = if social_core::file::sniff(&data).is_some() {
    "image"
  } else {
    match ext.as_str() {
      "mp4" | "mov" | "webm" | "mkv" | "avi" => "video",
      "mp3" | "amr" | "silk" | "wav" | "ogg" | "m4a" | "flac" | "aac" => "record",
      _ => "file",
    }
  };
  Ok(match kind {
    "file" => json!({"type": "file", "data": {"file": file, "name": name}}),
    k => json!({"type": k, "data": {"file": file}}),
  })
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let r = refs::message(arg)?;
  api
    .call("delete_msg", json!({"message_id": num(&r.id)}))
    .await?;
  Ok(Action::done("delete", arg))
}

/// Emoji reactions (`set_msg_emoji_like`, an extension). QQ names emoji by
/// number: system faces by their id, Unicode emoji by code point.
pub async fn react(api: &Api, arg: &str, emoji: &str, undo: bool) -> Result<Action> {
  let r = refs::message(arg)?;
  let emoji_id = if emoji.bytes().all(|b| b.is_ascii_digit()) {
    emoji.to_owned()
  } else {
    let c = emoji
      .chars()
      .next()
      .ok_or_else(|| Error::input("give an emoji or a QQ face id"))?;
    u32::from(c).to_string()
  };
  api
    .call(
      "set_msg_emoji_like",
      json!({"message_id": num(&r.id), "emoji_id": emoji_id, "set": !undo}),
    )
    .await?;
  Ok(Action::done(if undo { "unreact" } else { "react" }, arg))
}

fn list(v: &Value) -> Vec<Value> {
  v.as_array().cloned().unwrap_or_default()
}

/// One page of a list the implementation returns whole.
fn slice<T>(items: Vec<T>, offset: usize, size: usize) -> Page<T> {
  let total = items.len();
  let end = (offset + size).min(total);
  let page: Vec<T> = items.into_iter().skip(offset).take(size).collect();
  Page::new(page, (end < total).then(|| end.to_string()))
}
