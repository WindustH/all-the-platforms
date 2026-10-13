//! Messages: history, one message, sending (text, Markdown and files),
//! edits, reactions and deletions.

use social_core::chat;
use social_core::http::Method;
use social_core::{
  Action, Error, Media, Message, Page, PageReq, Post, Result, Value, ValueExt, json,
};

use crate::api::{Api, Body, Content};
use crate::refs::{self, ChatRef};
use crate::{chats, parse, uploads, users};

/// The emoji VoceChat accepts as reactions.
pub const EMOJI: &[&str] = &["❤️", "😄", "👀", "👍", "👎", "🎉", "🙁", "🚀"];

pub async fn history(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  api.require_user("messages")?;
  let chat = refs::chat(arg)?;
  history_of(api, chat, page.cursor.as_deref(), page.size_within(100)).await
}

/// Up to `size` messages before message `before`, newest first, with edits,
/// likes and deletions folded into the messages they touch.
async fn history_of(
  api: &Api,
  chat: ChatRef,
  before: Option<&str>,
  size: usize,
) -> Result<Page<Post>> {
  let server = api.server().unwrap_or_default();
  let mut path = format!("{}/history?limit={size}", chat.path());
  if let Some(b) = before {
    path.push_str(&format!("&before={b}"));
  }
  let rows = api
    .get(&path)
    .await?
    .as_array()
    .cloned()
    .unwrap_or_default();
  let users = users::all(api).await?;
  let name = chats::name(api, chat).await.unwrap_or_default();
  let key = chat.key();
  // The server lists oldest first; show newest first like other platforms.
  let mut posts: Vec<Post> = rows
    .iter()
    .map(|m| parse::message(m, &key, &name, &users, &server))
    .collect();
  posts.reverse();
  let next = (rows.len() >= size)
    .then(|| {
      rows
        .first()
        .and_then(|m| m.i64("mid"))
        .map(|m| m.to_string())
    })
    .flatten();
  Ok(Page::new(parse::fold(posts), next))
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  api.require_user("read")?;
  let (chat, mid) = refs::message_id(arg)?;
  let page = history_of(api, chat, Some(&(mid + 1).to_string()), 1).await?;
  page
    .items
    .into_iter()
    .find(|p| p.extra.get("mid").and_then(Value::as_i64) == Some(mid))
    .ok_or_else(|| Error::not_found(format!("no message {mid} in {}", chat.key())))
}

pub async fn media(api: &Api, arg: &str) -> Result<(Post, Vec<Media>)> {
  let post = read(api, arg).await?;
  if post.media.is_empty() {
    return Err(Error::not_found("the message has no image, video or audio"));
  }
  let media = post.media.clone();
  Ok((post, media))
}

/// Post one piece of content; its message id.
async fn post_content(
  api: &Api,
  chat: ChatRef,
  reply: Option<i64>,
  content: Content,
) -> Result<i64> {
  let path = match (reply, chat, api.is_bot()) {
    (Some(_), _, true) => {
      return Err(Error::unsupported("send --reply-to").with_hint("VoceChat bots cannot reply"));
    }
    (Some(mid), _, false) => format!("/message/{mid}/reply"),
    (None, ChatRef::Group(g), true) => format!("/bot/send_to_group/{g}"),
    (None, ChatRef::User(u), true) => format!("/bot/send_to_user/{u}"),
    (None, c, false) => format!("{}/send", c.path()),
  };
  let v = api.write(Method::POST, &path, content.into()).await?;
  Ok(v.as_i64().unwrap_or_default())
}

/// Send the text (as Markdown when `markdown`), then each file as its own
/// message; the reply goes on the first one.
pub async fn send(api: &Api, arg: &str, message: &Message, markdown: bool) -> Result<Action> {
  api.require()?;
  let chat = refs::chat(arg)?;
  let reply = match &message.reply_to {
    Some(r) => Some(refs::message_id(r)?.1),
    None => None,
  };
  let mut first = None;
  if !message.text.is_empty() {
    let content = if markdown {
      Content::Markdown(message.text.clone())
    } else {
      Content::Text(message.text.clone())
    };
    first = Some(post_content(api, chat, reply, content).await?);
  }
  for file in &message.files {
    let path = uploads::upload(api, file).await?;
    let reply = if first.is_none() { reply } else { None };
    let mid = post_content(api, chat, reply, Content::File(path)).await?;
    first.get_or_insert(mid);
  }
  let verb = if reply.is_some() { "reply" } else { "send" };
  let id = chat::message_id(&chat.key(), &first.unwrap_or_default().to_string());
  Ok(Action::done(verb, chat.key()).with_id(id))
}

pub async fn edit(api: &Api, arg: &str, text: &str) -> Result<Action> {
  api.require_user("edit")?;
  let (chat, mid) = refs::message_id(arg)?;
  api
    .call(
      Method::PUT,
      &format!("/message/{mid}/edit"),
      Content::Text(text.to_owned()).into(),
    )
    .await?;
  Ok(Action::done(
    "edit",
    chat::message_id(&chat.key(), &mid.to_string()),
  ))
}

pub async fn react(api: &Api, arg: &str, emoji: &str, undo: bool) -> Result<Action> {
  api.require_user("react")?;
  let (chat, mid) = refs::message_id(arg)?;
  let emoji = match emoji.trim() {
    "❤" => "❤️",
    e => e,
  };
  if !EMOJI.contains(&emoji) {
    return Err(Error::input(format!(
      "VoceChat takes these reactions: {}",
      EMOJI.join(" ")
    )));
  }
  // The same reaction again takes it back.
  api
    .call(
      Method::PUT,
      &format!("/message/{mid}/like"),
      Body::Json(json!({"action": emoji})),
    )
    .await?;
  let verb = if undo { "unreact" } else { "react" };
  Ok(Action::done(verb, chat::message_id(&chat.key(), &mid.to_string())).with_message(emoji))
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  api.require_user("delete")?;
  let (chat, mid) = refs::message_id(arg)?;
  api
    .call(Method::DELETE, &format!("/message/{mid}"), Body::None)
    .await?;
  Ok(Action::done(
    "delete",
    chat::message_id(&chat.key(), &mid.to_string()),
  ))
}
