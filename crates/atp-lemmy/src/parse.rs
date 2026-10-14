//! Lemmy views → the shared model: posts are posts, comments comments
//! (threads rebuilt from their `path`), communities collections, persons
//! users, replies and mentions notifications. Links point at the instance
//! in use (`https://<instance>/post/<id>`), so they work as arguments.

use std::collections::HashMap;

use atp_core::{
  Collection, Comment, Media, MediaKind, Metrics, Notification, Post, User, UserStats, Value,
  ValueExt, json,
};
use jiff::Timestamp;

fn time(v: &Value, path: &str) -> Option<Timestamp> {
  let text = v.str(path)?;
  text
    .parse()
    .ok()
    .or_else(|| format!("{text}Z").parse().ok())
}

/// The host of an ActivityPub id (`https://lemmy.ml/u/x` → `lemmy.ml`).
fn host_of(actor: &str) -> Option<&str> {
  actor.split("://").nth(1)?.split('/').next()
}

/// `name@host` of a person or community.
fn full_name(v: &Value) -> String {
  let name = v.str("name").unwrap_or_default();
  match v.str("actor_id").as_deref().and_then(host_of) {
    Some(host) => format!("{name}@{host}"),
    None => name,
  }
}

pub fn person(v: &Value) -> User {
  let full = full_name(v);
  User {
    id: full.clone(),
    name: v
      .str("display_name")
      .or_else(|| v.str("name"))
      .unwrap_or_default(),
    handle: Some(format!("@{full}")),
    url: v.str("actor_id"),
    avatar: v.str("avatar"),
    bio: v.str("bio"),
    created_at: time(v, "published"),
    raw: Some(v.clone()),
    ..User::default()
  }
}

/// A user page (`person_view`): the person with their counts.
pub fn person_view(v: &Value) -> User {
  let mut user = person(v.at("person"));
  user.stats = UserStats {
    posts: v.u64("counts.post_count"),
    ..UserStats::default()
  };
  if let Some(n) = v.u64("counts.comment_count") {
    user.stats.other.insert("comments".into(), n);
  }
  if v.bool("person.bot_account") == Some(true) {
    user.extra.insert("bot".into(), json!(true));
  }
  user.raw = Some(v.clone());
  user
}

