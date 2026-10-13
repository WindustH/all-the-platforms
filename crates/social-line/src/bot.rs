//! The bot, its people and its messages: profiles, group and room members,
//! followers, sending (push / multicast / broadcast; text, images by URL,
//! stickers), message content downloads, groups, rich menus and quota.

use social_core::{
  Action, Error, Media, MediaKind, Message, Page, PageReq, Post, Result, User, UserStats, Value,
  ValueExt, json,
};

use crate::api::{API, Api, DATA};

/// What a chat argument names: a user (`U…`), group (`C…`), room (`R…`),
/// several users (`U…,U…`: multicast) or everyone (`all`: broadcast).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
  User(String),
  Group(String),
  Room(String),
  Users(Vec<String>),
  All,
}

fn is_id(s: &str, prefix: char) -> bool {
  s.len() == 33 && s.starts_with(prefix) && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn target(arg: &str) -> Result<Target> {
  let arg = arg.trim();
  if arg == "all" {
    return Ok(Target::All);
  }
  if arg.contains(',') {
    let ids: Vec<String> = arg.split(',').map(|s| s.trim().to_owned()).collect();
    if ids.iter().all(|i| is_id(i, 'U')) {
      return Ok(Target::Users(ids));
    }
    return Err(Error::input(
      "multicast takes user ids only (U…, comma-separated)",
    ));
  }
  if is_id(arg, 'U') {
    Ok(Target::User(arg.to_owned()))
  } else if is_id(arg, 'C') {
    Ok(Target::Group(arg.to_owned()))
  } else if is_id(arg, 'R') {
    Ok(Target::Room(arg.to_owned()))
  } else {
    Err(Error::input(format!(
      "not a LINE chat: {arg} (a user U…, group C… or room R… id, `U…,U…`, or `all`)"
    )))
  }
}

pub fn profile(v: &Value) -> User {
  User {
    id: v.str("userId").unwrap_or_default(),
    name: v.str("displayName").unwrap_or_default(),
    avatar: v.str("pictureUrl"),
    bio: v.str("statusMessage"),
    raw: Some(v.clone()),
    ..User::default()
  }
}

pub async fn whoami(api: &Api) -> Result<User> {
  let v = api.get("/v2/bot/info").await?;
  let mut u = User {
    id: v.str("userId").unwrap_or_default(),
    name: v.str("displayName").unwrap_or_default(),
    handle: v.str("basicId"),
    avatar: v.str("pictureUrl"),
    raw: Some(v.clone()),
    ..User::default()
  };
  for key in ["premiumId", "chatMode", "markAsReadMode"] {
    if let Some(x) = v.str(key) {
      u.extra.insert(key.into(), json!(x));
    }
  }
  if let Ok(n) = api.get("/v2/bot/insight/followers").await
    && let Some(f) = n.u64("followers")
  {
    u.stats = UserStats {
      followers: Some(f),
      ..UserStats::default()
    };
  }
  Ok(u)
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  match target(arg)? {
    Target::User(id) => Ok(profile(&api.get(&format!("/v2/bot/profile/{id}")).await?)),
    Target::Group(id) => {
      let v = api.get(&format!("/v2/bot/group/{id}/summary")).await?;
      let count = api
        .get(&format!("/v2/bot/group/{id}/members/count"))
        .await
        .ok()
        .and_then(|c| c.u64("count"));
      Ok(User {
        id,
        name: v.str("groupName").unwrap_or_default(),
        avatar: v.str("pictureUrl"),
        stats: UserStats {
          followers: count,
          ..UserStats::default()
        },
        raw: Some(v),
        ..User::default()
      })
    }
    _ => Err(Error::input("give a user (U…) or group (C…) id")),
  }
}

/// Member ids of a group or room (verified / premium accounts), with
/// profiles; `next` pages on.
pub async fn members(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  let (kind, id) = match target(arg)? {
    Target::Group(id) => ("group", id),
    Target::Room(id) => ("room", id),
    Target::User(id) => {
      return Ok(Page::last(vec![
        whoami(api).await?,
        profile(&api.get(&format!("/v2/bot/profile/{id}")).await?),
      ]));
    }
    _ => return Err(Error::input("members of a group (C…) or room (R…)")),
  };
  let mut path = format!("/v2/bot/{kind}/{id}/members/ids");
  if let Some(c) = &page.cursor {
    path.push_str(&format!("?start={c}"));
  }
  let v = api.get(&path).await?;
  let mut users = Vec::new();
  for uid in v.list("memberIds").iter().filter_map(Value::as_str) {
    let p = api
      .get(&format!("/v2/bot/{kind}/{id}/member/{uid}"))
      .await
      .map(|p| profile(&p))
      .unwrap_or_else(|_| User {
        id: uid.to_owned(),
        ..User::default()
      });
    users.push(p);
    if users.len() >= page.size.max(1) {
      break;
    }
  }
  Ok(Page::new(users, v.str("next")))
}

/// Friends of the bot (verified / premium accounts), with profiles.
pub async fn followers(api: &Api, page: &PageReq) -> Result<Page<User>> {
  let size = page.size_within(1000);
  let mut path = format!("/v2/bot/followers/ids?limit={size}");
  if let Some(c) = &page.cursor {
    path.push_str(&format!("&start={c}"));
  }
  let v = api.get(&path).await?;
  let mut users = Vec::new();
  for uid in v.list("userIds").iter().filter_map(Value::as_str) {
    users.push(
      api
        .get(&format!("/v2/bot/profile/{uid}"))
        .await
        .map(|p| profile(&p))
        .unwrap_or_else(|_| User {
          id: uid.to_owned(),
          ..User::default()
        }),
    );
  }
  Ok(Page::new(users, v.str("next")))
}

/// Deliver message objects to a target; the sent ids (or the request id of
/// a broadcast) come back.
pub async fn deliver(api: &Api, to: &Target, messages: Vec<Value>) -> Result<Action> {
  if messages.is_empty() {
    return Err(Error::input("nothing to send"));
  }
  if messages.len() > 5 {
    return Err(Error::input("LINE sends at most 5 messages at once"));
  }
  let (path, body, name) = match to {
    Target::User(id) | Target::Group(id) | Target::Room(id) => (
      "/v2/bot/message/push",
      json!({"to": id, "messages": messages}),
      id.clone(),
    ),
    Target::Users(ids) => (
      "/v2/bot/message/multicast",
      json!({"to": ids, "messages": messages}),
      format!("{} users", ids.len()),
    ),
    Target::All => (
      "/v2/bot/message/broadcast",
      json!({"messages": messages}),
      "everyone".to_owned(),
    ),
  };
  let (v, request_id) = api.send(path, &body).await?;
  let action = match to {
    Target::All => "broadcast",
    Target::Users(_) => "multicast",
    _ => "send",
  };
  let mut a = Action::done(action, name);
  if let Some(id) = v.list("sentMessages").first().and_then(|m| m.str("id")) {
    a = a.with_id(id);
  } else if let Some(r) = request_id {
    a = a
      .with_id(r)
      .with_message("request id: use it with `insights`");
  }
  Ok(a)
}

pub async fn send(api: &Api, arg: &str, message: &Message) -> Result<Action> {
  if !message.files.is_empty() {
    return Err(
      Error::unsupported("send --attach")
        .with_hint("LINE takes images by public HTTPS URL: `social line image CHAT URL`"),
    );
  }
  let mut text = json!({"type": "text", "text": message.text});
  if let Some(q) = &message.reply_to {
    // Quote tokens come with webhook events and sent messages.
    text["quoteToken"] = json!(q);
  }
  deliver(api, &target(arg)?, vec![text]).await
}

pub async fn image(api: &Api, arg: &str, url: &str, preview: Option<&str>) -> Result<Action> {
  if !url.starts_with("https://") {
    return Err(Error::input("LINE needs an https:// image URL"));
  }
  let msg = json!({
    "type": "image",
    "originalContentUrl": url,
    "previewImageUrl": preview.unwrap_or(url),
  });
  deliver(api, &target(arg)?, vec![msg]).await
}

pub async fn sticker(api: &Api, arg: &str, package: &str, sticker: &str) -> Result<Action> {
  let msg = json!({"type": "sticker", "packageId": package, "stickerId": sticker});
  deliver(api, &target(arg)?, vec![msg]).await
}

/// A received message's content (image, video, audio, file), by message id.
pub fn content(api: &Api, id: &str) -> Result<(Post, Vec<Media>)> {
  let token = api.require()?;
  let id = id.trim();
  if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
    return Err(Error::input("give the id of a received message (digits)"));
  }
  let mut m = Media::new(
    MediaKind::File,
    format!("{DATA}/v2/bot/message/{id}/content"),
  );
  m.headers = vec![("authorization".into(), format!("Bearer {token}"))];
  let post = Post {
    id: id.to_owned(),
    kind: "message".into(),
    ..Post::default()
  };
  Ok((post, vec![m]))
}

