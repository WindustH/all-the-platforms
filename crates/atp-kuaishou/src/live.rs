//! `listen live:ROOM`: a live room's danmaku socket (protocol in
//! [`crate::gateway`]). Kuaishou hands the socket token only to sessions
//! with the live site's tickets; they are minted from `passToken` when the
//! session lacks them. Comments, gifts and system notices are printed;
//! likes and shares are not.

use std::collections::BTreeMap;
use std::time::Duration;

use atp_core::chat;
use atp_core::live::Backoff;
use atp_core::output::Stream;
use atp_core::socket::Frame;
use atp_core::text::from_millis;
use atp_core::{Error, ErrorCode, Post, Result, User, ValueExt, json};
use tokio::time::{Instant, interval_at};

use crate::api::{Api, LIVE};
use crate::gateway::{self, Msg};
use crate::sign::now_ms;
use crate::{login, refs, rooms};

const SOCKET_INFO: &str = "/live_api/liveroom/websocketinfo";
const HEARTBEAT: Duration = Duration::from_secs(20);
/// A connection that lasted this long resets the reconnect delay.
const STABLE: Duration = Duration::from_secs(60);

/// Follow every room in `rooms` until the connections fail.
pub async fn listen(api: &Api, rooms: &[String], out: &Stream) -> Result<()> {
  if rooms.is_empty() {
    return Err(Error::input(
      "give the live rooms to follow: `listen live:ID` or a live.kuaishou.com/u/… link",
    ));
  }
  api.require_login()?;
  login::live_tickets(api).await?;
  let mut tasks = Vec::new();
  for arg in rooms {
    let room = rooms::room(api, arg).await?;
    tasks.push(room_loop(api, room, out));
  }
  futures_util::future::try_join_all(tasks).await?;
  Ok(())
}

/// Connect, and reconnect with growing delays, until an error that is not
/// the network's.
async fn room_loop(api: &Api, room: Post, out: &Stream) -> Result<()> {
  let stream = rooms::stream_id(&room)?;
  let (token, urls) = socket_info(api, &room.id, &stream).await?;
  let chat = Chat {
    gifts: rooms::gift_names(api, &room.id, &stream).await,
    room: room.id,
    title: room.title.unwrap_or_default(),
  };
  let mut backoff = Backoff::default();
  for attempt in 0.. {
    let url = &urls[attempt % urls.len()];
    let started = Instant::now();
    match session(api, url, &token, &stream, attempt as u64, &chat, out).await {
      Ok(()) => tracing::debug!("live socket of {} closed", chat.room),
      Err(e) if e.code == ErrorCode::NetworkError => tracing::debug!("live socket: {e}"),
      Err(e) => return Err(e),
    }
    if started.elapsed() > STABLE {
      backoff.reset();
    }
    backoff.wait().await;
  }
  Ok(())
}

/// The socket token and addresses of a stream; the live tickets are
/// minted again once when Kuaishou gives no token.
async fn socket_info(api: &Api, room: &str, stream: &str) -> Result<(String, Vec<String>)> {
  if let Some(info) = try_socket_info(api, room, stream).await? {
    return Ok(info);
  }
  tracing::debug!("websocketinfo gave no token; minting the live tickets again");
  login::renew_live_tickets(api).await?;
  try_socket_info(api, room, stream).await?.ok_or_else(|| {
    Error::auth("Kuaishou gave no live socket token (the live site wants a login)")
      .with_hint(crate::api::LOGIN_HINT)
  })
}

async fn try_socket_info(
  api: &Api,
  room: &str,
  stream: &str,
) -> Result<Option<(String, Vec<String>)>> {
  let data = api
    .live(SOCKET_INFO)
    .arg("liveStreamId", stream)
    .referer(&refs::live_url(room))
    .send()
    .await?;
  let urls: Vec<String> = data
    .list("websocketUrls")
    .iter()
    .filter_map(|u| u.as_str().map(str::to_owned))
    .collect();
  Ok(
    data
      .str("token")
      .filter(|_| !urls.is_empty())
      .map(|token| (token, urls)),
  )
}

