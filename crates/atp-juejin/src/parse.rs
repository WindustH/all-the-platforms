//! Juejin payloads → the shared model. Articles are posts of kind
//! `article`, pins (沸点) of kind `pin`; comments carry their first replies;
//! users are users.

use std::sync::LazyLock;

use atp_core::text::{from_secs, html_to_text};
use atp_core::{Collection, Comment, Media, Metrics, Post, User, UserStats, Value, ValueExt, json};
use regex::Regex;

use crate::api::SITE;
use crate::markdown;

/// A topic in a pin's text: `[<topic id>#<title>#]`.
static TOPIC: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"\[\d+#([^#\]]+)#\]").expect("valid pattern"));

fn secs(v: &Value, path: &str) -> Option<jiff::Timestamp> {
  v.i64(path).and_then(from_secs)
}

pub fn user(v: &Value) -> User {
  let id = v.str("user_id").unwrap_or_default();
  let stats = UserStats {
    followers: v.u64("follower_count"),
    following: v.u64("followee_count"),
    posts: v.u64("post_article_count"),
    likes: v.u64("got_digg_count"),
    ..UserStats::default()
  };
  let mut user = User {
    url: Some(format!("{SITE}/user/{id}")),
    name: v
      .first_str(&["user_name", "name"])
      .unwrap_or_else(|| id.clone()),
    avatar: v.first_str(&["avatar_large", "avatar"]),
    bio: v.str("description").filter(|d| !d.trim().is_empty()),
    stats,
    followed: v.bool("isfollowed").or_else(|| v.bool("is_followed")),
    created_at: secs(v, "register_time"),
    raw: Some(v.clone()),
    id,
    ..User::default()
  };
  for (key, path) in [
    ("job_title", "job_title"),
    ("company", "company"),
    ("blog", "blog_address"),
  ] {
    if let Some(value) = v.str(path).filter(|s| !s.is_empty()) {
      user.extra.insert(key.into(), json!(value));
    }
  }
  for (key, path) in [
    ("level", "level"),
    ("views", "got_view_count"),
    ("pins", "post_shortmsg_count"),
  ] {
    if let Some(n) = v.u64(path).filter(|n| *n > 0) {
      user.extra.insert(key.into(), json!(n));
    }
  }
  user
}

fn author(v: &Value, path: &str) -> Option<User> {
  let at = v.at(path);
  at.is_object().then(|| user(at))
}

