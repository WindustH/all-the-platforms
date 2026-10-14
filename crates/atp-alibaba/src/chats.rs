//! Chats both sites keep on IMPaaS: the conversation list, a chat's history
//! (newest first, 20 messages at a time, paged by cursor), opening a direct
//! chat and sending into it; and parsers for what both sites answer alike.
//! Messages have no links: their ids are `CHAT/MESSAGE`.

use atp_core::chat::message_id;
use atp_core::text::from_millis;
use atp_core::{Collection, Error, Extra, Media, Post, Result, User, Value, ValueExt, json};

use crate::impaas::{self, Im, plain};

/// `listUserMessages` pages back from the largest safe integer.
const FIRST_CURSOR: i64 = 9_007_199_254_740_991;
const HISTORY_PAGE: u64 = 20;

/// One page of a listing and the cursor of the next one.
pub struct Batch {
  pub items: Vec<Value>,
  pub next: Option<String>,
}

impl Im {
  /// One page of a chat's messages, newest first.
  pub async fn history(&mut self, cid: &str, cursor: Option<&str>) -> Result<Batch> {
    let start = cursor.and_then(|c| c.parse().ok()).unwrap_or(FIRST_CURSOR);
    let chat = impaas::qualified(self.profile, cid);
    let body = json!([chat, false, start, HISTORY_PAGE, false]);
    let answer = self
      .request("/r/MessageManager/listUserMessages", body)
      .await?;
    Ok(batch(&answer, "userMessageModels", start))
  }

  /// One page of the account's chats, the most recently active first.
  pub async fn conversations(&mut self, cursor: Option<&str>, size: usize) -> Result<Batch> {
    let start = cursor
      .and_then(|c| c.parse().ok())
      .unwrap_or_else(impaas::now_ms);
    let answer = self
      .request("/r/Conversation/listNewestPagination", json!([start, size]))
      .await?;
    Ok(batch(&answer, "userConvs", start))
  }

  /// Open a direct chat (or find the one that exists); its id.
  pub async fn create(&mut self, conversation: Value) -> Result<String> {
    let answer = self
      .request("/r/SingleChatConversation/create", json!([conversation]))
      .await?;
    find_cid(&answer)
      .map(|cid| plain(&cid).to_owned())
      .ok_or_else(|| Error::upstream("the chat server opened no chat"))
  }

  /// Send `message` to `receivers` (both sides of the chat); the message id.
  pub async fn send(&mut self, message: Value, receivers: &[String]) -> Result<String> {
    let uuid = message.str("uuid").unwrap_or_default();
    let body = json!([message, {"actualReceivers": receivers}]);
    let answer = self
      .request("/r/MessageSend/sendByReceiverScope", body)
      .await?;
    Ok(answer.str("messageId").unwrap_or(uuid))
  }
}

fn batch(answer: &Value, key: &str, start: i64) -> Batch {
  let next = answer
    .i64("nextCursor")
    .filter(|n| answer.bool("hasMore") == Some(true) && *n != start)
    .map(|n| n.to_string());
  Batch {
    items: answer.list(key).to_vec(),
    next,
  }
}

/// The first `cid` anywhere in an answer.
fn find_cid(v: &Value) -> Option<String> {
  match v {
    Value::Object(map) => map
      .get("cid")
      .and_then(Value::as_str)
      .map(str::to_owned)
      .or_else(|| map.values().find_map(find_cid)),
    Value::Array(items) => items.iter().find_map(find_cid),
    _ => None,
  }
}

/// The other side of a Taobao chat id (`<uidA>.1-<uidB>.1#11001`).
pub fn cid_peer(cid: &str, me: &str) -> Option<String> {
  let pair = plain(cid).split('#').next()?;
  pair
    .split('-')
    .filter_map(|side| side.split('.').next())
    .find(|uid| !uid.is_empty() && *uid != me && uid.bytes().all(|b| b.is_ascii_digit()))
    .map(str::to_owned)
}

