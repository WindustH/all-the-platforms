//! `listen`: the user event stream (`/user/events`, server-sent events).
//! Chat events become posts; on (re)connect the server first replays recent
//! messages, which are skipped until it says `ready`, unless they are the
//! ones missed while reconnecting.

use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{Error, ErrorCode, Result, Value, ValueExt};

use crate::api::Api;
use crate::parse::{self, Users};
use crate::refs::{self, ChatRef};
use crate::users;

const MAX_FAILURES: u32 = 5;

/// What the events are filtered and drawn with.
struct Listener<'a> {
  out: &'a Stream,
  wanted: Vec<ChatRef>,
  server: String,
  users: Users,
  me: Option<i64>,
}

/// How one connection to the stream ended.
enum Ended {
  Closed,
  /// The server removed this client (logged in elsewhere, account removed).
  Kicked,
}

pub async fn listen(api: &Api, chats: &[String], out: &Stream) -> Result<()> {
  api.require_user("listen")?;
  let listener = Listener {
    out,
    wanted: chats.iter().map(|c| refs::chat(c)).collect::<Result<_>>()?,
    server: api.server().unwrap_or_default(),
    users: users::all(api).await?,
    me: users::me(api).await?,
  };
  let mut last_mid: Option<i64> = None;
  let mut failures = 0;
  loop {
    match connect(api, &listener, &mut last_mid).await {
      Ok(Ended::Kicked) => {
        return Err(Error::auth(
          "VoceChat ended the stream (the account logged in elsewhere or was removed)",
        ));
      }
      Ok(Ended::Closed) => failures = 0,
      Err(e) if e.code == ErrorCode::NotAuthenticated => return Err(e),
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

/// One connection, resuming after `last_mid` when set, until it ends.
async fn connect(api: &Api, listener: &Listener<'_>, last_mid: &mut Option<i64>) -> Result<Ended> {
  let token = api.fresh_token().await?;
  let mut url = format!("{}/api/user/events?api-key={token}", listener.server);
  if let Some(m) = last_mid {
    url.push_str(&format!("&after_mid={m}"));
  }
  let mut state = State {
    ready: last_mid.is_some(),
    kicked: false,
    buffer: String::new(),
    last_mid,
  };
  api
    .ctx
    .http
    .get(url)
    .no_cookies()
    .no_throttle()
    .retries(0)
    .header("accept", "text/event-stream")
    .timeout(Duration::from_secs(24 * 3600))
    .each_chunk(|chunk| state.feed(chunk, listener))
    .await?;
  Ok(if state.kicked {
    Ended::Kicked
  } else {
    Ended::Closed
  })
}

/// Where a connection is in the stream.
struct State<'a> {
  ready: bool,
  kicked: bool,
  buffer: String,
  last_mid: &'a mut Option<i64>,
}

impl State<'_> {
  /// Take in a chunk; false stops reading.
  fn feed(&mut self, chunk: &[u8], listener: &Listener) -> bool {
    self.buffer.push_str(&String::from_utf8_lossy(chunk));
    while let Some(end) = self.buffer.find("\n\n") {
      let block: String = self.buffer.drain(..end + 2).collect();
      let data: String = block
        .lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");
      let Ok(event) = serde_json::from_str::<Value>(&data) else {
        continue;
      };
      if !self.event(&event, listener) {
        return false;
      }
    }
    true
  }

  /// Handle one event; false when the server ended the stream.
  fn event(&mut self, event: &Value, listener: &Listener) -> bool {
    match event.str("type").as_deref() {
      Some("ready") => self.ready = true,
      Some("chat") => self.chat(event, listener),
      Some("kick") => {
        self.kicked = true;
        return false;
      }
      _ => {}
    }
    true
  }

  fn chat(&mut self, event: &Value, listener: &Listener) {
    if let Some(m) = event.i64("mid") {
      *self.last_mid = Some(self.last_mid.map_or(m, |l| l.max(m)));
    }
    if !self.ready {
      return;
    }
    let Some(chat) = parse::chat_of(event, listener.me) else {
      return;
    };
    if !listener.wanted.is_empty() && !listener.wanted.contains(&chat) {
      return;
    }
    let post = parse::message(event, &chat.key(), "", &listener.users, &listener.server);
    listener.out.post(&post);
  }
}
