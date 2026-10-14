//! Jike payloads → posts, users, comments, topics and notifications.
//!
//! A post (`ORIGINAL_POST`, kind `post`) or repost (`REPOST`, kind
//! `repost`, with the reposted post in `quoted`) links to its web page,
//! which other commands take back. Users are referred to by their
//! `username`, the id Jike's links and API use. Pictures point at the
//! original file instead of the thumbnail the API hands out.

use atp_core::text::parse_time;
use atp_core::{
  Collection, Comment, Extra, Media, Metrics, Notification, Post, User, UserStats, Value, ValueExt,
  json,
};
use jiff::Timestamp;

use crate::refs::{Kind, post_url, topic_url, user_url};

fn time(v: &Value, path: &str) -> Option<Timestamp> {
  v.str(path).and_then(|t| parse_time(&t))
}

fn put(extra: &mut Extra, pairs: Vec<(&str, Value)>) {
  for (key, value) in pairs {
    if !value.is_null() {
      extra.insert(key.into(), value);
    }
  }
}

/// The original file behind a picture URL with processing parameters
/// (`…/key.jpg?imageMogr2/…/thumbnail/…` → `…/key.jpg`). Extensionless
/// keys and HEIC originals keep the conversion to JPEG and drop only the
/// thumbnail size.
pub fn full_picture(url: &str) -> String {
  let Some((path, query)) = url.split_once('?') else {
    return url.to_owned();
  };
  let last = path.rsplit('/').next().unwrap_or_default();
  let ext = last.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
  let convert = matches!(ext.as_deref(), None | Some("heic" | "heif"));
  if !convert || !query.starts_with("imageMogr2/") {
    return path.to_owned();
  }
  match query.find("/thumbnail/") {
    Some(at) => format!("{path}?{}", &query[..at]),
    None => url.to_owned(),
  }
}

pub fn user(v: &Value) -> User {
  let username = v.str("username").unwrap_or_default();
  let counts = v.at("statsCount");
  let mut user = User {
    name: v.str("screenName").unwrap_or_else(|| username.clone()),
    url: (!username.is_empty()).then(|| user_url(&username)),
    avatar: v
      .first_str(&["avatarImage.picUrl", "profileImageUrl"])
      .map(|u| full_picture(&u)),
    bio: v.first_str(&["briefIntro", "bio"]),
    verified: v.bool("isVerified") == Some(true),
    stats: UserStats {
      followers: counts.u64("followedCount"),
      following: counts.u64("followingCount"),
      likes: counts.u64("liked"),
      ..UserStats::default()
    },
    followed: v.bool("following"),
    created_at: time(v, "createdAt"),
    raw: Some(v.clone()),
    id: username,
    ..User::default()
  };
  put(
    &mut user.extra,
    vec![
      ("user_id", json!(v.str("id"))),
      ("verify_message", json!(v.str("verifyMessage"))),
      ("topics_created", json!(counts.u64("topicCreated"))),
    ],
  );
  user
}

fn pictures(v: &Value) -> Vec<Media> {
  v.list("pictures")
    .iter()
    .filter_map(|p| {
      let mut media = Media::image(full_picture(&p.str("picUrl")?));
      media.width = p.u64("width").map(|w| w as u32);
      media.height = p.u64("height").map(|h| h as u32);
      Some(media)
    })
    .collect()
}

