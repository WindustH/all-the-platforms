//! Direct messages (the logged-in web session): threads as chats, their
//! items as messages, sending text, reactions, and `listen` by polling the
//! inbox. A message id is `THREAD/ITEM`.

use std::collections::BTreeMap;
use std::time::Duration;

use atp_core::output::Stream;
use atp_core::text::from_secs;
use atp_core::{
  Action, Collection, Error, Media, Message, Page, PageReq, Post, Result, User, Value, ValueExt,
};

use crate::client::Client;
use crate::parse;

/// How often `listen` looks at the inbox.
const POLL: Duration = Duration::from_secs(20);

pub async fn chats(c: &Client, req: &PageReq) -> Result<Page<Collection>> {
  c.require_session("direct messages")?;
  let v = inbox(c, req.cursor.as_deref(), req.size_within(20)).await?;
  let inbox = v.at("inbox");
  let chats = inbox.list("threads").iter().map(thread).collect();
  let next = (inbox.bool("has_older") == Some(true))
    .then(|| inbox.str("oldest_cursor"))
    .flatten();
  Ok(Page::new(chats, next))
}

async fn inbox(c: &Client, cursor: Option<&str>, limit: usize) -> Result<Value> {
  let mut q = vec![
    ("persistentBadging", "true".to_owned()),
    ("limit", limit.to_string()),
    ("thread_message_limit", "1".to_owned()),
  ];
  if let Some(cur) = cursor {
    q.push(("cursor", cur.to_owned()));
  }
  c.get("direct_v2/inbox/", &q).await
}

fn thread(t: &Value) -> Collection {
  let users: Vec<String> = t
    .list("users")
    .iter()
    .filter_map(|u| u.str("username"))
    .collect();
  Collection {
    id: t.str("thread_id").unwrap_or_default(),
    kind: if t.bool("is_group") == Some(true) {
      "group".into()
    } else {
      "dm".into()
    },
    name: t
      .str("thread_title")
      .filter(|n| !n.is_empty())
      .unwrap_or_else(|| users.join(", ")),
    description: t.list("items").first().and_then(|i| i.str("text")),
    extra: [
      ("users".to_owned(), Value::from(users)),
      (
        "last_activity_at".to_owned(),
        t.at("last_activity_at").clone(),
      ),
      (
        "unread".to_owned(),
        Value::from(t.i64("read_state") == Some(1)),
      ),
    ]
    .into(),
    raw: Some(t.clone()),
    ..Collection::default()
  }
}

pub async fn messages(c: &Client, chat: &str, req: &PageReq) -> Result<Page<Post>> {
  c.require_session("direct messages")?;
  let mut q = vec![("limit", req.size_within(20).to_string())];
  if let Some(cur) = &req.cursor {
    q.push(("cursor", cur.clone()));
  }
  let v = c.get(&format!("direct_v2/threads/{chat}/"), &q).await?;
  let t = v.at("thread");
  let users = people(t);
  let posts = t
    .list("items")
    .iter()
    .map(|i| item(chat, i, &users))
    .collect();
  let next = (t.bool("has_older") == Some(true))
    .then(|| t.str("oldest_cursor"))
    .flatten();
  Ok(Page::new(posts, next))
}

fn people(t: &Value) -> BTreeMap<String, User> {
  let mut out: BTreeMap<String, User> = t
    .list("users")
    .iter()
    .map(|u| {
      let u = parse::user(u);
      (u.id.clone(), u)
    })
    .collect();
  if t.at("inviter").is_object() {
    let me = parse::user(t.at("inviter"));
    out.entry(me.id.clone()).or_insert(me);
  }
  out
}

