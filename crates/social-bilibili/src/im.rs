//! Private messages (私信) through the shared chat commands: sessions are
//! chats (`dm:MID`), their messages are posts with ids `MID/SEQNO`.

use std::collections::BTreeMap;

use jiff::Timestamp;
use social_core::text::from_secs;
use social_core::{
  Action, Collection, Ctx, Error, Message, Page, PageReq, Post, Result, User, Value, ValueExt, json,
};

use crate::{account, api, parse, refs};

const SESSIONS: &str = "https://api.vc.bilibili.com/session_svr/v1/session_svr/get_sessions";
const MESSAGES: &str = "https://api.vc.bilibili.com/svr_sync/v1/svr_sync/fetch_session_msgs";
const SEND: &str = "https://api.vc.bilibili.com/web_im/v1/web_im/send_msg";
const UNREAD: &str = "https://api.vc.bilibili.com/session_svr/v1/session_svr/single_unread";
const CARDS: &str = "https://api.vc.bilibili.com/account/v1/user/cards";

fn chat_url(mid: impl std::fmt::Display) -> String {
  format!("https://message.bilibili.com/#/whisper/mid{mid}")
}

/// The other person of a chat argument: `dm:MID`, a whisper or space link,
/// or a numeric uid. Names are refused: a fuzzy name match must never pick
/// whom a message goes to.
pub fn talker(chat: &str) -> Result<String> {
  let chat = chat.trim();
  let mid = chat
    .strip_prefix("dm:")
    .or_else(|| chat.rsplit_once("whisper/mid").map(|(_, m)| m))
    .map(|m| m.trim_end_matches('/').to_owned())
    .or_else(|| match refs::user(chat) {
      refs::UserRef::Mid(mid) => Some(mid.to_string()),
      refs::UserRef::Name(_) => None,
    });
  match mid {
    Some(mid) if !mid.is_empty() && mid.bytes().all(|b| b.is_ascii_digit()) => Ok(mid),
    _ => Err(Error::input(format!(
      "not a private chat: {chat} (expected dm:MID from `chats`, a uid or a space link)"
    ))),
  }
}

/// Names and avatars of user ids (one request for all).
async fn cards(ctx: &Ctx, mids: &[String]) -> BTreeMap<String, Value> {
  if mids.is_empty() {
    return BTreeMap::new();
  }
  let data = api::get(ctx, CARDS)
    .arg("uids", mids.join(","))
    .arg("build", 0)
    .arg("mobi_app", "web")
    .send()
    .await
    .unwrap_or_default();
  data
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|c| Some((c.str("mid")?, c.clone())))
    .collect()
}

/// Private conversations, most recent first; the cursor is the last `session_ts`.
pub async fn chats(ctx: &Ctx, page: &PageReq) -> Result<Page<Collection>> {
  ctx.require_login(api::LOGIN_COOKIES)?;
  let mut call = api::get(ctx, SESSIONS)
    .arg("session_type", 1)
    .arg("group_fold", 1)
    .arg("unfollow_fold", 0)
    .arg("sort_rule", 2)
    .arg("build", 0)
    .arg("mobi_app", "web");
  if let Some(end) = page.cursor.as_deref() {
    call = call.arg("end_ts", end);
  }
  let data = call.wbi().send().await?;
  let sessions = data.list("session_list");
  let people: Vec<String> = sessions
    .iter()
    .filter(|s| s.u64("session_type") == Some(1))
    .filter_map(|s| s.str("talker_id"))
    .collect();
  let cards = cards(ctx, &people).await;
  let items: Vec<Collection> = sessions
    .iter()
    .map(|s| session(s, cards.get(&s.str("talker_id").unwrap_or_default())))
    .collect();
  let next = (data.u64("has_more") == Some(1))
    .then(|| sessions.last().and_then(|s| s.str("session_ts")))
    .flatten();
  Ok(Page::new(items, next))
}

fn session(s: &Value, card: Option<&Value>) -> Collection {
  let talker = s.str("talker_id").unwrap_or_default();
  let group = s.u64("session_type") == Some(2);
  let name = card
    .and_then(|c| c.str("name"))
    .or_else(|| s.str("group_name"))
    .unwrap_or_else(|| talker.clone());
  let last = s.at("last_msg");
  let mut c = Collection {
    id: if group {
      format!("group:{talker}")
    } else {
      format!("dm:{talker}")
    },
    kind: if group { "group" } else { "dm" }.into(),
    name,
    description: text(last.u64("msg_type"), last.str("content").as_deref()),
    url: (!group).then(|| chat_url(&talker)),
    raw: Some(s.clone()),
    ..Collection::default()
  };
  if let Some(n) = s.u64("unread_count").filter(|n| *n > 0) {
    c.extra.insert("unread".into(), json!(n));
  }
  if let Some(at) = last.i64("timestamp").and_then(from_secs) {
    c.extra.insert("last_message_at".into(), json!(at));
  }
  if s.u64("is_follow") == Some(1) {
    c.extra.insert("following".into(), json!(true));
  }
  c
}

/// Readable text of a message: text messages carry `{"content": …}`.
fn text(kind: Option<u64>, content: Option<&str>) -> Option<String> {
  let content = content?;
  let v: Value = serde_json::from_str(content).unwrap_or(Value::Null);
  Some(match kind {
    Some(1) => v.str("content").unwrap_or_else(|| content.to_owned()),
    Some(2) => "[image]".into(),
    Some(5) => "[withdrawn]".into(),
    Some(6) => "[sticker]".into(),
    Some(7) => v
      .str("title")
      .map(|t| format!("[share] {t}"))
      .unwrap_or_else(|| "[share]".into()),
    _ => v
      .first_str(&["content", "title", "text"])
      .unwrap_or_else(|| "[message]".into()),
  })
}

