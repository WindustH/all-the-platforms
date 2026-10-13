//! Sending: one `sendmessage` per item (text first, then each file), with
//! the conversation's latest `context_token`. Files go to the c2c CDN
//! first: `getuploadurl` hands out an upload parameter, the file is
//! uploaded AES-encrypted, and the CDN answers with the parameter the
//! message carries (`x-encrypted-param`).

use std::path::Path;
use std::time::Duration;

use md5::{Digest, Md5};
use social_core::{Action, Ctx, Error, Message, Result, Value, ValueExt, json};
use url::form_urlencoded::byte_serialize;

use crate::api::{self, Account, CDN};
use crate::{convert, crypto, local, refs};

/// `message_type` / `message_state` of a finished bot message.
const BOT: i64 = 2;
const FINISHED: i64 = 2;

pub async fn send(ctx: &Ctx, acct: &Account, chat: &str, msg: &Message) -> Result<Action> {
  let context = context_token(ctx, acct, chat, msg.reply_to.as_deref())?;
  let mut items = Vec::new();
  if !msg.text.is_empty() {
    items.push(convert::text_item(&msg.text));
  }
  for path in &msg.files {
    items.push(upload(ctx, acct, chat, path).await?);
  }
  let mut sent = Vec::new();
  for item in items {
    let client_id = format!("social-cli-{}", crypto::hex(&rand::random::<[u8; 8]>()));
    let mut m = json!({
      "from_user_id": "",
      "to_user_id": chat,
      "client_id": client_id,
      "message_type": BOT,
      "message_state": FINISHED,
      "item_list": [item],
    });
    if let Some(token) = &context {
      m["context_token"] = token.clone().into();
    }
    let resp = api::call(ctx, acct, "ilink/bot/sendmessage", json!({ "msg": m })).await?;
    if let Some(id) = resp.str("message_id") {
      m["message_id"] = id.into();
    }
    m["create_time_ms"] = jiff::Timestamp::now().as_millisecond().into();
    sent.push(m);
  }
  local::record(ctx, acct, &sent);
  let last = sent.last().map(convert::post).map(|p| p.id);
  let mut action = Action::done("send", chat);
  if let Some(id) = last {
    action = action.with_id(id);
  }
  if context.is_none() {
    action = action.with_message(
      "no context token on record for this chat: WeChat may not deliver it until they message the bot",
    );
  }
  Ok(action)
}

/// The context token to answer with: the one of the message replied to,
/// else the chat's latest.
fn context_token(
  ctx: &Ctx,
  acct: &Account,
  chat: &str,
  reply_to: Option<&str>,
) -> Result<Option<String>> {
  let Some(reply) = reply_to else {
    return Ok(local::contact(ctx, acct, chat).and_then(|c| c.context_token));
  };
  let (reply_chat, id) = refs::message(reply)?;
  if reply_chat != chat {
    return Err(Error::input(format!(
      "{reply} is a message of another chat ({reply_chat})"
    )));
  }
  let found = local::messages(ctx, acct, chat)
    .into_iter()
    .find(|m| m.str("message_id").as_deref() == Some(id.as_str()))
    .ok_or_else(|| Error::not_found(format!("message {reply} is not on record here")))?;
  Ok(
    found
      .str("context_token")
      .or_else(|| local::contact(ctx, acct, chat).and_then(|c| c.context_token)),
  )
}

/// Kinds of upload, and the message item each becomes.
enum Kind {
  Image,
  Video,
  File,
}

async fn upload(ctx: &Ctx, acct: &Account, chat: &str, path: &Path) -> Result<Value> {
  let data = tokio::fs::read(path)
    .await
    .map_err(|e| Error::input(format!("cannot read {}: {e}", path.display())))?;
  let kind = match social_core::file::sniff(&data) {
    Some("jpg" | "png" | "gif" | "webp") => Kind::Image,
    Some("mp4" | "mov") => Kind::Video,
    _ => Kind::File,
  };
  let media_type = match kind {
    Kind::Image => 1,
    Kind::Video => 2,
    Kind::File => 3,
  };
  let key: [u8; 16] = rand::random();
  let file_key = crypto::hex(&rand::random::<[u8; 16]>());
  let cipher = crypto::encrypt(&key, &data);
  let body = json!({
    "filekey": file_key,
    "media_type": media_type,
    "to_user_id": chat,
    "rawsize": data.len(),
    "rawfilemd5": crypto::hex(&Md5::digest(&data)),
    "filesize": crypto::padded_len(data.len()),
    "no_need_thumb": true,
    "aeskey": crypto::hex(&key),
  });
  let permit = api::call(ctx, acct, "ilink/bot/getuploadurl", body).await?;
  let url = match (permit.str("upload_full_url"), permit.str("upload_param")) {
    (Some(full), _) => full,
    (None, Some(param)) => format!(
      "{CDN}/upload?encrypted_query_param={}&filekey={}",
      byte_serialize(param.as_bytes()).collect::<String>(),
      byte_serialize(file_key.as_bytes()).collect::<String>()
    ),
    (None, None) => return Err(Error::upstream("getuploadurl gave no upload address")),
  };
  let resp = ctx
    .http
    .post(url)
    .bytes(cipher.clone(), "application/octet-stream")
    .no_cookies()
    .retries(2)
    .timeout(Duration::from_secs(300))
    .send()
    .await?;
  if !resp.status.is_success() {
    let why = resp
      .header("x-error-message")
      .map(str::to_owned)
      .unwrap_or_else(|| resp.text());
    return Err(Error::upstream(format!(
      "upload of {} failed: HTTP {} {why}",
      path.display(),
      resp.status
    )));
  }
  let param = resp
    .header("x-encrypted-param")
    .ok_or_else(|| Error::upstream("the CDN did not confirm the upload (no x-encrypted-param)"))?;
  let media = json!({
    "encrypt_query_param": param,
    "aes_key": crypto::key_for_message(&key),
    "encrypt_type": 1,
  });
  Ok(match kind {
    Kind::Image => json!({ "type": 2, "image_item": { "media": media, "mid_size": cipher.len() } }),
    Kind::Video => {
      json!({ "type": 5, "video_item": { "media": media, "video_size": cipher.len() } })
    }
    Kind::File => json!({
      "type": 4,
      "file_item": {
        "media": media,
        "file_name": path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        "len": data.len().to_string(),
      },
    }),
  })
}