/// A conversation of the list as a chat (`kind` `dm`); `me` is the account.
pub fn chat(conversation: &Value, me: &str) -> Option<Collection> {
  let user_chat = conversation.at("singleChatUserConversation");
  let single = user_chat.at("singleChatConversation");
  let cid = plain(&single.str("cid")?).to_owned();
  let peer = ["pairFirst", "pairSecond"]
    .iter()
    .filter_map(|k| single.str(k))
    .map(|p| plain(&p).to_owned())
    .find(|p| p != me)
    .or_else(|| cid_peer(&cid, me));
  let last = message(user_chat.at("lastMessage"), &cid);
  let peer_name = last
    .as_ref()
    .and_then(|m| m.author.as_ref())
    .filter(|a| peer.as_deref() == Some(a.id.as_str()) && !a.name.is_empty())
    .map(|a| a.name.clone());
  let item_title = single.str("extension.itemTitle");
  let mut extra = Extra::new();
  if let Some(peer) = &peer {
    extra.insert("peer".into(), json!(peer));
  }
  if let Some(item) = single.str("extension.itemId") {
    extra.insert("item".into(), json!(item));
  }
  if let Some(unread) = user_chat.u64("redPoint") {
    extra.insert("unread".into(), json!(unread));
  }
  if let Some(text) = last.as_ref().and_then(|m| m.text.clone()) {
    extra.insert("last_message".into(), json!(text));
  }
  Some(Collection {
    name: peer_name
      .or_else(|| item_title.clone())
      .or_else(|| peer.clone())
      .unwrap_or_else(|| cid.clone()),
    id: cid,
    kind: "dm".into(),
    description: item_title,
    extra,
    raw: Some(conversation.clone()),
    ..Collection::default()
  })
}

/// A message of a chat's history (`{message: {…}}`, the shape pushes are
/// brought into too) as a post with a `CHAT/MESSAGE` id.
pub fn message(model: &Value, chat: &str) -> Option<Post> {
  let m = model.at("message");
  let id = m.first_str(&["messageId", "uuid"])?;
  let cid = m
    .str("cid")
    .map(|c| plain(&c).to_owned())
    .unwrap_or_else(|| chat.to_owned());
  let ext = m.at("extension");
  let (kind, text, media) = body(m);
  let author = ext
    .str("senderUserId")
    .or_else(|| m.str("sender.uid"))
    .map(|uid| User {
      id: plain(&uid).to_owned(),
      name: ext
        .str("reminderTitle")
        .or_else(|| {
          ext
            .str("sender_nick")
            .map(|n| n.trim_start_matches("cntaobao").to_owned())
        })
        .unwrap_or_default(),
      ..User::default()
    });
  let mut post = Post {
    id: message_id(&cid, &id),
    kind: kind.into(),
    text,
    author,
    created_at: m.i64("createAt").and_then(from_millis),
    media,
    raw: Some(model.clone()),
    ..Post::default()
  };
  post.extra.insert("chat".into(), json!(cid));
  post.extra.insert("message_id".into(), json!(id));
  Some(post)
}

/// Kind, text and images of a message. Text comes plain (`contentType` 1,
/// Taobao) or inside a custom payload (`contentType` 101) holding base64
/// JSON: Xianyu's text (1), image (2) and cards, Taobao's images (type 7).
fn body(m: &Value) -> (&'static str, Option<String>, Vec<Media>) {
  let content = m.at("content");
  let custom = content.at("custom");
  let payload = custom_data(custom.at("data"));
  let summary = custom
    .str("summary")
    .or_else(|| m.str("extension.reminderContent"));
  if content.i64("contentType") == Some(1) {
    let text = content
      .first_str(&["text.content", "text.text"])
      .or(summary);
    return ("text", text, Vec::new());
  }
  match payload.i64("contentType") {
    Some(1) => ("text", payload.str("text.text").or(summary), Vec::new()),
    Some(2) => (
      "image",
      None,
      payload
        .list("image.pics")
        .iter()
        .filter_map(image)
        .collect(),
    ),
    _ if custom.i64("type") == Some(7) => ("image", None, image(&payload).into_iter().collect()),
    _ => ("card", summary.or_else(|| payload.str("title")), Vec::new()),
  }
}

fn image(v: &Value) -> Option<Media> {
  let mut media = Media::image(v.str("url")?);
  media.width = v.u64("width").map(|w| w as u32);
  media.height = v.u64("height").map(|h| h as u32);
  Some(media)
}

/// A custom payload: an object, a JSON string, or base64 of JSON.
pub fn custom_data(data: &Value) -> Value {
  let Some(text) = data.as_str() else {
    return data.clone();
  };
  serde_json::from_str(text)
    .ok()
    .or_else(|| {
      let bytes = impaas::base64(text)?;
      serde_json::from_slice(&bytes).ok()
    })
    .unwrap_or(Value::Null)
}
