//! Direct messages through the shared chat commands: conversations
//! (`dm/inbox_initial_state`, older pages from `dm/inbox_timeline`), the
//! messages of one (`dm/conversation/<id>`), sending (`dm/new2`), reactions
//! and deletions (GraphQL), and `listen` by polling `dm/user_updates`.
//!
//! A conversation id is `<user>-<user>` for a one-to-one chat (smaller id
//! first) and a number for a group. Messages have no permalink, so their id
//! is `CONVERSATION/MESSAGE`. X's newer end-to-end encrypted chats are not
//! served by these endpoints.

use std::time::Duration;

use social_core::text::from_millis;
use social_core::{
  Action, Collection, Error, Media, Message, Page, PageReq, Post, Result, User, Value, ValueExt,
  json,
};

use crate::api::{Api, REST};
use crate::graphql::{DM_DELETE, DM_REACT, DM_UNREACT};
use crate::refs::{self, user_url};
use crate::upload;
use crate::users;

const POLL: Duration = Duration::from_secs(10);

/// Query parameters the web client sends with every DM request.
const PARAMS: &[(&str, &str)] = &[
  ("nsfw_filtering_enabled", "false"),
  ("include_profile_interstitial_type", "1"),
  ("include_blocking", "1"),
  ("include_blocked_by", "1"),
  ("include_followed_by", "1"),
  ("include_can_dm", "1"),
  ("include_ext_is_blue_verified", "1"),
  ("skip_status", "1"),
  ("cards_platform", "Web-12"),
  ("include_cards", "1"),
  ("include_ext_alt_text", "true"),
  ("include_quote_count", "true"),
  ("include_reply_count", "1"),
  ("tweet_mode", "extended"),
  ("dm_users", "true"),
  ("include_groups", "true"),
  ("include_inbox_timelines", "true"),
  ("include_ext_media_color", "true"),
  ("supports_reactions", "true"),
  ("supports_edit", "true"),
  ("include_conversation_info", "true"),
];

async fn get(api: &Api, path: &str, extra: &[(&str, &str)]) -> Result<Value> {
  api.require_login()?;
  let mut query: Vec<(&str, &str)> = PARAMS.to_vec();
  query.extend_from_slice(extra);
  api.get(&format!("{REST}/dm/{path}.json"), &query).await
}

/// A message reference: `CONVERSATION/MESSAGE` (also `x.com/messages/CONV` + id).
pub fn message_ref(arg: &str) -> Option<(String, String)> {
  let (conv, id) = arg.trim().rsplit_once('/')?;
  let conv = conv
    .rsplit('/')
    .next()
    .filter(|c| !c.is_empty() && c.bytes().all(|b| b.is_ascii_digit() || b == b'-'))?;
  let ok = !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit());
  ok.then(|| (conv.to_owned(), id.to_owned()))
}

/// Conversations, most recent first. The cursor is the inbox's `min_entry_id`.
pub async fn chats(api: &Api, req: &PageReq) -> Result<Page<Collection>> {
  let (box_, key) = match &req.cursor {
    None => (
      get(api, "inbox_initial_state", &[]).await?,
      "inbox_initial_state",
    ),
    Some(c) => (
      get(api, "inbox_timeline/trusted", &[("max_id", c.as_str())]).await?,
      "inbox_timeline",
    ),
  };
  let v = box_.at(key);
  let me = api.own_id().unwrap_or_default();
  let mut items: Vec<(i64, Collection)> = v
    .at("conversations")
    .as_object()
    .into_iter()
    .flatten()
    .map(|(id, c)| (c.i64("sort_timestamp").unwrap_or(0), chat(id, c, v, &me)))
    .collect();
  items.sort_by_key(|item| std::cmp::Reverse(item.0));
  let status = v
    .str("inbox_timelines.trusted.status")
    .or_else(|| v.str("status"));
  let next = v
    .str("inbox_timelines.trusted.min_entry_id")
    .or_else(|| v.str("min_entry_id"))
    .filter(|_| status.as_deref() == Some("HAS_MORE"))
    .filter(|c| Some(c) != req.cursor.as_ref());
  Ok(Page::new(items.into_iter().map(|(_, c)| c).collect(), next))
}

