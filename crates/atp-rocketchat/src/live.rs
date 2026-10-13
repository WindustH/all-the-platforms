//! `listen`: the realtime API (DDP over `wss://SERVER/websocket`). After
//! `connect` and a `login` resuming the auth token, `stream-room-messages`
//! is subscribed for the named rooms (or `__my_messages__`, every room you
//! are in); each message — new or edited — becomes a post. DDP pings are
//! answered; dropped connections are reopened.

use atp_core::live::Backoff;
use atp_core::output::Stream;
use atp_core::socket::Socket;
use atp_core::{Error, Result, Value, ValueExt, json};

use crate::api::{Api, Creds};
use crate::{chats, messages};

/// Give up when this many connections close before the login went through.
const MAX_FAILURES: u32 = 3;

pub async fn listen(api: &Api, args: &[String], out: &Stream) -> Result<()> {
  let creds = api.require()?;
  let mut rids = Vec::new();
  for arg in args {
    rids.push(chats::room_id(api, arg).await?);
  }
  if rids.is_empty() {
    rids.push("__my_messages__".to_owned());
  }
  let mut backoff = Backoff::default();
  let mut failures = 0;
  loop {
    if session(api, &creds, &rids, out).await? {
      failures = 0;
      backoff.reset();
    } else {
      failures += 1;
      if failures >= MAX_FAILURES {
        return Err(Error::upstream(
          "the Rocket.Chat realtime connection keeps closing before the login",
        ));
      }
    }
    backoff.wait().await;
  }
}

/// One connection until it closes; whether the login went through.
async fn session(api: &Api, creds: &Creds, rids: &[String], out: &Stream) -> Result<bool> {
  let url = format!(
    "{}/websocket",
    creds
      .server
      .replacen("https://", "wss://", 1)
      .replacen("http://", "ws://", 1)
  );
  let mut socket = api.ctx.http.websocket(&url, &[], false).await?;
  socket
    .send_json(&json!({"msg": "connect", "version": "1", "support": ["1", "pre2", "pre1"]}))
    .await?;
  let mut ready = false;
  while let Some(frame) = socket.next_json().await? {
    match frame.str("msg").as_deref() {
      Some("ping") => socket.send_json(&json!({"msg": "pong"})).await?,
      Some("connected") => {
        socket
          .send_json(&json!({
            "msg": "method", "method": "login", "id": "login",
            "params": [{"resume": creds.token}],
          }))
          .await?;
      }
      Some("result") if frame.str("id").as_deref() == Some("login") => {
        subscribe(&mut socket, &frame, rids).await?;
        ready = true;
      }
      Some("changed") if frame.str("collection").as_deref() == Some("stream-room-messages") => {
        changed(api, &frame, out).await;
      }
      Some("nosub") => {
        return Err(Error::upstream(format!(
          "Rocket.Chat refused the subscription: {}",
          frame.str("error.reason").unwrap_or_default()
        )));
      }
      _ => {}
    }
  }
  Ok(ready)
}

/// After the login: subscribe to the rooms' message streams.
async fn subscribe(socket: &mut Socket, login: &Value, rids: &[String]) -> Result<()> {
  if let Some(e) = login.str("error.reason") {
    return Err(Error::auth(format!(
      "Rocket.Chat refused the realtime login: {e}"
    )));
  }
  for rid in rids {
    socket
      .send_json(&json!({
        "msg": "sub", "id": format!("sub-{rid}"),
        "name": "stream-room-messages", "params": [rid, false],
      }))
      .await?;
  }
  Ok(())
}

/// New and edited messages of a `changed` frame, as posts.
async fn changed(api: &Api, frame: &Value, out: &Stream) {
  for message in frame.list("fields.args") {
    if message.str("_id").is_none() {
      continue;
    }
    let Ok(mut posts) = messages::posts(api, std::slice::from_ref(message)).await else {
      continue;
    };
    if let Some(mut post) = posts.pop() {
      if post.kind == "message" && !message.at("editedAt").is_null() {
        post.kind = "edit".into();
      }
      out.post(&post);
    }
  }
}
