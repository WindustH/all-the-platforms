//! `listen`: the realtime API (DDP over `wss://SERVER/websocket`). After
//! `connect` and a `login` resuming the auth token, `stream-room-messages`
//! is subscribed for the named rooms (or `__my_messages__`, every room you
//! are in); each message — new or edited — becomes a post. DDP pings are
//! answered; dropped connections are reopened.

use std::time::Duration;

use social_core::output::Stream;
use social_core::{Error, Result, ValueExt, json};

use crate::api::Api;
use crate::{messages, rooms};

pub async fn listen(api: &Api, args: &[String], out: &Stream) -> Result<()> {
  let creds = api.require()?;
  let mut rids = Vec::new();
  for arg in args {
    rids.push(rooms::id(api, arg).await?);
  }
  if rids.is_empty() {
    rids.push("__my_messages__".to_owned());
  }
  let url = format!(
    "{}/websocket",
    creds
      .server
      .replacen("https://", "wss://", 1)
      .replacen("http://", "ws://", 1)
  );
  let mut failures = 0;
  loop {
    let mut ws = api.ctx.http.websocket(&url, &[], false).await?;
    ws.send_json(&json!({"msg": "connect", "version": "1", "support": ["1", "pre2", "pre1"]}))
      .await?;
    let mut ready = false;
    while let Some(frame) = ws.next_json().await? {
      match frame.str("msg").as_deref() {
        Some("ping") => ws.send_json(&json!({"msg": "pong"})).await?,
        Some("connected") => {
          ws.send_json(&json!({
            "msg": "method", "method": "login", "id": "login",
            "params": [{"resume": creds.token}],
          }))
          .await?;
        }
        Some("result") if frame.str("id").as_deref() == Some("login") => {
          if let Some(e) = frame.str("error.reason") {
            return Err(Error::auth(format!(
              "Rocket.Chat refused the realtime login: {e}"
            )));
          }
          for rid in &rids {
            ws.send_json(&json!({
              "msg": "sub", "id": format!("sub-{rid}"),
              "name": "stream-room-messages", "params": [rid, false],
            }))
            .await?;
          }
          ready = true;
          failures = 0;
        }
        Some("changed") if frame.str("collection").as_deref() == Some("stream-room-messages") => {
          for m in frame.list("fields.args") {
            if m.str("_id").is_none() {
              continue;
            }
            if let Ok(mut posts) = messages::convert(api, std::slice::from_ref(m)).await
              && let Some(mut p) = posts.pop()
            {
              if p.kind == "message" && !m.at("editedAt").is_null() {
                p.kind = "edit".into();
              }
              out.post(&p);
            }
          }
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
    if !ready {
      failures += 1;
      if failures >= 3 {
        return Err(Error::upstream(
          "the Rocket.Chat realtime connection keeps closing before the login",
        ));
      }
    }
    tokio::time::sleep(Duration::from_secs_f64(rand::random_range(1.0..4.0))).await;
  }
}