pub fn community(v: &Value) -> Collection {
  let c = v.at("community");
  let mut community = Collection {
    id: full_name(c),
    kind: "community".into(),
    name: c.str("title").or_else(|| c.str("name")).unwrap_or_default(),
    description: c.str("description"),
    url: c.str("actor_id"),
    items: v.u64("counts.posts"),
    followers: v.u64("counts.subscribers"),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  for (key, value) in [
    ("comments", json!(v.u64("counts.comments"))),
    ("active_month", json!(v.u64("counts.users_active_month"))),
    ("subscribed", json!(v.str("subscribed"))),
    ("nsfw", json!(c.bool("nsfw").filter(|nsfw| *nsfw))),
    ("icon", json!(c.str("icon"))),
  ] {
    if !value.is_null() {
      community.extra.insert(key.into(), value);
    }
  }
  community
}

/// The media of a post: its link when that is an image or video, else
/// its thumbnail.
fn post_media(p: &Value) -> Vec<Media> {
  let mime = p.str("url_content_type").unwrap_or_default();
  let kind = MediaKind::from_mime(&mime);
  match (p.str("url"), kind) {
    (Some(url), MediaKind::Image | MediaKind::Gif | MediaKind::Video) => {
      vec![Media::new(kind, url)]
    }
    _ => p
      .str("thumbnail_url")
      .map(|url| vec![Media::image(url)])
      .unwrap_or_default(),
  }
}

pub fn post(v: &Value, instance: &str) -> Post {
  let p = v.at("post");
  let id = p.str("id").unwrap_or_default();
  let mut metrics = Metrics {
    likes: v.u64("counts.upvotes"),
    comments: v.u64("counts.comments"),
    ..Metrics::default()
  };
  if let Some(down) = v.u64("counts.downvotes") {
    metrics.other.insert("downvotes".into(), down);
  }
  let community = full_name(v.at("community"));
  let mut post = Post {
    url: Some(format!("https://{instance}/post/{id}")),
    kind: "post".into(),
    title: p.str("name"),
    text: p.str("body").filter(|body| !body.trim().is_empty()),
    author: Some(person(v.at("creator"))).map(|user| User { raw: None, ..user }),
    created_at: time(p, "published"),
    updated_at: time(p, "updated"),
    metrics,
    media: post_media(p),
    tags: vec![format!("!{community}")],
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  for (key, value) in [
    ("link", json!(p.str("url"))),
    ("community", json!(community)),
    ("score", json!(v.i64("counts.score"))),
    ("ap_id", json!(p.str("ap_id"))),
    ("my_vote", json!(v.i64("my_vote"))),
    ("saved", json!(v.bool("saved").filter(|saved| *saved))),
    ("nsfw", json!(p.bool("nsfw").filter(|nsfw| *nsfw))),
    ("locked", json!(p.bool("locked").filter(|locked| *locked))),
    (
      "featured",
      json!(p.bool("featured_community").filter(|f| *f)),
    ),
  ] {
    if !value.is_null() {
      post.extra.insert(key.into(), value);
    }
  }
  post
}

pub fn comment(v: &Value, instance: &str) -> Comment {
  let c = v.at("comment");
  let id = c.str("id").unwrap_or_default();
  let deleted = c.bool("deleted") == Some(true) || c.bool("removed") == Some(true);
  let mut comment = Comment {
    author: Some(User {
      raw: None,
      ..person(v.at("creator"))
    }),
    text: if deleted {
      "[deleted]".into()
    } else {
      c.str("content").unwrap_or_default()
    },
    created_at: time(c, "published"),
    likes: v.u64("counts.upvotes"),
    reply_count: v.u64("counts.child_count").filter(|n| *n > 0),
    raw: Some(v.clone()),
    ..Comment::default()
  };
  for (key, value) in [
    ("url", json!(format!("https://{instance}/comment/{id}"))),
    ("post_id", json!(c.u64("post_id"))),
    ("post", json!(v.str("post.name"))),
    ("score", json!(v.i64("counts.score"))),
    ("downvotes", json!(v.u64("counts.downvotes"))),
    ("my_vote", json!(v.i64("my_vote"))),
  ] {
    if !value.is_null() {
      comment.extra.insert(key.into(), value);
    }
  }
  comment.id = id;
  comment
}

/// The parent id of a comment from its `path` (`0.12.34` → 12); 0 at the
/// top of a post.
fn parent(v: &Value) -> (u64, usize) {
  let path = v.str("comment.path").unwrap_or_default();
  let parts: Vec<&str> = path.split('.').collect();
  let parent = parts
    .len()
    .checked_sub(2)
    .and_then(|i| parts[i].parse().ok())
    .unwrap_or(0);
  (parent, parts.len())
}

/// Comments of one thread, nested under `root` (0: the whole post), in
/// the order Lemmy gave them.
pub fn tree(rows: &[Value], root: u64, instance: &str) -> Vec<Comment> {
  let mut order: Vec<(usize, u64, Comment)> = rows
    .iter()
    .map(|row| {
      let (parent, depth) = parent(row);
      (depth, parent, comment(row, instance))
    })
    .collect();
  // Deepest first: every comment finds its replies already gathered.
  order.sort_by_key(|(depth, _, _)| std::cmp::Reverse(*depth));
  let mut children: HashMap<u64, Vec<Comment>> = HashMap::new();
  for (_, parent, mut comment) in order {
    let id: u64 = comment.id.parse().unwrap_or(0);
    comment.replies = children.remove(&id).unwrap_or_default();
    if id == root {
      return comment.replies;
    }
    children.entry(parent).or_default().push(comment);
  }
  children.remove(&root).unwrap_or_default()
}

/// A reply to you or a mention of you (`kind`: `reply`, `mention`).
pub fn notification(v: &Value, kind: &str, record: &str, instance: &str) -> Notification {
  let comment = comment(v, instance);
  Notification {
    id: v.str(&format!("{record}.id")).unwrap_or_default(),
    kind: kind.into(),
    text: comment.text,
    actor: comment.author,
    target: v.str("post.name"),
    url: comment
      .extra
      .get("url")
      .and_then(Value::as_str)
      .map(str::to_owned),
    created_at: time(v, &format!("{record}.published")).or(comment.created_at),
    unread: v.bool(&format!("{record}.read")).map(|read| !read),
    raw: Some(v.clone()),
  }
}
