//! Discord payloads → the shared model: guilds and channels are
//! collections, messages are posts (`CHANNEL/MESSAGE` ids, permalinks where
//! the guild is known), users are users.

use social_core::text::{from_millis, parse_time};
use social_core::{Collection, Media, MediaKind, Metrics, Post, User, Value, ValueExt, json};

use crate::api::WEB;

const CDN: &str = "https://cdn.discordapp.com";
/// Milliseconds of 2015-01-01, where snowflake time starts.
const EPOCH: i64 = 1_420_070_400_000;

/// When a snowflake id was made.
pub fn snowflake_time(id: &str) -> Option<jiff::Timestamp> {
  let n: i64 = id.parse().ok()?;
  from_millis((n >> 22) + EPOCH)
}

/// The smallest snowflake of a moment (for `after=` / `min_id=`).
pub fn snowflake_at(t: jiff::Timestamp) -> String {
  ((t.as_millisecond() - EPOCH).max(0) << 22).to_string()
}

pub fn permalink(guild: &str, channel: &str, message: Option<&str>) -> String {
  match message {
    Some(m) => format!("{WEB}/channels/{guild}/{channel}/{m}"),
    None => format!("{WEB}/channels/{guild}/{channel}"),
  }
}

// ── users ───────────────────────────────────────────────────────────────

fn avatar(v: &Value) -> Option<String> {
  let id = v.str("id")?;
  Some(match v.str("avatar") {
    Some(hash) => {
      let ext = if hash.starts_with("a_") { "gif" } else { "png" };
      format!("{CDN}/avatars/{id}/{hash}.{ext}?size=256")
    }
    None => {
      // Default avatars: by id for new usernames, by discriminator for old tags.
      let index = match v.str("discriminator").and_then(|d| d.parse::<u64>().ok()) {
        Some(d) if d > 0 => d % 5,
        _ => (id.parse::<u64>().unwrap_or_default() >> 22) % 6,
      };
      format!("{CDN}/embed/avatars/{index}.png")
    }
  })
}

/// `name` (`#1234` for legacy tags).
fn handle(v: &Value) -> Option<String> {
  let name = v.str("username")?;
  Some(match v.str("discriminator").filter(|d| d != "0") {
    Some(d) => format!("{name}#{d}"),
    None => name,
  })
}

pub fn user(v: &Value) -> User {
  let id = v.str("id").unwrap_or_default();
  let mut u = User {
    name: v
      .first_str(&["global_name", "username"])
      .unwrap_or_default(),
    handle: handle(v),
    url: (!id.is_empty()).then(|| format!("{WEB}/users/{id}")),
    avatar: avatar(v),
    bio: v.str("bio"),
    created_at: snowflake_time(&id),
    id,
    raw: Some(v.clone()),
    ..User::default()
  };
  for key in ["bot", "system"] {
    if v.bool(key) == Some(true) {
      u.extra.insert(key.into(), json!(true));
    }
  }
  if let Some(p) = v.str("pronouns") {
    u.extra.insert("pronouns".into(), json!(p));
  }
  u
}

/// A guild member: the user with the guild's nickname and roles.
pub fn member(v: &Value) -> User {
  let mut u = user(v.at("user"));
  if let Some(nick) = v.str("nick") {
    u.extra.insert("global_name".into(), json!(u.name));
    u.name = nick;
  }
  for key in ["joined_at", "premium_since"] {
    if let Some(t) = v.str(key) {
      u.extra.insert(key.into(), json!(t));
    }
  }
  let roles = v.list("roles");
  if !roles.is_empty() {
    u.extra.insert("roles".into(), json!(roles));
  }
  u.raw = Some(v.clone());
  u
}