/// One conversation as a chat: its name (a group's, or the other people's).
fn chat(id: &str, c: &Value, v: &Value, me: &str) -> Collection {
  let others: Vec<User> = c
    .list("participants")
    .iter()
    .filter_map(|p| p.str("user_id"))
    .filter(|u| u != me)
    .filter_map(|u| user(v, &u))
    .collect();
  let group = c.str("type").as_deref() == Some("GROUP_DM");
  let name = c.str("name").unwrap_or_else(|| {
    others
      .iter()
      .map(|u| u.name.clone())
      .collect::<Vec<_>>()
      .join(", ")
  });
  let mut chat = Collection {
    id: id.to_owned(),
    kind: if group { "group" } else { "dm" }.into(),
    name,
    url: Some(format!("https://x.com/messages/{id}")),
    items: Some(c.list("participants").len() as u64),
    raw: Some(c.clone()),
    ..Collection::default()
  };
  let handles: Vec<Value> = others
    .iter()
    .filter_map(|u| u.handle.clone().map(Value::from))
    .collect();
  chat.extra.insert("participants".into(), handles.into());
  if let Some(t) = c.i64("sort_timestamp").and_then(from_millis) {
    chat
      .extra
      .insert("last_activity".into(), t.to_string().into());
  }
  let read = c
    .list("participants")
    .iter()
    .find(|p| p.str("user_id").as_deref() == Some(me))
    .and_then(|p| p.str("last_read_event_id"));
  if let (Some(read), Some(max)) = (
    read,
    c.str("max_entry_id").or_else(|| c.str("sort_event_id")),
  ) {
    chat
      .extra
      .insert("unread".into(), (cmp_ids(&max, &read) > 0).into());
  }
  for key in ["muted", "trusted", "read_only"] {
    if let Some(b) = c.bool(key) {
      chat.extra.insert(key.into(), b.into());
    }
  }
  chat
}

/// Compare decimal ids of any length.
fn cmp_ids(a: &str, b: &str) -> i32 {
  match a.len().cmp(&b.len()).then(a.cmp(b)) {
    std::cmp::Ordering::Greater => 1,
    std::cmp::Ordering::Less => -1,
    std::cmp::Ordering::Equal => 0,
  }
}

/// A participant from a response's `users` map (v1.1 user objects).
fn user(v: &Value, id: &str) -> Option<User> {
  let u = v.at(&format!("users.{id}"));
  let handle = u.str("screen_name")?;
  Some(User {
    id: id.to_owned(),
    name: u.str("name").unwrap_or_else(|| handle.clone()),
    url: Some(user_url(&handle)),
    avatar: u.str("profile_image_url_https"),
    verified: u.bool("is_blue_verified").unwrap_or(false) || u.bool("verified").unwrap_or(false),
    handle: Some(handle),
    ..User::default()
  })
}

/// The conversation of a chat argument: a conversation id, or a user (their
/// one-to-one conversation with you).
pub async fn conversation(api: &Api, arg: &str) -> Result<String> {
  let arg = arg.trim();
  let arg = arg.strip_prefix("https://x.com/messages/").unwrap_or(arg);
  if arg.contains('-')
    && arg
      .split('-')
      .all(|p| p.bytes().all(|b| b.is_ascii_digit()))
  {
    return Ok(arg.to_owned());
  }
  if arg.len() > 15 && arg.bytes().all(|b| b.is_ascii_digit()) {
    return Ok(arg.to_owned());
  }
  let other: u128 = users::user_id(api, arg).await?.parse().unwrap_or(0);
  let me: u128 = users::user_id_or_self(api, None)
    .await?
    .parse()
    .unwrap_or(0);
  let (a, b) = if me < other { (me, other) } else { (other, me) };
  Ok(format!("{a}-{b}"))
}

/// Messages of a conversation, newest first; the cursor is its `min_entry_id`.
pub async fn messages(api: &Api, arg: &str, req: &PageReq) -> Result<Page<Post>> {
  let conv = conversation(api, arg).await?;
  let mut extra = vec![("context", "FETCH_DM_CONVERSATION_HISTORY")];
  if let Some(c) = &req.cursor {
    extra.push(("max_id", c.as_str()));
  }
  let v = get(api, &format!("conversation/{conv}"), &extra).await?;
  let tl = v.at("conversation_timeline");
  let name = chat_name(tl, &conv, api);
  let mut items: Vec<Post> = tl
    .list("entries")
    .iter()
    .filter_map(|e| message(e, tl, name.as_deref()))
    .collect();
  items.sort_by_key(|p| std::cmp::Reverse(p.created_at));
  let next = (tl.str("status").as_deref() == Some("HAS_MORE"))
    .then(|| tl.str("min_entry_id"))
    .flatten()
    .filter(|c| Some(c) != req.cursor.as_ref());
  Ok(Page::new(items, next))
}

