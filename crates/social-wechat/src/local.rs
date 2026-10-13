//! What this machine has seen of the bot's conversations.
//!
//! iLink has no history, contact or conversation listing: messages only
//! arrive through `getupdates`. Everything `listen` / `sync` receive (and
//! what `send` sends) is kept in the private cache, per bot: the update
//! cursor, each contact's latest `context_token` (needed to answer them)
//! and the last messages of every chat.

use std::collections::BTreeMap;
use std::time::Duration;

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use social_core::{Ctx, Value, ValueExt};

use crate::api::Account;

const KEEP: Duration = Duration::from_secs(365 * 24 * 3600);
/// Messages kept per chat.
const PER_CHAT: usize = 500;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Contact {
  pub id: String,
  /// The latest context token: replies must carry one.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub context_token: Option<String>,
  /// Unix milliseconds of the last message either way.
  pub last_at: i64,
  #[serde(default)]
  pub last_text: String,
  #[serde(default)]
  pub messages: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct Cursor {
  buf: String,
}

fn hash(s: &str) -> String {
  let digest = Md5::digest(s.as_bytes());
  crate::crypto::hex(&digest[..8])
}

/// Cache keys of one bot.
fn key(acct: &Account, what: &str) -> String {
  format!("{}-{what}", hash(&acct.bot_id))
}

pub fn cursor(ctx: &Ctx, acct: &Account) -> String {
  ctx
    .store
    .cache_get::<Cursor>(&key(acct, "cursor"), KEEP)
    .map(|c| c.buf)
    .unwrap_or_default()
}

pub fn set_cursor(ctx: &Ctx, acct: &Account, buf: &str) {
  let c = Cursor {
    buf: buf.to_owned(),
  };
  ctx.store.cache_put(&key(acct, "cursor"), &c);
}

pub fn contacts(ctx: &Ctx, acct: &Account) -> BTreeMap<String, Contact> {
  ctx
    .store
    .cache_get(&key(acct, "contacts"), KEEP)
    .unwrap_or_default()
}

pub fn contact(ctx: &Ctx, acct: &Account, chat: &str) -> Option<Contact> {
  contacts(ctx, acct).remove(chat)
}

/// Messages of one chat, oldest first.
pub fn messages(ctx: &Ctx, acct: &Account, chat: &str) -> Vec<Value> {
  ctx
    .store
    .cache_get(&key(acct, &format!("chat-{}", hash(chat))), KEEP)
    .unwrap_or_default()
}

/// Keep messages (raw `WeixinMessage`s) and what they tell about contacts.
pub fn record(ctx: &Ctx, acct: &Account, msgs: &[Value]) {
  if msgs.is_empty() {
    return;
  }
  let mut people = contacts(ctx, acct);
  let mut by_chat: BTreeMap<String, Vec<Value>> = BTreeMap::new();
  for m in msgs {
    let chat = crate::convert::chat_of(m);
    if chat.is_empty() {
      continue;
    }
    let c = people.entry(chat.clone()).or_insert_with(|| Contact {
      id: chat.clone(),
      ..Contact::default()
    });
    if let Some(token) = m.str("context_token") {
      c.context_token = Some(token);
    }
    let at = m.i64("create_time_ms").unwrap_or(0);
    if at >= c.last_at {
      c.last_at = at;
      c.last_text = crate::convert::summary(m);
    }
    c.messages += 1;
    by_chat.entry(chat).or_default().push(m.clone());
  }
  ctx.store.cache_put(&key(acct, "contacts"), &people);
  for (chat, new) in by_chat {
    let mut all = messages(ctx, acct, &chat);
    for m in new {
      let id = m.str("message_id");
      // Updates may repeat a message (edits of a streaming reply): keep the last.
      if let Some(id) = &id {
        all.retain(|o| o.str("message_id").as_ref() != Some(id));
      }
      all.push(m);
    }
    if all.len() > PER_CHAT {
      all.drain(..all.len() - PER_CHAT);
    }
    let k = key(acct, &format!("chat-{}", hash(&chat)));
    ctx.store.cache_put(&k, &all);
  }
}
