//! Receiving: `getupdates` long polls with a cursor (`get_updates_buf`)
//! kept in the cache, so each message arrives once across runs.

use std::time::Duration;

use social_core::live::Backoff;
use social_core::output::Stream;
use social_core::{ErrorCode, Page, Post, Result, Value, ValueExt, json};

use crate::api::{self, Account, Api};
use crate::{archive, parse};

/// `message_state` of a reply still being generated (streamed by bots).
const GENERATING: i64 = 1;

/// One poll: the new messages (recorded), or none when `wait` ran out; and
/// how long the server suggests holding the next poll.
pub async fn fetch(api: &Api, account: &Account, wait: Duration) -> Result<(Vec<Value>, Duration)> {
  let body = json!({ "get_updates_buf": archive::cursor(&api.ctx, account) });
  let Some(v) = api
    .poll(account, "ilink/bot/getupdates", body, wait)
    .await?
  else {
    return Ok((Vec::new(), api::LONG_POLL));
  };
  let v = api::check("getupdates", v)?;
  let messages: Vec<Value> = v
    .list("msgs")
    .iter()
    .filter(|m| m.i64("message_state") != Some(GENERATING))
    .cloned()
    .collect();
  archive::record(&api.ctx, account, &messages);
  if let Some(next) = v.str("get_updates_buf") {
    archive::set_cursor(&api.ctx, account, &next);
  }
  let suggested = v
    .u64("longpolling_timeout_ms")
    .filter(|ms| *ms > 0)
    .map_or(api::LONG_POLL, Duration::from_millis);
  Ok((messages, suggested))
}

/// `sync`: fetch new messages once, newest first.
pub async fn sync(api: &Api, account: &Account, wait: Duration) -> Result<Page<Post>> {
  let (messages, _) = fetch(api, account, wait).await?;
  let mut posts: Vec<_> = messages.iter().map(parse::post).collect();
  posts.reverse();
  Ok(Page::last(posts))
}

/// Stream messages until interrupted; a lost login ends it, other
/// failures back off.
pub async fn listen(api: &Api, account: &Account, chats: &[String], out: &Stream) -> Result<()> {
  let mut wait = api::LONG_POLL;
  let mut backoff = Backoff::default();
  loop {
    match fetch(api, account, wait + Duration::from_secs(5)).await {
      Ok((messages, next)) => {
        backoff.reset();
        wait = next;
        for m in &messages {
          if chats.is_empty() || chats.iter().any(|c| *c == parse::chat_id(m)) {
            out.post(&parse::post(m));
          }
        }
      }
      Err(e) if e.code == ErrorCode::NotAuthenticated => return Err(e),
      Err(e) => {
        tracing::warn!("getupdates failed: {}", e.message);
        backoff.wait().await;
      }
    }
  }
}
