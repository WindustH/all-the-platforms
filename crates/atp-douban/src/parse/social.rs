//! People and what they write: users, short comments (短评), reviews,
//! groups, group topics and replies.

use atp_core::text::html_to_text;
use atp_core::{Comment, Media, Metrics, Post, User, UserStats, Value, ValueExt, json};

use super::time;

/// A user as lists show them (`author`, `user`) or as a profile.
pub fn user(v: &Value) -> User {
  let id = v.str("id").unwrap_or_default();
  let uid = v.str("uid").filter(|u| !u.is_empty());
  let name_in_url = uid.clone().unwrap_or_else(|| id.clone());
  let stats = UserStats {
    followers: v.u64("followers_count"),
    following: v.u64("following_count"),
    posts: v.u64("statuses_count"),
    ..UserStats::default()
  };
  let mut user = User {
    url: Some(format!("https://www.douban.com/people/{name_in_url}/")),
    name: v.str("name").unwrap_or_else(|| name_in_url.clone()),
    handle: uid,
    avatar: v.first_str(&["large_avatar", "avatar"]),
    bio: v.str("intro").filter(|t| !t.trim().is_empty()),
    location: v.str("loc.name"),
    stats,
    followed: v.bool("followed"),
    created_at: time(v, "reg_time"),
    raw: Some(v.clone()),
    id,
    ..User::default()
  };
  for key in [
    "movie_collected_count",
    "book_collected_count",
    "music_collected_count",
    "reviews_count",
    "notes_count",
    "joined_group_count",
  ] {
    if let Some(n) = v.u64(key).filter(|n| *n > 0) {
      user
        .extra
        .insert(key.trim_end_matches("_count").into(), json!(n));
    }
  }
  if let Some(place) = v.str("ip_location").filter(|p| !p.is_empty()) {
    user.extra.insert("ip_location".into(), json!(place));
  }
  user
}

fn author(v: &Value, path: &str) -> Option<User> {
  let at = v.at(path);
  at.is_object().then(|| user(at))
}

/// A short comment (短评) on a subject, with its stars (1–5) and state.
pub fn interest(v: &Value) -> Comment {
  let mut comment = Comment {
    id: v.str("id").unwrap_or_default(),
    author: author(v, "user"),
    text: v.str("comment").and_then(|t| clean(&t)).unwrap_or_default(),
    created_at: time(v, "create_time"),
    likes: v.u64("vote_count"),
    location: v.str("ip_location").filter(|p| !p.is_empty()),
    raw: Some(v.clone()),
    ..Comment::default()
  };
  if let Some(stars) = v.u64("rating.value") {
    comment.extra.insert("rating".into(), json!(stars));
  }
  if let Some(status) = v.str("status") {
    comment
      .extra
      .insert("status".into(), json!(super::status_name(&status)));
  }
  comment
}

/// A reply in a topic or under a review, with the replies under it.
pub fn comment(v: &Value) -> Comment {
  Comment {
    id: v.str("id").unwrap_or_default(),
    author: author(v, "author"),
    text: v.str("text").and_then(|t| clean(&t)).unwrap_or_default(),
    created_at: time(v, "create_time"),
    likes: v.u64("vote_count"),
    location: v.str("ip_location").filter(|p| !p.is_empty()),
    reply_to: v.str("ref_comment.author.name"),
    reply_count: v.u64("replies_count"),
    replies: v.list("replies").iter().map(comment).collect(),
    raw: Some(v.clone()),
    ..Comment::default()
  }
}

/// Text with Windows line ends made plain, trimmed; `None` when empty.
fn clean(text: &str) -> Option<String> {
  let text = text.replace("\r\n", "\n");
  Some(text.trim().to_owned()).filter(|t| !t.is_empty())
}

