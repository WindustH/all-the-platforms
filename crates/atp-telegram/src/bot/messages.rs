//! Sending (text, a file, an album), editing, deleting, reactions, pins and
//! forwards.

use std::path::Path;

use atp_core::file::Upload;
use atp_core::http::Part;
use atp_core::{Action, Ctx, Message, Result, Value, ValueExt, json};

use super::parse::post;
use super::{Bot, chat_id, message_ref};
use crate::refs;
use crate::uploads::is_photo;

impl Bot {
  pub async fn send(&self, ctx: &Ctx, chat: &str, message: &Message) -> Result<Action> {
    let chat_ref = refs::chat(chat)?;
    let chat_id = chat_id(&chat_ref)?;
    let reply = match &message.reply_to {
      Some(r) => Some(json!({"message_id": refs::message(r)?.id})),
      None => None,
    };
    let sent = match message.files.as_slice() {
      [] => {
        let mut body = json!({"chat_id": chat_id, "text": message.text});
        if let Some(r) = reply {
          body["reply_parameters"] = r;
        }
        self.call(ctx, "sendMessage", body).await?
      }
      [file] => {
        self
          .send_file(ctx, &chat_id, file, &message.text, reply)
          .await?
      }
      files => {
        self
          .send_album(ctx, &chat_id, files, &message.text, reply)
          .await?
      }
    };
    let post = post(&sent);
    let mut action = Action::done("send", chat_ref.to_string()).with_id(&post.id);
    if let Some(url) = post.url {
      action = action.with_url(url);
    }
    Ok(action)
  }

  /// One file: as a photo when it is one, else as a document.
  async fn send_file(
    &self,
    ctx: &Ctx,
    chat_id: &Value,
    file: &Path,
    caption: &str,
    reply: Option<Value>,
  ) -> Result<Value> {
    let (method, field) = if is_photo(file) {
      ("sendPhoto", "photo")
    } else {
      ("sendDocument", "document")
    };
    let mut parts = vec![
      Part::text("chat_id", chat_id.to_string()),
      file_part(field, file).await?,
    ];
    if !caption.is_empty() {
      parts.push(Part::text("caption", caption.to_owned()));
    }
    if let Some(r) = reply {
      parts.push(Part::text("reply_parameters", r.to_string()));
    }
    self.call_parts(ctx, method, parts).await
  }

  /// Several files as one album; the caption goes on the first.
  async fn send_album(
    &self,
    ctx: &Ctx,
    chat_id: &Value,
    files: &[std::path::PathBuf],
    caption: &str,
    reply: Option<Value>,
  ) -> Result<Value> {
    let mut parts = vec![Part::text("chat_id", chat_id.to_string())];
    let mut media = Vec::new();
    for (i, file) in files.iter().enumerate() {
      let name = format!("file{i}");
      let kind = if is_photo(file) { "photo" } else { "document" };
      let mut item = json!({"type": kind, "media": format!("attach://{name}")});
      if i == 0 && !caption.is_empty() {
        item["caption"] = caption.into();
      }
      media.push(item);
      parts.push(file_part(&name, file).await?);
    }
    parts.push(Part::text("media", Value::Array(media).to_string()));
    if let Some(r) = reply {
      parts.push(Part::text("reply_parameters", r.to_string()));
    }
    let sent = self.call_parts(ctx, "sendMediaGroup", parts).await?;
    Ok(sent.list("").first().cloned().unwrap_or(sent))
  }

  pub async fn edit(&self, ctx: &Ctx, arg: &str, text: &str) -> Result<Action> {
    let (chat_id, id) = message_ref(arg)?;
    self
      .call(
        ctx,
        "editMessageText",
        json!({"chat_id": chat_id, "message_id": id, "text": text}),
      )
      .await?;
    Ok(Action::done("edit", arg))
  }

  pub async fn delete(&self, ctx: &Ctx, arg: &str) -> Result<Action> {
    let (chat_id, id) = message_ref(arg)?;
    self
      .call(
        ctx,
        "deleteMessage",
        json!({"chat_id": chat_id, "message_id": id}),
      )
      .await?;
    Ok(Action::done("delete", arg))
  }

  pub async fn react(&self, ctx: &Ctx, arg: &str, emoji: &str, undo: bool) -> Result<Action> {
    let (chat_id, id) = message_ref(arg)?;
    let reaction = if undo {
      json!([])
    } else {
      json!([{"type": "emoji", "emoji": emoji}])
    };
    self
      .call(
        ctx,
        "setMessageReaction",
        json!({"chat_id": chat_id, "message_id": id, "reaction": reaction}),
      )
      .await?;
    Ok(Action::done(if undo { "unreact" } else { "react" }, arg).with_message(emoji))
  }

  pub async fn pin(&self, ctx: &Ctx, arg: &str, undo: bool) -> Result<Action> {
    let (chat_id, id) = message_ref(arg)?;
    let method = if undo {
      "unpinChatMessage"
    } else {
      "pinChatMessage"
    };
    self
      .call(ctx, method, json!({"chat_id": chat_id, "message_id": id}))
      .await?;
    Ok(Action::done(if undo { "unpin" } else { "pin" }, arg))
  }

  pub async fn forward(&self, ctx: &Ctx, arg: &str, to: &str) -> Result<Action> {
    let (from, id) = message_ref(arg)?;
    let to = chat_id(&refs::chat(to)?)?;
    let sent = self
      .call(
        ctx,
        "forwardMessage",
        json!({"chat_id": to, "from_chat_id": from, "message_id": id}),
      )
      .await?;
    let post = post(&sent);
    let mut action = Action::done("forward", arg).with_id(&post.id);
    if let Some(url) = post.url {
      action = action.with_url(url);
    }
    Ok(action)
  }
}

async fn file_part(field: &str, path: &Path) -> Result<Part> {
  let file = Upload::read(path).await?;
  Ok(Part::file(field, file.data, &file.name, file.mime))
}
