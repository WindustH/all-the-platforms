//! Messages: history, one message, thread replies (as comments), sending
//! with files and in threads, editing, deleting, reactions and pins.

use std::path::{Path, PathBuf};

use social_core::chat;
use social_core::file::{Upload, sniff};
use social_core::text::emoji_shortcode;
use social_core::{
  Action, Comment, Error, Media, Message, Page, PageReq, Post, Result, Value, ValueExt, json,
};

use crate::api::Api;
use crate::chats;
use crate::parse::{self, View};
use crate::refs::{self, MessageRef};
use crate::users;

/// Turn message payloads of `channel` into posts, names resolved first.
pub async fn posts(api: &Api, rows: &[Value], channel: &str) -> Result<Vec<Post>> {
  prepare(api, rows, channel).await;
  render(api, |view| {
    rows
      .iter()
      .map(|m| parse::message(m, channel, view))
      .collect()
  })
  .await
}

async fn prepare(api: &Api, rows: &[Value], channel: &str) {
  users::resolve(api, parse::user_ids(rows)).await;
  chats::info(api, channel).await;
}

/// Run `f` with what parsing needs.
pub async fn render<T>(api: &Api, f: impl FnOnce(&View) -> T) -> Result<T> {
  let team_url = api.team_url().await;
  let token = api.require()?;
  let users = api.users.borrow();
  let chats = api.chats.borrow();
  Ok(f(&View {
    team_url: team_url.as_deref(),
    token: &token,
    users: &users,
    chats: &chats,
  }))
}

/// Messages of a conversation, newest first.
pub async fn list(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let channel = chats::id(api, arg).await?;
  let mut params = vec![
    ("channel", channel.clone()),
    ("limit", page.size_within(200).to_string()),
  ];
  if let Some(c) = &page.cursor {
    params.push(("cursor", c.clone()));
  }
  let v = api.call("conversations.history", params).await?;
  let rows = v.list("messages").to_vec();
  let next = (v.bool("has_more") == Some(true))
    .then(|| v.str("response_metadata.next_cursor"))
    .flatten()
    .filter(|c| !c.is_empty());
  Ok(Page::new(posts(api, &rows, &channel).await?, next))
}

/// One message's payload: from the channel history, or from its thread.
pub async fn payload(api: &Api, r: &MessageRef) -> Result<Value> {
  let find = |v: &Value| {
    v.list("messages")
      .iter()
      .find(|m| m.str("ts").as_deref() == Some(r.ts.as_str()))
      .cloned()
  };
  if let Some(t) = r.thread_ts.as_deref().filter(|t| *t != r.ts) {
    let v = replies(api, &r.channel, t, None, 1000).await?;
    if let Some(m) = find(&v) {
      return Ok(m);
    }
  }
  let v = api
    .call(
      "conversations.history",
      vec![
        ("channel", r.channel.clone()),
        ("latest", r.ts.clone()),
        ("oldest", r.ts.clone()),
        ("inclusive", "true".into()),
        ("limit", "1".into()),
      ],
    )
    .await?;
  if let Some(m) = find(&v) {
    return Ok(m);
  }
  // A thread reply is not in the history; its thread has it.
  let v = replies(api, &r.channel, &r.ts, None, 1000).await?;
  find(&v).ok_or_else(|| Error::not_found(format!("no message {} in {}", r.ts, r.channel)))
}

async fn replies(
  api: &Api,
  channel: &str,
  ts: &str,
  cursor: Option<&str>,
  limit: usize,
) -> Result<Value> {
  let mut params = vec![
    ("channel", channel.to_owned()),
    ("ts", ts.to_owned()),
    ("limit", limit.to_string()),
  ];
  if let Some(c) = cursor {
    params.push(("cursor", c.to_owned()));
  }
  api.call("conversations.replies", params).await
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let r = refs::message(arg)?;
  let v = payload(api, &r).await?;
  Ok(posts(api, &[v], &r.channel).await?.remove(0))
}

pub async fn media(api: &Api, arg: &str) -> Result<(Post, Vec<Media>)> {
  let post = read(api, arg).await?;
  if post.media.is_empty() {
    return Err(Error::not_found("the message has no files"));
  }
  let media = post.media.clone();
  Ok((post, media))
}

