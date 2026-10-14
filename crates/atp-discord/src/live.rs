//! `listen`: the realtime gateway (`wss://gateway.discord.gg`). Messages,
//! edits, deletions and reactions become posts on the stream as they come.
//!
//! Bots identify with intents (guilds, guild and direct messages and their
//! reactions, message content); without the privileged Message Content
//! intent the connection is refused once and retried without it, so text
//! arrives only for mentions and direct messages. User accounts identify like
//! the web client and, for the chats named, subscribe to their servers so
//! large servers send their messages too. Lost connections reconnect.

use std::collections::HashSet;
use std::time::Duration;

use atp_core::chat;
use atp_core::live::Backoff;
use atp_core::output::{Stream, note};
use atp_core::socket::Socket;
use atp_core::{Error, Post, Result, User, Value, ValueExt, json};
use tokio::time::Instant;

use crate::api::{Api, Kind, SOCKET_HEADERS, Token};
use crate::refs::{self, Chat};
use crate::{chats, parse};

const USER_GATEWAY: &str = "wss://gateway.discord.gg/?encoding=json&v=9";
const BOT_GATEWAY: &str = "wss://gateway.discord.gg/?encoding=json&v=10";
const GUILDS: u64 = 1 << 0;
const GUILD_MESSAGES: u64 = 1 << 9;
const GUILD_MESSAGE_REACTIONS: u64 = 1 << 10;
const DIRECT_MESSAGES: u64 = 1 << 12;
const DIRECT_MESSAGE_REACTIONS: u64 = 1 << 13;
const MESSAGE_CONTENT: u64 = 1 << 15;
/// Give up after this many connections in a row that never got ready.
const MAX_FAILURES: u32 = 3;

/// Which events to print: all, or those of the named channels and servers.
struct Filter {
  ids: HashSet<String>,
}

impl Filter {
  fn wants(&self, channel: Option<&str>, guild: Option<&str>) -> bool {
    self.ids.is_empty()
      || channel.is_some_and(|c| self.ids.contains(c))
      || guild.is_some_and(|g| self.ids.contains(g))
  }
}

/// How a connection ended.
struct Ended {
  /// It got as far as READY.
  ready: bool,
  /// The server closed it soon after we subscribed to servers.
  refused_subscriptions: bool,
}

pub async fn listen(api: &Api, chats: &[String], out: &Stream) -> Result<()> {
  let token = api.require()?;
  // A bad token fails here instead of in a reconnect loop.
  api.get("/users/@me", &[]).await?;
  let filter = Filter {
    ids: channel_ids(api, chats).await?,
  };
  let mut content = true;
  let mut subscribe = token.kind == Kind::User && !filter.ids.is_empty();
  let mut failures = 0;
  let mut backoff = Backoff::default();
  loop {
    let ended = session(api, &token, &filter, out, content, subscribe).await?;
    if ended.refused_subscriptions {
      subscribe = false;
    }
    if ended.ready {
      failures = 0;
      backoff.reset();
    } else if token.kind == Kind::Bot && content {
      note(
        "The bot may not use the Message Content intent; listening without it (text only for mentions and DMs).",
      );
      content = false;
      continue;
    } else {
      failures += 1;
      if failures >= MAX_FAILURES {
        return Err(Error::upstream(
          "the Discord gateway keeps closing the connection before it is ready",
        ));
      }
    }
    tracing::debug!("gateway closed; reconnecting");
    backoff.wait().await;
  }
}

/// The channels (or servers) to listen to; a user means the DM with them.
async fn channel_ids(api: &Api, chats: &[String]) -> Result<HashSet<String>> {
  let mut ids = HashSet::new();
  for arg in chats {
    match refs::chat(arg)? {
      Chat::User(user) => {
        ids.insert(chats::open_dm(api, &user).await?);
      }
      Chat::Id(id) | Chat::Channel(id) | Chat::Server(id) => {
        ids.insert(id);
      }
    }
  }
  Ok(ids)
}

/// Open the gateway as a bot library or the web app would; its heartbeat
/// interval comes with the `hello`.
async fn connect(api: &Api, token: &Token) -> Result<(Socket, Duration)> {
  let (url, headers): (&str, Vec<(&str, &str)>) = match token.kind {
    Kind::Bot => (
      BOT_GATEWAY,
      vec![(
        "User-Agent",
        "DiscordBot (https://github.com/WindustH/atp)",
      )],
    ),
    Kind::User => (USER_GATEWAY, SOCKET_HEADERS.to_vec()),
  };
  let mut ws = api.ctx.http.websocket(url, &headers, false).await?;
  let hello = ws
    .next_json()
    .await?
    .ok_or_else(|| Error::upstream("the Discord gateway closed before saying hello"))?;
  let interval = Duration::from_millis(hello.u64("d.heartbeat_interval").unwrap_or(41_250));
  Ok((ws, interval))
}

