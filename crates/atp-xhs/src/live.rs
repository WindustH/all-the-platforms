//! `listen`: private messages and live rooms as they happen, over the
//! push connection ([`crate::gateway`]). Without arguments every private
//! message the account receives; user ids keep to those conversations;
//! `live:ROOM` follows a live room's comments, gifts, likes, arrivals and
//! follows (then private messages only when users are named too). A
//! dropped connection is opened again.

use std::time::Duration;

use atp_core::live::{Backoff, Seen};
use atp_core::output::Stream;
use atp_core::text::from_millis;
use atp_core::{ErrorCode, Post, Result, User, Value, ValueExt, chat, json};
use tokio::time::{Instant, interval_at};

use crate::api::Api;
use crate::gateway;
use crate::sign::now_ms;
use crate::{messages, refs, users};

/// A connection that lasted this long resets the reconnect delay.
const STABLE: Duration = Duration::from_secs(60);

/// What to follow: live rooms, and private messages (of `peers`, or all).
struct Follow {
  me: String,
  rooms: Vec<String>,
  peers: Vec<String>,
  messages: bool,
}

pub async fn listen(api: &Api, args: &[String], out: &Stream) -> Result<()> {
  api.require_login()?;
  let mut rooms = Vec::new();
  let mut peers = Vec::new();
  for arg in args {
    match refs::live_room(arg) {
      Some(room) => rooms.push(room),
      None => peers.push(messages::chat_id(arg)?),
    }
  }
  let follow = Follow {
    me: users::whoami(api).await?.id,
    messages: rooms.is_empty() || !peers.is_empty(),
    rooms,
    peers,
  };
  let mut seen = Seen::default();
  let mut backoff = Backoff::default();
  loop {
    let started = Instant::now();
    match session(api, &follow, &mut seen, out).await {
      Ok(()) => tracing::debug!("the push connection closed"),
      Err(e) if e.code == ErrorCode::NetworkError => tracing::debug!("push connection: {e}"),
      Err(e) => return Err(e),
    }
    if started.elapsed() > STABLE {
      backoff.reset();
    }
    backoff.wait().await;
  }
}

/// One connection, until the server closes it.
async fn session(api: &Api, follow: &Follow, seen: &mut Seen, out: &Stream) -> Result<()> {
  let mut push = gateway::connect(api, &follow.me, &follow.rooms).await?;
  let mut beat = interval_at(Instant::now() + gateway::HEARTBEAT, gateway::HEARTBEAT);
  let mut sync = interval_at(Instant::now() + gateway::SYNC, gateway::SYNC);
  loop {
    tokio::select! {
      _ = beat.tick() => push.send(&gateway::heartbeat()).await?,
      _ = sync.tick(), if follow.messages => push.send(&gateway::sync()).await?,
      frame = push.next() => {
        let Some(frame) = frame? else {
          return Ok(());
        };
        for post in events(api, follow, &frame) {
          if seen.insert(&post.id) {
            out.post(&post);
          }
        }
      }
    }
  }
}

/// The posts a frame carries for what is followed.
fn events(api: &Api, follow: &Follow, frame: &Value) -> Vec<Post> {
  let mut posts: Vec<Post> = gateway::room_events(frame)
    .iter()
    .filter_map(room_event)
    .collect();
  if follow.messages {
    let wanted = |p: &Post| {
      let chat = p
        .extra
        .get("chat")
        .and_then(Value::as_str)
        .unwrap_or_default();
      follow.peers.is_empty() || follow.peers.iter().any(|peer| peer == chat)
    };
    posts.extend(
      gateway::ims(frame)
        .iter()
        .filter_map(|im| messages::incoming(api, im.incoming.as_ref()?, &follow.me))
        .filter(wanted),
    );
  }
  posts
}

/// A live room event: `customData.type` says what happened.
fn room_event(v: &Value) -> Option<Post> {
  let room = v.str("roomId")?;
  let custom = v.at("customData");
  let kind = custom.str("type")?;
  let desc = custom.str("desc").filter(|d| !d.is_empty());
  let (kind, text) = match kind.as_str() {
    "text" => ("message", desc),
    "gift" => ("gift", Some(gift(custom))),
    "praise" | "light" => ("like", Some("liked".into())),
    "come" | "enter" | "enter_room" => ("enter", Some("joined".into())),
    "follow" => ("follow", Some("followed".into())),
    "viewer_heart" | "audience_num" => return None,
    _ => ("notice", Some(desc?)),
  };
  let at = v
    .i64("ts")
    .or_else(|| custom.i64("current_time"))
    .unwrap_or_else(|| now_ms() as i64);
  let mut post = Post {
    id: chat::message_id(&room, &format!("{kind}-{at}-{}", sender_id(custom))),
    kind: kind.into(),
    text,
    author: sender(custom),
    url: Some(refs::live_url(&room)),
    created_at: from_millis(at),
    raw: Some(v.clone()),
    ..Post::default()
  };
  post
    .extra
    .insert("chat".into(), json!(format!("live:{room}")));
  Some(post)
}

/// `name × count` of a gift.
fn gift(custom: &Value) -> String {
  let name = custom
    .first_str(&["gift.name", "gift_info.name", "desc"])
    .unwrap_or_else(|| "gift".into());
  let count = custom
    .first_count(&["gift.count", "gift_info.count", "count"])
    .unwrap_or(1)
    .max(1);
  format!("{name} × {count}")
}

fn sender_id(custom: &Value) -> String {
  custom.str("profile.user_id").unwrap_or_default()
}

/// The `profile` of whoever caused the event.
fn sender(custom: &Value) -> Option<User> {
  let profile = custom.at("profile");
  let id = profile.str("user_id").filter(|id| !id.is_empty())?;
  Some(User {
    name: profile.str("nickname").unwrap_or_else(|| id.clone()),
    avatar: profile.str("avatar").filter(|a| !a.is_empty()),
    url: Some(refs::user_url(&id)),
    id,
    ..User::default()
  })
}