/// The replies in a message's thread, oldest first.
pub async fn thread(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Comment>> {
  let r = refs::message(arg)?;
  let v = replies(
    api,
    &r.channel,
    r.root(),
    page.cursor.as_deref(),
    page.size_within(200) + 1,
  )
  .await?;
  let rows: Vec<Value> = v
    .list("messages")
    .iter()
    .filter(|m| m.str("ts").as_deref() != Some(r.root()))
    .cloned()
    .collect();
  prepare(api, &rows, &r.channel).await;
  let items = render(api, |view| {
    rows
      .iter()
      .map(|m| parse::comment(m, &r.channel, view))
      .collect()
  })
  .await?;
  let next = (v.bool("has_more") == Some(true))
    .then(|| v.str("response_metadata.next_cursor"))
    .flatten()
    .filter(|c| !c.is_empty());
  Ok(Page::new(items, next))
}

// ── writing ─────────────────────────────────────────────────────────────

pub async fn send(api: &Api, arg: &str, message: &Message) -> Result<Action> {
  let channel = chats::id(api, arg).await?;
  let thread = match message.reply_to.as_deref() {
    Some(r) => {
      let r = refs::message(r)?;
      if r.channel != channel {
        return Err(Error::input(
          "--reply-to must be a message of the same conversation",
        ));
      }
      Some(r.root().to_owned())
    }
    None => None,
  };
  post(
    api,
    &channel,
    &message.text,
    thread.as_deref(),
    &message.files,
  )
  .await
}

/// `comment`: a reply in the message's thread (Slack threads are flat).
pub async fn reply(api: &Api, arg: &str, text: &str, files: &[PathBuf]) -> Result<Action> {
  let r = refs::message(arg)?;
  post(api, &r.channel, text, Some(r.root()), files).await
}

async fn post(
  api: &Api,
  channel: &str,
  text: &str,
  thread: Option<&str>,
  files: &[PathBuf],
) -> Result<Action> {
  if !files.is_empty() {
    let ids = upload(api, channel, text, thread, files).await?;
    return Ok(Action::done("send", channel).with_message(format!("uploaded {}", ids.join(", "))));
  }
  let mut params = vec![("channel", channel.to_owned()), ("text", text.to_owned())];
  if let Some(t) = thread {
    params.push(("thread_ts", t.to_owned()));
  }
  let v = api.write("chat.postMessage", params).await?;
  let ts = v.str("ts").unwrap_or_default();
  let channel = v.str("channel").unwrap_or_else(|| channel.to_owned());
  let mut action = Action::done("send", &channel).with_id(chat::message_id(&channel, &ts));
  if let Some(base) = api.team_url().await {
    action = action.with_url(parse::permalink(&base, &channel, &ts, thread));
  }
  Ok(action)
}

/// Upload files to a conversation (`files.getUploadURLExternal`, the bytes,
/// then `files.completeUploadExternal` with the text as its comment).
async fn upload(
  api: &Api,
  channel: &str,
  text: &str,
  thread: Option<&str>,
  files: &[PathBuf],
) -> Result<Vec<String>> {
  let mut done = Vec::new();
  for path in files {
    let data = Upload::read(path).await?.data;
    let name = file_name(path, &data);
    let v = api
      .write(
        "files.getUploadURLExternal",
        vec![
          ("filename", name.clone()),
          ("length", data.len().to_string()),
        ],
      )
      .await?;
    let (Some(url), Some(id)) = (v.str("upload_url"), v.str("file_id")) else {
      return Err(Error::upstream("no upload URL for the file"));
    };
    api.upload(&url, data).await?;
    done.push(json!({"id": id, "title": name}));
  }
  let mut params = vec![
    ("files", json!(done).to_string()),
    ("channel_id", channel.to_owned()),
  ];
  if !text.is_empty() {
    params.push(("initial_comment", text.to_owned()));
  }
  if let Some(t) = thread {
    params.push(("thread_ts", t.to_owned()));
  }
  let v = api.write("files.completeUploadExternal", params).await?;
  Ok(v.list("files").iter().filter_map(|f| f.str("id")).collect())
}

fn file_name(path: &Path, data: &[u8]) -> String {
  let name = path
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or("file")
    .to_owned();
  if Path::new(&name).extension().is_none()
    && let Some(ext) = sniff(data)
  {
    return format!("{name}.{ext}");
  }
  name
}

pub async fn edit(api: &Api, arg: &str, text: &str) -> Result<Action> {
  let r = refs::message(arg)?;
  api
    .write(
      "chat.update",
      vec![
        ("channel", r.channel.clone()),
        ("ts", r.ts.clone()),
        ("text", text.to_owned()),
      ],
    )
    .await?;
  Ok(Action::done("edit", r.id()))
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let r = refs::message(arg)?;
  api
    .write(
      "chat.delete",
      vec![("channel", r.channel.clone()), ("ts", r.ts.clone())],
    )
    .await?;
  Ok(Action::done("delete", r.id()))
}

/// Delete a reply of a thread: a message reference, or the bare `TS` of a
/// reply in the post's conversation.
pub async fn delete_comment(api: &Api, post: &str, comment: &str) -> Result<Action> {
  let comment = if comment.contains('/') || comment.starts_with("http") {
    comment.to_owned()
  } else {
    chat::message_id(&refs::message(post)?.channel, comment)
  };
  delete(api, &comment).await
}

pub async fn react(api: &Api, arg: &str, emoji: &str, undo: bool) -> Result<Action> {
  let r = refs::message(arg)?;
  let name = emoji_shortcode(emoji);
  let method = if undo {
    "reactions.remove"
  } else {
    "reactions.add"
  };
  api
    .write(
      method,
      vec![
        ("channel", r.channel.clone()),
        ("timestamp", r.ts.clone()),
        ("name", name.clone()),
      ],
    )
    .await?;
  let action = if undo { "unreact" } else { "react" };
  Ok(Action::done(action, r.id()).with_message(format!(":{name}:")))
}

pub async fn pins(api: &Api, arg: &str) -> Result<Page<Post>> {
  let channel = chats::id(api, arg).await?;
  let v = api
    .call("pins.list", vec![("channel", channel.clone())])
    .await?;
  let rows: Vec<Value> = v
    .list("items")
    .iter()
    .filter(|i| i.at("message").is_object())
    .map(|i| i.at("message").clone())
    .collect();
  Ok(Page::last(posts(api, &rows, &channel).await?))
}

pub async fn pin(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let r = refs::message(arg)?;
  let method = if undo { "pins.remove" } else { "pins.add" };
  api
    .write(
      method,
      vec![("channel", r.channel.clone()), ("timestamp", r.ts.clone())],
    )
    .await?;
  Ok(Action::done(if undo { "unpin" } else { "pin" }, r.id()))
}
