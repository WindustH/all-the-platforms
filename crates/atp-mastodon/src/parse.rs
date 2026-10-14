//! Mastodon payloads → the shared model: statuses are posts (a boost
//! quotes the boosted status, a quote post its quoted one), accounts are
//! users, hashtags are collections. Links point at the instance in use
//! (`https://<instance>/@user@host/<id>`), so they work as arguments
//! without looking anything up.

use std::collections::BTreeMap;

use atp_core::text::{html_to_text, parse_time};
use atp_core::{
  Collection, Comment, Extra, Media, MediaKind, Metrics, Notification, Post, User, UserStats,
  Value, ValueExt, json,
};

pub fn status_url(instance: &str, acct: &str, id: &str) -> String {
  format!("https://{instance}/@{acct}/{id}")
}

pub fn account_url(instance: &str, acct: &str) -> String {
  format!("https://{instance}/@{acct}")
}

pub fn user(v: &Value, instance: &str) -> User {
  let acct = v.str("acct").unwrap_or_default();
  let mut user = User {
    id: v.str("id").unwrap_or_default(),
    name: v
      .str("display_name")
      .filter(|n| !n.is_empty())
      .or_else(|| v.str("username"))
      .unwrap_or_default(),
    handle: (!acct.is_empty()).then(|| format!("@{acct}")),
    url: (!acct.is_empty()).then(|| account_url(instance, &acct)),
    avatar: v.str("avatar"),
    bio: v
      .str("note")
      .map(|note| html_to_text(&note))
      .filter(|b| !b.is_empty()),
    created_at: v.str("created_at").and_then(|t| parse_time(&t)),
    stats: UserStats {
      followers: v.u64("followers_count"),
      following: v.u64("following_count"),
      posts: v.u64("statuses_count"),
      ..UserStats::default()
    },
    raw: Some(v.clone()),
    ..User::default()
  };
  for key in ["bot", "locked", "group"] {
    if v.bool(key) == Some(true) {
      user.extra.insert(key.into(), json!(true));
    }
  }
  if let Some(home) = v.str("url").filter(|u| !u.contains(instance)) {
    user.extra.insert("home_url".into(), json!(home));
  }
  let fields: Vec<Value> = v
    .list("fields")
    .iter()
    .map(|f| json!({"name": f.str("name"), "value": f.str("value").map(|t| html_to_text(&t))}))
    .collect();
  if !fields.is_empty() {
    user.extra.insert("fields".into(), json!(fields));
  }
  user
}

fn media(v: &Value) -> Vec<Media> {
  v.list("media_attachments")
    .iter()
    .filter_map(|m| {
      let kind = match m.str("type").as_deref() {
        Some("image") => MediaKind::Image,
        Some("gifv") => MediaKind::Gif,
        Some("video") => MediaKind::Video,
        Some("audio") => MediaKind::Audio,
        _ => MediaKind::File,
      };
      let url = m.str("url").or_else(|| m.str("remote_url"))?;
      let mut media = Media::new(kind, url);
      media.width = m.u64("meta.original.width").map(|w| w as u32);
      media.height = m.u64("meta.original.height").map(|h| h as u32);
      media.duration = m.f64("meta.original.duration");
      media.alt = m.str("description");
      Some(media)
    })
    .collect()
}

/// A status as a post; a boost quotes the boosted status.
pub fn post(v: &Value, instance: &str) -> Post {
  let id = v.str("id").unwrap_or_default();
  let boosted = v.at("reblog");
  if boosted.is_object() {
    let original = post(boosted, instance);
    return Post {
      id,
      kind: "boost".into(),
      url: original.url.clone(),
      author: Some(user(v.at("account"), instance)),
      created_at: v.str("created_at").and_then(|t| parse_time(&t)),
      quoted: Some(Box::new(original)),
      raw: Some(v.clone()),
      ..Post::default()
    };
  }
  let acct = v.str("account.acct").unwrap_or_default();
  let quoted = v.at("quote.quoted_status");
  let mut post = Post {
    url: Some(status_url(instance, &acct, &id)),
    kind: if v.str("in_reply_to_id").is_some() {
      "reply".into()
    } else {
      "status".into()
    },
    id,
    text: v.str("content").map(|html| html_to_text(&html)),
    author: Some(user(v.at("account"), instance)),
    created_at: v.str("created_at").and_then(|t| parse_time(&t)),
    updated_at: v.str("edited_at").and_then(|t| parse_time(&t)),
    metrics: metrics(v),
    media: media(v),
    tags: v
      .list("tags")
      .iter()
      .filter_map(|t| t.str("name"))
      .collect(),
    quoted: quoted.is_object().then(|| Box::new(post(quoted, instance))),
    raw: Some(v.clone()),
    ..Post::default()
  };
  status_extra(&mut post.extra, v);
  post
}