/// One gateway connection, until Discord closes it or asks to reconnect.
async fn session(
  api: &Api,
  token: &Token,
  filter: &Filter,
  out: &Stream,
  content: bool,
  subscribe: bool,
) -> Result<Ended> {
  let (mut ws, interval) = connect(api, token).await?;
  ws.send_json(&identify(api, token, content).await).await?;
  let mut seq: Option<u64> = None;
  let mut ended = Ended {
    ready: false,
    refused_subscriptions: false,
  };
  let mut subscribed_at: Option<Instant> = None;
  // The first beat comes after a random part of the interval, as Discord asks.
  let mut beat = Instant::now() + interval.mul_f64(rand::random_range(0.1..1.0));
  loop {
    tokio::select! {
      frame = ws.next_json() => {
        let Some(v) = frame? else {
          ended.refused_subscriptions =
            subscribed_at.is_some_and(|at| at.elapsed() < Duration::from_secs(10));
          return Ok(ended);
        };
        if let Some(s) = v.u64("s") {
          seq = Some(s);
        }
        match v.u64("op") {
          Some(0) => {
            let event = v.str("t").unwrap_or_default();
            if event == "READY" {
              ended.ready = true;
              ready(api, v.at("d"));
              if subscribe && subscribed_at.is_none() {
                subscribe_to(api, &mut ws, filter).await?;
                subscribed_at = Some(Instant::now());
              }
            }
            dispatch(api, &event, v.at("d"), filter, out);
          }
          Some(1) => ws.send_json(&json!({"op": 1, "d": seq})).await?,
          // Reconnect, or a session Discord dropped: start over.
          Some(7) | Some(9) => return Ok(ended),
          _ => {}
        }
      }
      _ = tokio::time::sleep_until(beat) => {
        ws.send_json(&json!({"op": 1, "d": seq})).await?;
        beat = Instant::now() + interval;
      }
    }
  }
}

async fn identify(api: &Api, token: &Token, content: bool) -> Value {
  match token.kind {
    Kind::Bot => {
      let mut intents = GUILDS
        | GUILD_MESSAGES
        | GUILD_MESSAGE_REACTIONS
        | DIRECT_MESSAGES
        | DIRECT_MESSAGE_REACTIONS;
      if content {
        intents |= MESSAGE_CONTENT;
      }
      json!({"op": 2, "d": {
        "token": token.secret,
        "intents": intents,
        "properties": {"os": std::env::consts::OS, "browser": "all-the-platforms", "device": "all-the-platforms"},
      }})
    }
    Kind::User => json!({"op": 2, "d": {
      "token": token.secret,
      "capabilities": 30717,
      "properties": api.client_properties().await,
      "presence": {"status": "unknown", "since": 0, "activities": [], "afk": false},
      "compress": false,
      "client_state": {"guild_versions": {}},
    }}),
  }
}

/// Learn channel names from READY (user accounts get every server's channels).
fn ready(api: &Api, d: &Value) {
  for c in d.list("private_channels") {
    api.remember_channel(c);
  }
  for g in d.list("guilds") {
    remember_guild(api, g);
  }
}

fn remember_guild(api: &Api, g: &Value) {
  let Some(id) = g.str("id") else { return };
  for c in g.list("channels").iter().chain(g.list("threads")) {
    let mut c = c.clone();
    if let Some(o) = c.as_object_mut() {
      o.entry("guild_id").or_insert(json!(id));
    }
    api.remember_channel(&c);
  }
}

/// Ask for the messages of the named channels' servers (large servers send
/// nothing to user accounts otherwise).
async fn subscribe_to(api: &Api, ws: &mut Socket, filter: &Filter) -> Result<()> {
  let mut subs = serde_json::Map::new();
  for id in &filter.ids {
    let (guild, channel) = match api.channel(id).await {
      Ok(info) if info.guild != "@me" => (info.guild, Some(id.clone())),
      Ok(_) => continue,
      // Not a channel: a server.
      Err(_) => (id.clone(), None),
    };
    let entry = subs.entry(guild).or_insert_with(|| {
      json!({"typing": true, "threads": true, "activities": true, "member_updates": false,
             "members": [], "channels": {}, "thread_member_lists": []})
    });
    if let Some(c) = channel {
      entry["channels"][c] = json!([[0, 99]]);
    }
  }
  if !subs.is_empty() {
    ws.send_json(&json!({"op": 37, "d": {"subscriptions": subs}}))
      .await?;
  }
  Ok(())
}

