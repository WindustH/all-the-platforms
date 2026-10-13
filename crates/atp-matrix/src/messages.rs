//! Messages: history (`/messages`, back from the newest), one event,
//! sending (text, files through the media repository, replies), edits
//! (`m.replace`), redactions, reactions (`m.annotation`), server-side search
//! and media downloads.

use std::path::Path;

use atp_core::chat;
use atp_core::file::Upload;
use atp_core::http::Method;
use atp_core::{
  Action, Error, Media, MediaKind, Message, Page, PageReq, Post, Query, Result, Value, ValueExt,
  json,
};

use crate::api::{Api, seg, txn};
use crate::chats;
use crate::parse::{self, Members};
use crate::refs::{self, Event};

/// Only events that read as messages; member events come along for names.
const HISTORY_FILTER: &str = r#"{"lazy_load_members":true,"types":["m.room.message","m.sticker","m.room.encrypted","m.reaction","m.room.redaction"]}"#;

async fn room_name(api: &Api, rid: &str) -> String {
  let v = api
    .get(&format!("/rooms/{}/state/m.room.name/", seg(rid)), &[])
    .await;
  v.ok().and_then(|v| v.str("name")).unwrap_or_default()
}

/// Turn `mxc://` media into authenticated download URLs.
async fn resolve_media(api: &Api, post: &mut Post) {
  let headers = api.media_headers();
  for m in &mut post.media {
    if let Some(url) = api.media_url(&m.url).await {
      m.url = url;
      m.headers = headers.clone();
    }
  }
}

pub async fn history(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let rid = chats::id(api, arg).await?;
  let name = room_name(api, &rid).await;
  let mut query = vec![
    ("dir", "b".to_owned()),
    ("limit", page.size_within(100).to_string()),
    ("filter", HISTORY_FILTER.to_owned()),
  ];
  if let Some(c) = &page.cursor {
    query.push(("from", c.clone()));
  }
  let v = api
    .get(&format!("/rooms/{}/messages", seg(&rid)), &query)
    .await?;
  let mut members = Members::default();
  members.absorb(v.list("state"));
  let mut items = Vec::new();
  for e in v.list("chunk") {
    if let Some(mut p) = parse::event(&rid, &name, e, &members) {
      resolve_media(api, &mut p).await;
      items.push(p);
    }
  }
  let next = v.str("end").filter(|_| !v.list("chunk").is_empty());
  Ok(Page::new(items, next))
}

async fn event_room(api: &Api, r: &Event) -> Result<String> {
  match &r.room {
    refs::Room::Id(id) => Ok(id.clone()),
    refs::Room::Alias(a) => chats::resolve_alias(api, a).await,
  }
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let r = refs::event(arg)?;
  let rid = event_room(api, &r).await?;
  let e = api
    .get(&format!("/rooms/{}/event/{}", seg(&rid), seg(&r.id)), &[])
    .await?;
  let mut members = Members::default();
  if let Some(sender) = e.str("sender") {
    let profile = api
      .get(&format!("/profile/{}", seg(&sender)), &[])
      .await
      .unwrap_or_default();
    members.0.insert(
      sender,
      (profile.str("displayname"), profile.str("avatar_url")),
    );
  }
  let name = room_name(api, &rid).await;
  let mut post = parse::event(&rid, &name, &e, &members)
    .ok_or_else(|| Error::not_found(format!("{} is not a message", r.id)))?;
  resolve_media(api, &mut post).await;
  Ok(post)
}

pub async fn media(api: &Api, arg: &str) -> Result<(Post, Vec<Media>)> {
  let post = read(api, arg).await?;
  if post.media.is_empty() {
    return Err(Error::not_found("the message has no image, video or audio"));
  }
  let media = post.media.clone();
  Ok((post, media))
}

async fn send_event(api: &Api, rid: &str, kind: &str, content: &Value) -> Result<String> {
  let v = api
    .write(
      Method::PUT,
      &format!("/rooms/{}/send/{kind}/{}", seg(rid), txn()),
      content,
    )
    .await?;
  Ok(v.str("event_id").unwrap_or_default())
}

/// A file message: uploaded to the media repository, typed by its content.
async fn file_content(api: &Api, path: &Path) -> Result<Value> {
  let file = Upload::read(path).await?;
  let msgtype = match file.kind() {
    MediaKind::Image | MediaKind::Gif => "m.image",
    MediaKind::Video => "m.video",
    _ => "m.file",
  };
  let mut info = json!({"mimetype": file.mime, "size": file.data.len()});
  if let Some((w, h)) = file.size().filter(|_| msgtype == "m.image") {
    info["w"] = json!(w);
    info["h"] = json!(h);
  }
  let name = file.name.clone();
  let uri = api.upload(&name, file.data, file.mime).await?;
  Ok(json!({"msgtype": msgtype, "body": name, "url": uri, "info": info}))
}

