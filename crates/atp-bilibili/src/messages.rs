//! Private messages (私信) through the shared chat commands: sessions are
//! chats (`dm:MID`), their messages are posts with ids `MID/SEQNO`.

use std::collections::BTreeMap;

use atp_core::chat;
use atp_core::text::from_secs;
use atp_core::{
  Action, Collection, Error, Message, Page, PageReq, Post, Result, User, Value, ValueExt, json,
};
use jiff::Timestamp;

use crate::api::Api;
use crate::users;
use crate::{parse, refs};

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
async fn cards(api: &Api, mids: &[String]) -> BTreeMap<String, Value> {
  if mids.is_empty() {
    return BTreeMap::new();
  }
  let data = api
    .get(CARDS)
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
pub async fn chats(api: &Api, page: &PageReq) -> Result<Page<Collection>> {
  api.require_login()?;
  let mut call = api
    .get(SESSIONS)
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
  let cards = cards(api, &people).await;
  let items: Vec<Collection> = sessions
    .iter()
    .map(|s| session(s, cards.get(&s.str("talker_id").unwrap_or_default())))
    .collect();
  let next = (data.u64("has_more") == Some(1))
    .then(|| sessions.last().and_then(|s| s.str("session_ts")))
    .flatten();
  Ok(Page::new(items, next))
}

fn session(v: &Value, card: Option<&Value>) -> Collection {
  let talker = v.str("talker_id").unwrap_or_default();
  let group = v.u64("session_type") == Some(2);
  let name = card
    .and_then(|c| c.str("name"))
    .or_else(|| v.str("group_name"))
    .unwrap_or_else(|| talker.clone());
  let last = v.at("last_msg");
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
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if let Some(n) = v.u64("unread_count").filter(|n| *n > 0) {
    c.extra.insert("unread".into(), json!(n));
  }
  if let Some(at) = last.i64("timestamp").and_then(from_secs) {
    c.extra.insert("last_message_at".into(), json!(at));
  }
  if v.u64("is_follow") == Some(1) {
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
pub async fn messages(api: &Api, chat: &str, page: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let talker = talker(chat)?;
  let mut call = api
    .get(MESSAGES)
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
  let me = users::my_mid(api).await.unwrap_or_default();
  let senders: Vec<String> = [talker.clone(), me.clone()]
    .into_iter()
    .filter(|m| !m.is_empty())
    .collect();
  let cards = cards(api, &senders).await;
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

fn message(v: &Value, talker: &str, cards: &BTreeMap<String, Value>) -> Post {
  let sender = v.str("sender_uid").unwrap_or_default();
  let card = cards.get(&sender);
  let kind = v.u64("msg_type");
  let mut post = Post {
    id: chat::message_id(talker, &v.str("msg_seqno").unwrap_or_default()),
    kind: "message".into(),
    text: text(kind, v.str("content").as_deref()),
    author: Some(User {
      url: Some(refs::space_url(&sender)),
      name: card
        .and_then(|c| c.str("name"))
        .unwrap_or_else(|| sender.clone()),
      avatar: card.and_then(|c| c.str("face")).map(parse::https),
      id: sender,
      ..User::default()
    }),
    created_at: v.i64("timestamp").and_then(from_secs),
    raw: Some(v.clone()),
    ..Post::default()
  };
  if kind == Some(2)
    && let Some(url) = v
      .str("content")
      .and_then(|c| serde_json::from_str::<Value>(&c).ok())
      .and_then(|c| c.str("url"))
  {
    post.media.push(atp_core::Media::image(parse::https(url)));
  }
  post
    .extra
    .insert("chat".into(), json!(format!("dm:{talker}")));
  post
}

/// Send a text message (attachments are not supported).
pub async fn send(api: &Api, chat: &str, msg: &Message) -> Result<Action> {
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
  let me = users::my_mid(api).await?;
  let dev = device_id();
  let content = json!({ "content": msg.text }).to_string();
  let data = api
    .post(SEND)
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
    action = action.with_id(chat::message_id(&receiver, &key));
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
pub async fn unread(api: &Api) -> Result<BTreeMap<String, u64>> {
  let data = api
    .get(UNREAD)
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
