//! Receiving: `getupdates` long polls with a cursor (`get_updates_buf`)
//! kept in the cache, so each message arrives once across runs.

use std::time::Duration;

use social_core::output::Stream;
use social_core::{Ctx, ErrorCode, Result, Value, ValueExt, json};

use crate::api::{self, Account};
use crate::{convert, local};

/// `message_state` of a reply still being generated (streamed by bots).
const GENERATING: i64 = 1;

/// One poll: the new messages (recorded), or none when `wait` ran out.
pub async fn fetch(ctx: &Ctx, acct: &Account, wait: Duration) -> Result<(Vec<Value>, Duration)> {
  let body = json!({ "get_updates_buf": local::cursor(ctx, acct) });
  let Some(v) = api::poll(ctx, acct, "ilink/bot/getupdates", body, wait).await? else {
    return Ok((Vec::new(), api::LONG_POLL));
  };
  let v = api::check("getupdates", v)?;
  let msgs: Vec<Value> = v
    .list("msgs")
    .iter()
    .filter(|m| m.i64("message_state") != Some(GENERATING))
    .cloned()
    .collect();
  local::record(ctx, acct, &msgs);
  if let Some(next) = v.str("get_updates_buf") {
    local::set_cursor(ctx, acct, &next);
  }
  let suggested = v
    .u64("longpolling_timeout_ms")
    .filter(|ms| *ms > 0)
    .map_or(api::LONG_POLL, Duration::from_millis);
  Ok((msgs, suggested))
}

/// Stream messages until interrupted; a lost login ends it, other
/// failures back off (2 s, then 30 s after three in a row).
pub async fn listen(ctx: &Ctx, acct: &Account, chats: &[String], out: &Stream) -> Result<()> {
  let mut wait = api::LONG_POLL;
  let mut failures = 0u32;
  loop {
    match fetch(ctx, acct, wait + Duration::from_secs(5)).await {
      Ok((msgs, next)) => {
        failures = 0;
        wait = next;
        for m in &msgs {
          if chats.is_empty() || chats.iter().any(|c| *c == convert::chat_of(m)) {
            out.post(&convert::post(m));
          }
        }
      }
      Err(e) if e.code == ErrorCode::NotAuthenticated => return Err(e),
      Err(e) => {
        failures += 1;
        tracing::warn!("getupdates failed ({failures}): {}", e.message);
        let pause = if failures >= 3 { 30 } else { 2 };
        tokio::time::sleep(Duration::from_secs(pause)).await;
      }
    }
  }
}