/// The text of a long post: its HTML `content` (whose paragraphs keep
/// line breaks as plain newlines), else its `abstract`.
fn body(v: &Value) -> Option<String> {
  v.str("content")
    .map(|html| html_to_text(&html.replace("\r\n", "\n").replace('\n', "<br>")))
    .or_else(|| v.str("abstract"))
    .and_then(|t| clean(&t))
}

fn photos(v: &Value) -> Vec<Media> {
  v.list("photos")
    .iter()
    .filter_map(|p| p.first_str(&["image.large.url", "image.normal.url", "src"]))
    .map(Media::image)
    .collect()
}

/// A review (影评 / 书评 / 乐评), whole on its own page or abridged in lists.
pub fn review(v: &Value) -> Post {
  let id = v.str("id").unwrap_or_default();
  let metrics = Metrics {
    comments: v.u64("comments_count"),
    likes: v.u64("useful_count"),
    shares: v.u64("reshares_count"),
    views: v.u64("read_count"),
    favorites: v.u64("collections_count"),
    ..Metrics::default()
  };
  let mut post = Post {
    url: v
      .str("url")
      .or_else(|| Some(format!("https://www.douban.com/review/{id}/"))),
    kind: "review".into(),
    title: v.str("title"),
    text: body(v),
    author: author(v, "user"),
    created_at: time(v, "create_time"),
    metrics,
    media: photos(v),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  if let Some(stars) = v.u64("rating.value") {
    post.extra.insert("rating".into(), json!(stars));
  }
  let subject = v.at("subject");
  if subject.is_object() {
    post.extra.insert(
      "subject".into(),
      json!({"id": subject.str("id"), "title": subject.str("title"), "type": subject.str("type")}),
    );
  }
  if v.bool("spoiler") == Some(true) {
    post.extra.insert("spoiler".into(), json!(true));
  }
  post
}

/// A group topic (小组话题).
pub fn topic(v: &Value) -> Post {
  let id = v.str("id").unwrap_or_default();
  let metrics = Metrics {
    comments: v.u64("comments_count"),
    likes: v.u64("like_count").or_else(|| v.u64("reactions_count")),
    shares: v.u64("reshares_count"),
    favorites: v.u64("collections_count"),
    views: v.u64("read_count"),
    ..Metrics::default()
  };
  let mut post = Post {
    // Douban's own link carries the `_spm_id` the topic is served with.
    url: v
      .str("url")
      .filter(|u| u.contains("/group/topic/"))
      .or_else(|| Some(format!("https://www.douban.com/group/topic/{id}/"))),
    kind: "topic".into(),
    title: v.str("title"),
    text: body(v),
    author: author(v, "author"),
    created_at: time(v, "create_time"),
    updated_at: time(v, "update_time"),
    metrics,
    media: photos(v),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  let group = v.at("group");
  if group.is_object() {
    post.extra.insert(
      "group".into(),
      json!({"id": group.str("id"), "name": group.str("name")}),
    );
  }
  if let Some(place) = v.str("ip_location").filter(|p| !p.is_empty()) {
    post.extra.insert("ip_location".into(), json!(place));
  }
  post
}

/// A group (小组): its name, introduction and size.
pub fn group(v: &Value) -> Post {
  let id = v.str("id").unwrap_or_default();
  let mut post = Post {
    url: Some(format!("https://www.douban.com/group/{id}/")),
    kind: "group".into(),
    title: v.str("name"),
    text: v
      .first_str(&["desc_abstract", "desc"])
      .and_then(|t| clean(&html_to_text(&t))),
    created_at: time(v, "create_time"),
    media: v
      .first_str(&["large_avatar", "avatar"])
      .map(Media::image)
      .into_iter()
      .collect(),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  if let Some(members) = v.u64("member_count") {
    post.metrics.other.insert("members".into(), members);
  }
  if let Some(topics) = v.u64("topic_count") {
    post.metrics.other.insert("topics".into(), topics);
  }
  if let Some(name) = v.str("member_name").filter(|n| !n.is_empty()) {
    post.extra.insert("member_name".into(), json!(name));
  }
  post
}
