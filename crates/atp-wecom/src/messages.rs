//! 微信客服 conversations, sending as the app (to users, departments, tags,
//! app group chats and 微信客服 customers) or through group robot webhooks,
//! and recalling. WeCom has no API for ordinary chat history (that is
//! 会话存档, a paid add-on with its own SDK).

use atp_core::chat::message_id;
use atp_core::{Action, Error, Message, Page, PageReq, Post, Result, Value, ValueExt, json};

use crate::api::{Api, Body};
use crate::refs::{self, Target};
use crate::{parse, uploads};

/// `messages kf:OPEN_KFID`: what customers and servicers exchanged, oldest
/// first, as `kf/sync_msg` hands it out (strictly rate limited without the
/// token of a callback).
pub async fn list(api: &Api, chat: &str, req: &PageReq) -> Result<Page<Post>> {
  let account = match chat.trim().strip_prefix("kf:") {
    Some(rest) => rest.split('/').next().unwrap_or(rest).to_owned(),
    None => {
      return Err(Error::unsupported("messages").with_hint(
        "WeCom only reads 微信客服 conversations (`messages kf:OPEN_KFID`); other chat history needs 会话存档",
      ));
    }
  };
  let mut body = json!({
    "open_kfid": account,
    "limit": req.size_within(1000),
  });
  if let Some(c) = &req.cursor {
    body["cursor"] = json!(c);
  }
  let v = api.post("/kf/sync_msg", body).await?;
  let posts = v
    .list("msg_list")
    .iter()
    .map(|m| parse::kf_message(&account, m))
    .collect();
  let next = (v.i64("has_more") == Some(1))
    .then(|| v.str("next_cursor"))
    .flatten();
  Ok(Page::new(posts, next))
}

pub async fn send(api: &Api, chat: &str, message: &Message) -> Result<Action> {
  if message.reply_to.is_some() {
    return Err(
      Error::unsupported("send --reply-to")
        .with_hint("WeCom apps and robots cannot quote a message"),
    );
  }
  let target = refs::target(chat)?;
  let mut ids = Vec::new();
  if !message.text.is_empty() {
    let kind = if looks_markdown(&message.text) {
      "markdown"
    } else {
      "text"
    };
    let body = json!({ "content": message.text });
    ids.push(deliver(api, &target, kind, body).await?);
  }
  for file in &message.files {
    let (kind, body) = uploads::upload(api, &target, file).await?;
    ids.push(deliver(api, &target, kind, body).await?);
  }
  let mut action = Action::done("send", target.id());
  if let Some(first) = ids.iter().flatten().next() {
    action = action.with_id(message_id(&target.id(), first));
  }
  if ids.len() > 1 {
    action = action.with_message(format!("{} messages", ids.len()));
  }
  Ok(action)
}

/// One message; the msgid comes back where WeCom gives one.
async fn deliver(api: &Api, target: &Target, kind: &str, body: Value) -> Result<Option<String>> {
  let mut payload = json!({ "msgtype": kind });
  payload[kind] = body;
  let v = match target {
    Target::Webhook(key) => {
      api
        .webhook("/webhook/send", key, &[], Body::Json(payload))
        .await?;
      return Ok(None);
    }
    Target::Chat(id) => {
      payload["chatid"] = json!(id);
      api.post("/appchat/send", payload).await?
    }
    Target::Kf { account, customer } => {
      payload["touser"] = json!(customer);
      payload["open_kfid"] = json!(account);
      api.post("/kf/send_msg", payload).await?
    }
    Target::Users(_) | Target::Party(_) | Target::Tag(_) => {
      deliver_as_app(api, target, payload).await?
    }
  };
  Ok(v.str("msgid"))
}

/// `message/send` to users, a department or a tag; recipients WeCom could
/// not reach are logged.
async fn deliver_as_app(api: &Api, target: &Target, mut payload: Value) -> Result<Value> {
  let (_, agent) = api.require_agent()?;
  payload["agentid"] = json!(agent.parse::<i64>().unwrap_or_default());
  match target {
    Target::Users(u) => payload["touser"] = json!(u),
    Target::Party(p) => payload["toparty"] = json!(p),
    Target::Tag(t) => payload["totag"] = json!(t),
    _ => {}
  }
  let v = api.post("/message/send", payload).await?;
  let invalid: Vec<String> = ["invaliduser", "invalidparty", "invalidtag"]
    .iter()
    .filter_map(|k| v.str(k).filter(|s| !s.is_empty()))
    .collect();
  if !invalid.is_empty() {
    tracing::warn!("WeCom could not deliver to: {}", invalid.join(", "));
  }
  Ok(v)
}

fn looks_markdown(text: &str) -> bool {
  text.lines().any(|l| {
    let l = l.trim_start();
    l.starts_with('#') || l.starts_with("> ") || l.contains("](")
  }) || text.contains("**")
}

/// Recall a message sent as the app (within 24 hours) or to a 微信客服
/// customer.
pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let (target, msgid) = refs::sent(arg)?;
  match &target {
    Target::Kf { account, .. } => {
      let body = json!({ "open_kfid": account, "msgid": msgid });
      api.post("/kf/recall_msg", body).await?
    }
    Target::Webhook(_) | Target::Chat(_) => {
      return Err(
        Error::unsupported("delete")
          .with_hint("WeCom cannot recall robot webhook or app group chat messages"),
      );
    }
    _ => {
      api
        .post("/message/recall", json!({ "msgid": msgid }))
        .await?
    }
  };
  Ok(Action::done("delete", arg))
}
