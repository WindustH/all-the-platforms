//! Threads media objects → the shared model: posts (with the images and
//! videos of a carousel), replies as comments, mentions as notifications,
//! profiles as users.

use atp_core::{Comment, Media, Notification, Post, User, UserStats, Value, ValueExt, json};
use atp_meta::time;

use crate::refs;

/// The fields of a post that every listing asks for.
pub const POST_FIELDS: &str = "id,media_product_type,media_type,media_url,permalink,owner,username,\
text,timestamp,shortcode,thumbnail_url,children{id,media_type,media_url,thumbnail_url},\
is_quote_post,quoted_post,reposted_post,alt_text,link_attachment_url,topic_tag";
/// The fields of a reply.
pub const REPLY_FIELDS: &str = "id,media_type,media_url,permalink,username,text,timestamp,\
shortcode,thumbnail_url,children{id,media_type,media_url,thumbnail_url},has_replies,is_reply,\
root_post,replied_to,hide_status";

fn author(v: &Value) -> Option<User> {
  let name = v.str("username")?;
  Some(User {
    id: v.str("owner.id").unwrap_or_default(),
    name: name.clone(),
    handle: Some(format!("@{name}")),
    url: Some(refs::profile_url(&name)),
    ..User::default()
  })
}

/// One image or video (a carousel item or a whole post).
fn medium(v: &Value) -> Option<Media> {
  let url = v.str("media_url")?;
  match v.str("media_type").as_deref() {
    Some("VIDEO") => Some(Media::video(url)),
    Some("IMAGE") => {
      let mut media = Media::image(url);
      media.alt = v.str("alt_text");
      Some(media)
    }
    _ => None,
  }
}

fn media(v: &Value) -> Vec<Media> {
  let children = v.list("children.data");
  if children.is_empty() {
    return medium(v).into_iter().collect();
  }
  children.iter().filter_map(medium).collect()
}

pub fn post(v: &Value) -> Post {
  let is_reply = v.bool("is_reply") == Some(true);
  let mut post = Post {
    id: v.str("id").unwrap_or_default(),
    kind: if is_reply { "reply" } else { "thread" }.into(),
    text: v.str("text"),
    url: v.str("permalink"),
    author: author(v),
    created_at: v.str("timestamp").and_then(|t| time::parse(&t)),
    media: media(v),
    tags: v.str("topic_tag").into_iter().collect(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  let mut put = |key: &str, value: Value| {
    if !value.is_null() {
      post.extra.insert(key.into(), value);
    }
  };
  put("shortcode", json!(v.str("shortcode")));
  put("media_type", json!(v.str("media_type")));
  put("link", json!(v.str("link_attachment_url")));
  put("quoted_post", json!(v.str("quoted_post.id")));
  put("reposted_post", json!(v.str("reposted_post.id")));
  put("replied_to", json!(v.str("replied_to.id")));
  put("root_post", json!(v.str("root_post.id")));
  put(
    "hidden",
    json!(v.str("hide_status").filter(|s| s != "NOT_HUSHED")),
  );
  if v.bool("has_replies") == Some(true) {
    put("has_replies", json!(true));
  }
  post
}

pub fn comment(v: &Value) -> Comment {
  let post = post(v);
  let mut extra = post.extra;
  if let Some(url) = &post.url {
    extra.insert("url".into(), json!(url));
  }
  Comment {
    id: post.id,
    author: post.author,
    text: post.text.unwrap_or_default(),
    created_at: post.created_at,
    reply_to: v.str("replied_to.id"),
    extra,
    raw: post.raw,
    ..Comment::default()
  }
}

/// A post that mentions you.
pub fn mention(v: &Value) -> Notification {
  let post = post(v);
  Notification {
    id: post.id,
    kind: "mention".into(),
    text: post.text.unwrap_or_default(),
    actor: post.author,
    target: None,
    url: post.url,
    created_at: post.created_at,
    unread: None,
    raw: post.raw,
  }
}

/// Your profile (`/me`).
pub fn me(v: &Value) -> User {
  let name = v.str("username").unwrap_or_default();
  User {
    id: v.str("id").unwrap_or_default(),
    name: v
      .str("name")
      .filter(|n| !n.is_empty())
      .unwrap_or_else(|| name.clone()),
    handle: Some(format!("@{name}")),
    url: Some(refs::profile_url(&name)),
    avatar: v.str("threads_profile_picture_url"),
    bio: v.str("threads_biography").filter(|b| !b.is_empty()),
    verified: v.bool("is_verified") == Some(true),
    raw: Some(v.clone()),
    ..User::default()
  }
}

/// A public profile (`/profile_lookup`), with its counters.
pub fn profile(v: &Value) -> User {
  let name = v.str("username").unwrap_or_default();
  let mut user = User {
    id: v.str("id").unwrap_or_else(|| name.clone()),
    name: v
      .str("name")
      .filter(|n| !n.is_empty())
      .unwrap_or_else(|| name.clone()),
    handle: Some(format!("@{name}")),
    url: Some(refs::profile_url(&name)),
    avatar: v.str("profile_picture_url"),
    bio: v.str("biography").filter(|b| !b.is_empty()),
    verified: v.bool("is_verified") == Some(true),
    stats: UserStats {
      followers: v.u64("follower_count"),
      likes: v.u64("likes_count"),
      ..UserStats::default()
    },
    raw: Some(v.clone()),
    ..User::default()
  };
  for (key, field) in [
    ("views", "views_count"),
    ("replies", "replies_count"),
    ("reposts", "reposts_count"),
    ("quotes", "quotes_count"),
  ] {
    if let Some(n) = v.u64(field) {
      user.stats.other.insert(key.into(), n);
    }
  }
  user
}