/// A post, repost or personal update.
pub fn post(v: &Value) -> Post {
  let kind_name = v.str("type").unwrap_or_default();
  let kind = Kind::from_type(&kind_name).unwrap_or(Kind::Post);
  let id = v.str("id").unwrap_or_default();
  let author = v.get("user").filter(|u| u.is_object()).map(user);
  let url = author
    .as_ref()
    .filter(|_| kind_name != "PERSONAL_UPDATE")
    .map(|a| post_url(&a.id, kind, &id));
  let mut post = Post {
    kind: match kind_name.as_str() {
      "REPOST" => "repost",
      "PERSONAL_UPDATE" => "update",
      _ => "post",
    }
    .into(),
    text: v.str("content").or_else(|| update_text(v)),
    url,
    author,
    created_at: time(v, "createdAt").or_else(|| time(v, "actionTime")),
    metrics: Metrics {
      likes: v.u64("likeCount"),
      comments: v.u64("commentCount"),
      shares: v.u64("repostCount"),
      ..Metrics::default()
    },
    media: pictures(v),
    tags: v.str("topic.content").into_iter().collect(),
    quoted: v
      .get("target")
      .filter(|t| t.is_object())
      .map(|t| Box::new(post(t))),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  post
    .metrics
    .other
    .extend(v.u64("shareCount").map(|n| ("share_count".to_owned(), n)));
  post_extra(&mut post.extra, v);
  post
}

fn post_extra(extra: &mut Extra, v: &Value) {
  let topic = v
    .str("topic.id")
    .map(|id| json!({"id": id, "name": v.str("topic.content"), "url": topic_url(&id)}));
  let link = v
    .str("linkInfo.linkUrl")
    .map(|url| json!({"url": url, "title": v.str("linkInfo.title")}));
  let video = v
    .get("video")
    .filter(|video| video.is_object())
    .map(|video| json!({"duration": video.u64("duration").map(|ms| ms / 1000), "cover": video.str("image.picUrl")}));
  put(
    extra,
    vec![
      ("topic", json!(topic)),
      ("link", json!(link)),
      ("video", json!(video)),
      ("liked", json!(v.bool("liked"))),
      ("collected", json!(v.bool("collected"))),
    ],
  );
}

/// What a personal update (`USER_FOLLOW` …) says, in words.
fn update_text(v: &Value) -> Option<String> {
  let action = v.str("action")?;
  let names = |path: &str| {
    v.list(path)
      .iter()
      .filter_map(|u| u.str("screenName"))
      .collect::<Vec<_>>()
      .join(", ")
  };
  let (who, whom) = (names("users"), names("targetUsers"));
  Some(match action.as_str() {
    "USER_FOLLOW" => format!("{who} followed {whom}"),
    other => format!("{who}: {} {whom}", other.to_lowercase())
      .trim()
      .to_owned(),
  })
}

pub fn comment(v: &Value) -> Comment {
  let author = v.get("user").filter(|u| u.is_object()).map(user);
  let mut comment = Comment {
    id: v.str("id").unwrap_or_default(),
    text: v.str("content").unwrap_or_default(),
    created_at: time(v, "createdAt"),
    likes: v.u64("likeCount"),
    reply_count: v.u64("replyCount"),
    reply_to: v.str("replyToComment.user.screenName"),
    replies: v.list("hotReplies").iter().map(comment).collect(),
    author,
    raw: Some(v.clone()),
    ..Comment::default()
  };
  let pictures: Vec<String> = pictures(v).into_iter().map(|m| m.url).collect();
  put(
    &mut comment.extra,
    vec![
      ("level", json!(v.u64("level"))),
      ("thread_id", json!(v.str("threadId"))),
      ("pinned", json!(v.bool("isPinned").filter(|p| *p))),
      (
        "pictures",
        if pictures.is_empty() {
          Value::Null
        } else {
          json!(pictures)
        },
      ),
    ],
  );
  comment
}

/// A topic (圈子).
pub fn topic(v: &Value) -> Collection {
  let id = v.str("id").unwrap_or_default();
  let mut topic = Collection {
    kind: "topic".into(),
    name: v.str("content").unwrap_or_default(),
    description: v.str("briefIntro"),
    url: (!id.is_empty()).then(|| topic_url(&id)),
    followers: v.u64("subscribersCount"),
    raw: Some(v.clone()),
    id,
    ..Collection::default()
  };
  put(
    &mut topic.extra,
    vec![
      ("image", json!(v.str("squarePicture.picUrl"))),
      ("subscribers", json!(v.str("subscribersDescription"))),
    ],
  );
  topic
}

/// A notification: likes, comments, replies, mentions, follows …
pub fn notification(v: &Value) -> Notification {
  let kind = v.str("type").unwrap_or_default();
  let item = v.at("actionItem");
  let actor = item.list("users").first().map(user);
  let others = item.u64("usersCount").unwrap_or(1).saturating_sub(1);
  let who = actor.as_ref().map(|a| a.name.clone()).unwrap_or_default();
  let who = if others > 0 {
    format!("{who} +{others}")
  } else {
    who
  };
  let what = item
    .str("content")
    .unwrap_or_else(|| kind.to_lowercase().replace('_', " "));
  Notification {
    id: v.str("id").unwrap_or_default(),
    kind: notification_kind(&kind).into(),
    text: format!("{who}: {what}").trim_start_matches(": ").to_owned(),
    actor,
    target: v.str("referenceItem.content"),
    url: v.str("linkUrl").filter(|u| u.starts_with("http")),
    created_at: time(v, "updatedAt").or_else(|| time(v, "createdAt")),
    unread: None,
    raw: Some(v.clone()),
  }
}

fn notification_kind(kind: &str) -> &'static str {
  match kind {
    k if k.contains("MENTION") => "mention",
    k if k.starts_with("REPLIED") => "reply",
    k if k.contains("COMMENT") => "comment",
    k if k.starts_with("LIKE") => "like",
    k if k.contains("FOLLOW") => "follow",
    k if k.contains("REPOST") => "repost",
    _ => "other",
  }
}
