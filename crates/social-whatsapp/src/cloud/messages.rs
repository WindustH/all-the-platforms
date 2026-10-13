//! Sending through the Cloud API: text, media, templates, reactions and
//! read receipts.

use std::path::Path;

use social_core::http::Part;
use social_core::{Action, Ctx, Error, Message, Result, Value, ValueExt, json};

use super::{Cloud, check, message_id, recipient};

impl Cloud {
  /// Send text and files; the text becomes the first file's caption.
  pub async fn send(&self, ctx: &Ctx, to: &str, m: &Message) -> Result<Action> {
    let to = recipient(to)?;
    let context = m
      .reply_to
      .as_ref()
      .map(|r| json!({ "message_id": message_id(r) }));
    let mut caption = Some(m.text.clone()).filter(|t| !t.is_empty());
    let mut last = None;
    for path in &m.files {
      let (id, kind, name) = self.upload(ctx, path).await?;
      let mut media = json!({ "id": id });
      if kind != "audio"
        && kind != "sticker"
        && let Some(c) = caption.take()
      {
        media["caption"] = c.into();
      }
      if kind == "document" {
        media["filename"] = name.into();
      }
      let mut body = json!({
        "messaging_product": "whatsapp",
        "recipient_type": "individual",
        "to": to,
        "type": kind,
      });
      body[kind] = media;
      if let Some(c) = &context {
        body["context"] = c.clone();
      }
      last = self
        .post(ctx, &format!("{}/messages", self.phone), &body)
        .await?
        .str("messages.0.id");
    }
    if let Some(text) = caption {
      let mut body = json!({
        "messaging_product": "whatsapp",
        "recipient_type": "individual",
        "to": to,
        "type": "text",
        "text": { "body": text, "preview_url": true },
      });
      if let Some(c) = &context {
        body["context"] = c.clone();
      }
      last = self
        .post(ctx, &format!("{}/messages", self.phone), &body)
        .await?
        .str("messages.0.id");
    }
    let mut action = Action::done("send", format!("+{to}"));
    if let Some(id) = last {
      action = action.with_id(format!("{to}/{id}"));
    }
    Ok(action)
  }

  /// Upload a file for sending: `(media id, message type, file name)`.
  async fn upload(&self, ctx: &Ctx, path: &Path) -> Result<(String, &'static str, String)> {
    let file = crate::files::read(path).await?;
    let (data, name, mime) = (file.data, file.name, file.mime.to_owned());
    let kind = match mime.split('/').next() {
      _ if mime == "image/webp" => "sticker",
      Some("image") => "image",
      Some("video") => "video",
      Some("audio") => "audio",
      _ => "document",
    };
    let resp = ctx
      .http
      .post(self.url(&format!("{}/media", self.phone)))
      .no_cookies()
      .header("authorization", format!("Bearer {}", self.token))
      .multipart(vec![
        Part::text("messaging_product", "whatsapp"),
        Part::text("type", mime.clone()),
        Part::file("file", data, &name, &mime),
      ])
      .send()
      .await?;
    let v = check(resp.value()?)?;
    let id = v
      .str("id")
      .ok_or_else(|| Error::upstream("the media upload returned no id"))?;
    Ok((id, kind, name))
  }

  /// `TO/WAMID` (as `send` prints it) or a bare `wamid.…` with `--to`.
  pub async fn react(&self, ctx: &Ctx, post: &str, emoji: &str, undo: bool) -> Result<Action> {
    let Some((to, id)) = post.rsplit_once('/') else {
      return Err(Error::input(
        "give the message as TO/MESSAGE_ID (as `send` prints it)",
      ));
    };
    let body = json!({
      "messaging_product": "whatsapp",
      "recipient_type": "individual",
      "to": recipient(to)?,
      "type": "reaction",
      "reaction": { "message_id": id, "emoji": if undo { "" } else { emoji } },
    });
    self
      .post(ctx, &format!("{}/messages", self.phone), &body)
      .await?;
    Ok(Action::done(if undo { "unreact" } else { "react" }, post))
  }

  /// Mark a received message (and those before it) as read.
  pub async fn mark_read(&self, ctx: &Ctx, message: &str) -> Result<Action> {
    let body = json!({
      "messaging_product": "whatsapp",
      "status": "read",
      "message_id": message_id(message),
    });
    self
      .post(ctx, &format!("{}/messages", self.phone), &body)
      .await?;
    Ok(Action::done("mark-read", message))
  }

  /// Send an approved template (the only way to open a conversation).
  pub async fn send_template(
    &self,
    ctx: &Ctx,
    to: &str,
    name: &str,
    language: &str,
    params: &[String],
  ) -> Result<Action> {
    let to = recipient(to)?;
    let mut template = json!({ "name": name, "language": { "code": language } });
    if !params.is_empty() {
      let parameters: Vec<Value> = params
        .iter()
        .map(|p| json!({ "type": "text", "text": p }))
        .collect();
      template["components"] = json!([{ "type": "body", "parameters": parameters }]);
    }
    let body = json!({
      "messaging_product": "whatsapp",
      "recipient_type": "individual",
      "to": to,
      "type": "template",
      "template": template,
    });
    let v = self
      .post(ctx, &format!("{}/messages", self.phone), &body)
      .await?;
    let mut action = Action::done("send-template", format!("+{to}"));
    if let Some(id) = v.str("messages.0.id") {
      action = action.with_id(format!("{to}/{id}"));
    }
    Ok(action)
  }
}