/// One connection: enter the room, then heartbeat at the interval the
/// server asks for and print what it pushes.
async fn session(
  api: &Api,
  url: &str,
  token: &str,
  stream: &str,
  attempt: u64,
  chat: &Chat,
  out: &Stream,
) -> Result<()> {
  let mut sock = api
    .ctx
    .http
    .websocket(url, &[("Origin", LIVE)], false)
    .await?;
  sock
    .send_binary(gateway::enter_room(token, stream, attempt))
    .await?;
  let mut beat = interval_at(Instant::now() + HEARTBEAT, HEARTBEAT);
  loop {
    tokio::select! {
      _ = beat.tick() => sock.send_binary(gateway::heartbeat(now_ms())).await?,
      frame = sock.next() => match frame? {
        Some(Frame::Binary(data)) => {
          let frame = gateway::decode(&data);
          let msg = Msg::parse(&frame.payload);
          match frame.kind {
            gateway::SC_ENTER_ROOM_ACK => {
              let every = Duration::from_millis(msg.int(3).unwrap_or(20_000).max(1_000));
              beat = interval_at(Instant::now() + every, every);
            }
            gateway::SC_FEED_PUSH => chat.events(&msg).iter().for_each(|p| out.post(p)),
            gateway::SC_ERROR => return Err(Error::upstream(format!(
              "Kuaishou live socket: {} ({})",
              msg.str(2).unwrap_or_default(),
              msg.int(1).unwrap_or(0)
            ))),
            _ => {}
          }
        }
        Some(Frame::Text(_)) => {}
        None => {
          let _ = sock.send_binary(gateway::exit(now_ms())).await;
          return Ok(());
        }
      },
    }
  }
}

struct Chat {
  room: String,
  title: String,
  gifts: BTreeMap<String, String>,
}

impl Chat {
  /// `SCWebFeedPush`: comments (5), gifts (9) and system notices (11).
  fn events(&self, push: &Msg) -> Vec<Post> {
    let mut posts = Vec::new();
    for f in push.msgs(5) {
      posts.push(self.post("message", &f, f.str(3), f.int(9)));
    }
    for f in push.msgs(9) {
      let gift = f.int(4).map(|g| g.to_string()).unwrap_or_default();
      let name = self.gifts.get(&gift).cloned().unwrap_or(gift);
      let count = [f.int(7), f.int(8)]
        .into_iter()
        .flatten()
        .find(|n| *n > 0)
        .unwrap_or(1);
      let text = format!("{name} × {count}");
      posts.push(self.post("gift", &f, Some(text), f.int(3)));
    }
    for f in push.msgs(11) {
      posts.push(self.post("notice", &f, f.str(4), f.int(3)));
    }
    posts
  }

  /// A feed (`{id = 1, user = 2, ...}`) as a message of the room.
  fn post(&self, kind: &str, feed: &Msg, text: Option<String>, at: Option<u64>) -> Post {
    let id = feed
      .str(1)
      .unwrap_or_else(|| format!("{kind}-{}", at.unwrap_or_else(now_ms)));
    let author = feed.msg(2).and_then(|u| {
      let uid = u.str(1)?;
      Some(User {
        url: Some(refs::live_url(&uid)),
        name: u.str(2).unwrap_or_default(),
        avatar: u.str(3),
        id: uid,
        ..User::default()
      })
    });
    let mut post = Post {
      id: chat::message_id(&self.room, &id),
      kind: kind.into(),
      text,
      author,
      created_at: at.and_then(|ms| from_millis(ms as i64)),
      ..Post::default()
    };
    post
      .extra
      .insert("chat".into(), json!(format!("live:{}", self.room)));
    post
      .extra
      .insert("chat_name".into(), json!(self.title.clone()));
    post
  }
}
