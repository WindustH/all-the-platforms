//! The live chat of a stream, and its replay once the stream has ended.
//!
//! The watch page (`next`) names the chat in `liveChatRenderer` with a first
//! continuation (the "Top chat" view). The chat's own answers carry a view
//! selector whose last entry is every message ("Live chat" / "Live chat
//! replay"); the selector tokens in the watch page alone are refused (400),
//! so the chat is opened first and then switched to that view.
//! `live_chat/get_live_chat` answers a batch plus the next continuation and
//! how long to wait before asking (`timeoutMs`); `get_live_chat_replay`
//! pages through a replay in video time (YouTube.js `LiveChat`, yt-dlp's
//! `youtube_live_chat`). Messages are posts with the id `VIDEO/MESSAGE`.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use social_core::output::Stream;
use social_core::text::from_secs;
use social_core::{Action, Error, Page, PageReq, Post, Result, User, Value, ValueExt, json};

use crate::api::Api;
use crate::parse::{self, text};
use crate::proto::Msg;
use crate::refs;

/// The chat of a video: its first continuation and whether it is a replay.
pub struct Chat {
  pub video: String,
  pub token: String,
  pub replay: bool,
}

pub async fn open(api: &Api, video: &str) -> Result<Chat> {
  let next = api.call("next", json!({ "videoId": video })).await?;
  let Some(r) = parse::first(&next, "liveChatRenderer") else {
    // "Chat is disabled for this live stream." and the like.
    let why = parse::first(&next, "conversationBarRenderer")
      .and_then(|c| parse::first(c, "messageRenderer"))
      .and_then(|m| text(m.at("text")))
      .unwrap_or_else(|| "not a stream, or no chat replay".into());
    return Err(Error::not_found(format!(
      "video {video} has no live chat: {why}"
    )));
  };
  let token = r
    .list("continuations")
    .iter()
    .find_map(|c| c.str("reloadContinuationData.continuation"))
    .ok_or_else(|| Error::upstream("live chat without a continuation"))?;
  let mut chat = Chat {
    video: video.to_owned(),
    token,
    replay: r.bool("isReplay") == Some(true),
  };
  // Switch from "Top chat" to every message when the chat offers it.
  let first = fetch(api, &chat, &chat.token.clone()).await?;
  if let Some(all) = first.all {
    chat.token = all;
  }
  Ok(chat)
}

/// One batch: the messages, the next token, the suggested wait and the
/// token of the all-messages view when another view is selected.
struct Batch {
  posts: Vec<Post>,
  next: Option<String>,
  wait: Option<Duration>,
  all: Option<String>,
}

async fn fetch(api: &Api, chat: &Chat, token: &str) -> Result<Batch> {
  let (endpoint, body) = if chat.replay {
    (
      "live_chat/get_live_chat_replay",
      json!({ "continuation": token, "currentPlayerState": { "playerOffsetMs": "0" } }),
    )
  } else {
    ("live_chat/get_live_chat", json!({ "continuation": token }))
  };
  let v = api.call(endpoint, body).await?;
  let c = v.at("continuationContents.liveChatContinuation");
  let mut posts = Vec::new();
  for action in c.list("actions") {
    // Replays wrap each action with its offset in the video.
    let (actions, offset) = match action.get("replayChatItemAction") {
      Some(r) => (r.list("actions"), r.f64("videoOffsetTimeMsec")),
      None => (std::slice::from_ref(action), None),
    };
    for a in actions {
      if let Some(mut p) = message(a.at("addChatItemAction.item"), &chat.video) {
        if let Some(ms) = offset {
          p.extra.insert("offset".into(), json!(ms / 1000.0));
        }
        posts.push(p);
      }
    }
  }
  let cont = c.list("continuations").first();
  let data = cont
    .and_then(|c| c.as_object())
    .and_then(|m| m.values().next());
  let views =
    c.list("header.liveChatHeaderRenderer.viewSelector.sortFilterSubMenuRenderer.subMenuItems");
  let all = views
    .last()
    .filter(|v| v.bool("selected") != Some(true))
    .and_then(|v| v.str("continuation.reloadContinuationData.continuation"));
  Ok(Batch {
    posts,
    next: data.and_then(|d| d.str("continuation")),
    wait: data
      .and_then(|d| d.u64("timeoutMs"))
      .map(Duration::from_millis),
    all,
  })
}

/// The kinds of chat items worth reporting; tickers and banners are skipped.
const KINDS: &[(&str, &str)] = &[
  ("liveChatTextMessageRenderer", "message"),
  ("liveChatPaidMessageRenderer", "superchat"),
  ("liveChatPaidStickerRenderer", "sticker"),
  ("liveChatMembershipItemRenderer", "membership"),
  (
    "liveChatSponsorshipsGiftPurchaseAnnouncementRenderer",
    "gift",
  ),
];

/// Text of a message: runs with emoji shortcuts (`:smile:`) in place.
fn runs(v: &Value) -> Option<String> {
  let out: String = v
    .list("runs")
    .iter()
    .map(|r| match r.str("text") {
      Some(t) => t,
      None => r
        .list("emoji.shortcuts")
        .first()
        .and_then(|s| s.as_str().map(str::to_owned))
        .or_else(|| r.str("emoji.emojiId"))
        .unwrap_or_default(),
    })
    .collect();
  let out = out.trim().to_owned();
  (!out.is_empty()).then_some(out)
}

