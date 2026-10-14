//! Discourse payloads → the shared model: topics are posts (their first
//! post's text when the topic itself was read), the posts after the first
//! are comments, search hits and user activity are posts pointing at one
//! post of a topic, categories are collections.

use std::collections::HashMap;

use atp_core::text::{html_to_text, parse_time};
use atp_core::{
  Collection, Comment, Metrics, Notification, Post, User, UserStats, Value, ValueExt, json,
};

/// Links and avatars need the forum's base URL.
pub struct Forum<'a> {
  pub base: &'a str,
}

impl Forum<'_> {
  fn link(&self, path: &str) -> String {
    if path.starts_with("http") {
      path.to_owned()
    } else {
      format!("{}{path}", self.base)
    }
  }

  pub fn topic_url(&self, slug: &str, id: u64) -> String {
    self.link(&format!("/t/{slug}/{id}"))
  }

  fn avatar(&self, template: Option<String>) -> Option<String> {
    let template = template?.replace("{size}", "144");
    Some(if template.starts_with("//") {
      format!("https:{template}")
    } else {
      self.link(&template)
    })
  }

  /// The few fields of a user every listing carries.
  pub fn user_brief(&self, v: &Value) -> Option<User> {
    let username = v.str("username")?;
    Some(User {
      id: v.str("user_id").or_else(|| v.str("id")).unwrap_or_default(),
      name: v
        .str("name")
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| username.clone()),
      handle: Some(format!("@{username}")),
      url: Some(self.link(&format!("/u/{username}"))),
      avatar: self.avatar(v.str("avatar_template")),
      ..User::default()
    })
  }

  /// A full profile (`/u/<name>.json`), with its summary counters.
  pub fn user(&self, v: &Value, summary: &Value) -> User {
    let mut user = self.user_brief(v).unwrap_or_default();
    user.id = v.str("id").unwrap_or_default();
    user.bio = v
      .str("bio_cooked")
      .map(|html| html_to_text(&html))
      .or_else(|| v.str("bio_raw"))
      .filter(|b| !b.is_empty());
    user.location = v.str("location").filter(|l| !l.is_empty());
    user.created_at = v.str("created_at").and_then(|t| parse_time(&t));
    user.stats = UserStats {
      posts: summary.u64("post_count"),
      likes: summary.u64("likes_received"),
      ..UserStats::default()
    };
    for (key, field) in [
      ("topics", "topic_count"),
      ("likes_given", "likes_given"),
      ("days_visited", "days_visited"),
    ] {
      if let Some(n) = summary.u64(field) {
        user.stats.other.insert(key.into(), n);
      }
    }
    for key in [
      "trust_level",
      "title",
      "last_posted_at",
      "last_seen_at",
      "profile_view_count",
    ] {
      if !v.at(key).is_null() {
        user.extra.insert(key.into(), v.at(key).clone());
      }
    }
    user.raw = Some(v.clone());
    user
  }

  /// A topic of a listing; `users` maps user ids to the listing's users.
  pub fn topic(&self, v: &Value, users: &HashMap<String, Value>) -> Post {
    let id = v.u64("id").unwrap_or(0);
    let slug = v.str("slug").unwrap_or_else(|| "topic".into());
    let op = v
      .str("posters.0.user_id")
      .and_then(|uid| users.get(&uid))
      .and_then(|u| self.user_brief(u));
    let mut post = Post {
      id: id.to_string(),
      kind: "topic".into(),
      title: v.str("title"),
      text: v.str("excerpt").map(|html| html_to_text(&html)),
      url: Some(self.topic_url(&slug, id)),
      author: op,
      created_at: v.str("created_at").and_then(|t| parse_time(&t)),
      metrics: Metrics {
        views: v.u64("views"),
        likes: v.u64("like_count"),
        comments: v.u64("posts_count").map(|n| n.saturating_sub(1)),
        ..Metrics::default()
      },
      tags: tags(v),
      raw: Some(v.clone()),
      ..Post::default()
    };
    topic_extra(&mut post, v);
    post
  }

  /// A topic read whole (`/t/<id>.json`): its first post is the text.
  pub fn topic_page(&self, v: &Value) -> Post {
    let mut post = self.topic(v, &HashMap::new());
    let first = v
      .list("post_stream.posts")
      .iter()
      .find(|p| p.u64("post_number") == Some(1));
    if let Some(first) = first {
      post.text = first.str("cooked").map(|html| html_to_text(&html));
      post.author = self.user_brief(first);
      post.updated_at = first.str("updated_at").and_then(|t| parse_time(&t));
    }
    post
  }

  /// A post of a topic as a comment.
  pub fn comment(&self, v: &Value) -> Comment {
    let mut extra = atp_core::Extra::new();
    extra.insert("post_number".into(), json!(v.u64("post_number")));
    if let Some(url) = v.str("post_url") {
      extra.insert("url".into(), json!(self.link(&url)));
    }
    if let Some(n) = v.u64("reply_to_post_number") {
      extra.insert("reply_to_post_number".into(), json!(n));
    }
    Comment {
      id: v.str("id").unwrap_or_default(),
      author: self.user_brief(v),
      text: v
        .str("cooked")
        .map(|html| html_to_text(&html))
        .unwrap_or_default(),
      created_at: v.str("created_at").and_then(|t| parse_time(&t)),
      likes: likes(v),
      reply_count: v.u64("reply_count"),
      reply_to: v.str("reply_to_user.username").map(|u| format!("@{u}")),
      extra,
      raw: Some(v.clone()),
      ..Comment::default()
    }
  }

  /// One post (`/posts/<id>.json`, a search hit, an activity row) as a
  /// post that points at it inside its topic.
  pub fn post(&self, v: &Value, title: Option<String>) -> Post {
    let topic = v.u64("topic_id").unwrap_or(0);
    let number = v.u64("post_number").unwrap_or(1);
    let slug = v
      .str("topic_slug")
      .or_else(|| v.str("slug"))
      .unwrap_or_else(|| "topic".into());
    let text = v
      .str("cooked")
      .or_else(|| v.str("blurb"))
      .or_else(|| v.str("excerpt"))
      .map(|html| html_to_text(&html));
    let mut post = Post {
      id: v.str("post_id").or_else(|| v.str("id")).unwrap_or_default(),
      kind: if number > 1 { "reply" } else { "topic" }.into(),
      title: title
        .or_else(|| v.str("title"))
        .or_else(|| v.str("topic_title")),
      text,
      url: Some(format!("{}/{number}", self.topic_url(&slug, topic))),
      author: self.user_brief(v),
      created_at: v.str("created_at").and_then(|t| parse_time(&t)),
      metrics: Metrics {
        likes: likes(v).or_else(|| v.u64("like_count")),
        ..Metrics::default()
      },
      raw: Some(v.clone()),
      ..Post::default()
    };
    post.extra.insert("topic_id".into(), json!(topic));
    post.extra.insert("post_number".into(), json!(number));
    post
  }

  pub fn category(&self, v: &Value) -> Collection {
    let id = v.u64("id").unwrap_or(0);
    let slug = v.str("slug").unwrap_or_default();
    let mut category = Collection {
      id: id.to_string(),
      kind: "category".into(),
      name: v.str("name").unwrap_or_default(),
      description: v.str("description_text").filter(|d| !d.is_empty()),
      url: Some(self.link(&format!("/c/{slug}/{id}"))),
      items: v.u64("topic_count"),
      raw: Some(v.clone()),
      ..Collection::default()
    };
    for key in ["post_count", "parent_category_id", "read_restricted"] {
      if !v.at(key).is_null() {
        category.extra.insert(key.into(), v.at(key).clone());
      }
    }
    category
  }

  pub fn notification(&self, v: &Value) -> Notification {
    let kind = notification_kind(v.u64("notification_type").unwrap_or(0));
    let data = v.at("data");
    let actor = data
      .str("display_username")
      .or_else(|| data.str("original_username"))
      .map(|name| User {
        name: name.clone(),
        handle: Some(format!("@{name}")),
        url: Some(self.link(&format!("/u/{name}"))),
        ..User::default()
      });
    let url = match (v.u64("topic_id"), v.str("slug")) {
      (Some(topic), Some(slug)) => {
        let number = v.u64("post_number").unwrap_or(1);
        Some(format!("{}/{number}", self.topic_url(&slug, topic)))
      }
      _ => None,
    };
    Notification {
      id: v.str("id").unwrap_or_default(),
      kind,
      text: v
        .str("fancy_title")
        .or_else(|| data.str("topic_title"))
        .or_else(|| data.str("badge_name"))
        .unwrap_or_default(),
      actor,
      target: data.str("topic_title"),
      url,
      created_at: v.str("created_at").and_then(|t| parse_time(&t)),
      unread: v.bool("read").map(|read| !read),
      raw: Some(v.clone()),
    }
  }
}

