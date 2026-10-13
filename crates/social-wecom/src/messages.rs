//! Sending as the app (to users, departments, tags, app group chats and
//! 微信客服 customers) or through group robot webhooks, and recalling.

use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use md5::{Digest, Md5};
use social_core::file::sniff;
use social_core::http::Part;
use social_core::{Action, Error, Message, Result, Value, ValueExt, json};

use crate::api::{API, Api, Body, check};
use crate::refs::{self, Target};

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
    let markdown = looks_markdown(&message.text);
    let (kind, body) = if markdown {
      ("markdown", json!({ "content": message.text }))
    } else {
      ("text", json!({ "content": message.text }))
    };
    ids.push(deliver(api, &target, kind, body).await?);
  }
  for file in &message.files {
    let (kind, body) = upload(api, &target, file).await?;
    ids.push(deliver(api, &target, kind, body).await?);
  }
  let mut action = Action::done("send", target.id());
  if let Some(first) = ids.iter().flatten().next() {
    action = action.with_id(format!("{}/{first}", target.id()));
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
      let v: Value = api
        .ctx
        .http
        .post(format!("{API}/webhook/send"))
        .no_cookies()
        .query("key", key)
        .json(&payload)
        .value()
        .await?;
      return check(v, "/webhook/send").map(|_| None);
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
      v
    }
  };
  Ok(v.str("msgid"))
}

/// Upload a file for a message: images as `image`, `.amr` as `voice`,
/// `.mp4` as `video`, everything else as `file`. Webhooks take images inline
/// (base64 + md5) and files through their own upload.
async fn upload(api: &Api, target: &Target, path: &Path) -> Result<(&'static str, Value)> {
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
  let image = matches!(sniff(&data[..data.len().min(32)]), Some("jpg" | "png"));
  if let Target::Webhook(key) = target {
    if image {
      let md5: String = Md5::digest(&data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
      return Ok((
        "image",
        json!({ "base64": STANDARD.encode(&data), "md5": md5 }),
      ));
    }
    let v: Value = api
      .ctx
      .http
      .post(format!("{API}/webhook/upload_media"))
      .no_cookies()
      .query("key", key)
      .query("type", "file")
      .multipart(vec![Part::file(
        "media",
        data,
        &name,
        "application/octet-stream",
      )])
      .timeout(std::time::Duration::from_secs(300))
      .value()
      .await?;
    let v = check(v, "/webhook/upload_media")?;
    return Ok(("file", json!({ "media_id": v.str("media_id") })));
  }
  let kind = if image {
    "image"
  } else if ext == "amr" {
    "voice"
  } else if ext == "mp4" {
    "video"
  } else {
    "file"
  };
  let v = api
    .call(
      "/media/upload",
      &[("type", kind.to_owned())],
      Body::Multipart(vec![Part::file(
        "media",
        data,
        &name,
        "application/octet-stream",
      )]),
    )
    .await?;
  let media_id = v
    .str("media_id")
    .ok_or_else(|| Error::upstream("media upload returned no media_id"))?;
  Ok((kind, json!({ "media_id": media_id })))
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
      api
        .post(
          "/kf/recall_msg",
          json!({ "open_kfid": account, "msgid": msgid }),
        )
        .await?
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