pub async fn leave(api: &Api, arg: &str) -> Result<Action> {
  let (kind, id) = match target(arg)? {
    Target::Group(id) => ("group", id),
    Target::Room(id) => ("room", id),
    _ => return Err(Error::input("leave a group (C…) or room (R…)")),
  };
  api
    .post(&format!("/v2/bot/{kind}/{id}/leave"), &json!({}))
    .await?;
  Ok(Action::done("leave", id))
}

/// Monthly message quota and what was used.
pub async fn quota(api: &Api) -> Result<Value> {
  let q = api.get("/v2/bot/message/quota").await?;
  let used = api.get("/v2/bot/message/quota/consumption").await?;
  let webhook = api
    .get(&format!("{API}/v2/bot/channel/webhook/endpoint"))
    .await
    .unwrap_or_default();
  Ok(json!({
    "type": q.str("type"),
    "limit": q.u64("value"),
    "used": used.u64("totalUsage"),
    "webhook": webhook.str("endpoint"),
    "webhook_active": webhook.bool("active"),
  }))
}

pub async fn rich_menus(api: &Api) -> Result<Value> {
  let list = api.get("/v2/bot/richmenu/list").await?;
  let default = api
    .get("/v2/bot/user/all/richmenu")
    .await
    .ok()
    .and_then(|d| d.str("richMenuId"));
  Ok(json!({"default": default, "richmenus": list.at("richmenus")}))
}