/// Likes of a post: the like action's count, or everyone who reacted.
fn likes(v: &Value) -> Option<u64> {
  v.list("actions_summary")
    .iter()
    .find(|a| a.u64("id") == Some(2))
    .and_then(|a| a.u64("count"))
    .or_else(|| v.u64("reaction_users_count"))
}

/// Tags as names (newer forums send `{id, name, slug}` objects).
fn tags(v: &Value) -> Vec<String> {
  v.list("tags")
    .iter()
    .filter_map(|t| t.as_str().map(str::to_owned).or_else(|| t.str("name")))
    .collect()
}

fn topic_extra(post: &mut Post, v: &Value) {
  for key in [
    "category_id",
    "pinned",
    "closed",
    "archived",
    "last_posted_at",
    "last_poster_username",
    "has_accepted_answer",
  ] {
    let value = v.at(key);
    if !value.is_null() && value != &json!(false) {
      post.extra.insert(key.into(), value.clone());
    }
  }
}

/// The names of Discourse's notification types.
pub fn notification_kind(code: u64) -> String {
  let name = match code {
    1 => "mention",
    2 => "reply",
    3 => "quote",
    4 => "edited",
    5 => "like",
    6 => "message",
    7 => "invited_to_message",
    8 => "invitee_accepted",
    9 => "post",
    10 => "moved_post",
    11 => "linked",
    12 => "badge",
    13 => "invited_to_topic",
    14 => "custom",
    15 => "group_mention",
    16 => "group_message_summary",
    17 => "watching_first_post",
    18 => "topic_reminder",
    19 => "like",
    20 => "post_approved",
    24 => "bookmark_reminder",
    25 => "reaction",
    _ => return format!("type_{code}"),
  };
  name.to_owned()
}

/// The users of a listing by id.
pub fn users_by_id(v: &Value) -> HashMap<String, Value> {
  v.list("users")
    .iter()
    .filter_map(|u| Some((u.str("id")?, u.clone())))
    .collect()
}
