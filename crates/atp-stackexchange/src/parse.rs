//! Stack Exchange payloads → the shared model: questions and answers are
//! posts, answers (listed under a question) and comments are comments,
//! users are users, tags and sites are collections, inbox items are
//! notifications. Titles and comment bodies arrive HTML-escaped and bodies
//! as HTML; both become plain text.

use std::collections::BTreeMap;

use atp_core::text::{from_secs, html_to_text};
use atp_core::{
  Collection, Comment, Extra, Metrics, Notification, Post, User, UserStats, Value, ValueExt, json,
};

/// HTML (or HTML-escaped text) as plain text.
fn plain(v: &Value, path: &str) -> Option<String> {
  v.str(path).map(|html| html_to_text(&html))
}

fn time(v: &Value, path: &str) -> Option<jiff::Timestamp> {
  v.i64(path).and_then(from_secs)
}

/// A score as a like count; negative scores count none (`extra.score` keeps them).
fn likes(v: &Value) -> Option<u64> {
  v.i64("score").map(|score| score.max(0) as u64)
}

pub fn user(v: &Value) -> User {
  let mut user = User {
    id: v
      .u64("user_id")
      .map(|id| id.to_string())
      .unwrap_or_default(),
    name: plain(v, "display_name").unwrap_or_default(),
    url: v.str("link"),
    avatar: v.str("profile_image"),
    bio: plain(v, "about_me").filter(|bio| !bio.is_empty()),
    location: plain(v, "location"),
    created_at: time(v, "creation_date"),
    stats: UserStats {
      other: [("reputation", v.u64("reputation"))]
        .into_iter()
        .filter_map(|(k, n)| Some((k.to_owned(), n?)))
        .collect(),
      ..UserStats::default()
    },
    raw: Some(v.clone()),
    ..User::default()
  };
  let mut put = |key: &str, value: Value| {
    if !value.is_null() {
      user.extra.insert(key.into(), value);
    }
  };
  put("user_type", json!(v.str("user_type")));
  put("account_id", json!(v.u64("account_id")));
  put("accept_rate", json!(v.u64("accept_rate")));
  put(
    "website",
    json!(v.str("website_url").filter(|w| !w.is_empty())),
  );
  put("badges", v.at("badge_counts").clone());
  put("last_seen_at", json!(time(v, "last_access_date")));
  user
}

fn owner(v: &Value) -> Option<User> {
  let owner = v.at("owner");
  (!owner.is_null()).then(|| user(owner))
}

fn put_all(extra: &mut Extra, pairs: Vec<(&str, Value)>) {
  for (key, value) in pairs {
    if !value.is_null() {
      extra.insert(key.into(), value);
    }
  }
}

pub fn question(v: &Value) -> Post {
  let mut post = Post {
    id: v
      .u64("question_id")
      .map(|id| id.to_string())
      .unwrap_or_default(),
    kind: "question".into(),
    title: plain(v, "title"),
    text: plain(v, "body"),
    url: v.str("link"),
    author: owner(v),
    created_at: time(v, "creation_date"),
    updated_at: time(v, "last_edit_date"),
    metrics: Metrics {
      views: v.u64("view_count"),
      likes: likes(v),
      comments: v.u64("answer_count"),
      ..Metrics::default()
    },
    tags: tags(v),
    raw: Some(v.clone()),
    ..Post::default()
  };
  put_all(
    &mut post.extra,
    vec![
      ("score", json!(v.i64("score"))),
      ("answered", json!(v.bool("is_answered"))),
      ("accepted_answer_id", json!(v.u64("accepted_answer_id"))),
      ("comment_count", json!(v.u64("comment_count"))),
      ("bounty", json!(v.u64("bounty_amount"))),
      ("closed_reason", json!(v.str("closed_reason"))),
      ("closed_at", json!(time(v, "closed_date"))),
      ("last_activity_at", json!(time(v, "last_activity_date"))),
    ],
  );
  post
}

fn tags(v: &Value) -> Vec<String> {
  v.list("tags")
    .iter()
    .filter_map(|tag| tag.as_str().map(str::to_owned))
    .collect()
}

/// An answer as a post (its title is the question's).
pub fn answer(v: &Value) -> Post {
  let mut post = Post {
    id: v
      .u64("answer_id")
      .map(|id| id.to_string())
      .unwrap_or_default(),
    kind: "answer".into(),
    title: plain(v, "title"),
    text: plain(v, "body"),
    url: v.str("link"),
    author: owner(v),
    created_at: time(v, "creation_date"),
    updated_at: time(v, "last_edit_date"),
    metrics: Metrics {
      likes: likes(v),
      comments: v.u64("comment_count"),
      ..Metrics::default()
    },
    raw: Some(v.clone()),
    ..Post::default()
  };
  put_all(
    &mut post.extra,
    vec![
      ("score", json!(v.i64("score"))),
      ("accepted", json!(v.bool("is_accepted"))),
      ("question_id", json!(v.u64("question_id"))),
    ],
  );
  post
}