/// An article from a feed, a search result, a user's list or its detail
/// (which also has the whole text).
pub fn article(v: &Value) -> Post {
  let info = v.at("article_info");
  let id = info
    .str("article_id")
    .or_else(|| v.str("article_id"))
    .unwrap_or_default();
  let mut tags: Vec<String> = v
    .list("tags")
    .iter()
    .filter_map(|t| t.str("tag_name"))
    .collect();
  if let Some(category) = v.str("category.category_name")
    && !tags.contains(&category)
  {
    tags.insert(0, category);
  }
  let (text, images) = body(info);
  let mut media: Vec<Media> = info
    .str("cover_image")
    .map(Media::image)
    .into_iter()
    .collect();
  media.extend(images.into_iter().map(Media::image));
  let mut post = Post {
    url: Some(format!("{SITE}/post/{id}")),
    kind: "article".into(),
    title: info.str("title"),
    text,
    author: author(v, "author_user_info"),
    created_at: secs(info, "ctime"),
    updated_at: secs(info, "mtime"),
    metrics: Metrics {
      views: info.u64("view_count"),
      likes: info.u64("digg_count"),
      comments: info.u64("comment_count"),
      favorites: info.u64("collect_count"),
      ..Metrics::default()
    },
    media,
    tags,
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  if let Some(minutes) = info.str("read_time") {
    post.extra.insert("read_time".into(), json!(minutes));
  }
  post
}

/// The whole text (Markdown or HTML) when the payload has it, else the brief.
fn body(info: &Value) -> (Option<String>, Vec<String>) {
  if let Some(md) = info.str("mark_content") {
    let (text, images) = markdown::to_text(&md);
    return (Some(text).filter(|t| !t.is_empty()), images);
  }
  let html = info.first_str(&["content", "web_html_content"]);
  match html {
    Some(html) => (Some(html_to_text(&html)), Vec::new()),
    None => (info.str("brief_content"), Vec::new()),
  }
}

/// An entry of the hot ranking (热榜): the article in brief, with its heat.
pub fn ranked(v: &Value, rank: usize) -> Post {
  let content = v.at("content");
  let counter = v.at("content_counter");
  let id = content.str("content_id").unwrap_or_default();
  let author = v.at("author");
  let mut post = Post {
    url: Some(format!("{SITE}/post/{id}")),
    kind: "article".into(),
    title: content.str("title"),
    text: content.str("brief").filter(|b| !b.is_empty()),
    author: author.is_object().then(|| user(author)),
    metrics: Metrics {
      views: counter.u64("view"),
      likes: counter.u64("like"),
      comments: counter.u64("comment_count"),
      favorites: counter.u64("collect"),
      ..Metrics::default()
    },
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  post.extra.insert("rank".into(), json!(rank));
  if let Some(heat) = counter.u64("hot_rank") {
    post.extra.insert("heat".into(), json!(heat));
  }
  post
}

/// A pin (沸点).
pub fn pin(v: &Value) -> Post {
  let msg = v.at("msg_Info");
  let id = v
    .str("msg_id")
    .or_else(|| msg.str("msg_id"))
    .unwrap_or_default();
  let mut post = Post {
    url: Some(format!("{SITE}/pin/{id}")),
    kind: "pin".into(),
    text: msg
      .str("content")
      .map(|t| TOPIC.replace_all(&t, "#$1#").into_owned()),
    author: author(v, "author_user_info"),
    created_at: secs(msg, "ctime"),
    metrics: Metrics {
      likes: msg.u64("digg_count"),
      comments: msg.u64("comment_count"),
      ..Metrics::default()
    },
    media: msg
      .list("pic_list")
      .iter()
      .filter_map(Value::as_str)
      .map(Media::image)
      .collect(),
    tags: v.str("topic.title").into_iter().collect(),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  if let Some(link) = msg.str("url").filter(|u| !u.is_empty()) {
    post.extra.insert(
      "link".into(),
      json!({"url": link, "title": msg.str("url_title")}),
    );
  }
  post
}

/// A reply under a comment.
pub fn reply(v: &Value) -> Comment {
  let info = v.at("reply_info");
  Comment {
    id: info
      .str("reply_id")
      .or_else(|| v.str("reply_id"))
      .unwrap_or_default(),
    author: author(v, "user_info"),
    text: info.str("reply_content").unwrap_or_default(),
    created_at: secs(info, "ctime"),
    likes: info.u64("digg_count"),
    reply_to: v.str("reply_user.user_name").filter(|n| !n.is_empty()),
    raw: Some(v.clone()),
    ..Comment::default()
  }
}

/// A comment with the replies shown under it.
pub fn comment(v: &Value) -> Comment {
  let info = v.at("comment_info");
  let mut comment = Comment {
    id: info
      .str("comment_id")
      .or_else(|| v.str("comment_id"))
      .unwrap_or_default(),
    author: author(v, "user_info"),
    text: info.str("comment_content").unwrap_or_default(),
    created_at: secs(info, "ctime"),
    likes: info.u64("digg_count"),
    reply_count: info.u64("reply_count"),
    replies: v.list("reply_infos").iter().map(reply).collect(),
    raw: Some(v.clone()),
    ..Comment::default()
  };
  let pictures: Vec<&str> = info
    .list("comment_pics")
    .iter()
    .filter_map(|p| p.as_str().or_else(|| p.get("url").and_then(Value::as_str)))
    .collect();
  if !pictures.is_empty() {
    comment.extra.insert("pictures".into(), json!(pictures));
  }
  comment
}

/// A collection set (收藏集).
pub fn collection(v: &Value) -> Collection {
  let id = v.str("tag_id").unwrap_or_default();
  Collection {
    url: Some(format!("{SITE}/collection/{id}")),
    kind: "folder".into(),
    name: v.str("tag_name").unwrap_or_default(),
    description: v.str("description").filter(|d| !d.is_empty()),
    items: v.u64("post_article_count").or_else(|| v.u64("entry_count")),
    followers: v.u64("concern_user_count"),
    raw: Some(v.clone()),
    id,
    ..Collection::default()
  }
}