/// `/users/{id}/profile`: the user, their bio, connections and mutual servers.
pub fn profile(v: &Value) -> User {
  let mut u = user(v.at("user"));
  u.bio = v
    .first_str(&["user_profile.bio", "user.bio"])
    .filter(|b| !b.is_empty());
  if let Some(p) = v.str("user_profile.pronouns").filter(|p| !p.is_empty()) {
    u.extra.insert("pronouns".into(), json!(p));
  }
  let connections: Vec<Value> = v
    .list("connected_accounts")
    .iter()
    .map(|c| json!({"type": c.str("type"), "name": c.str("name"), "verified": c.bool("verified")}))
    .collect();
  if !connections.is_empty() {
    u.extra.insert("connections".into(), json!(connections));
  }
  let mutual = v.list("mutual_guilds");
  if !mutual.is_empty() {
    u.extra.insert("mutual_servers".into(), json!(mutual.len()));
  }
  if let Some(n) = v.u64("mutual_friends_count") {
    u.extra.insert("mutual_friends".into(), json!(n));
  }
  if let Some(t) = v.str("premium_since") {
    u.extra.insert("nitro_since".into(), json!(t));
  }
  u.raw = Some(v.clone());
  u
}

// ── guilds and channels ─────────────────────────────────────────────────

pub fn guild(v: &Value) -> Collection {
  let id = v.str("id").unwrap_or_default();
  let mut c = Collection {
    kind: "server".into(),
    name: v.str("name").unwrap_or_default(),
    description: v.str("description"),
    url: Some(format!("{WEB}/channels/{id}")),
    followers: v.u64("approximate_member_count"),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if let Some(n) = v.u64("approximate_presence_count") {
    c.extra.insert("online".into(), json!(n));
  }
  if v.bool("owner") == Some(true) {
    c.extra.insert("owner".into(), json!(true));
  }
  if let Some(hash) = v.str("icon") {
    c.extra
      .insert("icon".into(), json!(format!("{CDN}/icons/{id}/{hash}.png")));
  }
  if let Some(t) = snowflake_time(&id) {
    c.extra.insert("created_at".into(), json!(t.to_string()));
  }
  c.id = id;
  c
}

/// `kind` of a channel type.
pub fn channel_kind(t: u64) -> &'static str {
  match t {
    0 => "channel",
    1 => "dm",
    2 => "voice",
    3 => "group",
    4 => "category",
    5 => "announcement",
    10 | 11 => "thread",
    12 => "private-thread",
    13 => "stage",
    14 => "directory",
    15 => "forum",
    16 => "media",
    _ => "channel",
  }
}

/// What a channel is called: `#name`, the other people of a direct chat, or
/// a group's name.
pub fn chat_name(v: &Value) -> String {
  let t = v.u64("type").unwrap_or_default();
  if let Some(name) = v.str("name").filter(|n| !n.is_empty()) {
    return match t {
      1 | 3 => name,
      _ => format!("#{name}"),
    };
  }
  let people: Vec<String> = v
    .list("recipients")
    .iter()
    .filter_map(|r| r.first_str(&["global_name", "username"]))
    .collect();
  if people.is_empty() {
    v.str("id").unwrap_or_default()
  } else {
    format!("@{}", people.join(", @"))
  }
}

pub fn channel(v: &Value) -> Collection {
  let id = v.str("id").unwrap_or_default();
  let t = v.u64("type").unwrap_or_default();
  let guild = v.str("guild_id");
  let mut c = Collection {
    kind: channel_kind(t).into(),
    name: chat_name(v),
    description: v.str("topic").filter(|t| !t.is_empty()),
    url: Some(permalink(guild.as_deref().unwrap_or("@me"), &id, None)),
    items: v.u64("message_count"),
    followers: v
      .u64("member_count")
      .or_else(|| (matches!(t, 1 | 3)).then(|| v.list("recipients").len() as u64 + 1)),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if let Some(g) = guild {
    c.extra.insert("guild".into(), json!(g));
  }
  if let Some(p) = v.str("parent_id") {
    c.extra.insert("parent".into(), json!(p));
  }
  if let Some(t) = v.str("last_message_id").and_then(|m| snowflake_time(&m)) {
    c.extra
      .insert("last_message_at".into(), json!(t.to_string()));
  }
  if let Some(s) = v.u64("rate_limit_per_user").filter(|s| *s > 0) {
    c.extra.insert("slowmode_seconds".into(), json!(s));
  }
  if v.bool("nsfw") == Some(true) {
    c.extra.insert("nsfw".into(), json!(true));
  }
  if v.bool("thread_metadata.archived") == Some(true) {
    c.extra.insert("archived".into(), json!(true));
  }
  c.id = id;
  c
}

// ── messages ────────────────────────────────────────────────────────────

fn kind(t: u64) -> &'static str {
  match t {
    0 => "message",
    19 => "reply",
    20 | 23 => "command",
    18 | 21 => "thread",
    6 => "pin",
    7 => "join",
    8..=11 => "boost",
    _ => "system",
  }
}

