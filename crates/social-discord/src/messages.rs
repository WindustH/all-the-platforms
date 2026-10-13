//! Messages: history, one message, sending (with files and replies),
//! editing, deleting, reactions, pins, and a message's thread as comments.

use std::path::{Path, PathBuf};

use social_core::file::sniff;
use social_core::http::Part;
use social_core::{
  Action, Comment, Error, ErrorCode, Media, Message, Page, PageReq, Post, Result, Value, ValueExt,
  json,
};

use crate::api::{Api, ChannelInfo};
use crate::chats;
use crate::parse;
use crate::refs::{self, MessageRef};

/// Longest message a regular account may send.
const MAX_CHARS: usize = 2000;

/// The channel a chat argument names, with where it lives; a server id gets
/// a pointer to its channels.
async fn channel(api: &Api, arg: &str) -> Result<(String, ChannelInfo)> {
  let id = chats::channel_id(api, arg).await?;
  let info = api.channel(&id).await.map_err(|e| {
    if e.code == ErrorCode::NotFound {
      e.with_hint(format!(
        "if {id} is a server, `social discord chats {id}` lists its channels"
      ))
    } else {
      e
    }
  })?;
  Ok((id, info))
}

/// Messages of a channel, newest first, 100 a page (`before=` paging).
pub async fn list(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let (id, info) = channel(api, arg).await?;
  history(api, &id, &info, page).await
}

async fn history(api: &Api, id: &str, info: &ChannelInfo, page: &PageReq) -> Result<Page<Post>> {
  let size = page.size_within(100);
  let mut query = vec![("limit", size.to_string())];
  if let Some(before) = &page.cursor {
    query.push(("before", before.clone()));
  }
  let v = api.get(&format!("/channels/{id}/messages"), &query).await?;
  let rows = v.as_array().cloned().unwrap_or_default();
  let next = (rows.len() >= size)
    .then(|| rows.last().and_then(|m| m.str("id")))
    .flatten();
  let items = rows
    .iter()
    .map(|m| parse::message(m, Some(&info.guild), Some(&info.name)))
    .collect();
  Ok(Page::new(items, next))
}

/// One message (via `around=`, which bots and user accounts both may use).
pub async fn fetch(api: &Api, r: &MessageRef) -> Result<(Value, ChannelInfo)> {
  let info = api.channel(&r.channel).await?;
  let v = api
    .get(
      &format!("/channels/{}/messages", r.channel),
      &[("around", r.message.clone()), ("limit", "5".into())],
    )
    .await?;
  let found = v
    .as_array()
    .and_then(|rows| {
      rows
        .iter()
        .find(|m| m.str("id").as_deref() == Some(r.message.as_str()))
    })
    .cloned()
    .ok_or_else(|| Error::not_found(format!("no message {} in {}", r.message, info.name)))?;
  Ok((found, info))
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let r = refs::message(arg)?;
  let (v, info) = fetch(api, &r).await?;
  Ok(parse::message(&v, Some(&info.guild), Some(&info.name)))
}

pub async fn media(api: &Api, arg: &str) -> Result<(Post, Vec<Media>)> {
  let post = read(api, arg).await?;
  let media = post.media.clone();
  if media.is_empty() {
    let err = Error::not_found("the message has no images, video or audio");
    return Err(if post.extra.contains_key("files") {
      err
        .with_hint("its other files are linked in `extra.files` (`social discord read ... --json`)")
    } else {
      err
    });
  }
  Ok((post, media))
}

// ── sending ─────────────────────────────────────────────────────────────

pub async fn send(api: &Api, arg: &str, message: &Message) -> Result<Action> {
  let (id, info) = channel(api, arg).await?;
  let reply = message.reply_to.as_deref().map(refs::message).transpose()?;
  post_message(
    api,
    &id,
    &info,
    &message.text,
    reply.as_ref(),
    &message.files,
  )
  .await
}

/// Answer a message in its own channel (`comment`): `reply_to` (another
/// message there) is answered instead of the message itself when given.
pub async fn reply(
  api: &Api,
  post: &str,
  text: &str,
  reply_to: Option<&str>,
  files: &[PathBuf],
) -> Result<Action> {
  let target = match reply_to {
    Some(r) => in_channel(post, r)?,
    None => refs::message(post)?,
  };
  let info = api.channel(&target.channel).await?;
  post_message(api, &target.channel, &info, text, Some(&target), files).await
}

