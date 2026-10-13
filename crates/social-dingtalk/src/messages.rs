//! What the app's robot can do with messages: send to groups and users
//! (text, images, files), send through robot webhooks, recall, and read the
//! delivery and read status of its own messages. DingTalk gives robots no
//! history: incoming messages arrive only live ([`crate::live`]).

use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use social_core::file::sniff;
use social_core::http::{Method, Part};
use social_core::text::from_millis;
use social_core::{Action, Error, Message, Post, Result, User, Value, ValueExt, json};

use crate::api::{Api, OapiBody, check_oapi};
use crate::refs::{self, Target};

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
    webhook(api, url, secret.as_deref(), &message.text).await?;
    return Ok(Action::done("send", "webhook"));
  }
  let mut keys = Vec::new();
  if !message.text.is_empty() {
    keys.push(
      deliver(
        api,
        &target,
        "sampleText",
        json!({ "content": message.text }),
      )
      .await?,
    );
  }
  for file in &message.files {
    let (msg_key, param) = upload(api, file).await?;
    keys.push(deliver(api, &target, msg_key, param).await?);
  }
  let mut action = Action::done("send", target.id());
  if let Some(first) = keys.first() {
    action = action.with_id(format!("{}/{first}", target.id()));
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
    Target::Group(cid) => {
      api
        .post(
          "/v1.0/robot/groupMessages/send",
          json!({
            "msgKey": msg_key,
            "msgParam": param.to_string(),
            "openConversationId": cid,
            "robotCode": creds.robot_code,
          }),
        )
        .await?
    }
    Target::User(id) => {
      let v = api
        .post(
          "/v1.0/robot/oToMessages/batchSend",
          json!({
            "robotCode": creds.robot_code,
            "userIds": [id],
            "msgKey": msg_key,
            "msgParam": param.to_string(),
          }),
        )
        .await?;
      if v
        .list("invalidStaffIdList")
        .iter()
        .any(|s| s.as_str() == Some(id))
      {
        return Err(Error::input(format!("no such staff id: {id}")));
      }
      v
    }
    Target::Webhook { .. } => unreachable!("webhooks are sent separately"),
  };
  Ok(v.str("processQueryKey").unwrap_or_default())
}

/// Upload a file: images as `sampleImageMsg`, others as `sampleFile`.
async fn upload(api: &Api, path: &Path) -> Result<(&'static str, Value)> {
  let data = tokio::fs::read(path)
    .await
    .map_err(|e| Error::input(format!("cannot read {}: {e}", path.display())))?;
  let name = path
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or("file")
    .to_owned();
  let image = matches!(
    sniff(&data[..data.len().min(32)]),
    Some("jpg" | "png" | "gif")
  );
  let kind = if image { "image" } else { "file" };
  let v = api
    .oapi(
      "/media/upload",
      &[("type", kind.to_owned())],
      OapiBody::Multipart(vec![Part::file(
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
  if image {
    return Ok(("sampleImageMsg", json!({ "photoURL": media_id })));
  }
  let ext = path
    .extension()
    .and_then(|e| e.to_str())
    .unwrap_or("bin")
    .to_owned();
  Ok((
    "sampleFile",
    json!({ "mediaId": media_id, "fileName": name, "fileType": ext }),
  ))
}

/// A robot webhook message, signed when the robot uses 加签.
async fn webhook(api: &Api, url: &str, secret: Option<&str>, text: &str) -> Result<()> {
  let mut req = api.ctx.http.post(url).no_cookies();
  if let Some(secret) = secret {
    let ts = jiff::Timestamp::now().as_millisecond();
    req = req.query("timestamp", ts).query("sign", sign(secret, ts));
  }
  let body = if looks_markdown(text) {
    json!({
      "msgtype": "markdown",
      "markdown": { "title": text.lines().next().unwrap_or("message"), "text": text },
    })
  } else {
    json!({ "msgtype": "text", "text": { "content": text } })
  };
  let v: Value = req.json(&body).send().await?.value()?;
  check_oapi(v, "robot webhook").map(drop)
}

/// `base64(HmacSHA256(secret, "<timestamp>\n<secret>"))`.
fn sign(secret: &str, ts: i64) -> String {
  let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("any key length");
  mac.update(format!("{ts}\n{secret}").as_bytes());
  STANDARD.encode(mac.finalize().into_bytes())
}

fn looks_markdown(text: &str) -> bool {
  text.lines().any(|l| {
    let l = l.trim_start();
    l.starts_with('#') || l.starts_with("- ") || l.starts_with("> ") || l.contains("](")
  }) || text.contains("**")
}

/// Recall a message the robot sent.
pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let (target, key) = refs::sent(arg)?;
  let creds = api.require()?;
  match &target {
    Target::Group(cid) => {
      api
        .post(
          "/v1.0/robot/groupMessages/recall",
          json!({
            "robotCode": creds.robot_code,
            "openConversationId": cid,
            "processQueryKeys": [key],
          }),
        )
        .await?
    }
    _ => {
      api
        .post(
          "/v1.0/robot/otoMessages/batchRecall",
          json!({ "robotCode": creds.robot_code, "processQueryKeys": [key] }),
        )
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
    id: format!("{}/{key}", target.id()),
    kind: "receipt".into(),
    ..Post::default()
  };
  match &target {
    Target::Group(cid) => {
      let mut read: Vec<Value> = Vec::new();
      let mut next: Option<String> = None;
      loop {
        let mut body = json!({
          "openConversationId": cid,
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
        read.extend(v.list("readUserIds").iter().cloned());
        next = v.str("nextToken");
        if next.is_none() {
          break;
        }
      }
      post.metrics.views = Some(read.len() as u64);
      post.extra.insert("read_by".into(), read.into());
    }
    _ => {
      let v = api
        .api(
          Method::GET,
          "/v1.0/robot/oToMessages/readStatus",
          &[
            ("robotCode", creds.robot_code.clone()),
            ("processQueryKey", key.clone()),
          ],
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
    }
  }
  Ok(post)
}

/// One staff member (`topapi/v2/user/get`).
pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let id = arg.trim().trim_start_matches("user:");
  let v = api
    .oapi(
      "/topapi/v2/user/get",
      &[],
      OapiBody::Json(json!({ "userid": id, "language": "zh_CN" })),
    )
    .await?;
  let r = v.at("result");
  Ok(User {
    id: r.str("userid").unwrap_or_else(|| id.to_owned()),
    name: r.str("name").unwrap_or_default(),
    handle: r.str("job_number"),
    avatar: r.str("avatar").filter(|a| !a.is_empty()),
    bio: r.str("title").filter(|t| !t.is_empty()),
    location: r.str("work_place").filter(|w| !w.is_empty()),
    extra: [
      "unionid",
      "email",
      "org_email",
      "dept_id_list",
      "hired_date",
      "admin",
      "boss",
    ]
    .into_iter()
    .filter_map(|k| {
      let v = r.at(k);
      (!v.is_null()).then(|| (k.to_owned(), v.clone()))
    })
    .collect(),
    raw: Some(r.clone()),
    ..User::default()
  })
}