fn chat_name(v: &Value, conv: &str, api: &Api) -> Option<String> {
  let c = v.at(&format!("conversations.{conv}"));
  if !c.is_object() {
    return None;
  }
  Some(chat(conv, c, v, &api.own_id().unwrap_or_default()).name).filter(|n| !n.is_empty())
}

/// One timeline entry as a post: messages, and reactions / deletions as events.
fn message(entry: &Value, v: &Value, chat_name: Option<&str>) -> Option<Post> {
  let (kind, e) = if entry.at("message").is_object() {
    ("message", entry.at("message"))
  } else if entry.at("reaction_create").is_object() {
    ("reaction", entry.at("reaction_create"))
  } else if entry.at("reaction_delete").is_object() {
    ("unreaction", entry.at("reaction_delete"))
  } else if entry.at("message_delete").is_object() {
    ("delete", entry.at("message_delete"))
  } else {
    return None;
  };
  let conv = e.str("conversation_id")?;
  let data = e.at("message_data");
  let id = data.str("id").or_else(|| e.str("id"))?;
  let sender = data.str("sender_id").or_else(|| e.str("sender_id"));
  let mut post = Post {
    id: format!("{conv}/{id}"),
    kind: kind.into(),
    text: data
      .str("text")
      .or_else(|| e.str("emoji_reaction"))
      .map(|t| expand(&t, data)),
    author: sender.and_then(|s| user(v, &s)),
    created_at: e
      .str("time")
      .and_then(|t| t.parse().ok())
      .and_then(from_millis),
    media: attachment(data.at("attachment")),
    raw: Some(entry.clone()),
    ..Post::default()
  };
  let mut put = |key: &str, value: Option<Value>| {
    if let Some(value) = value {
      post.extra.insert(key.into(), value);
    }
  };
  put("chat", Some(conv.clone().into()));
  put("chat_name", chat_name.map(Value::from));
  put(
    "reply_to",
    data
      .str("reply_data.id")
      .map(|r| Value::from(format!("{conv}/{r}"))),
  );
  put(
    "message",
    e.str("message_id")
      .map(|m| Value::from(format!("{conv}/{m}"))),
  );
  put(
    "tweet",
    data.str("attachment.tweet.expanded_url").map(Value::from),
  );
  if data.at("edit_count").is_number() {
    put("edits", data.u64("edit_count").map(Value::from));
  }
  Some(post)
}

/// The text with t.co links expanded.
fn expand(text: &str, data: &Value) -> String {
  let mut out = text.to_owned();
  for u in data.list("entities.urls") {
    if let (Some(short), Some(long)) = (u.str("url"), u.str("expanded_url")) {
      out = out.replace(&short, &long);
    }
  }
  out.trim().to_owned()
}

fn attachment(a: &Value) -> Vec<Media> {
  let m = ["photo", "video", "animated_gif"]
    .iter()
    .map(|k| a.at(k))
    .find(|m| m.is_object());
  let Some(m) = m else {
    return Vec::new();
  };
  let best = m
    .list("video_info.variants")
    .iter()
    .filter(|v| v.str("content_type").as_deref() == Some("video/mp4"))
    .max_by_key(|v| v.u64("bitrate").unwrap_or(0))
    .and_then(|v| v.str("url"));
  match (best, m.str("media_url_https")) {
    (Some(url), _) => vec![Media::video(url)],
    (None, Some(url)) => vec![Media::image(url)],
    _ => Vec::new(),
  }
}

