//! Messages: text with mentions resolved, media, reactions and replies.

use atp_core::chat;
use atp_core::text::parse_time;
use atp_core::{Media, MediaKind, Metrics, Post, Value, ValueExt, json};

use super::{permalink, user};

fn kind(code: u64) -> &'static str {
  match code {
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
    match attachment_kind(&a.str("content_type").unwrap_or_default()) {
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
  media.extend(v.list("embeds").iter().filter_map(embed_media));
  (media, files)
}

/// Images, GIFs, videos and audio download; other files do not.
fn attachment_kind(content_type: &str) -> Option<MediaKind> {
  match content_type.split('/').next() {
    _ if content_type == "image/gif" => Some(MediaKind::Gif),
    Some("image") => Some(MediaKind::Image),
    Some("video") => Some(MediaKind::Video),
    Some("audio") => Some(MediaKind::Audio),
    _ => None,
  }
}

/// The picture or clip of an embed (GIFs, images, uploaded videos).
fn embed_media(embed: &Value) -> Option<Media> {
  match embed.str("type").as_deref() {
    Some("gifv") => Some(Media::new(MediaKind::Gif, embed.str("video.url")?)),
    Some("image") => Some(Media::image(
      embed.first_str(&["image.url", "thumbnail.url"])?,
    )),
    Some("video") if embed.str("provider.name").is_none() => {
      Some(Media::video(embed.str("video.url")?))
    }
    _ => Some(Media::image(embed.str("image.url")?)),
  }
}

/// A message. `guild` and `chat_name` fill in what message payloads lack.
pub fn message(v: &Value, guild: Option<&str>, chat_name: Option<&str>) -> Post {
  let channel = v.str("channel_id").unwrap_or_default();
  let id = v.str("id").unwrap_or_default();
  let guild = v.str("guild_id").or_else(|| guild.map(str::to_owned));
  let content = v.str("content").unwrap_or_default();
  let mut author = v.at("author").is_object().then(|| user(v.at("author")));
  if let (Some(a), Some(nick)) = (author.as_mut(), v.str("member.nick")) {
    a.name = nick;
  }
  let reactions = reactions(v);
  let total: u64 = v
    .list("reactions")
    .iter()
    .filter_map(|r| r.u64("count"))
    .sum();
  let (media, files) = media(v);
  let quoted = v.at("referenced_message");
  let mut post = Post {
    id: chat::message_id(&channel, &id),
    kind: kind(v.u64("type").unwrap_or_default()).into(),
    text: (!content.is_empty()).then(|| readable(&content, v.list("mentions"))),
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
    quoted: quoted
      .is_object()
      .then(|| Box::new(message(quoted, guild.as_deref(), chat_name))),
    raw: Some(v.clone()),
    ..Post::default()
  };
  let extra = &mut post.extra;
  extra.insert("chat".into(), json!(channel));
  if let Some(name) = chat_name {
    extra.insert("chat_name".into(), json!(name));
  }
  if let Some(g) = guild.filter(|g| g != "@me") {
    extra.insert("guild".into(), json!(g));
  }
  extra.insert("message_id".into(), json!(id));
  if !reactions.is_empty() {
    extra.insert("reactions".into(), json!(reactions));
  }
  if !files.is_empty() {
    extra.insert("files".into(), json!(files));
  }
  message_extra(&mut post, v, &channel);
  post
}

fn reactions(v: &Value) -> Vec<Value> {
  v.list("reactions")
    .iter()
    .map(|r| {
      json!({
        "emoji": emoji_name(r.at("emoji")),
        "count": r.u64("count"),
        "me": r.bool("me"),
      })
    })
    .collect()
}

/// Replies, pins, mentions, embeds, threads, stickers and webhooks.
fn message_extra(post: &mut Post, v: &Value, channel: &str) {
  let extra = &mut post.extra;
  if let Some(r) = v.str("message_reference.message_id") {
    let in_channel = v
      .str("message_reference.channel_id")
      .unwrap_or_else(|| channel.to_owned());
    extra.insert("reply_to".into(), json!(chat::message_id(&in_channel, &r)));
  }
  if v.bool("pinned") == Some(true) {
    extra.insert("pinned".into(), json!(true));
  }
  let names: Vec<String> = v
    .list("mentions")
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
}

/// `👍`, or `name:id` for a custom emoji (the form reactions take back).
pub fn emoji_name(emoji: &Value) -> String {
  match (emoji.str("name"), emoji.str("id")) {
    (Some(name), Some(id)) => format!("{name}:{id}"),
    (Some(name), None) => name,
    (None, Some(id)) => id,
    _ => String::new(),
  }
}
