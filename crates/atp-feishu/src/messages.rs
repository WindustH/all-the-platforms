//! Messages: history, one message, sending (text and files, replies),
//! editing, recalling, forwarding and read receipts.

use atp_core::{
  Action, Error, Media, Message, Page, PageReq, Post, Result, User, Value, ValueExt, json,
};

use crate::api::{Api, next_page, page_query};
use crate::{chats, parse, refs, uploads, users};

pub async fn list(api: &Api, chat: &str, req: &PageReq) -> Result<Page<Post>> {
  let (kind, id) = refs::container(chat)?;
  let query = page_query(
    vec![
      ("container_id_type", kind.to_owned()),
      ("container_id", id.clone()),
      ("sort_type", "ByCreateTimeDesc".to_owned()),
    ],
    req,
    50,
  );
  let v = api.get("/im/v1/messages", &query).await?;
  let base = api.base();
  let mut posts: Vec<Post> = v
    .list("data.items")
    .iter()
    .map(|m| parse::message(m, &base))
    .collect();
  if kind == "chat" {
    chats::name(api, &id).await;
  }
  users::names(api, &mut posts).await;
  Ok(Page::new(posts, next_page(&v)))
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let id = refs::message(arg)?;
  let v = api.get(&format!("/im/v1/messages/{id}"), &[]).await?;
  let item = v
    .list("data.items")
    .first()
    .cloned()
    .ok_or_else(|| Error::not_found(format!("no message {id}")))?;
  let mut posts = vec![parse::message(&item, &api.base())];
  if let Some(chat) = item.str("chat_id") {
    chats::name(api, &chat).await;
  }
  users::names(api, &mut posts).await;
  Ok(posts.remove(0))
}

/// The message and its files, with the app token the resource endpoint wants.
pub async fn media(api: &Api, arg: &str) -> Result<(Post, Vec<Media>)> {
  let post = read(api, arg).await?;
  let bearer = api.bearer().await?;
  let media = post
    .media
    .iter()
    .cloned()
    .map(|mut m| {
      m.headers = vec![("authorization".into(), bearer.clone())];
      m
    })
    .collect();
  Ok((post, media))
}

/// Text first (as a reply when asked), then one message per file.
pub async fn send(api: &Api, chat: &str, message: &Message) -> Result<Action> {
  let reply_to = message.reply_to.as_deref().map(refs::message).transpose()?;
  let mut ids = Vec::new();
  if !message.text.is_empty() {
    let content = json!({ "text": message.text });
    ids.push(deliver(api, chat, reply_to.as_deref(), "text", &content).await?);
  }
  for file in &message.files {
    let (msg_type, content) = uploads::upload(api, file).await?;
    let to = if ids.is_empty() {
      reply_to.as_deref()
    } else {
      None
    };
    ids.push(deliver(api, chat, to, msg_type, &content).await?);
  }
  let mut action = Action::done(if reply_to.is_some() { "reply" } else { "send" }, chat);
  if let Some(first) = ids.first() {
    action = action.with_id(first.clone());
  }
  if ids.len() > 1 {
    action = action.with_message(format!("{} messages: {}", ids.len(), ids.join(", ")));
  }
  Ok(action)
}

async fn deliver(
  api: &Api,
  chat: &str,
  reply_to: Option<&str>,
  msg_type: &str,
  content: &Value,
) -> Result<String> {
  let uuid = format!("{:032x}", rand::random::<u128>());
  let v = match reply_to {
    Some(parent) => {
      let body = json!({ "msg_type": msg_type, "content": content.to_string(), "uuid": uuid });
      api
        .post(&format!("/im/v1/messages/{parent}/reply"), &[], body)
        .await?
    }
    None => {
      let (kind, id) = refs::receiver(chat);
      let body = json!({
        "receive_id": id,
        "msg_type": msg_type,
        "content": content.to_string(),
        "uuid": uuid,
      });
      let query = [("receive_id_type", kind.to_owned())];
      api.post("/im/v1/messages", &query, body).await?
    }
  };
  Ok(v.str("data.message_id").unwrap_or_default())
}

/// Replace the text of a text message the bot sent.
pub async fn edit(api: &Api, arg: &str, text: &str) -> Result<Action> {
  let id = refs::message(arg)?;
  api
    .put(
      &format!("/im/v1/messages/{id}"),
      json!({ "msg_type": "text", "content": json!({ "text": text }).to_string() }),
    )
    .await?;
  Ok(Action::done("edit", &id).with_id(id.clone()))
}

/// Recall a message (the bot's own, or any in a group it manages).
pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let id = refs::message(arg)?;
  api.delete(&format!("/im/v1/messages/{id}")).await?;
  Ok(Action::done("delete", &id))
}

pub async fn forward(api: &Api, arg: &str, to: &str) -> Result<Action> {
  let id = refs::message(arg)?;
  let (kind, target) = refs::receiver(to);
  let v = api
    .post(
      &format!("/im/v1/messages/{id}/forward"),
      &[("receive_id_type", kind.to_owned())],
      json!({ "receive_id": target }),
    )
    .await?;
  let mut action = Action::done("forward", &id);
  if let Some(new) = v.str("data.message_id") {
    action = action.with_id(new);
  }
  Ok(action)
}

/// Who read a message the bot sent.
pub async fn readers(api: &Api, arg: &str, req: &PageReq) -> Result<Page<User>> {
  let id = refs::message(arg)?;
  let query = page_query(vec![("user_id_type", "open_id".to_owned())], req, 100);
  let v = api
    .get(&format!("/im/v1/messages/{id}/read_users"), &query)
    .await?;
  let users = v
    .list("data.items")
    .iter()
    .map(|u| User {
      id: u.str("user_id").unwrap_or_default(),
      extra: u
        .str("timestamp")
        .map(|t| ("read_at".to_owned(), Value::from(t)))
        .into_iter()
        .collect(),
      ..User::default()
    })
    .collect();
  Ok(Page::new(users, next_page(&v)))
}
