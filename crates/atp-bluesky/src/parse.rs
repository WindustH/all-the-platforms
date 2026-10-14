//! Bluesky views → the shared model: post views are posts (a quote embed
//! is the quoted post, a repost in a feed is marked in `extra`),
//! profiles are users, thread replies are comments with their own
//! replies, trending topics are collections.

use std::collections::BTreeMap;

use atp_core::text::parse_time;
use atp_core::{
  Collection, Comment, Media, Metrics, Notification, Post, User, UserStats, Value, ValueExt, json,
};

use crate::refs::{self, AtUri};

pub fn user(v: &Value) -> User {
  let handle = v.str("handle").unwrap_or_default();
  let did = v.str("did").unwrap_or_default();
  let mut user = User {
    name: v
      .str("displayName")
      .filter(|n| !n.trim().is_empty())
      .unwrap_or_else(|| handle.clone()),
    handle: (!handle.is_empty()).then(|| format!("@{handle}")),
    url: Some(refs::profile_url(if handle.is_empty() {
      &did
    } else {
      &handle
    })),
    avatar: v.str("avatar"),
    bio: v.str("description").filter(|d| !d.is_empty()),
    created_at: v.str("createdAt").and_then(|t| parse_time(&t)),
    followed: v
      .at("viewer")
      .is_object()
      .then(|| v.str("viewer.following").is_some()),
    verified: v.str("verification.verifiedStatus").as_deref() == Some("valid"),
    stats: UserStats {
      followers: v.u64("followersCount"),
      following: v.u64("followsCount"),
      posts: v.u64("postsCount"),
      ..UserStats::default()
    },
    id: did,
    raw: Some(v.clone()),
    ..User::default()
  };
  if v.str("viewer.followedBy").is_some() {
    user.extra.insert("follows_you".into(), json!(true));
  }
  user
}

/// A post view (`app.bsky.feed.defs#postView`).
pub fn post(v: &Value) -> Post {
  let mut post = from_record(v, v.at("record"));
  embed(v.at("embed"), &mut post);
  post.metrics = metrics(v);
  for (key, field) in [("liked", "viewer.like"), ("reposted", "viewer.repost")] {
    if let Some(uri) = v.str(field) {
      post.extra.insert(key.into(), json!(uri));
    }
  }
  if v.bool("viewer.bookmarked") == Some(true) {
    post.extra.insert("bookmarked".into(), json!(true));
  }
  post
}

/// What a post view and an embedded record view share: the record, its
/// author and its link.
fn from_record(v: &Value, record: &Value) -> Post {
  let uri = v.str("uri").unwrap_or_default();
  let handle = v.str("author.handle").unwrap_or_default();
  let url = AtUri::parse(&uri).map(|at| {
    let repo = if handle.is_empty() { at.repo } else { handle };
    refs::post_url(&repo, &at.rkey)
  });
  let mut post = Post {
    id: uri,
    kind: if record.at("reply").is_object() {
      "reply".into()
    } else {
      "post".into()
    },
    text: record.str("text"),
    url,
    author: v.at("author").is_object().then(|| user(v.at("author"))),
    created_at: record
      .str("createdAt")
      .or_else(|| v.str("indexedAt"))
      .and_then(|t| parse_time(&t)),
    tags: tags(record),
    raw: Some(v.clone()),
    ..Post::default()
  };
  let mut put = |key: &str, value: Value| {
    if !value.is_null() {
      post.extra.insert(key.into(), value);
    }
  };
  put("cid", json!(v.str("cid")));
  put("reply_to", json!(record.str("reply.parent.uri")));
  if !record.list("langs").is_empty() {
    put("langs", record.at("langs").clone());
  }
  let links = links(record);
  if !links.is_empty() {
    put("links", json!(links));
  }
  post
}

fn metrics(v: &Value) -> Metrics {
  Metrics {
    likes: v.u64("likeCount"),
    comments: v.u64("replyCount"),
    shares: v.u64("repostCount"),
    favorites: v.u64("bookmarkCount"),
    other: v
      .u64("quoteCount")
      .filter(|n| *n > 0)
      .map(|n| BTreeMap::from([("quotes".to_owned(), n)]))
      .unwrap_or_default(),
    ..Metrics::default()
  }
}

/// Hashtags: the record's own tags and the tag facets in its text.
fn tags(record: &Value) -> Vec<String> {
  let mut tags: Vec<String> = record
    .list("tags")
    .iter()
    .filter_map(|t| t.as_str().map(str::to_owned))
    .collect();
  for facet in record.list("facets") {
    for feature in facet.list("features") {
      if let Some(tag) = feature.str("tag") {
        tags.push(tag);
      }
    }
  }
  tags.dedup();
  tags
}

/// Links in the text (link facets).
fn links(record: &Value) -> Vec<String> {
  record
    .list("facets")
    .iter()
    .flat_map(|facet| facet.list("features"))
    .filter(|f| f.str("$type").is_some_and(|t| t.ends_with("#link")))
    .filter_map(|f| f.str("uri"))
    .collect()
}

