//! Sending (push, multicast, broadcast; text, images by URL, stickers) and
//! the content of received messages.

use social_core::{Action, Error, Media, MediaKind, Message, Post, Result, Value, ValueExt, json};

use crate::api::{Api, DATA};
use crate::refs::{self, ChatRef};

/// Deliver message objects to a target; the sent ids (or the request id of
/// a broadcast) come back.
pub async fn deliver(api: &Api, to: &ChatRef, messages: Vec<Value>) -> Result<Action> {
  if messages.is_empty() {
    return Err(Error::input("nothing to send"));
  }
  if messages.len() > 5 {
    return Err(Error::input("LINE sends at most 5 messages at once"));
  }
  let (path, body, name) = match to {
    ChatRef::User(id) | ChatRef::Group(id) | ChatRef::Room(id) => (
      "/v2/bot/message/push",
      json!({"to": id, "messages": messages}),
      id.clone(),
    ),
    ChatRef::Users(ids) => (
      "/v2/bot/message/multicast",
      json!({"to": ids, "messages": messages}),
      format!("{} users", ids.len()),
    ),
    ChatRef::All => (
      "/v2/bot/message/broadcast",
      json!({"messages": messages}),
      "everyone".to_owned(),
    ),
  };
  let (v, request_id) = api.send(path, &body).await?;
  let action = match to {
    ChatRef::All => "broadcast",
    ChatRef::Users(_) => "multicast",
    _ => "send",
  };
  let mut a = Action::done(action, name);
  if let Some(id) = v.list("sentMessages").first().and_then(|m| m.str("id")) {
    a = a.with_id(id);
  } else if let Some(r) = request_id {
    a = a
      .with_id(r)
      .with_message("request id: use it with `insights`");
  }
  Ok(a)
}

pub async fn send(api: &Api, arg: &str, message: &Message) -> Result<Action> {
  if !message.files.is_empty() {
    return Err(
      Error::unsupported("send --attach")
        .with_hint("LINE takes images by public HTTPS URL: `social line image CHAT URL`"),
    );
  }
  let mut text = json!({"type": "text", "text": message.text});
  if let Some(q) = &message.reply_to {
    // Quote tokens come with webhook events and sent messages.
    text["quoteToken"] = json!(q);
  }
  deliver(api, &refs::chat(arg)?, vec![text]).await
}

pub async fn image(api: &Api, arg: &str, url: &str, preview: Option<&str>) -> Result<Action> {
  if !url.starts_with("https://") {
    return Err(Error::input("LINE needs an https:// image URL"));
  }
  let msg = json!({
    "type": "image",
    "originalContentUrl": url,
    "previewImageUrl": preview.unwrap_or(url),
  });
  deliver(api, &refs::chat(arg)?, vec![msg]).await
}

pub async fn sticker(api: &Api, arg: &str, package: &str, sticker: &str) -> Result<Action> {
  let msg = json!({"type": "sticker", "packageId": package, "stickerId": sticker});
  deliver(api, &refs::chat(arg)?, vec![msg]).await
}

/// A received message's content (image, video, audio, file), by message id.
pub fn content(api: &Api, id: &str) -> Result<(Post, Vec<Media>)> {
  let token = api.require()?;
  let id = id.trim();
  if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
    return Err(Error::input("give the id of a received message (digits)"));
  }
  let mut m = Media::new(
    MediaKind::File,
    format!("{DATA}/v2/bot/message/{id}/content"),
  );
  m.headers = vec![("authorization".into(), format!("Bearer {token}"))];
  let post = Post {
    id: id.to_owned(),
    kind: "message".into(),
    ..Post::default()
  };
  Ok((post, vec![m]))
}