fn message(item: &Value, video: &str) -> Option<Post> {
  let (r, kind) = KINDS
    .iter()
    .find_map(|(key, kind)| item.get(*key).map(|r| (r, *kind)))?;
  // Gift announcements nest their author in a header.
  let r = match kind {
    "gift" => r.at("header.liveChatSponsorshipsHeaderRenderer"),
    _ => r,
  };
  let id = item
    .as_object()
    .and_then(|m| m.values().next())
    .and_then(|r| r.str("id"))?;
  let channel = r.str("authorExternalChannelId").unwrap_or_default();
  let author = User {
    name: text(r.at("authorName")).unwrap_or_default(),
    avatar: parse::image(r.at("authorPhoto")),
    url: (!channel.is_empty()).then(|| refs::channel_url(&channel)),
    id: channel,
    ..User::default()
  };
  let mut post = Post {
    id: format!("{video}/{id}"),
    kind: kind.into(),
    text: runs(r.at("message"))
      .or_else(|| runs(r.at("headerSubtext")))
      .or_else(|| runs(r.at("primaryText"))),
    url: Some(refs::video_url(video)),
    author: Some(author),
    created_at: r
      .i64("timestampUsec")
      .and_then(|us| from_secs(us / 1_000_000)),
    raw: Some(item.clone()),
    ..Post::default()
  };
  let x = &mut post.extra;
  x.insert("chat".into(), video.into());
  if let Some(amount) = text(r.at("purchaseAmountText")) {
    x.insert("amount".into(), amount.into());
  }
  let badges: Vec<Value> = r
    .list("authorBadges")
    .iter()
    .filter_map(|b| b.str("liveChatAuthorBadgeRenderer.tooltip"))
    .map(Value::from)
    .collect();
  if !badges.is_empty() {
    x.insert("badges".into(), badges.into());
  }
  Some(post)
}

/// One page of a chat: a replay in video order, or what a live chat shows
/// now. Cursors are `<video>:<replay>:<token>`.
pub async fn page(api: &Api, arg: &str, req: &PageReq) -> Result<Page<Post>> {
  let chat = match req.cursor.as_deref().and_then(cursor) {
    Some(c) => c,
    None => open(api, &refs::video(arg)?).await?,
  };
  let token = chat.token.clone();
  let batch = fetch(api, &chat, &token).await?;
  // A live chat's next token only brings messages yet to be written.
  let next = batch
    .next
    .filter(|_| chat.replay)
    .map(|t| format!("{}:1:{t}", chat.video));
  Ok(Page::new(batch.posts, next))
}

fn cursor(c: &str) -> Option<Chat> {
  let mut parts = c.splitn(3, ':');
  Some(Chat {
    video: parts.next()?.to_owned(),
    replay: parts.next()? == "1",
    token: parts.next()?.to_owned(),
  })
}

/// Follow the live chats of `videos` until they end or the user interrupts.
pub async fn listen(api: &Api, videos: &[String], out: &Stream) -> Result<()> {
  if videos.is_empty() {
    return Err(Error::input(
      "name the live stream(s) to follow: `social yt listen VIDEO`",
    ));
  }
  let mut chats = Vec::new();
  for v in videos {
    let chat = open(api, &refs::video(v)?).await?;
    if chat.replay {
      return Err(Error::input(format!(
        "{} has ended; read its chat replay with `social yt chat {}`",
        chat.video, chat.video
      )));
    }
    chats.push(chat);
  }
  // The first batch is what the chat shows now; later ones are new.
  loop {
    let mut wait = Duration::from_secs(10);
    let mut open_chats = 0;
    for chat in chats.iter_mut() {
      if chat.token.is_empty() {
        continue;
      }
      let token = chat.token.clone();
      let batch = fetch(api, chat, &token).await?;
      for p in &batch.posts {
        out.post(p);
      }
      chat.token = batch.next.unwrap_or_default();
      if !chat.token.is_empty() {
        open_chats += 1;
      }
      if let Some(w) = batch.wait {
        wait = wait.min(w);
      }
    }
    if open_chats == 0 {
      return Ok(());
    }
    tokio::time::sleep(wait.max(Duration::from_secs(1))).await;
  }
}

/// Post a message to a live chat (YouTube.js `LiveChat.sendMessage`): the
/// params name the video and its channel, base64 over a URI-escaped base64.
pub async fn send(api: &Api, arg: &str, text: &str) -> Result<Action> {
  api.require_login()?;
  let video = refs::video(arg)?;
  let player = crate::video::player(api, &video).await?;
  let channel = player
    .str("videoDetails.channelId")
    .ok_or_else(|| Error::not_found(format!("video {video} not found")))?;
  if player.bool("videoDetails.isLive") != Some(true) {
    return Err(Error::input(format!("{video} is not live now")));
  }
  let ids = Msg::new().str(1, &channel).str(2, &video);
  let raw = Msg::new()
    .msg(1, Msg::new().msg(5, ids))
    .int(2, 1)
    .int(3, 4)
    .into_bytes();
  let inner = STANDARD.encode(raw);
  let escaped: String = url::form_urlencoded::byte_serialize(inner.as_bytes()).collect();
  let params = STANDARD.encode(escaped);
  let client_id = format!("{:032x}", rand::random::<u128>());
  let body = json!({
    "richMessage": { "textSegments": [{ "text": text }] },
    "clientMessageId": client_id,
    "params": params,
  });
  let v = api.write("live_chat/send_message", body).await?;
  let id = parse::first(&v, "liveChatTextMessageRenderer").and_then(|m| m.str("id"));
  let mut action = Action::done("send", &video).with_url(refs::video_url(&video));
  if let Some(id) = id {
    action = action.with_id(format!("{video}/{id}"));
  }
  Ok(action)
}
