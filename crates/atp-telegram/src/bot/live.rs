//! `listen` for bots: long-polled `getUpdates`.

use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{Ctx, Result, ValueExt, json};

use super::parse::post;
use super::{Bot, same_chat};
use crate::refs;

impl Bot {
  /// Long-poll `getUpdates` and print each message, edit and reaction.
  pub async fn listen(&self, ctx: &Ctx, chats: &[String], out: &Stream) -> Result<()> {
    // Only chats a bot can address (no invite links, no `me`).
    let wanted = chats
      .iter()
      .map(|c| refs::chat(c).and_then(|chat| super::chat_id(&chat).map(|_| chat)))
      .collect::<Result<Vec<_>>>()?;
    let mut offset: Option<i64> = None;
    loop {
      let mut body = json!({
        "timeout": 50,
        "allowed_updates": ["message", "edited_message", "channel_post", "edited_channel_post", "message_reaction_count"],
      });
      if let Some(o) = offset {
        body["offset"] = o.into();
      }
      let updates = self
        .call_long(ctx, "getUpdates", body, Duration::from_secs(70))
        .await
        .map_err(|e| {
          if e.message.contains("webhook") {
            e.with_hint("the bot has a webhook; remove it (deleteWebhook) to receive updates here")
          } else {
            e
          }
        })?;
      for u in updates.as_array().map(Vec::as_slice).unwrap_or_default() {
        offset = u.i64("update_id").map(|id| id + 1).or(offset);
        let Some((kind, message)) = [
          ("message", "message"),
          ("edit", "edited_message"),
          ("message", "channel_post"),
          ("edit", "edited_channel_post"),
          ("reactions", "message_reaction_count"),
        ]
        .into_iter()
        .find_map(|(kind, key)| u.get(key).map(|m| (kind, m))) else {
          continue;
        };
        if !wanted.is_empty() && !wanted.iter().any(|c| same_chat(c, message.at("chat"))) {
          continue;
        }
        let mut p = post(message);
        p.kind = kind.into();
        out.post(&p);
      }
    }
  }
}
