//! `listen`: messages others send, pushed over the chat connection as they
//! arrive. The connection is opened again when the server drops it, and the
//! session is kept alive the way the web chat does (every 10 minutes).

use std::collections::HashSet;
use std::time::Duration;

use atp_alibaba::chats::message;
use atp_core::live::Backoff;
use atp_core::output::{Stream, note};
use atp_core::{Error, ErrorCode, Result, Value};
use tokio::time::Instant;

use crate::api::Api;
use crate::parse;
use crate::refs::{self, ChatRef};

const WAIT: Duration = Duration::from_secs(30);
const KEEP_ALIVE: Duration = Duration::from_secs(600);

pub async fn listen(api: &Api, chats: &[String], out: &Stream) -> Result<()> {
  let me = api.me()?;
  let mut wanted = HashSet::new();
  for chat in chats {
    match refs::chat(api, chat).await? {
      ChatRef::Chat(cid) => wanted.insert(cid),
      ChatRef::Item(_) => {
        return Err(Error::input(
          "listen takes chat ids (see `chats`), not items",
        ));
      }
    };
  }
  let mut failures = 0;
  let mut backoff = Backoff::default();
  loop {
    match session(api, &me, &wanted, out).await {
      Ok(()) => {
        failures = 0;
        backoff.reset();
      }
      Err(e) if e.code == ErrorCode::NotAuthenticated => return Err(e),
      Err(e) => {
        failures += 1;
        if failures >= 3 {
          return Err(e);
        }
        tracing::debug!("chat connection error: {}", e.message);
      }
    }
    note("the Xianyu chat connection closed; reconnecting");
    backoff.wait().await;
  }
}

async fn session(api: &Api, me: &str, wanted: &HashSet<String>, out: &Stream) -> Result<()> {
  let mut im = api.im().await?;
  let mut keep_alive = Instant::now() + KEEP_ALIVE;
  while let Some(pushes) = im.pushes(WAIT).await? {
    for decoded in pushes {
      let Some(post) = parse::push(&decoded).and_then(|m| message(&m, "")) else {
        continue;
      };
      let chat = post
        .extra
        .get("chat")
        .and_then(Value::as_str)
        .unwrap_or_default();
      let mine = post.author.as_ref().is_some_and(|a| a.id == me);
      if !mine && (wanted.is_empty() || wanted.contains(chat)) {
        out.post(&post);
      }
    }
    if Instant::now() >= keep_alive {
      if let Err(e) = api.refresh().await {
        tracing::debug!("keep-alive failed: {}", e.message);
      }
      keep_alive = Instant::now() + KEEP_ALIVE;
    }
  }
  Ok(())
}