/// An embed view: images, a video, a link card, a quoted record, or a
/// quote with media.
fn embed(e: &Value, post: &mut Post) {
  let kind = e.str("$type").unwrap_or_default();
  match kind.split('#').next().unwrap_or_default() {
    "app.bsky.embed.images" => {
      for image in e.list("images") {
        let Some(url) = image.str("fullsize") else {
          continue;
        };
        let mut media = Media::image(url);
        media.alt = image.str("alt").filter(|a| !a.is_empty());
        media.width = image.u64("aspectRatio.width").map(|w| w as u32);
        media.height = image.u64("aspectRatio.height").map(|h| h as u32);
        post.media.push(media);
      }
    }
    "app.bsky.embed.video" => {
      if let Some(url) = e.str("playlist") {
        let mut media = Media::video(url);
        media.alt = e.str("alt").filter(|a| !a.is_empty());
        media.width = e.u64("aspectRatio.width").map(|w| w as u32);
        media.height = e.u64("aspectRatio.height").map(|h| h as u32);
        post.media.push(media);
      }
      if let Some(cid) = e.str("cid") {
        post.extra.insert("video_cid".into(), json!(cid));
      }
    }
    "app.bsky.embed.external" => {
      let card = e.at("external");
      post.extra.insert(
        "card".into(),
        json!({"url": card.str("uri"), "title": card.str("title"), "description": card.str("description")}),
      );
    }
    "app.bsky.embed.record" => quote(e.at("record"), post),
    "app.bsky.embed.recordWithMedia" => {
      quote(e.at("record.record"), post);
      embed(e.at("media"), post);
    }
    _ => {}
  }
}

/// The quoted record of an embed (`#viewRecord`); feeds, lists and
/// missing or blocked posts are noted in `extra`.
fn quote(record: &Value, post: &mut Post) {
  let kind = record.str("$type").unwrap_or_default();
  if !kind.ends_with("#viewRecord") {
    let what = record.str("uri").unwrap_or(kind);
    post.extra.insert("embedded".into(), json!(what));
    return;
  }
  let mut quoted = from_record(record, record.at("value"));
  if let Some(inner) = record.list("embeds").first() {
    embed(inner, &mut quoted);
  }
  quoted.metrics = metrics(record);
  post.quoted = Some(Box::new(quoted));
}

/// An item of a feed (`#feedViewPost`): the post, and who reposted it.
pub fn feed_item(v: &Value) -> Option<Post> {
  let mut post = post(v.at("post"));
  if post.id.is_empty() {
    return None;
  }
  if v
    .str("reason.$type")
    .is_some_and(|t| t.ends_with("#reasonRepost"))
  {
    let by = v.str("reason.by.handle").unwrap_or_default();
    post
      .extra
      .insert("reposted_by".into(), json!(format!("@{by}")));
  }
  Some(post)
}

/// A reply in a thread (`#threadViewPost`), with the replies under it.
pub fn comment(v: &Value) -> Option<Comment> {
  let view = v.at("post");
  if !view.is_object() {
    return None;
  }
  let post = post(view);
  let mut extra = post.extra;
  if let Some(url) = &post.url {
    extra.insert("url".into(), json!(url));
  }
  Some(Comment {
    id: post.id,
    author: post.author,
    text: post.text.unwrap_or_default(),
    created_at: post.created_at,
    likes: post.metrics.likes,
    reply_count: post.metrics.comments,
    replies: v.list("replies").iter().filter_map(comment).collect(),
    extra,
    raw: post.raw,
    ..Comment::default()
  })
}

pub fn notification(v: &Value) -> Notification {
  let reason = v.str("reason").unwrap_or_default();
  let subject = v.str("reasonSubject").and_then(|s| AtUri::parse(&s));
  let own = AtUri::parse(&v.str("uri").unwrap_or_default());
  let text = match reason.as_str() {
    "like" | "like-via-repost" => "liked your post".to_owned(),
    "repost" | "repost-via-repost" => "reposted your post".to_owned(),
    "follow" => "followed you".to_owned(),
    "starterpack-joined" => "joined through your starter pack".to_owned(),
    _ => v.str("record.text").unwrap_or_default(),
  };
  let url = match reason.as_str() {
    "follow" => v.str("author.handle").map(|h| refs::profile_url(&h)),
    _ if subject.is_some() && !matches!(reason.as_str(), "reply" | "mention" | "quote") => {
      subject.map(|at| refs::post_url(&at.repo, &at.rkey))
    }
    _ => own
      .filter(|at| at.collection == refs::POST)
      .map(|at| refs::post_url(&at.repo, &at.rkey)),
  };
  let kind = match reason.as_str() {
    "like-via-repost" => "like",
    "repost-via-repost" => "repost",
    other => other,
  };
  Notification {
    id: v.str("uri").unwrap_or_default(),
    kind: kind.to_owned(),
    text,
    actor: v.at("author").is_object().then(|| user(v.at("author"))),
    target: None,
    url,
    created_at: v.str("indexedAt").and_then(|t| parse_time(&t)),
    unread: v.bool("isRead").map(|read| !read),
    raw: Some(v.clone()),
  }
}

/// A trending topic; its link is a feed of the topic's posts.
pub fn trend(v: &Value) -> Collection {
  let link = v.str("link").unwrap_or_default();
  let url = format!("https://bsky.app{link}");
  let feed = refs::feed(&url).ok().map(|at| at.uri());
  let topic = v.str("topic").unwrap_or_default();
  Collection {
    id: feed.unwrap_or_else(|| topic.clone()),
    kind: "trend".into(),
    name: v.str("displayName").unwrap_or(topic),
    description: v.str("description"),
    url: (!link.is_empty()).then_some(url),
    raw: Some(v.clone()),
    ..Collection::default()
  }
}