/// Send a message (text and images; a reply with `reply_to`).
pub async fn send(api: &Api, chat: &str, message: &Message) -> Result<Action> {
  let conv = conversation(api, chat).await?;
  let mut ids = Vec::new();
  for file in &message.files {
    ids.push(upload::dm_image(api, file).await?);
  }
  let reply = match message.reply_to.as_deref() {
    Some(r) => Some(
      message_ref(r)
        .map(|(_, id)| id)
        .ok_or_else(|| Error::input(format!("not a direct message: {r}")))?,
    ),
    None => None,
  };
  api.write_pause().await;
  // A message carries one attachment: the first file goes with the text.
  let mut last = None;
  let texts = std::iter::once(message.text.as_str()).chain(std::iter::repeat(""));
  let parts: Vec<(Option<&String>, &str)> = if ids.is_empty() {
    vec![(None, message.text.as_str())]
  } else {
    ids.iter().map(Some).zip(texts).collect()
  };
  for (media, text) in parts {
    let mut body = json!({
      "conversation_id": conv,
      "text": text,
      "cards_platform": "Web-12",
      "include_cards": 1,
      "include_quote_count": true,
      "dm_users": false,
      "recipient_ids": false,
    });
    if let Some(m) = media {
      body["media_id"] = m.as_str().into();
    }
    if let Some(r) = &reply {
      body["reply_to_dm_id"] = r.as_str().into();
    }
    let v = api
      .post_json(
        &format!("{REST}/dm/new2.json"),
        &[
          ("ext", "mediaColor,altText"),
          ("include_ext_alt_text", "true"),
        ],
        &body,
      )
      .await?;
    last = v
      .list("entries")
      .iter()
      .find_map(|e| e.str("message.id"))
      .or(last);
  }
  let mut action = Action::done("send", &conv).with_url(format!("https://x.com/messages/{conv}"));
  if let Some(id) = last {
    action = action.with_id(format!("{conv}/{id}"));
  }
  Ok(action)
}

/// React to a message with an emoji, or remove the reaction.
pub async fn react(api: &Api, arg: &str, emoji: &str, undo: bool) -> Result<Action> {
  let (conv, id) = message_ref(arg).ok_or_else(|| {
    Error::input(format!(
      "not a direct message: {arg} (X has reactions only on direct messages; use `like` for posts)"
    ))
  })?;
  api.require_login()?;
  api.write_pause().await;
  let op = if undo { &DM_UNREACT } else { &DM_REACT };
  let variables = json!({
    "messageId": id,
    "conversationId": conv,
    "reactionTypes": ["Emoji"],
    "emojiReactions": [emoji],
  });
  api.graphql(op, variables).await?;
  let name = if undo { "unreact" } else { "react" };
  Ok(Action::done(name, format!("{conv}/{id}")).with_message(emoji.to_owned()))
}

/// Delete one of your messages (for you).
pub async fn delete(api: &Api, conv: &str, id: &str) -> Result<Action> {
  api.require_login()?;
  api.write_pause().await;
  api.graphql(&DM_DELETE, json!({ "messageId": id })).await?;
  Ok(Action::done("delete", format!("{conv}/{id}")))
}

/// People in a conversation.
pub async fn members(api: &Api, arg: &str) -> Result<Page<User>> {
  let conv = conversation(api, arg).await?;
  let v = get(
    api,
    &format!("conversation/{conv}"),
    &[("context", "FETCH_DM_CONVERSATION_HISTORY")],
  )
  .await?;
  let tl = v.at("conversation_timeline");
  let c = tl.at(&format!("conversations.{conv}"));
  let users = c
    .list("participants")
    .iter()
    .filter_map(|p| p.str("user_id"))
    .filter_map(|u| user(tl, &u))
    .collect();
  Ok(Page::last(users))
}

/// New messages (and reactions / deletions) as they arrive, polled every few
/// seconds from the inbox's update stream.
pub async fn listen(api: &Api, chats: &[String], out: &social_core::output::Stream) -> Result<()> {
  let mut wanted = Vec::new();
  for c in chats {
    wanted.push(conversation(api, c).await?);
  }
  let start = get(api, "inbox_initial_state", &[]).await?;
  let mut cursor = start
    .str("inbox_initial_state.cursor")
    .ok_or_else(|| Error::upstream("X returned no direct-message cursor"))?;
  loop {
    api
      .ctx
      .http
      .pause(POLL, POLL + Duration::from_secs(5))
      .await;
    let v = get(api, "user_updates", &[("cursor", cursor.as_str())]).await?;
    let ev = v.at("user_events");
    for e in ev.list("entries") {
      let Some(mut post) = message(e, ev, None) else {
        continue;
      };
      let conv = post
        .extra
        .get("chat")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
      if wanted.is_empty() || wanted.contains(&conv) {
        if let Some(name) = chat_name(ev, &conv, api) {
          post.extra.insert("chat_name".into(), name.into());
        }
        out.post(&post);
      }
    }
    if let Some(next) = ev.str("cursor") {
      cursor = next;
    }
  }
}

/// Whether `arg` names a direct message rather than a post.
pub fn is_dm_ref(arg: &str) -> bool {
  message_ref(arg).is_some() && refs::tweet_id(arg).is_err()
}
