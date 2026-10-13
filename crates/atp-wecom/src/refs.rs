//! Where WeCom messages go:
//!
//! - users by id (`user:ID`, or bare ids; several joined with `|`), a
//!   department `party:ID`, a tag `tag:ID`, or `@all`, through the app;
//! - an app group chat (群聊会话) `chat:CHATID`;
//! - a 微信客服 conversation `kf:OPEN_KFID/EXTERNAL_USERID`;
//! - a group robot webhook (`https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=…`
//!   or `webhook:KEY`).
//!
//! A sent message is `TARGET/MSGID`, which `delete` recalls.

use atp_core::chat::split_message_id;
use atp_core::{Error, Result};

#[derive(Debug, Clone)]
pub enum Target {
  Users(String),
  Party(String),
  Tag(String),
  Chat(String),
  Kf { account: String, customer: String },
  Webhook(String),
}

impl Target {
  pub fn id(&self) -> String {
    match self {
      Target::Users(u) => format!("user:{u}"),
      Target::Party(p) => format!("party:{p}"),
      Target::Tag(t) => format!("tag:{t}"),
      Target::Chat(c) => format!("chat:{c}"),
      Target::Kf { account, customer } => format!("kf:{account}/{customer}"),
      Target::Webhook(_) => "webhook".into(),
    }
  }
}

pub fn target(arg: &str) -> Result<Target> {
  let arg = arg.trim();
  if let Some(key) = webhook_key(arg) {
    return Ok(Target::Webhook(key));
  }
  if let Some(rest) = arg.strip_prefix("kf:") {
    return match rest.split_once('/') {
      Some((account, customer)) if !account.is_empty() && !customer.is_empty() => Ok(Target::Kf {
        account: account.to_owned(),
        customer: customer.to_owned(),
      }),
      _ => Err(Error::input(
        "a 微信客服 chat is kf:OPEN_KFID/EXTERNAL_USERID",
      )),
    };
  }
  for (prefix, make) in [
    ("chat:", Target::Chat as fn(String) -> Target),
    ("party:", Target::Party),
    ("tag:", Target::Tag),
    ("user:", Target::Users),
  ] {
    if let Some(rest) = arg.strip_prefix(prefix) {
      return Ok(make(rest.to_owned()));
    }
  }
  if arg.is_empty() || arg.contains('/') {
    return Err(Error::input(format!(
      "not a WeCom chat: {arg}; give user:ID, party:ID, tag:ID, chat:ID, kf:KFID/USER or a webhook"
    )));
  }
  Ok(Target::Users(arg.to_owned()))
}

fn webhook_key(arg: &str) -> Option<String> {
  if let Some(key) = arg.strip_prefix("webhook:") {
    return Some(key.to_owned());
  }
  let rest = arg.strip_prefix("https://qyapi.weixin.qq.com/cgi-bin/webhook/send?")?;
  rest
    .split('&')
    .find_map(|p| p.strip_prefix("key="))
    .map(str::to_owned)
}

/// `(target, msgid)` of a sent message.
pub fn sent(arg: &str) -> Result<(Target, String)> {
  let arg = arg.trim();
  let Some((chat, msgid)) = split_message_id(arg) else {
    return Err(Error::input(format!(
      "not a sent WeCom message: {arg}; expected TARGET/MSGID as `send` prints it"
    )));
  };
  Ok((target(chat)?, msgid.to_owned()))
}