/// A question or answer of a user's posts (`post_type`).
pub fn post(v: &Value) -> Post {
  let kind = v.str("post_type").unwrap_or_else(|| "post".into());
  let mut post = Post {
    id: v
      .u64("post_id")
      .map(|id| id.to_string())
      .unwrap_or_default(),
    kind,
    title: plain(v, "title"),
    text: plain(v, "body"),
    url: v.str("link"),
    author: owner(v),
    created_at: time(v, "creation_date"),
    updated_at: time(v, "last_edit_date"),
    metrics: Metrics {
      likes: likes(v),
      ..Metrics::default()
    },
    raw: Some(v.clone()),
    ..Post::default()
  };
  put_all(&mut post.extra, vec![("score", json!(v.i64("score")))]);
  post
}

/// An answer listed under its question, with its comments as replies.
pub fn answer_comment(v: &Value) -> Comment {
  let replies: Vec<Comment> = v.list("comments").iter().map(comment).collect();
  let mut extra = Extra::new();
  put_all(
    &mut extra,
    vec![
      ("score", json!(v.i64("score"))),
      ("accepted", json!(v.bool("is_accepted"))),
      ("url", json!(v.str("link"))),
    ],
  );
  Comment {
    id: v
      .u64("answer_id")
      .map(|id| id.to_string())
      .unwrap_or_default(),
    author: owner(v),
    text: plain(v, "body").unwrap_or_default(),
    created_at: time(v, "creation_date"),
    likes: likes(v),
    reply_count: v.u64("comment_count").or(Some(replies.len() as u64)),
    replies,
    extra,
    raw: Some(v.clone()),
    ..Comment::default()
  }
}

pub fn comment(v: &Value) -> Comment {
  let mut extra = Extra::new();
  put_all(
    &mut extra,
    vec![
      ("post_id", json!(v.u64("post_id"))),
      ("url", json!(v.str("link"))),
      ("edited", json!(v.bool("edited").filter(|e| *e))),
    ],
  );
  Comment {
    id: v
      .u64("comment_id")
      .map(|id| id.to_string())
      .unwrap_or_default(),
    author: owner(v),
    text: plain(v, "body").unwrap_or_default(),
    created_at: time(v, "creation_date"),
    likes: likes(v),
    reply_to: plain(v, "reply_to_user.display_name"),
    extra,
    raw: Some(v.clone()),
    ..Comment::default()
  }
}

pub fn tag(v: &Value) -> Collection {
  let name = v.str("name").unwrap_or_default();
  let mut extra = Extra::new();
  put_all(
    &mut extra,
    vec![
      ("has_synonyms", json!(v.bool("has_synonyms").filter(|s| *s))),
      (
        "moderator_only",
        json!(v.bool("is_moderator_only").filter(|s| *s)),
      ),
    ],
  );
  Collection {
    id: name.clone(),
    kind: "tag".into(),
    name,
    items: v.u64("count"),
    extra,
    raw: Some(v.clone()),
    ..Collection::default()
  }
}

/// A site of the network; its `id` is what `--site` / `SE_SITE` take.
pub fn site(v: &Value) -> Collection {
  Collection {
    id: v.str("api_site_parameter").unwrap_or_default(),
    kind: v.str("site_type").unwrap_or_else(|| "site".into()),
    name: plain(v, "name").unwrap_or_default(),
    description: plain(v, "audience"),
    url: v.str("site_url"),
    raw: Some(v.clone()),
    ..Collection::default()
  }
}

/// An inbox item; it has no id of its own, so its link stands in.
pub fn notification(v: &Value) -> Notification {
  let kind = v.str("item_type").unwrap_or_default();
  let shared = match kind.as_str() {
    "new_answer" => "answer",
    "comment" => "comment",
    "chat_message" => "message",
    other => other,
  };
  let id = ["comment_id", "answer_id", "question_id"]
    .iter()
    .find_map(|key| v.u64(key))
    .map(|id| id.to_string())
    .or_else(|| v.str("link"))
    .unwrap_or_default();
  Notification {
    id,
    kind: shared.to_owned(),
    text: plain(v, "body").unwrap_or_default(),
    target: plain(v, "title"),
    url: v.str("link"),
    created_at: time(v, "creation_date"),
    unread: v.bool("is_unread"),
    raw: Some(v.clone()),
    ..Notification::default()
  }
}

/// Requests left (`quota_remaining`) and allowed (`quota_max`) today.
pub fn quota(v: &Value) -> BTreeMap<String, u64> {
  [("left", "quota_remaining"), ("max", "quota_max")]
    .into_iter()
    .filter_map(|(name, key)| Some((name.to_owned(), v.u64(key)?)))
    .collect()
}
