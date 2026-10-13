//! `listen`: the user event stream (`/user/events`, server-sent events).
//! Chat events become posts; on (re)connect the server first replays recent
//! messages, which are skipped until it says `ready`, unless they are the
//! ones missed while reconnecting.

use std::time::Duration;

use social_core::output::Stream;
use social_core::{Error, Result, Value, ValueExt};

use crate::api::Api;
use crate::chat::Directory;
use crate::parse;
use crate::refs::{self, Chat};

const MAX_FAILURES: u32 = 5;

pub async fn listen(api: &Api, dir: &Directory, chats: &[String], out: &Stream) -> Result<()> {
  api.require_user("listen")?;
  let wanted: Vec<Chat> = chats.iter().map(|c| refs::chat(c)).collect::<Result<_>>()?;
  let server = api.server().unwrap_or_default();
  let users = dir.users(api).await?;
  let me = dir.me(api).await?;
  let mut last_mid: Option<i64> = None;
  let mut failures = 0;
  loop {
    let token = api.fresh_token().await?;
    let mut url = format!("{server}/api/user/events?api-key={token}");
    if let Some(m) = last_mid {
      url.push_str(&format!("&after_mid={m}"));
    }
    let mut ready = last_mid.is_some();
    let mut kicked = false;
    let mut buf = String::new();
    let result = api
      .ctx
      .http
      .get(url)
      .no_cookies()
      .no_throttle()
      .retries(0)
      .header("accept", "text/event-stream")
      .timeout(Duration::from_secs(24 * 3600))
      .each_chunk(|chunk| {
        buf.push_str(&String::from_utf8_lossy(chunk));
        while let Some(end) = buf.find("\n\n") {
          let block: String = buf.drain(..end + 2).collect();
          let data: String = block
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
          let Ok(v) = serde_json::from_str::<Value>(&data) else {
            continue;
          };
          match v.str("type").as_deref() {
            Some("ready") => ready = true,
            Some("chat") => {
              if let Some(m) = v.i64("mid") {
                last_mid = Some(last_mid.map_or(m, |l| l.max(m)));
              }
              if !ready {
                continue;
              }
              let Some(chat) = parse::chat_of(&v, me) else {
                continue;
              };
              if !wanted.is_empty() && !wanted.contains(&chat) {
                continue;
              }
              out.post(&parse::message(&v, &chat.key(), "", &users, &server));
            }
            Some("kick") => {
              kicked = true;
              return false;
            }
            _ => {}
          }
        }
        true
      })
      .await;
    if kicked {
      return Err(Error::auth(
        "VoceChat ended the stream (the account logged in elsewhere or was removed)",
      ));
    }
    match result {
      Ok(()) => failures = 0,
      Err(e) if e.code == social_core::ErrorCode::NotAuthenticated => return Err(e),
      Err(e) => {
        failures += 1;
        tracing::debug!("event stream failed: {e}");
        if failures >= MAX_FAILURES {
          return Err(Error::upstream(format!(
            "the VoceChat event stream keeps failing: {}",
            e.message
          )));
        }
      }
    }
    tokio::time::sleep(Duration::from_secs(2 + u64::from(failures) * 3)).await;
  }
}
