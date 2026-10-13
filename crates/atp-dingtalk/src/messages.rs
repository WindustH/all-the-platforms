//! What the app's robot can do with messages: send to groups and users
//! (text, images, files), send through robot webhooks, recall, and read the
//! delivery and read status of its own messages. DingTalk gives robots no
//! history: incoming messages arrive only live ([`crate::live`]).

use atp_core::chat::message_id;
use atp_core::http::Method;
use atp_core::text::from_millis;
use atp_core::{Action, Error, Message, Post, Result, Value, ValueExt, json};

use crate::api::{Api, Creds};
use crate::refs::{self, Target};
use crate::{uploads, webhooks};

pub async fn send(api: &Api, chat: &str, message: &Message) -> Result<Action> {
  if message.reply_to.is_some() {
    return Err(
      Error::unsupported("send --reply-to")
        .with_hint("DingTalk robots cannot quote a message; send to the chat instead"),
    );
  }
  let target = refs::target(chat)?;
  if let Target::Webhook { url, secret } = &target {
    if !message.files.is_empty() {
      return Err(Error::unsupported("send --attach to a webhook").with_hint(
        "robot webhooks take text and Markdown only; send files through the app (group or user)",
      ));
    }
    webhooks::send(api, url, secret.as_deref(), &message.text).await?;
    return Ok(Action::done("send", "webhook"));
  }
  let mut keys = Vec::new();
  if !message.text.is_empty() {
    let param = json!({ "content": message.text });
    keys.push(deliver(api, &target, "sampleText", param).await?);
  }
  for file in &message.files {
    let (msg_key, param) = uploads::upload(api, file).await?;
    keys.push(deliver(api, &target, msg_key, param).await?);
  }
  let mut action = Action::done("send", target.id());
  if let Some(first) = keys.first() {
    action = action.with_id(message_id(&target.id(), first));
  }
  if keys.len() > 1 {
    action = action.with_message(format!("{} messages", keys.len()));
  }
  Ok(action)
}

/// One robot message; its `processQueryKey` comes back.
async fn deliver(api: &Api, target: &Target, msg_key: &str, param: Value) -> Result<String> {
  let creds = api.require()?;
  let v = match target {
    Target::Group(id) => {
      let body = json!({
        "msgKey": msg_key,
        "msgParam": param.to_string(),
        "openConversationId": id,
        "robotCode": creds.robot_code,
      });
      api.post("/v1.0/robot/groupMessages/send", body).await?
    }
    Target::User(id) => {
      let body = json!({
        "robotCode": creds.robot_code,
        "userIds": [id],
        "msgKey": msg_key,
        "msgParam": param.to_string(),
      });
      let v = api.post("/v1.0/robot/oToMessages/batchSend", body).await?;
      let invalid = v.list("invalidStaffIdList");
      if invalid.iter().any(|s| s.as_str() == Some(id)) {
        return Err(Error::input(format!("no such staff id: {id}")));
      }
      v
    }
    Target::Webhook { .. } => unreachable!("webhooks are sent separately"),
  };
  Ok(v.str("processQueryKey").unwrap_or_default())
}

/// Recall a message the robot sent.
pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let (target, key) = refs::sent(arg)?;
  let creds = api.require()?;
  match &target {
    Target::Group(id) => {
      let body = json!({
        "robotCode": creds.robot_code,
        "openConversationId": id,
        "processQueryKeys": [key],
      });
      api.post("/v1.0/robot/groupMessages/recall", body).await?
    }
    _ => {
      let body = json!({ "robotCode": creds.robot_code, "processQueryKeys": [key] });
      api
        .post("/v1.0/robot/otoMessages/batchRecall", body)
        .await?
    }
  };
  Ok(Action::done("delete", arg))
}

/// A sent message's delivery and read status.
pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let (target, key) = refs::sent(arg)?;
  let creds = api.require()?;
  let mut post = Post {
    id: message_id(&target.id(), &key),
    kind: "receipt".into(),
    ..Post::default()
  };
  match &target {
    Target::Group(id) => group_receipt(api, &creds, id, &key, &mut post).await?,
    _ => user_receipt(api, &creds, &key, &mut post).await?,
  }
  Ok(post)
}

/// Who in a group read the message, page by page.
async fn group_receipt(
  api: &Api,
  creds: &Creds,
  conversation: &str,
  key: &str,
  post: &mut Post,
) -> Result<()> {
  let mut readers: Vec<Value> = Vec::new();
  let mut next: Option<String> = None;
  loop {
    let mut body = json!({
      "openConversationId": conversation,
      "robotCode": creds.robot_code,
      "processQueryKey": key,
      "maxResults": 200,
    });
    if let Some(t) = &next {
      body["nextToken"] = json!(t);
    }
    let v = api.post("/v1.0/robot/groupMessages/query", body).await?;
    post
      .extra
      .insert("send_status".into(), v.at("sendStatus").clone());
    readers.extend(v.list("readUserIds").iter().cloned());
    next = v.str("nextToken");
    if next.is_none() {
      break;
    }
  }
  post.metrics.views = Some(readers.len() as u64);
  post.extra.insert("read_by".into(), readers.into());
  Ok(())
}

/// Whether the users a direct message went to read it.
async fn user_receipt(api: &Api, creds: &Creds, key: &str, post: &mut Post) -> Result<()> {
  let query = [
    ("robotCode", creds.robot_code.clone()),
    ("processQueryKey", key.to_owned()),
  ];
  let v = api
    .api(
      Method::GET,
      "/v1.0/robot/oToMessages/readStatus",
      &query,
      None,
    )
    .await?;
  post
    .extra
    .insert("send_status".into(), v.at("sendStatus").clone());
  let readers: Vec<Value> = v
    .list("messageReadInfoList")
    .iter()
    .map(|r| {
      json!({
        "user": r.str("userId"),
        "name": r.str("name"),
        "status": r.str("readStatus"),
        "read_at": r.i64("readTimestamp").and_then(from_millis).map(|t| t.to_string()),
      })
    })
    .collect();
  post.extra.insert("readers".into(), readers.into());
  Ok(())
}
