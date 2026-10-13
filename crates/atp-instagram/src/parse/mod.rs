//! Instagram objects into the shared model. Two shapes arrive: the v1 API's
//! (`pk`, `code`, `image_versions2`, `video_versions`, `carousel_media`) and
//! the GraphQL / web-info one (`shortcode`, `display_url`, `edge_…`).

pub mod messages;
mod posts;

use atp_core::text::from_secs;
use atp_core::{Collection, Comment, Notification, User, Value, ValueExt};

pub use self::posts::{items, post, sections};
use crate::api::WEB;
use crate::refs;

pub fn user(v: &Value) -> User {
  let username = v.str("username");
  let mut out = User {
    id: v.first_str(&["pk", "pk_id", "id"]).unwrap_or_default(),
    name: v
      .str("full_name")
      .filter(|n| !n.is_empty())
      .or_else(|| username.clone())
      .unwrap_or_default(),
    url: username.as_ref().map(|n| format!("{WEB}/{n}/")),
    handle: username,
    avatar: v.first_str(&[
      "hd_profile_pic_url_info.url",
      "profile_pic_url_hd",
      "profile_pic_url",
    ]),
    bio: v.str("biography").filter(|b| !b.is_empty()),
    verified: v.bool("is_verified").unwrap_or(false),
    raw: Some(v.clone()),
    ..User::default()
  };
  out.stats.followers = v.first_count(&["follower_count", "edge_followed_by.count"]);
  out.stats.following = v.first_count(&["following_count", "edge_follow.count"]);
  out.stats.posts = v.first_count(&["media_count", "edge_owner_to_timeline_media.count"]);
  for (key, path) in [
    ("private", "is_private"),
    ("business", "is_business_account"),
    ("category", "category_name"),
    ("external_url", "external_url"),
  ] {
    if let Some(x) = v.get(path).filter(|x| !x.is_null() && *x != "") {
      out.extra.insert(key.into(), x.clone());
    }
  }
  if let Some(f) = v
    .bool("followed_by_viewer")
    .or_else(|| v.bool("friendship_status.following"))
  {
    out.followed = Some(f);
  }
  out
}

/// The post author of either shape (`user` or `owner`).
fn owner(v: &Value) -> Option<User> {
  let owner = if v.at("user").is_object() {
    v.at("user")
  } else {
    v.at("owner")
  };
  owner.is_object().then(|| user(owner))
}

/// A comment in either shape, with the replies it ships.
pub fn comment(v: &Value) -> Comment {
  Comment {
    id: v.first_str(&["pk", "id"]).unwrap_or_default(),
    author: owner(v),
    text: v.str("text").unwrap_or_default(),
    created_at: v
      .first_str(&["created_at_utc", "created_at"])
      .and_then(|t| t.parse().ok())
      .and_then(from_secs),
    likes: v.first_count(&["comment_like_count", "edge_liked_by.count"]),
    reply_count: v.first_count(&["child_comment_count", "edge_threaded_comments.count"]),
    replies: v
      .list("preview_child_comments")
      .iter()
      .chain(
        v.list("edge_threaded_comments.edges")
          .iter()
          .map(|e| e.at("node")),
      )
      .map(comment)
      .collect(),
    raw: Some(v.clone()),
    ..Comment::default()
  }
}

/// An entry of the activity inbox (`new_stories` are `unread`).
pub fn notification(story: &Value, unread: bool) -> Notification {
  let args = story.at("args");
  let kind = match story.i64("story_type") {
    Some(60) => "like",
    Some(12 | 13 | 66) => "comment",
    Some(101) => "follow",
    Some(102) => "mention",
    _ => "activity",
  };
  Notification {
    id: story
      .str("pk")
      .or_else(|| args.str("tuuid"))
      .unwrap_or_default(),
    kind: kind.into(),
    text: args.str("text").unwrap_or_default(),
    actor: args.str("profile_name").map(|name| User {
      id: args.str("profile_id").unwrap_or_default(),
      url: Some(format!("{WEB}/{name}/")),
      handle: Some(name.clone()),
      name,
      avatar: args.str("profile_image"),
      ..User::default()
    }),
    target: args
      .str("media.0.shortcode")
      .map(|code| refs::post_url(&code)),
    url: args.str("destination"),
    created_at: args.f64("timestamp").map(|t| t as i64).and_then(from_secs),
    unread: Some(unread),
    raw: Some(story.clone()),
  }
}

/// A hashtag of the top search.
pub fn hashtag(v: &Value) -> Collection {
  let name = v.str("name").unwrap_or_default();
  Collection {
    id: name.clone(),
    kind: "hashtag".into(),
    url: Some(format!("{WEB}/explore/tags/{name}/")),
    items: v.count("media_count"),
    name: format!("#{name}"),
    ..Collection::default()
  }
}

/// A place of the top search.
pub fn place(v: &Value) -> Collection {
  let location = v.at("place.location");
  let id = location.str("pk").unwrap_or_default();
  Collection {
    kind: "location".into(),
    name: location.str("name").unwrap_or_default(),
    description: location.str("address").filter(|a| !a.is_empty()),
    url: Some(format!("{WEB}/explore/locations/{id}/")),
    id,
    ..Collection::default()
  }
}

/// A saved-posts collection.
pub fn folder(v: &Value) -> Collection {
  Collection {
    id: v.str("collection_id").unwrap_or_default(),
    kind: "folder".into(),
    name: v.str("collection_name").unwrap_or_default(),
    items: v.count("collection_media_count"),
    ..Collection::default()
  }
}

/// A story highlight of a profile.
pub fn highlight(v: &Value) -> Collection {
  Collection {
    id: v.str("id").unwrap_or_default(),
    kind: "highlight".into(),
    name: v.str("title").unwrap_or_default(),
    items: v.count("media_count"),
    ..Collection::default()
  }
}