/// `<@123>` → `@name` with the message's mention list, `<:emoji:1>` → `:emoji:`.
fn readable(content: &str, mentions: &[Value]) -> String {
  let mut text = content.to_owned();
  for m in mentions {
    let (Some(id), Some(name)) = (m.str("id"), m.first_str(&["global_name", "username"])) else {
      continue;
    };
    for token in [format!("<@{id}>"), format!("<@!{id}>")] {
      text = text.replace(&token, &format!("@{name}"));
    }
  }
  let emoji = regex_lite();
  emoji.replace_all(&text, ":$1:").into_owned()
}

fn regex_lite() -> &'static regex::Regex {
  static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
  RE.get_or_init(|| regex::Regex::new(r"<a?:(\w+):\d+>").expect("valid regex"))
}

/// Attachments and embeds that can be downloaded; other files stay in `extra`.
fn media(v: &Value) -> (Vec<Media>, Vec<Value>) {
  let mut media = Vec::new();
  let mut files = Vec::new();
  for a in v.list("attachments") {
    let Some(url) = a.str("url") else { continue };
    let ct = a.str("content_type").unwrap_or_default();
    let kind = if ct == "image/gif" {
      Some(MediaKind::Gif)
    } else if ct.starts_with("image/") {
      Some(MediaKind::Image)
    } else if ct.starts_with("video/") {
      Some(MediaKind::Video)
    } else if ct.starts_with("audio/") {
      Some(MediaKind::Audio)
    } else {
      None
    };
    match kind {
      Some(kind) => {
        let mut m = Media::new(kind, url);
        m.width = a.u64("width").map(|w| w as u32);
        m.height = a.u64("height").map(|h| h as u32);
        m.size = a.u64("size");
        m.duration = a.f64("duration_secs");
        m.alt = a.first_str(&["description", "filename"]);
        media.push(m);
      }
      None => files.push(json!({
        "filename": a.str("filename"),
        "url": url,
        "size": a.u64("size"),
        "content_type": a.str("content_type"),
      })),
    }
  }
  for e in v.list("embeds") {
    match e.str("type").as_deref() {
      Some("gifv") => {
        if let Some(url) = e.str("video.url") {
          media.push(Media::new(MediaKind::Gif, url));
        }
      }
      Some("image") => {
        if let Some(url) = e.first_str(&["image.url", "thumbnail.url"]) {
          media.push(Media::image(url));
        }
      }
      Some("video") if e.str("provider.name").is_none() => {
        if let Some(url) = e.str("video.url") {
          media.push(Media::video(url));
        }
      }
      _ => {
        if let Some(url) = e.str("image.url") {
          media.push(Media::image(url));
        }
      }
    }
  }
  (media, files)
}

