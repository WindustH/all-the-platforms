//! Where DingTalk messages go, and what identifies a sent one.
//!
//! - a group by its `openConversationId` (`cid…`, from `listen` or a
//!   scene group);
//! - a user by staff id (`user:ID`, or a bare id);
//! - a robot webhook URL (`https://oapi.dingtalk.com/robot/send?access_token=…`,
//!   with `&secret=SEC…` appended for 加签 robots; the `sessionWebhook`
//!   of an incoming message works too), or `webhook:TOKEN[:SECRET]`.
//!
//! A message the app's robot sent is `CHAT/processQueryKey`.

use atp_core::chat::split_message_id;
use atp_core::{Error, Result, credentials};
use url::Url;

#[derive(Debug, Clone)]
pub enum Target {
  Group(String),
  User(String),
  Webhook { url: String, secret: Option<String> },
}

impl Target {
  /// How the chat part of a message id spells this target.
  pub fn id(&self) -> String {
    match self {
      Target::Group(id) => id.clone(),
      Target::User(id) => format!("user:{id}"),
      Target::Webhook { .. } => "webhook".into(),
    }
  }
}

pub fn target(arg: &str) -> Result<Target> {
  let arg = arg.trim();
  if arg.starts_with("https://oapi.dingtalk.com/robot/") {
    let mut url = Url::parse(arg).map_err(|e| Error::input(format!("bad webhook URL: {e}")))?;
    let secret = url
      .query_pairs()
      .find(|(k, _)| k == "secret")
      .map(|(_, v)| v.into_owned())
      .or_else(|| credentials::env("DINGTALK_WEBHOOK_SECRET"));
    let kept: Vec<(String, String)> = url
      .query_pairs()
      .filter(|(k, _)| k != "secret")
      .map(|(k, v)| (k.into_owned(), v.into_owned()))
      .collect();
    url.query_pairs_mut().clear().extend_pairs(kept);
    return Ok(Target::Webhook {
      url: url.into(),
      secret,
    });
  }
  if let Some(rest) = arg.strip_prefix("webhook:") {
    let (token, secret) = match rest.split_once(':') {
      Some((t, s)) => (t, Some(s.to_owned())),
      None => (rest, None),
    };
    return Ok(Target::Webhook {
      url: format!("https://oapi.dingtalk.com/robot/send?access_token={token}"),
      secret,
    });
  }
  if arg.starts_with("cid") {
    return Ok(Target::Group(arg.to_owned()));
  }
  let user = arg.strip_prefix("user:").unwrap_or(arg);
  if user.is_empty() || user.contains('/') {
    return Err(Error::input(format!(
      "not a DingTalk chat: {arg}; give a group's openConversationId (cid…), user:STAFFID or a webhook URL"
    )));
  }
  Ok(Target::User(user.to_owned()))
}

/// `(target, processQueryKey)` of a sent message (`CHAT/KEY`).
pub fn sent(arg: &str) -> Result<(Target, String)> {
  let arg = arg.trim();
  let Some((chat, key)) = split_message_id(arg) else {
    return Err(Error::input(format!(
      "not a sent DingTalk message: {arg}; expected CHAT/processQueryKey as `send` prints it"
    )));
  };
  let target = target(chat)?;
  if matches!(target, Target::Webhook { .. }) {
    return Err(Error::unsupported(
      "webhook messages cannot be read back or recalled",
    ));
  }
  Ok((target, key.to_owned()))
}