fn item(chat: &str, i: &Value, users: &BTreeMap<String, User>) -> Post {
  let kind = i.str("item_type").unwrap_or_else(|| "text".into());
  let sender = i.str("user_id").unwrap_or_default();
  let mut media = Vec::new();
  for path in ["media", "visual_media.media", "clip.clip", "media_share"] {
    let m = i.at(path);
    if m.is_object() {
      media.extend(parse::media(m).media);
    }
  }
  if let Some(url) = i.str("voice_media.media.audio.audio_src") {
    media.push(Media::new(atp_core::MediaKind::Audio, url));
  }
  Post {
    id: format!("{chat}/{}", i.str("item_id").unwrap_or_default()),
    kind: if kind == "text" {
      "message".into()
    } else {
      kind
    },
    text: i
      .str("text")
      .or_else(|| i.str("link.text"))
      .or_else(|| i.str("reel_share.text")),
    author: Some(users.get(&sender).cloned().unwrap_or(User {
      id: sender,
      ..User::default()
    })),
    // Microseconds.
    created_at: i
      .i64("timestamp")
      .map(|t| t / 1_000_000)
      .and_then(from_secs),
    media,
    extra: [("chat".to_owned(), Value::from(chat))].into(),
    raw: Some(i.clone()),
    ..Post::default()
  }
}

fn context() -> String {
  // A client message id, as the web app makes one.
  format!("{}", rand::random::<u64>() >> 1)
}

pub async fn send(c: &Client, chat: &str, msg: &Message) -> Result<Action> {
  c.require_session("sending messages")?;
  if !msg.files.is_empty() {
    return Err(Error::unsupported("send --attach on Instagram"));
  }
  let ctx = context();
  let mut form = vec![
    ("action", "send_item".to_owned()),
    ("thread_ids", format!("[{chat}]")),
    ("text", msg.text.clone()),
    ("client_context", ctx.clone()),
    ("mutation_token", ctx),
  ];
  if let Some(r) = &msg.reply_to {
    form.push((
      "replied_to_item_id",
      r.rsplit('/').next().unwrap_or(r).to_owned(),
    ));
  }
  let v = c.post("direct_v2/threads/broadcast/text/", &form).await?;
  let id = v.str("payload.item_id").unwrap_or_default();
  Ok(Action::done("send", chat).with_id(format!("{chat}/{id}")))
}

pub async fn react(c: &Client, message: &str, emoji: &str, undo: bool) -> Result<Action> {
  c.require_session("reactions")?;
  let (chat, item_id) = message
    .split_once('/')
    .ok_or_else(|| Error::input("a message is THREAD/ITEM (from `messages`)"))?;
  let ctx = context();
  c.post(
    "direct_v2/threads/broadcast/reaction/",
    &[
      ("thread_ids", format!("[{chat}]")),
      ("item_id", item_id.to_owned()),
      ("node_type", "item".to_owned()),
      ("reaction_type", "like".to_owned()),
      (
        "reaction_status",
        if undo { "deleted" } else { "created" }.to_owned(),
      ),
      ("emoji", emoji.to_owned()),
      ("client_context", ctx.clone()),
      ("mutation_token", ctx),
    ],
  )
  .await?;
  Ok(Action::done(
    if undo { "unreact" } else { "react" },
    message,
  ))
}

/// Poll the inbox and print each new message of `chats` (all when empty).
pub async fn listen(c: &Client, chats: &[String], out: &Stream) -> Result<()> {
  c.require_session("listening to direct messages")?;
  let mut seen: BTreeMap<String, i64> = BTreeMap::new();
  let mut first = true;
  loop {
    let v = inbox(c, None, 20).await?;
    for t in v.list("inbox.threads") {
      let id = t.str("thread_id").unwrap_or_default();
      if !chats.is_empty() && !chats.contains(&id) {
        continue;
      }
      let last = t.i64("last_activity_at").unwrap_or(0);
      let before = seen.insert(id.clone(), last);
      if first || before.is_some_and(|b| b >= last) {
        continue;
      }
      let since = before.unwrap_or(0);
      let page = messages(
        c,
        &id,
        &PageReq {
          cursor: None,
          size: 20,
        },
      )
      .await?;
      let mut fresh: Vec<&Post> = page
        .items
        .iter()
        .filter(|p| {
          p.raw
            .as_ref()
            .and_then(|r| r.i64("timestamp"))
            .is_some_and(|ts| ts > since)
        })
        .collect();
      fresh.reverse();
      for p in fresh {
        out.post(p);
      }
    }
    first = false;
    tokio::time::sleep(POLL).await;
  }
}
