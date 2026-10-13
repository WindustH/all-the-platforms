//! Messages: history, one message, sending (text and files, replies),
//! editing, recalling, reactions, pins and forwarding.

use std::path::Path;

use atp_core::file::sniff;
use atp_core::http::Part;
use atp_core::{Action, Error, Media, Message, Page, PageReq, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::{chats, parse, refs, users};

pub async fn list(api: &Api, chat: &str, req: &PageReq) -> Result<Page<Post>> {
  let (kind, id) = refs::container(chat)?;
  let mut query = vec![
    ("container_id_type", kind.to_owned()),
    ("container_id", id.clone()),
    ("sort_type", "ByCreateTimeDesc".to_owned()),
    ("page_size", req.size_within(50).to_string()),
  ];
  if let Some(c) = &req.cursor {
    query.push(("page_token", c.clone()));
  }
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
  Ok(Page::new(posts, chats::next(&v)))
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
    let (msg_type, content) = upload(api, file).await?;
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
      api
        .post(
          &format!("/im/v1/messages/{parent}/reply"),
          &[],
          json!({ "msg_type": msg_type, "content": content.to_string(), "uuid": uuid }),
        )
        .await?
    }
    None => {
      let (kind, id) = refs::receiver(chat);
      api
        .post(
          "/im/v1/messages",
          &[("receive_id_type", kind.to_owned())],
          json!({
            "receive_id": id,
            "msg_type": msg_type,
            "content": content.to_string(),
            "uuid": uuid,
          }),
        )
        .await?
    }
  };
  Ok(v.str("data.message_id").unwrap_or_default())
}

/// Upload a file for a message: images as `image`, opus audio as `audio`,
/// everything else as `file`.
async fn upload(api: &Api, path: &Path) -> Result<(&'static str, Value)> {
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
  let image = matches!(
    sniff(&data[..data.len().min(32)]),
    Some("jpg" | "png" | "gif" | "webp")
  ) || matches!(ext.as_str(), "bmp" | "ico" | "tiff" | "heic");
  if image {
    let v = api
      .upload(
        "/im/v1/images",
        vec![
          Part::text("image_type", "message"),
          Part::file("image", data, &name, "application/octet-stream"),
        ],
      )
      .await?;
    let key = v
      .str("data.image_key")
      .ok_or_else(|| Error::upstream("image upload returned no image_key"))?;
    return Ok(("image", json!({ "image_key": key })));
  }
  let file_type = match ext.as_str() {
    "opus" => "opus",
    "mp4" => "mp4",
    "pdf" => "pdf",
    "doc" | "docx" => "doc",
    "xls" | "xlsx" => "xls",
    "ppt" | "pptx" => "ppt",
    _ => "stream",
  };
  let v = api
    .upload(
      "/im/v1/files",
      vec![
        Part::text("file_type", file_type),
        Part::text("file_name", name.clone()),
        Part::file("file", data, &name, "application/octet-stream"),
      ],
    )
    .await?;
  let key = v
    .str("data.file_key")
    .ok_or_else(|| Error::upstream("file upload returned no file_key"))?;
  let msg_type = if file_type == "opus" { "audio" } else { "file" };
  Ok((msg_type, json!({ "file_key": key })))
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

/// Feishu names reactions (`THUMBSUP`, `SMILE` …); a few emoji are
/// translated, any other value is passed on as the name.
fn emoji_type(emoji: &str) -> String {
  match emoji.trim() {
    "👍" | "+1" | "like" => "THUMBSUP".into(),
    "👌" | "ok" => "OK".into(),
    "😄" | "😀" | "🙂" | "smile" => "SMILE".into(),
    other => other.to_ascii_uppercase(),
  }
}

pub async fn react(api: &Api, arg: &str, emoji: &str, undo: bool) -> Result<Action> {
  let id = refs::message(arg)?;
  let kind = emoji_type(emoji);
  if !undo {
    api
      .post(
        &format!("/im/v1/messages/{id}/reactions"),
        &[],
        json!({ "reaction_type": { "emoji_type": kind } }),
      )
      .await?;
    return Ok(Action::done("react", &id).with_message(kind));
  }
  let creds = api.require()?;
  let v = api
    .get(
      &format!("/im/v1/messages/{id}/reactions"),
      &[
        ("reaction_type", kind.clone()),
        ("page_size", "50".to_owned()),
      ],
    )
    .await?;
  let ours = v.list("data.items").iter().find(|r| {
    r.str("operator.operator_type").as_deref() == Some("app")
      && r
        .str("operator.operator_id")
        .is_none_or(|op| op == creds.app_id)
  });
  let Some(reaction) = ours.and_then(|r| r.str("reaction_id")) else {
    return Err(Error::not_found(format!(
      "the bot has no {kind} reaction on {id}"
    )));
  };
  api
    .delete(&format!("/im/v1/messages/{id}/reactions/{reaction}"))
    .await?;
  Ok(Action::done("unreact", &id).with_message(kind))
}

pub async fn pin(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let id = refs::message(arg)?;
  if undo {
    api.delete(&format!("/im/v1/pins/{id}")).await?;
    Ok(Action::done("unpin", &id))
  } else {
    api
      .post("/im/v1/pins", &[], json!({ "message_id": id }))
      .await?;
    Ok(Action::done("pin", &id))
  }
}

pub async fn pins(api: &Api, chat: &str, req: &PageReq) -> Result<Page<Post>> {
  let id = refs::chat(chat)?;
  let mut query = vec![
    ("chat_id", id),
    ("page_size", req.size_within(50).to_string()),
  ];
  if let Some(c) = &req.cursor {
    query.push(("page_token", c.clone()));
  }
  let v = api.get("/im/v1/pins", &query).await?;
  let mut posts = Vec::new();
  for p in v.list("data.items") {
    if let Some(mid) = p.str("message_id") {
      let mut post = read(api, &mid).await?;
      if let Some(by) = p.str("operator_id") {
        post.extra.insert("pinned_by".into(), by.into());
      }
      posts.push(post);
    }
  }
  Ok(Page::new(posts, chats::next(&v)))
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
pub async fn read_users(api: &Api, arg: &str, req: &PageReq) -> Result<Page<atp_core::User>> {
  let id = refs::message(arg)?;
  let mut query = vec![
    ("user_id_type", "open_id".to_owned()),
    ("page_size", req.size_within(100).to_string()),
  ];
  if let Some(c) = &req.cursor {
    query.push(("page_token", c.clone()));
  }
  let v = api
    .get(&format!("/im/v1/messages/{id}/read_users"), &query)
    .await?;
  let users = v
    .list("data.items")
    .iter()
    .map(|u| atp_core::User {
      id: u.str("user_id").unwrap_or_default(),
      extra: u
        .str("timestamp")
        .map(|t| ("read_at".to_owned(), Value::from(t)))
        .into_iter()
        .collect(),
      ..atp_core::User::default()
    })
    .collect();
  Ok(Page::new(users, chats::next(&v)))
}