/// A comment argument: a full reference, or a bare message id in the post's channel.
fn in_channel(post: &str, comment: &str) -> Result<MessageRef> {
  match refs::message(comment) {
    Ok(r) => Ok(r),
    Err(_) if refs::is_snowflake(comment.trim()) => {
      let p = refs::message(post)?;
      Ok(MessageRef {
        channel: p.channel,
        message: comment.trim().to_owned(),
        guild: p.guild,
      })
    }
    Err(e) => Err(e),
  }
}

async fn post_message(
  api: &Api,
  channel: &str,
  info: &ChannelInfo,
  text: &str,
  reply: Option<&MessageRef>,
  files: &[PathBuf],
) -> Result<Action> {
  if text.chars().count() > MAX_CHARS && !api.is_bot() {
    return Err(Error::input(format!(
      "the message is longer than {MAX_CHARS} characters; split it into several"
    )));
  }
  let mut body = json!({"content": text, "tts": false, "flags": 0});
  if !api.is_bot() {
    body["nonce"] = json!(parse::snowflake_at(jiff::Timestamp::now()));
  }
  if let Some(r) = reply {
    body["message_reference"] = json!({
      "message_id": r.message,
      "channel_id": r.channel,
      "fail_if_not_exists": false,
    });
  }
  let path = format!("/channels/{channel}/messages");
  let v = if files.is_empty() {
    api.post(&path, body).await?
  } else {
    let mut parts = Vec::new();
    let mut attachments = Vec::new();
    for (i, file) in files.iter().enumerate() {
      let (name, data, mime) = read_file(file).await?;
      attachments.push(json!({"id": i, "filename": name}));
      parts.push(Part::file(&format!("files[{i}]"), data, &name, &mime));
    }
    body["attachments"] = json!(attachments);
    let mut payload = Part::text("payload_json", body.to_string());
    payload.mime = Some("application/json".into());
    parts.insert(0, payload);
    api.post_multipart(&path, parts).await?
  };
  let mid = v.str("id").unwrap_or_default();
  let action = if reply.is_some() { "reply" } else { "send" };
  Ok(
    Action::done(action, &info.name)
      .with_id(format!("{channel}/{mid}"))
      .with_url(parse::permalink(&info.guild, channel, Some(&mid))),
  )
}

async fn read_file(path: &Path) -> Result<(String, Vec<u8>, String)> {
  let data = tokio::fs::read(path)
    .await
    .map_err(|e| Error::input(format!("cannot read {}: {e}", path.display())))?;
  let name = path
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or("file")
    .to_owned();
  let mime = match sniff(&data) {
    Some("jpg") => "image/jpeg",
    Some("png") => "image/png",
    Some("gif") => "image/gif",
    Some("webp") => "image/webp",
    Some("mp4") => "video/mp4",
    _ => "application/octet-stream",
  };
  Ok((name, data, mime.to_owned()))
}

pub async fn edit(api: &Api, arg: &str, text: &str) -> Result<Action> {
  let r = refs::message(arg)?;
  if text.chars().count() > MAX_CHARS && !api.is_bot() {
    return Err(Error::input(format!(
      "the text is longer than {MAX_CHARS} characters"
    )));
  }
  let v = api
    .patch(
      &format!("/channels/{}/messages/{}", r.channel, r.message),
      json!({"content": text}),
    )
    .await?;
  let info = api.channel(&r.channel).await?;
  Ok(
    Action::done("edit", format!("{}/{}", r.channel, r.message)).with_url(parse::permalink(
      &info.guild,
      &r.channel,
      v.str("id").as_deref(),
    )),
  )
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let r = refs::message(arg)?;
  api
    .delete(&format!("/channels/{}/messages/{}", r.channel, r.message))
    .await?;
  Ok(Action::done(
    "delete",
    format!("{}/{}", r.channel, r.message),
  ))
}

/// Delete a comment: a full reference, or a message id in the post's channel.
pub async fn delete_comment(api: &Api, post: &str, comment: &str) -> Result<Action> {
  let r = in_channel(post, comment)?;
  delete(api, &format!("{}/{}", r.channel, r.message)).await
}

// ── reactions and pins ──────────────────────────────────────────────────

/// `👍`, `name:id`, `<:name:id>` or `<a:name:id>` → the path segment Discord takes.
fn emoji_path(emoji: &str) -> Result<String> {
  let e = emoji.trim();
  let e = e
    .strip_prefix("<a:")
    .or_else(|| e.strip_prefix("<:"))
    .and_then(|s| s.strip_suffix('>'))
    .unwrap_or(e);
  if e.is_empty() {
    return Err(Error::input("give an emoji"));
  }
  if e.starts_with(':') && e.ends_with(':') && e.len() > 2 {
    return Err(Error::input(format!(
      "{e}: custom emoji need their id (`name:id`); `social discord emojis SERVER` lists them"
    )));
  }
  Ok(url::form_urlencoded::byte_serialize(e.as_bytes()).collect())
}

