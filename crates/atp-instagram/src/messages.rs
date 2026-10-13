//! Direct messages (the logged-in web session): threads as chats, their
//! items as messages, sending text and reactions. A message id is
//! `THREAD/ITEM`.

use atp_core::{Action, Collection, Error, Message, Page, PageReq, Post, Result, ValueExt, chat};

use crate::api::{Api, with_cursor};
use crate::parse;

pub async fn chats(api: &Api, req: &PageReq) -> Result<Page<Collection>> {
  api.require_session("direct messages")?;
  let v = api
    .inbox(req.cursor.as_deref(), req.size_within(20))
    .await?;
  let inbox = v.at("inbox");
  let chats = inbox
    .list("threads")
    .iter()
    .map(parse::messages::chat)
    .collect();
  let next = (inbox.bool("has_older") == Some(true))
    .then(|| inbox.str("oldest_cursor"))
    .flatten();
  Ok(Page::new(chats, next))
}

pub async fn messages(api: &Api, chat: &str, req: &PageReq) -> Result<Page<Post>> {
  api.require_session("direct messages")?;
  let query = with_cursor(
    vec![("limit", req.size_within(20).to_string())],
    "cursor",
    req.cursor.as_ref(),
  );
  let v = api
    .get(&format!("direct_v2/threads/{chat}/"), &query)
    .await?;
  let thread = v.at("thread");
  let members = parse::messages::members(thread);
  let messages = thread
    .list("items")
    .iter()
    .map(|item| parse::messages::message(chat, item, &members))
    .collect();
  let next = (thread.bool("has_older") == Some(true))
    .then(|| thread.str("oldest_cursor"))
    .flatten();
  Ok(Page::new(messages, next))
}

/// A client message id, as the web app makes one.
fn client_context() -> String {
  format!("{}", rand::random::<u64>() >> 1)
}

pub async fn send(api: &Api, chat: &str, message: &Message) -> Result<Action> {
  api.require_session("sending messages")?;
  if !message.files.is_empty() {
    return Err(Error::unsupported("send --attach on Instagram"));
  }
  let context = client_context();
  let mut form = vec![
    ("action", "send_item".to_owned()),
    ("thread_ids", format!("[{chat}]")),
    ("text", message.text.clone()),
    ("client_context", context.clone()),
    ("mutation_token", context),
  ];
  if let Some(reply_to) = &message.reply_to {
    let item = chat::split_message_id(reply_to).map_or(reply_to.as_str(), |(_, item)| item);
    form.push(("replied_to_item_id", item.to_owned()));
  }
  let v = api.post("direct_v2/threads/broadcast/text/", &form).await?;
  let id = v.str("payload.item_id").unwrap_or_default();
  Ok(Action::done("send", chat).with_id(chat::message_id(chat, &id)))
}

pub async fn react(api: &Api, message: &str, emoji: &str, undo: bool) -> Result<Action> {
  api.require_session("reactions")?;
  let (chat, item) = chat::split_message_id(message)
    .ok_or_else(|| Error::input("a message is THREAD/ITEM (from `messages`)"))?;
  let context = client_context();
  let status = if undo { "deleted" } else { "created" };
  let form = vec![
    ("thread_ids", format!("[{chat}]")),
    ("item_id", item.to_owned()),
    ("node_type", "item".to_owned()),
    ("reaction_type", "like".to_owned()),
    ("reaction_status", status.to_owned()),
    ("emoji", emoji.to_owned()),
    ("client_context", context.clone()),
    ("mutation_token", context),
  ];
  api
    .post("direct_v2/threads/broadcast/reaction/", &form)
    .await?;
  Ok(Action::done(
    if undo { "unreact" } else { "react" },
    message,
  ))
}