pub async fn send(api: &Api, arg: &str, message: &Message) -> Result<Action> {
  let rid = chats::id(api, arg).await?;
  let reply = match &message.reply_to {
    Some(r) => Some(refs::event(r)?.id),
    None => None,
  };
  let with_reply = |mut content: Value| {
    if let Some(r) = &reply {
      content["m.relates_to"] = json!({"m.in_reply_to": {"event_id": r}});
    }
    content
  };
  let mut first = None;
  if !message.text.is_empty() {
    let content = with_reply(json!({"msgtype": "m.text", "body": message.text}));
    first = Some(send_event(api, &rid, "m.room.message", &content).await?);
  }
  for f in &message.files {
    let content = file_content(api, f).await?;
    let content = if first.is_none() {
      with_reply(content)
    } else {
      content
    };
    let id = send_event(api, &rid, "m.room.message", &content).await?;
    first.get_or_insert(id);
  }
  let id = first.unwrap_or_default();
  Ok(
    Action::done(if reply.is_some() { "reply" } else { "send" }, &rid)
      .with_id(chat::message_id(&rid, &id))
      .with_url(refs::permalink(&rid, Some(&id))),
  )
}

pub async fn edit(api: &Api, arg: &str, text: &str) -> Result<Action> {
  let r = refs::event(arg)?;
  let rid = event_room(api, &r).await?;
  let content = json!({
    "msgtype": "m.text",
    "body": format!("* {text}"),
    "m.new_content": {"msgtype": "m.text", "body": text},
    "m.relates_to": {"rel_type": "m.replace", "event_id": r.id},
  });
  let id = send_event(api, &rid, "m.room.message", &content).await?;
  Ok(
    Action::done("edit", chat::message_id(&rid, &r.id))
      .with_id(chat::message_id(&rid, &id))
      .with_url(refs::permalink(&rid, Some(&r.id))),
  )
}

async fn redact(api: &Api, rid: &str, event: &str) -> Result<()> {
  api
    .write(
      Method::PUT,
      &format!("/rooms/{}/redact/{}/{}", seg(rid), seg(event), txn()),
      &json!({}),
    )
    .await?;
  Ok(())
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let r = refs::event(arg)?;
  let rid = event_room(api, &r).await?;
  redact(api, &rid, &r.id).await?;
  Ok(Action::done("delete", chat::message_id(&rid, &r.id)))
}

pub async fn react(api: &Api, arg: &str, key: &str, undo: bool) -> Result<Action> {
  let r = refs::event(arg)?;
  let rid = event_room(api, &r).await?;
  let key = key.trim();
  let target = chat::message_id(&rid, &r.id);
  if !undo {
    let content = json!({
      "m.relates_to": {"rel_type": "m.annotation", "event_id": r.id, "key": key}
    });
    send_event(api, &rid, "m.reaction", &content).await?;
    return Ok(Action::done("react", target).with_message(key));
  }
  // Undo: find our own annotation with this key and redact it.
  let me = match api.user_id() {
    Some(id) => id,
    None => api
      .get("/account/whoami", &[])
      .await?
      .str("user_id")
      .unwrap_or_default(),
  };
  let v = api
    .get(
      &format!(
        "/_matrix/client/v1/rooms/{}/relations/{}/m.annotation/m.reaction",
        seg(&rid),
        seg(&r.id)
      ),
      &[("limit", "100".into())],
    )
    .await?;
  let mine = v.list("chunk").iter().find(|e| {
    e.str("sender").as_deref() == Some(me.as_str())
      && parse::field(e.at("content"), "m.relates_to")
        .str("key")
        .as_deref()
        == Some(key)
  });
  let Some(id) = mine.and_then(|e| e.str("event_id")) else {
    return Err(Error::not_found(format!(
      "you have not reacted {key} to {target}"
    )));
  };
  redact(api, &rid, &id).await?;
  Ok(Action::done("unreact", target).with_message(key))
}

/// Server-side search over the rooms you are in (`--filter ROOM` narrows it).
pub async fn search(api: &Api, q: &Query, page: &PageReq) -> Result<Page<Post>> {
  let mut criteria = json!({
    "search_term": q.keyword,
    "order_by": if q.sort.as_deref() == Some("rank") { "rank" } else { "recent" },
    "filter": {"limit": page.size_within(50)},
    "event_context": {"before_limit": 0, "after_limit": 0, "include_profile": true},
  });
  if let Some(room) = &q.filter {
    criteria["filter"]["rooms"] = json!([chats::id(api, room).await?]);
  }
  let mut query = Vec::new();
  if let Some(c) = &page.cursor {
    query.push(("next_batch", c.clone()));
  }
  let v = api
    .auth(
      Method::POST,
      "/search",
      &query,
      Some(&json!({"search_categories": {"room_events": criteria}})),
    )
    .await?;
  let events = v.at("search_categories").at("room_events");
  let mut items = Vec::new();
  for r in events.list("results") {
    let e = r.at("result");
    let rid = e.str("room_id").unwrap_or_default();
    let mut members = Members::default();
    let profiles = r.at("context").at("profile_info");
    if let Some(sender) = e.str("sender")
      && let Some(p) = profiles.get(&sender)
    {
      members
        .0
        .insert(sender, (p.str("displayname"), p.str("avatar_url")));
    }
    if let Some(mut p) = parse::event(&rid, "", e, &members) {
      resolve_media(api, &mut p).await;
      items.push(p);
    }
  }
  Ok(Page::new(items, events.str("next_batch")))
}
