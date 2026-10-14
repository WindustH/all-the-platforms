//! Chat messages: a chat's history and sending text and images. A chat
//! belongs to an item: writing to an item opens the chat with its seller
//! (or finds the one there is). Each command opens its own connection.

use atp_alibaba::chats::message;
use atp_alibaba::impaas::{GOOFISH, Im, qualified};
use atp_core::chat::message_id;
use atp_core::{Action, Error, Message, Page, PageReq, Post, Result, Value, json};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::api::Api;
use crate::refs::{self, ChatRef};
use crate::{posts, uploads};

/// How many history pages to search for the other person of a chat.
const PEER_PAGES: usize = 10;

/// Where a message goes, resolved before anything is uploaded.
enum Target {
  Item { item: String, seller: String },
  Chat(String),
}

/// A chat id; items have no history of their own.
async fn chat_id(api: &Api, arg: &str) -> Result<String> {
  match refs::chat(api, arg).await? {
    ChatRef::Chat(cid) => Ok(cid),
    ChatRef::Item(_) => Err(Error::input(
      "an item is not a chat; find the chat about it with `atp xianyu chats`",
    )),
  }
}

/// Newest first, 20 at a time.
pub async fn list(api: &Api, chat: &str, page: &PageReq) -> Result<Page<Post>> {
  let cid = chat_id(api, chat).await?;
  let mut im = api.im().await?;
  let batch = im.history(&cid, page.cursor.as_deref()).await?;
  let items = batch
    .items
    .iter()
    .filter_map(|m| message(m, &cid))
    .collect();
  Ok(Page::new(items, batch.next))
}

/// Text, then each image as a message of its own.
pub async fn send(api: &Api, chat: &str, outgoing: &Message) -> Result<Action> {
  if outgoing.reply_to.is_some() {
    return Err(Error::unsupported("send --reply-to"));
  }
  if outgoing.text.trim().is_empty() && outgoing.files.is_empty() {
    return Err(Error::input("nothing to send"));
  }
  let me = api.me()?;
  let target = match refs::chat(api, chat).await? {
    ChatRef::Item(item) => {
      let seller = seller(api, &item, &me).await?;
      Target::Item { item, seller }
    }
    ChatRef::Chat(cid) => Target::Chat(cid),
  };
  let mut images = Vec::new();
  for path in &outgoing.files {
    images.push(uploads::image(api, path).await?);
  }
  let mut im = api.im().await?;
  let (cid, peer) = match target {
    Target::Item { item, seller } => (open(&mut im, &me, &seller, &item).await?, seller),
    Target::Chat(cid) => {
      let peer = peer(&mut im, &cid, &me).await?;
      (cid, peer)
    }
  };
  let receivers = [qualified(&GOOFISH, &peer), qualified(&GOOFISH, &me)];
  let mut sent = String::new();
  if !outgoing.text.trim().is_empty() {
    let payload = json!({"contentType": 1, "text": {"text": outgoing.text}});
    sent = im.send(custom(&cid, &payload), &receivers).await?;
  }
  for image in images {
    let pic = json!({"type": 0, "url": image.url, "width": image.width, "height": image.height});
    let payload = json!({"contentType": 2, "image": {"pics": [pic]}});
    sent = im.send(custom(&cid, &payload), &receivers).await?;
  }
  Ok(Action::done("send", chat).with_id(message_id(&cid, &sent)))
}

/// The seller of an item, who must be someone else.
async fn seller(api: &Api, item: &str, me: &str) -> Result<String> {
  let post = posts::read(api, item).await?;
  let seller = post
    .author
    .map(|a| a.id)
    .filter(|id| !id.is_empty())
    .ok_or_else(|| Error::upstream("the item page names no seller"))?;
  if seller == me {
    return Err(Error::input(
      "this is your own item; write to a buyer in the chat they opened (see `chats`)",
    ));
  }
  Ok(seller)
}

/// The chat about `item` between the two accounts (lower id first, as the
/// web app pairs them).
async fn open(im: &mut Im, me: &str, peer: &str, item: &str) -> Result<String> {
  let mut pair = [me, peer];
  pair.sort_by_key(|id| id.parse::<u64>().unwrap_or(u64::MAX));
  im.create(json!({
    "pairFirst": qualified(&GOOFISH, pair[0]),
    "pairSecond": qualified(&GOOFISH, pair[1]),
    "bizType": "1",
    "extension": {"itemId": item},
    "ctx": {"appVersion": "1.0", "platform": "web"},
  }))
  .await
}

/// The other person of a chat: the first sender who is not `me`.
async fn peer(im: &mut Im, cid: &str, me: &str) -> Result<String> {
  let mut cursor = None;
  for _ in 0..PEER_PAGES {
    let batch = im.history(cid, cursor.as_deref()).await?;
    let found = batch
      .items
      .iter()
      .filter_map(|m| message(m, cid))
      .filter_map(|p| p.author.map(|a| a.id))
      .find(|id| !id.is_empty() && id != me);
    if let Some(peer) = found {
      return Ok(peer);
    }
    match batch.next {
      Some(next) => cursor = Some(next),
      None => break,
    }
  }
  Err(Error::input(format!(
    "chat {cid} holds only your own messages, so its other person is unknown; write to the item instead (`item:<id>`)"
  )))
}

/// A message in Xianyu's custom envelope: the payload as base64 JSON.
fn custom(cid: &str, payload: &Value) -> Value {
  json!({
    "uuid": (GOOFISH.uuid)(),
    "cid": qualified(&GOOFISH, cid),
    "conversationType": 1,
    "content": {"contentType": 101, "custom": {
      "type": payload["contentType"],
      "data": STANDARD.encode(payload.to_string()),
    }},
    "redPointPolicy": 0,
    "extension": {"extJson": "{}"},
    "ctx": {"appVersion": "1.0", "platform": "web"},
    "mtags": {},
    "msgReadStatusSetting": 1,
  })
}