/// Messages of one conversation, newest first; the cursor is the oldest seqno seen.
pub async fn messages(ctx: &Ctx, chat: &str, page: &PageReq) -> Result<Page<Post>> {
  ctx.require_login(api::LOGIN_COOKIES)?;
  let talker = talker(chat)?;
  let mut call = api::get(ctx, MESSAGES)
    .arg("talker_id", &talker)
    .arg("session_type", 1)
    .arg("size", page.size_within(50))
    .arg("sender_device_id", 1)
    .arg("build", 0)
    .arg("mobi_app", "web")
    .arg("web_location", "333.1296");
  if let Some(end) = page.cursor.as_deref() {
    call = call.arg("end_seqno", end);
  }
  let data = call.wbi().send().await?;
  let me = account::my_mid(ctx).await.unwrap_or_default();
  let senders: Vec<String> = [talker.clone(), me.clone()]
    .into_iter()
    .filter(|m| !m.is_empty())
    .collect();
  let cards = cards(ctx, &senders).await;
  let mut posts: Vec<Post> = data
    .list("messages")
    .iter()
    .map(|m| message(m, &talker, &cards))
    .collect();
  // Upstream sends oldest first within a page.
  posts.reverse();
  let next = (data.u64("has_more") == Some(1))
    .then(|| data.str("min_seqno"))
    .flatten();
  Ok(Page::new(posts, next))
}

fn message(m: &Value, talker: &str, cards: &BTreeMap<String, Value>) -> Post {
  let sender = m.str("sender_uid").unwrap_or_default();
  let card = cards.get(&sender);
  let kind = m.u64("msg_type");
  let mut post = Post {
    id: format!("{talker}/{}", m.str("msg_seqno").unwrap_or_default()),
    kind: "message".into(),
    text: text(kind, m.str("content").as_deref()),
    author: Some(User {
      url: Some(refs::space_url(&sender)),
      name: card
        .and_then(|c| c.str("name"))
        .unwrap_or_else(|| sender.clone()),
      avatar: card.and_then(|c| c.str("face")).map(parse::https),
      id: sender,
      ..User::default()
    }),
    created_at: m.i64("timestamp").and_then(from_secs),
    raw: Some(m.clone()),
    ..Post::default()
  };
  if kind == Some(2)
    && let Some(url) = m
      .str("content")
      .and_then(|c| serde_json::from_str::<Value>(&c).ok())
      .and_then(|c| c.str("url"))
  {
    post
      .media
      .push(social_core::Media::image(parse::https(url)));
  }
  post
    .extra
    .insert("chat".into(), json!(format!("dm:{talker}")));
  post
}

/// Send a text message (attachments are not supported).
pub async fn send(ctx: &Ctx, chat: &str, msg: &Message) -> Result<Action> {
  if !msg.files.is_empty() {
    return Err(Error::unsupported(
      "send --attach (Bilibili private messages)",
    ));
  }
  if msg.reply_to.is_some() {
    return Err(Error::unsupported(
      "send --reply-to (Bilibili private messages)",
    ));
  }
  let receiver = talker(chat)?;
  let me = account::my_mid(ctx).await?;
  let dev = device_id();
  let content = json!({ "content": msg.text }).to_string();
  let data = api::post(ctx, SEND)
    .arg("msg[sender_uid]", &me)
    .arg("msg[receiver_id]", &receiver)
    .arg("msg[receiver_type]", 1)
    .arg("msg[msg_type]", 1)
    .arg("msg[msg_status]", 0)
    .arg("msg[dev_id]", &dev)
    .arg("msg[timestamp]", Timestamp::now().as_second())
    .arg("msg[new_face_version]", 1)
    .arg("msg[content]", content)
    .arg("from_firework", 0)
    .arg("build", 0)
    .arg("mobi_app", "web")
    .query("w_sender_uid", &me)
    .query("w_receiver_id", &receiver)
    .query("w_dev_id", &dev)
    .wbi()
    .send()
    .await?;
  let mut action = Action::done("send", format!("dm:{receiver}")).with_url(chat_url(&receiver));
  if let Some(key) = data.str("msg_key") {
    action = action.with_id(format!("{receiver}/{key}"));
  }
  Ok(action)
}

/// A random UUID-shaped device id, as the web client sends.
fn device_id() -> String {
  let r: u128 = rand::random();
  let hex = format!("{r:032X}");
  format!(
    "{}-{}-4{}-{}-{}",
    &hex[0..8],
    &hex[8..12],
    &hex[13..16],
    &hex[16..20],
    &hex[20..32]
  )
}

/// Unread private messages: from people you follow, others, and groups.
pub async fn unread(ctx: &Ctx) -> Result<BTreeMap<String, u64>> {
  let data = api::get(ctx, UNREAD)
    .arg("unread_type", 0)
    .arg("show_dustbin", 1)
    .arg("build", 0)
    .arg("mobi_app", "web")
    .send()
    .await?;
  Ok(
    [
      ("dm_followed", "follow_unread"),
      ("dm_others", "unfollow_unread"),
      ("dm_groups", "biz_msg_follow_unread"),
    ]
    .into_iter()
    .filter_map(|(k, path)| Some((k.to_owned(), data.u64(path)?)))
    .collect(),
  )
}
