//! `listen`: the direct inbox polled, and the new messages of threads with
//! fresh activity printed.

use std::collections::BTreeMap;
use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{PageReq, Post, Result, ValueExt, live};

use crate::api::Api;
use crate::messages;

/// How often `listen` looks at the inbox.
const POLL: Duration = Duration::from_secs(20);

/// Print each new message of `chats` (all when empty).
pub async fn listen(api: &Api, chats: &[String], out: &Stream) -> Result<()> {
  api.require_session("listening to direct messages")?;
  // Each thread's last activity (microseconds) as of the previous round.
  let mut activity: BTreeMap<String, i64> = BTreeMap::new();
  let mut first = true;
  live::poll(out, POLL, async || {
    let inbox = api.inbox(None, 20).await?;
    let mut fresh = Vec::new();
    for thread in inbox.list("inbox.threads") {
      let id = thread.str("thread_id").unwrap_or_default();
      if !chats.is_empty() && !chats.contains(&id) {
        continue;
      }
      let last = thread.i64("last_activity_at").unwrap_or(0);
      let before = activity.insert(id.clone(), last);
      if first || before.is_some_and(|b| b >= last) {
        continue;
      }
      fresh.extend(newer(api, &id, before.unwrap_or(0)).await?);
    }
    first = false;
    Ok(fresh)
  })
  .await
}

/// The messages of thread `chat` sent after `since` (microseconds).
async fn newer(api: &Api, chat: &str, since: i64) -> Result<Vec<Post>> {
  let req = PageReq {
    cursor: None,
    size: 20,
  };
  let page = messages::messages(api, chat, &req).await?;
  Ok(
    page
      .items
      .into_iter()
      .filter(|post| {
        post
          .raw
          .as_ref()
          .and_then(|raw| raw.i64("timestamp"))
          .is_some_and(|ts| ts > since)
      })
      .collect(),
  )
}
