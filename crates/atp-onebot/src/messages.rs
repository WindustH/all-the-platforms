//! Messages: history (through the common extensions), one message, sending,
//! recalling and emoji reactions.

use atp_core::chat::message_id;
use atp_core::{Action, Error, Message, Page, PageReq, Post, Result, ValueExt, json};

use crate::api::Api;
use crate::refs::{self, Chat, num};
use crate::{parse, uploads};

/// History through `get_group_msg_history` / `get_friend_msg_history`:
/// pages go back from a message's sequence.
pub async fn list(api: &Api, chat: &str, page: &PageReq) -> Result<Page<Post>> {
  let chat = refs::chat(chat)?;
  let count = page.size_within(50);
  let (action, mut params) = match &chat {
    Chat::Group(g) => (
      "get_group_msg_history",
      json!({"group_id": num(g), "count": count, "reverseOrder": false}),
    ),
    Chat::Private(u) => (
      "get_friend_msg_history",
      json!({"user_id": num(u), "count": count, "reverseOrder": false}),
    ),
  };
  if let Some(seq) = &page.cursor {
    params["message_seq"] = num(seq);
  }
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
  let message = refs::message(arg)?;
  let v = api
    .call("get_msg", json!({"message_id": num(&message.id)}))
    .await?;
  Ok(parse::message(&v, message.chat.as_ref()))
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
    segments.push(uploads::segment(path).await?);
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
  Ok(Action::done("send", chat.id()).with_id(message_id(&chat.id(), &id)))
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let message = refs::message(arg)?;
  api
    .call("delete_msg", json!({"message_id": num(&message.id)}))
    .await?;
  Ok(Action::done("delete", arg))
}

/// Emoji reactions (`set_msg_emoji_like`, an extension). QQ names emoji by
/// number: system faces by their id, Unicode emoji by code point.
pub async fn react(api: &Api, arg: &str, emoji: &str, undo: bool) -> Result<Action> {
  let message = refs::message(arg)?;
  let emoji_id = if emoji.bytes().all(|b| b.is_ascii_digit()) {
    emoji.to_owned()
  } else {
    let c = emoji
      .chars()
      .next()
      .ok_or_else(|| Error::input("give an emoji or a QQ face id"))?;
    u32::from(c).to_string()
  };
  let params = json!({"message_id": num(&message.id), "emoji_id": emoji_id, "set": !undo});
  api.call("set_msg_emoji_like", params).await?;
  Ok(Action::done(if undo { "unreact" } else { "react" }, arg))
}
