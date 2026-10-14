//! Chat messages with shops: a chat's history and sending text and images.
//! Writing to an item opens the chat with its seller, who is named in the
//! item page by an encrypted id. Each command opens its own connection.

use atp_alibaba::account;
use atp_alibaba::chats::{cid_peer, message};
use atp_alibaba::impaas::{CNTAOBAO, qualified};
use atp_core::chat::message_id;
use atp_core::{Action, Error, Message, Page, PageReq, Post, Result, Value, json};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::api::Api;
use crate::parse;
use crate::refs::{self, ChatRef};
use crate::uploads::{self, Uploaded};

/// The business domain both sides of a Taobao chat are in.
const BIZ: &str = "taobao";

/// Where a message goes: a chat, or a new chat with the seller an
/// encrypted id names.
enum Target {
  Chat(String),
  Seller(String),
}

/// A chat id; items have no history of their own.
async fn chat_id(api: &Api, arg: &str) -> Result<String> {
  match refs::chat(api, arg).await? {
    ChatRef::Chat(cid) => Ok(cid),
    ChatRef::Item(_) => Err(Error::input(
      "an item is not a chat; find the chat with its shop in `atp taobao chats`",
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
  let nick = format!(
    "cntaobao{}",
    account::raw_nick(&api.ctx).unwrap_or_default()
  );
  let (target, peer) = match refs::chat(api, chat).await? {
    ChatRef::Chat(cid) => {
      let peer = cid_peer(&cid, &me)
        .ok_or_else(|| Error::input(format!("chat {cid} is not one of yours")))?;
      (Target::Chat(cid), peer)
    }
    ChatRef::Item(item) => {
      let seller = parse::seller(&api.item_page(&item).await?)
        .ok_or_else(|| Error::upstream("the item page names no seller"))?;
      (Target::Seller(seller.encrypted), seller.id)
    }
  };
  if peer == me {
    return Err(Error::input("this is your own shop's item"));
  }
  let mut images = Vec::new();
  for path in &outgoing.files {
    images.push(uploads::image(api, path).await?);
  }
  let mut im = api.im().await?;
  let cid = match target {
    Target::Chat(cid) => cid,
    Target::Seller(encrypted) => im.create(open(&me, &encrypted)).await?,
  };
  let receivers = [qualified(&CNTAOBAO, &me), qualified(&CNTAOBAO, &peer)];
  let mut sent = String::new();
  if !outgoing.text.trim().is_empty() {
    sent = im
      .send(text(&cid, &nick, &outgoing.text), &receivers)
      .await?;
  }
  for image in &images {
    sent = im.send(picture(&cid, image), &receivers).await?;
  }
  Ok(Action::done("send", chat).with_id(message_id(&cid, &sent)))
}

/// The request that opens the chat with the seller `encrypted` names.
fn open(me: &str, encrypted: &str) -> Value {
  json!({
    "pairFirst": qualified(&CNTAOBAO, me),
    "bizType": "11001",
    "ctx": {
      "createConversationCtx": json!({"encryptUid": encrypted}).to_string(),
      "selfBizDomain": BIZ,
    },
  })
}

/// A text message; Taobao names the sender by the raw nickname cookie.
fn text(cid: &str, nick: &str, text: &str) -> Value {
  json!({
    "cid": qualified(&CNTAOBAO, cid),
    "uuid": (CNTAOBAO.uuid)(),
    "conversationType": 1,
    "redPointPolicy": 0,
    "extension": {"senderBizDomain": BIZ, "receiverBizDomain": BIZ, "sender_nick": nick},
    "content": {"contentType": 1, "text": {"extension": {"sender_nick": nick}, "content": text}},
    "ctx": {"senderBizDomain": BIZ, "receiverBizDomain": BIZ},
  })
}

/// An image message: custom type 7 holding the upload as base64 JSON.
fn picture(cid: &str, image: &Uploaded) -> Value {
  let object = &image.object;
  let data = json!({
    "fileId": object["fileId"],
    "size": object["size"],
    "url": object["url"],
    "width": image.width,
    "height": image.height,
    "isOriginal": 1,
    "suffix": "png",
  });
  json!({
    "cid": qualified(&CNTAOBAO, cid),
    "uuid": (CNTAOBAO.uuid)(),
    "conversationType": 1,
    "redPointPolicy": 0,
    "extension": {"senderBizDomain": BIZ, "receiverBizDomain": BIZ},
    "content": {"contentType": 101, "custom": {"type": 7, "data": STANDARD.encode(data.to_string())}},
    "ctx": {"senderBizDomain": BIZ, "receiverBizDomain": BIZ},
  })
}