fn dispatch(api: &Api, event: &str, d: &Value, filter: &Filter, out: &Stream) {
  let channel = d.str("channel_id");
  let guild = d.str("guild_id");
  match event {
    "GUILD_CREATE" => remember_guild(api, d),
    "CHANNEL_CREATE" | "CHANNEL_UPDATE" | "THREAD_CREATE" | "THREAD_UPDATE" => {
      api.remember_channel(d);
    }
    _ => {}
  }
  if !event.starts_with("MESSAGE_") || !filter.wants(channel.as_deref(), guild.as_deref()) {
    return;
  }
  let place = Place::of(api, channel.as_deref(), guild);
  for mut post in message_events(event, d, &place) {
    post.extra.insert("event".into(), json!(event));
    // Every event would repeat its payload; the stream shows the parsed form.
    post.raw = None;
    out.post(&post);
  }
}

/// Where an event happened: the guild (`@me` for direct chats) and the
/// channel's name when known.
#[derive(Default)]
struct Place {
  guild: String,
  chat_name: Option<String>,
}

impl Place {
  fn of(api: &Api, channel: Option<&str>, guild: Option<String>) -> Self {
    let Some(channel) = channel else {
      return Self::default();
    };
    let info = api.cached(channel);
    Self {
      guild: guild
        .or_else(|| info.as_ref().map(|i| i.guild.clone()))
        .unwrap_or_else(|| "@me".into()),
      chat_name: info.map(|i| i.name),
    }
  }
}

/// The posts a `MESSAGE_*` event becomes.
fn message_events(event: &str, d: &Value, place: &Place) -> Vec<Post> {
  let (guild, chat_name) = (place.guild.as_str(), place.chat_name.as_deref());
  match event {
    "MESSAGE_CREATE" => vec![parse::message(d, Some(guild), chat_name)],
    // Embed unfurls arrive as updates without an author; skip those.
    "MESSAGE_UPDATE" if d.at("author").is_object() => {
      let mut p = parse::message(d, Some(guild), chat_name);
      p.kind = "edit".into();
      vec![p]
    }
    "MESSAGE_DELETE" => {
      let id = d.str("id").unwrap_or_default();
      vec![event_post(d, "delete", &id, guild, chat_name)]
    }
    "MESSAGE_DELETE_BULK" => d
      .list("ids")
      .iter()
      .filter_map(Value::as_str)
      .map(|id| event_post(d, "delete", id, guild, chat_name))
      .collect(),
    "MESSAGE_REACTION_ADD" | "MESSAGE_REACTION_REMOVE" => {
      vec![reaction_post(d, event.ends_with("ADD"), guild, chat_name)]
    }
    _ => Vec::new(),
  }
}

fn reaction_post(d: &Value, added: bool, guild: &str, chat_name: Option<&str>) -> Post {
  let kind = if added { "reaction" } else { "unreaction" };
  let message = d.str("message_id").unwrap_or_default();
  let mut p = event_post(d, kind, &message, guild, chat_name);
  p.text = Some(parse::emoji_name(d.at("emoji")));
  p.author = Some(if d.at("member.user").is_object() {
    parse::member(d.at("member"))
  } else {
    User {
      id: d.str("user_id").unwrap_or_default(),
      ..User::default()
    }
  });
  p
}

/// An event about a message that is not itself a message payload.
fn event_post(d: &Value, kind: &str, message: &str, guild: &str, chat_name: Option<&str>) -> Post {
  let channel = d.str("channel_id").unwrap_or_default();
  let mut p = Post {
    id: chat::message_id(&channel, message),
    kind: kind.into(),
    url: Some(parse::permalink(guild, &channel, Some(message))),
    created_at: Some(jiff::Timestamp::now()),
    raw: Some(d.clone()),
    ..Post::default()
  };
  p.extra.insert("chat".into(), json!(channel));
  if let Some(name) = chat_name {
    p.extra.insert("chat_name".into(), json!(name));
  }
  p.extra.insert("message_id".into(), json!(message));
  p
}