/// A message. `guild` and `chat_name` fill in what message payloads lack.
pub fn message(v: &Value, guild: Option<&str>, chat_name: Option<&str>) -> Post {
  let channel = v.str("channel_id").unwrap_or_default();
  let id = v.str("id").unwrap_or_default();
  let guild = v.str("guild_id").or_else(|| guild.map(str::to_owned));
  let mentions = v.list("mentions");
  let content = v.str("content").unwrap_or_default();
  let mut author = v.at("author").is_object().then(|| user(v.at("author")));
  if let (Some(a), Some(nick)) = (author.as_mut(), v.str("member.nick")) {
    a.name = nick;
  }
  let reactions: Vec<Value> = v
    .list("reactions")
    .iter()
    .map(|r| {
      json!({
        "emoji": emoji_name(r.at("emoji")),
        "count": r.u64("count"),
        "me": r.bool("me"),
      })
    })
    .collect();
  let total: u64 = v
    .list("reactions")
    .iter()
    .filter_map(|r| r.u64("count"))
    .sum();
  let (media, files) = media(v);
  let mut p = Post {
    kind: kind(v.u64("type").unwrap_or_default()).into(),
    text: (!content.is_empty()).then(|| readable(&content, mentions)),
    url: guild.as_deref().map(|g| permalink(g, &channel, Some(&id))),
    author,
    created_at: v.str("timestamp").and_then(|t| parse_time(&t)),
    updated_at: v.str("edited_timestamp").and_then(|t| parse_time(&t)),
    metrics: Metrics {
      likes: (!reactions.is_empty()).then_some(total),
      comments: v.u64("thread.message_count"),
      ..Metrics::default()
    },
    media,
    quoted: v.at("referenced_message").is_object().then(|| {
      Box::new(message(
        v.at("referenced_message"),
        guild.as_deref(),
        chat_name,
      ))
    }),
    raw: Some(v.clone()),
    ..Post::default()
  };
  let extra = &mut p.extra;
  extra.insert("chat".into(), json!(channel));
  if let Some(name) = chat_name {
    extra.insert("chat_name".into(), json!(name));
  }
  if let Some(g) = guild.filter(|g| g != "@me") {
    extra.insert("guild".into(), json!(g));
  }
  extra.insert("message_id".into(), json!(id));
  if let Some(r) = v.str("message_reference.message_id") {
    extra.insert(
      "reply_to".into(),
      json!(format!(
        "{}/{r}",
        v.str("message_reference.channel_id")
          .unwrap_or(channel.clone())
      )),
    );
  }
  if v.bool("pinned") == Some(true) {
    extra.insert("pinned".into(), json!(true));
  }
  if !reactions.is_empty() {
    extra.insert("reactions".into(), json!(reactions));
  }
  if !files.is_empty() {
    extra.insert("files".into(), json!(files));
  }
  let names: Vec<String> = mentions
    .iter()
    .filter_map(|m| m.first_str(&["username"]))
    .collect();
  if !names.is_empty() {
    extra.insert("mentions".into(), json!(names));
  }
  let embeds: Vec<Value> = v
    .list("embeds")
    .iter()
    .filter(|e| e.str("title").is_some() || e.str("description").is_some())
    .map(|e| json!({"title": e.str("title"), "description": e.str("description"), "url": e.str("url")}))
    .collect();
  if !embeds.is_empty() {
    extra.insert("embeds".into(), json!(embeds));
  }
  if let Some(t) = v.str("thread.id") {
    extra.insert(
      "thread".into(),
      json!({"id": t, "name": v.str("thread.name"), "messages": v.u64("thread.message_count")}),
    );
  }
  let stickers: Vec<String> = v
    .list("sticker_items")
    .iter()
    .filter_map(|s| s.str("name"))
    .collect();
  if !stickers.is_empty() {
    extra.insert("stickers".into(), json!(stickers));
  }
  if v.str("webhook_id").is_some() {
    extra.insert("webhook".into(), json!(true));
  }
  p.id = format!("{channel}/{id}");
  p
}

/// `👍`, or `name:id` for a custom emoji (the form reactions take back).
pub fn emoji_name(e: &Value) -> String {
  match (e.str("name"), e.str("id")) {
    (Some(name), Some(id)) => format!("{name}:{id}"),
    (Some(name), None) => name,
    (None, Some(id)) => id,
    _ => String::new(),
  }
}