fn metrics(v: &Value) -> Metrics {
  Metrics {
    likes: v.u64("favourites_count"),
    comments: v.u64("replies_count"),
    shares: v.u64("reblogs_count"),
    other: v
      .u64("quotes_count")
      .filter(|n| *n > 0)
      .map(|n| BTreeMap::from([("quotes".to_owned(), n)]))
      .unwrap_or_default(),
    ..Metrics::default()
  }
}

/// Content warning, visibility, the status it answers, its home link,
/// link card, poll and your own marks.
fn status_extra(extra: &mut Extra, v: &Value) {
  let mut put = |key: &str, value: Value| {
    if !value.is_null() && value != json!("") {
      extra.insert(key.into(), value);
    }
  };
  put("cw", json!(v.str("spoiler_text")));
  put("visibility", json!(v.str("visibility")));
  put("language", json!(v.str("language")));
  put("in_reply_to_id", json!(v.str("in_reply_to_id")));
  put("home_url", json!(v.str("url").or_else(|| v.str("uri"))));
  if v.at("card").is_object() {
    let card = v.at("card");
    put(
      "card",
      json!({"url": card.str("url"), "title": card.str("title"), "description": card.str("description")}),
    );
  }
  if v.at("poll").is_object() {
    let options: Vec<Value> = v
      .list("poll.options")
      .iter()
      .map(|o| json!({"title": o.str("title"), "votes": o.u64("votes_count")}))
      .collect();
    put(
      "poll",
      json!({"options": options, "expires_at": v.str("poll.expires_at")}),
    );
  }
  for (key, field) in [
    ("liked", "favourited"),
    ("boosted", "reblogged"),
    ("bookmarked", "bookmarked"),
  ] {
    if v.bool(field) == Some(true) {
      put(key, json!(true));
    }
  }
}

/// A reply in a thread as a comment.
pub fn comment(v: &Value, instance: &str) -> Comment {
  let post = post(v, instance);
  let mut extra = post.extra;
  if let Some(url) = &post.url {
    extra.insert("url".into(), json!(url));
  }
  Comment {
    id: post.id,
    author: post.author,
    text: post.text.unwrap_or_default(),
    created_at: post.created_at,
    likes: post.metrics.likes,
    reply_count: post.metrics.comments,
    reply_to: v.str("in_reply_to_id"),
    extra,
    raw: post.raw,
    ..Comment::default()
  }
}

pub fn notification(v: &Value, instance: &str) -> Notification {
  let kind = v.str("type").unwrap_or_default();
  let status = v.at("status");
  let status_text = status.str("content").map(|html| html_to_text(&html));
  let text = match kind.as_str() {
    "follow" => "followed you".to_owned(),
    "follow_request" => "asked to follow you".to_owned(),
    "favourite" => "liked your post".to_owned(),
    "reblog" => "boosted your post".to_owned(),
    "poll" => "a poll ended".to_owned(),
    "update" => "edited a post you interacted with".to_owned(),
    _ => status_text.clone().unwrap_or_default(),
  };
  let shared = match kind.as_str() {
    "favourite" => "like",
    "reblog" => "repost",
    "status" => "post",
    other => other,
  };
  let url = status.str("id").map(|id| {
    let acct = status.str("account.acct").unwrap_or_default();
    status_url(instance, &acct, &id)
  });
  Notification {
    id: v.str("id").unwrap_or_default(),
    kind: shared.to_owned(),
    text,
    actor: v
      .at("account")
      .is_object()
      .then(|| user(v.at("account"), instance)),
    target: status_text.filter(|_| matches!(kind.as_str(), "favourite" | "reblog" | "poll")),
    url,
    created_at: v.str("created_at").and_then(|t| parse_time(&t)),
    unread: None,
    raw: Some(v.clone()),
  }
}

/// A hashtag; trending ones carry their use over the last days.
pub fn tag(v: &Value, instance: &str) -> Collection {
  let name = v.str("name").unwrap_or_default();
  let history = v.list("history");
  let sum = |key: &str| -> Option<u64> {
    let values: Vec<u64> = history.iter().filter_map(|day| day.count(key)).collect();
    (!values.is_empty()).then(|| values.iter().sum())
  };
  let mut tag = Collection {
    id: name.clone(),
    kind: "hashtag".into(),
    name: format!("#{name}"),
    url: Some(format!("https://{instance}/tags/{name}")),
    items: sum("uses"),
    followers: sum("accounts"),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if v.bool("following") == Some(true) {
    tag.extra.insert("following".into(), json!(true));
  }
  tag
}

/// A trending link as a post.
pub fn link(v: &Value, _instance: &str) -> Post {
  let url = v.str("url").unwrap_or_default();
  let mut post = Post {
    id: url.clone(),
    kind: "link".into(),
    title: v.str("title"),
    text: v.str("description"),
    url: Some(url),
    media: v.str("image").map(Media::image).into_iter().collect(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  if let Some(provider) = v.str("provider_name").filter(|p| !p.is_empty()) {
    post.extra.insert("provider".into(), json!(provider));
  }
  post
}
