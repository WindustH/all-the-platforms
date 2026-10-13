//! Sending: one `sendmessage` per item (text first, then each file, see
//! [`crate::uploads`]), with the conversation's latest `context_token`; and
//! the typing indicator.

use social_core::{Action, Error, Message, Result, Value, ValueExt, json};

use crate::api::{Account, Api};
use crate::{archive, crypto, parse, refs, uploads};

/// `message_type` / `message_state` of a finished bot message.
const BOT: i64 = 2;
const FINISHED: i64 = 2;

pub async fn send(api: &Api, account: &Account, chat: &str, message: &Message) -> Result<Action> {
  let context = context_token(api, account, chat, message.reply_to.as_deref())?;
  let mut items = Vec::new();
  if !message.text.is_empty() {
    items.push(parse::text_item(&message.text));
  }
  for path in &message.files {
    items.push(uploads::upload(api, account, chat, path).await?);
  }
  let mut sent = Vec::new();
  for item in items {
    sent.push(send_item(api, account, chat, item, context.as_deref()).await?);
  }
  archive::record(&api.ctx, account, &sent);
  let mut action = Action::done("send", chat);
  if let Some(id) = sent.last().map(parse::post).map(|p| p.id) {
    action = action.with_id(id);
  }
  if context.is_none() {
    action = action.with_message(
      "no context token on record for this chat: WeChat may not deliver it until they message the bot",
    );
  }
  Ok(action)
}

/// One `sendmessage`; the message as the archive keeps it.
async fn send_item(
  api: &Api,
  account: &Account,
  chat: &str,
  item: Value,
  context: Option<&str>,
) -> Result<Value> {
  let client_id = format!("social-cli-{}", crypto::hex(&rand::random::<[u8; 8]>()));
  let mut message = json!({
    "from_user_id": "",
    "to_user_id": chat,
    "client_id": client_id,
    "message_type": BOT,
    "message_state": FINISHED,
    "item_list": [item],
  });
  if let Some(token) = context {
    message["context_token"] = token.into();
  }
  let body = json!({ "msg": message });
  let resp = api.call(account, "ilink/bot/sendmessage", body).await?;
  if let Some(id) = resp.str("message_id") {
    message["message_id"] = id.into();
  }
  message["create_time_ms"] = jiff::Timestamp::now().as_millisecond().into();
  Ok(message)
}

/// The context token to answer with: the one of the message replied to,
/// else the chat's latest.
fn context_token(
  api: &Api,
  account: &Account,
  chat: &str,
  reply_to: Option<&str>,
) -> Result<Option<String>> {
  let ctx = &api.ctx;
  let latest = || archive::contact(ctx, account, chat).and_then(|c| c.context_token);
  let Some(reply) = reply_to else {
    return Ok(latest());
  };
  let (reply_chat, id) = refs::message(reply)?;
  if reply_chat != chat {
    return Err(Error::input(format!(
      "{reply} is a message of another chat ({reply_chat})"
    )));
  }
  let found = archive::messages(ctx, account, chat)
    .into_iter()
    .find(|m| m.str("message_id").as_deref() == Some(id.as_str()))
    .ok_or_else(|| Error::not_found(format!("message {reply} is not on record here")))?;
  Ok(found.str("context_token").or_else(latest))
}

/// Show "typing…" in a chat, or stop it.
pub async fn typing(api: &Api, account: &Account, chat: &str, stop: bool) -> Result<Action> {
  let token = archive::contact(&api.ctx, account, chat).and_then(|c| c.context_token);
  let mut body = json!({ "ilink_user_id": chat });
  if let Some(t) = &token {
    body["context_token"] = t.clone().into();
  }
  let config = api.call(account, "ilink/bot/getconfig", body).await?;
  let ticket = config.str("typing_ticket").ok_or_else(|| {
    Error::upstream("WeChat gave no typing ticket for this chat")
      .with_hint("typing works in chats that messaged the bot recently")
  })?;
  let status = if stop { 2 } else { 1 };
  let body = json!({ "ilink_user_id": chat, "typing_ticket": ticket, "status": status });
  api.call(account, "ilink/bot/sendtyping", body).await?;
  let name = if stop { "stop-typing" } else { "typing" };
  Ok(Action::done(name, chat))
}
