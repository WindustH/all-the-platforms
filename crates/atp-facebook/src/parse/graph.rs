//! Graph API objects: Page posts and their comments, Pages, Messenger
//! conversations and messages.

use atp_core::{Collection, Comment, Media, MediaKind, Metrics, Post, User, Value, ValueExt, chat};
use atp_meta::time;

/// The fields [`post`] reads.
pub const POST_FIELDS: &str = "id,message,story,created_time,updated_time,permalink_url,full_picture,status_type,from{id,name},attachments{media_type,media,url,title,subattachments},shares,reactions.summary(total_count).limit(0),comments.summary(total_count).limit(0)";

/// `from` / `author` objects (`{id, name}`).
fn author(v: &Value) -> Option<User> {
  v.is_object().then(|| User {
    id: v.str("id").unwrap_or_default(),
    name: v.str("name").unwrap_or_default(),
    ..User::default()
  })
}

pub fn post(v: &Value) -> Post {
  let mut media = Vec::new();
  for a in v.list("attachments.data") {
    attachment_media(a, &mut media);
  }
  let mut p = Post {
    id: v.str("id").unwrap_or_default(),
    kind: v.str("status_type").unwrap_or_else(|| "post".into()),
    text: v.str("message").or_else(|| v.str("story")),
    url: v.str("permalink_url"),
    author: author(v.at("from")),
    created_at: v.str("created_time").and_then(|t| time::parse(&t)),
    updated_at: v.str("updated_time").and_then(|t| time::parse(&t)),
    metrics: Metrics {
      likes: v.count("reactions.summary.total_count"),
      comments: v.count("comments.summary.total_count"),
      shares: v.count("shares.count"),
      ..Metrics::default()
    },
    media,
    raw: Some(v.clone()),
    ..Post::default()
  };
  if let Some(picture) = v.str("full_picture") {
    p.extra.insert("picture".into(), picture.into());
  }
  p
}

fn attachment_media(a: &Value, out: &mut Vec<Media>) {
  let source = match a.str("media_type").as_deref() {
    Some("video") => a.str("media.source").map(Media::video),
    _ => a.str("media.image.src").map(Media::image),
  };
  out.extend(source);
  for sub in a.list("subattachments.data") {
    attachment_media(sub, out);
  }
}

pub fn comment(v: &Value) -> Comment {
  Comment {
    id: v.str("id").unwrap_or_default(),
    author: author(v.at("from")),
    text: v.str("message").unwrap_or_default(),
    created_at: v.str("created_time").and_then(|t| time::parse(&t)),
    likes: v.count("like_count"),
    reply_count: v.count("comment_count"),
    raw: Some(v.clone()),
    ..Comment::default()
  }
}

/// A Page the token manages.
pub fn page(p: &Value) -> Collection {
  Collection {
    id: p.str("id").unwrap_or_default(),
    kind: "page".into(),
    name: p.str("name").unwrap_or_default(),
    description: p.str("category"),
    url: p.str("link"),
    followers: p.count("followers_count").or_else(|| p.count("fan_count")),
    extra: [
      ("username", p.at("username").clone()),
      ("likes", p.at("fan_count").clone()),
      ("tasks", p.at("tasks").clone()),
    ]
    .into_iter()
    .filter(|(_, v)| !v.is_null())
    .map(|(k, v)| (k.to_owned(), v))
    .collect(),
    ..Collection::default()
  }
}

/// A conversation of `page`'s inbox, named after the other participant.
pub fn conversation(v: &Value, page: &str) -> Collection {
  let other = v
    .list("participants.data")
    .iter()
    .find(|p| p.str("id").as_deref() != Some(page))
    .cloned()
    .unwrap_or_default();
  Collection {
    id: v.str("id").unwrap_or_default(),
    kind: "dm".into(),
    name: other.str("name").unwrap_or_default(),
    description: v.str("snippet"),
    url: v
      .str("link")
      .map(|link| format!("https://www.facebook.com{link}")),
    items: v.count("message_count"),
    extra: [
      ("page".to_owned(), Value::from(page)),
      ("updated_time".to_owned(), v.at("updated_time").clone()),
      ("unread".to_owned(), v.at("unread_count").clone()),
      ("participant".to_owned(), other.at("id").clone()),
    ]
    .into_iter()
    .filter(|(_, v)| !v.is_null())
    .collect(),
    raw: Some(v.clone()),
    ..Collection::default()
  }
}

/// A message of conversation `chat`, its id `CHAT/MESSAGE`.
pub fn message(chat: &str, m: &Value) -> Post {
  let media = m
    .list("attachments.data")
    .iter()
    .filter_map(|a| {
      a.str("image_data.url")
        .map(Media::image)
        .or_else(|| a.str("video_data.url").map(Media::video))
        .or_else(|| a.str("file_url").map(|u| Media::new(MediaKind::File, u)))
    })
    .collect();
  let mut author = author(m.at("from"));
  if let Some(a) = &mut author {
    a.handle = m.str("from.email");
  }
  Post {
    id: chat::message_id(chat, &m.str("id").unwrap_or_default()),
    kind: "message".into(),
    text: m.str("message"),
    author,
    created_at: m.str("created_time").and_then(|t| time::parse(&t)),
    media,
    extra: [("chat".to_owned(), Value::from(chat))].into(),
    raw: Some(m.clone()),
    ..Post::default()
  }
}