pub async fn react(api: &Api, arg: &str, emoji: &str, undo: bool) -> Result<Action> {
  let r = refs::message(arg)?;
  let path = format!(
    "/channels/{}/messages/{}/reactions/{}/@me",
    r.channel,
    r.message,
    emoji_path(emoji)?
  );
  if undo {
    api.delete(&path).await?;
  } else {
    api.put(&path).await?;
  }
  Ok(
    Action::done(
      if undo { "unreact" } else { "react" },
      format!("{}/{}", r.channel, r.message),
    )
    .with_message(emoji.trim()),
  )
}

/// Pinned messages of a channel, newest pin first.
pub async fn pins(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let (id, info) = channel(api, arg).await?;
  let mut query = vec![("limit", page.size_within(50).to_string())];
  if let Some(before) = &page.cursor {
    query.push(("before", before.clone()));
  }
  let v = match api
    .get(&format!("/channels/{id}/messages/pins"), &query)
    .await
  {
    Ok(v) => v,
    // Older API: every pin at once.
    Err(e) if e.code == ErrorCode::NotFound => {
      let all = api.get(&format!("/channels/{id}/pins"), &[]).await?;
      let items = all
        .as_array()
        .map(|rows| {
          rows
            .iter()
            .map(|m| parse::message(m, Some(&info.guild), Some(&info.name)))
            .collect()
        })
        .unwrap_or_default();
      return Ok(Page::last(items));
    }
    Err(e) => return Err(e),
  };
  let rows = v.list("items");
  let next = v
    .bool("has_more")
    .unwrap_or(false)
    .then(|| rows.last().and_then(|p| p.str("pinned_at")))
    .flatten();
  let items = rows
    .iter()
    .map(|p| {
      let mut post = parse::message(p.at("message"), Some(&info.guild), Some(&info.name));
      if let Some(at) = p.str("pinned_at") {
        post.extra.insert("pinned_at".into(), json!(at));
      }
      post
    })
    .collect();
  Ok(Page::new(items, next))
}

pub async fn pin(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let r = refs::message(arg)?;
  let new = format!("/channels/{}/messages/pins/{}", r.channel, r.message);
  let old = format!("/channels/{}/pins/{}", r.channel, r.message);
  let call = async |path: &str| {
    if undo {
      api.delete(path).await
    } else {
      api.put(path).await
    }
  };
  match call(&new).await {
    Err(e) if e.code == ErrorCode::NotFound => {
      call(&old).await?;
    }
    other => {
      other?;
    }
  }
  Ok(Action::done(
    if undo { "unpin" } else { "pin" },
    format!("{}/{}", r.channel, r.message),
  ))
}

// ── threads as comments ─────────────────────────────────────────────────

/// The thread started from a message (or the forum post a message opens),
/// newest message first.
pub async fn thread(api: &Api, post: &str, page: &PageReq) -> Result<Page<Comment>> {
  let r = refs::message(post)?;
  let (v, info) = fetch(api, &r).await?;
  // A forum post's thread has the id of its first message.
  let thread = v.str("thread.id").or_else(|| {
    matches!(info.kind, 10..=12)
      .then(|| r.channel.clone())
      .filter(|c| *c == r.message)
  });
  let Some(thread) = thread else {
    return Ok(Page::last(Vec::new()));
  };
  let tinfo = api.channel(&thread).await?;
  let posts = history(api, &thread, &tinfo, page).await?;
  let items = posts
    .items
    .into_iter()
    .filter(|p| p.extra.get("message_id").and_then(Value::as_str) != Some(r.message.as_str()))
    .map(comment)
    .collect();
  Ok(Page::new(items, posts.next_cursor))
}

fn comment(p: Post) -> Comment {
  let reply_to = p
    .quoted
    .as_ref()
    .and_then(|q| q.author.as_ref())
    .map(|a| a.name.clone());
  Comment {
    id: p.id,
    author: p.author,
    text: p.text.unwrap_or_default(),
    created_at: p.created_at,
    likes: p.metrics.likes,
    reply_to,
    extra: p.extra,
    raw: p.raw,
    ..Comment::default()
  }
}
