//! Page data → posts, comments and users. A show is a post of kind `show`
//! linking to its page; an episode is a post of kind `episode` whose
//! author is its show and whose media is the audio file the page plays
//! (the publisher's file on Xiaoyuzhou's CDN); hot comments keep the
//! replies the page shows under them.

use atp_core::text::html_to_text;
use atp_core::{Comment, Media, MediaKind, Metrics, Post, User, Value, ValueExt, json};
use jiff::Timestamp;

use crate::refs::{episode_url, show_url, user_url};

fn time(v: &Value, path: &str) -> Option<Timestamp> {
  v.str(path).and_then(|t| t.parse().ok())
}

fn put(extra: &mut atp_core::Extra, pairs: Vec<(&str, Value)>) {
  for (key, value) in pairs {
    if !value.is_null() {
      extra.insert(key.into(), value);
    }
  }
}

/// The largest picture of an image object.
fn picture(v: &Value, path: &str) -> Option<String> {
  let image = v.at(path);
  image.first_str(&["picUrl", "largePicUrl", "middlePicUrl", "smallPicUrl"])
}

/// A show as the author of its episodes.
fn show_as_author(show: &Value, pid: Option<String>) -> Option<User> {
  let pid = pid.or_else(|| show.str("pid"))?;
  Some(User {
    name: show.str("title").unwrap_or_else(|| pid.clone()),
    url: Some(show_url(&pid)),
    avatar: picture(show, "image"),
    id: pid,
    ..User::default()
  })
}

/// A user, podcaster or comment author (`uid`, `nickname`, `avatar`).
pub fn user(v: &Value) -> User {
  let id = v.str("uid").unwrap_or_default();
  let mut user = User {
    name: v.str("nickname").unwrap_or_else(|| id.clone()),
    url: (!id.is_empty()).then(|| user_url(&id)),
    avatar: picture(v, "avatar.picture"),
    bio: v.str("bio"),
    location: v.str("ipLoc"),
    raw: Some(v.clone()),
    id,
    ..User::default()
  };
  put(&mut user.extra, vec![("gender", json!(v.str("gender")))]);
  user
}

/// A show (`/podcast/<id>`, or a user's hosted show).
pub fn show(v: &Value) -> Post {
  let id = v.str("pid").unwrap_or_default();
  let mut post = Post {
    url: Some(show_url(&id)),
    kind: "show".into(),
    title: v.str("title"),
    text: v.first_str(&["description", "brief"]),
    author: v.str("author").map(|name| User {
      id: name.clone(),
      name,
      ..User::default()
    }),
    updated_at: time(v, "latestEpisodePubDate"),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  post.metrics.other.extend(
    v.u64("subscriptionCount")
      .map(|n| ("subscribers".to_owned(), n)),
  );
  let hosts: Vec<Value> = v
    .list("podcasters")
    .iter()
    .map(|p| json!({"id": p.str("uid"), "name": p.str("nickname"), "url": p.str("uid").map(|id| user_url(&id))}))
    .collect();
  put(
    &mut post.extra,
    vec![
      ("brief", json!(v.str("brief"))),
      ("episodes", json!(v.u64("episodeCount"))),
      ("image", json!(picture(v, "image"))),
      (
        "podcasters",
        if hosts.is_empty() {
          Value::Null
        } else {
          json!(hosts)
        },
      ),
      ("pay_type", json!(v.str("payType").filter(|t| t != "FREE"))),
    ],
  );
  post
}

/// The audio file of an episode, when the page gives one.
fn audio(v: &Value) -> Option<Media> {
  let url = v.first_str(&["enclosure.url", "media.source.url"])?;
  let mime = v.str("media.mimeType").unwrap_or_default();
  let kind = match MediaKind::from_mime(&mime) {
    MediaKind::Video => MediaKind::Video,
    _ => MediaKind::Audio,
  };
  let mut media = Media::new(kind, url);
  media.duration = v.f64("duration");
  media.size = v.u64("media.size");
  Some(media)
}

/// An episode (`/episode/<id>`, or one of a show's).
pub fn episode(v: &Value) -> Post {
  let id = v.str("eid").unwrap_or_default();
  let notes = v.str("shownotes").map(|html| html_to_text(&html));
  let mut post = Post {
    url: Some(episode_url(&id)),
    kind: "episode".into(),
    title: v.str("title"),
    text: notes.or_else(|| v.str("description")),
    author: show_as_author(v.at("podcast"), v.str("pid")),
    created_at: time(v, "pubDate"),
    metrics: Metrics {
      views: v.u64("playCount"),
      likes: v.u64("clapCount"),
      comments: v.u64("commentCount"),
      favorites: v.u64("favoriteCount"),
      ..Metrics::default()
    },
    media: audio(v).into_iter().collect(),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  put(
    &mut post.extra,
    vec![
      ("show_id", json!(v.str("pid"))),
      ("show", json!(v.str("podcast.title"))),
      ("duration", json!(v.u64("duration"))),
      (
        "image",
        json!(picture(v, "image").or_else(|| picture(v, "podcast.image"))),
      ),
      ("pay_type", json!(v.str("payType").filter(|t| t != "FREE"))),
    ],
  );
  post
}

/// A comment and the replies the page shows under it.
pub fn comment(v: &Value) -> Comment {
  let author = user(v.at("author"));
  Comment {
    id: v.str("id").unwrap_or_default(),
    text: v.str("text").unwrap_or_default(),
    created_at: time(v, "createdAt"),
    likes: v.u64("likeCount"),
    reply_count: v.u64("replyCount"),
    reply_to: v.str("replyToComment.author.nickname"),
    location: author.location.clone(),
    replies: v.list("replies").iter().map(comment).collect(),
    author: Some(author),
    raw: Some(v.clone()),
    ..Comment::default()
  }
}

/// A user page (`/user/<id>`): the profile, counts and hosted shows.
pub fn profile(v: &Value) -> User {
  let mut user = user(v.at("user"));
  let stats = v.at("userStats");
  user.stats.followers = stats.u64("followerCount");
  user.stats.following = stats.u64("followingCount");
  user.stats.other.extend(
    stats
      .u64("realSubscriptionCount")
      .map(|n| ("subscriptions".to_owned(), n)),
  );
  let shows: Vec<Value> = v
    .list("ownedPodcasts")
    .iter()
    .map(|s| json!({"id": s.str("pid"), "title": s.str("title"), "url": s.str("pid").map(|id| show_url(&id))}))
    .collect();
  put(
    &mut user.extra,
    vec![
      (
        "shows",
        if shows.is_empty() {
          Value::Null
        } else {
          json!(shows)
        },
      ),
      ("played_seconds", json!(stats.u64("totalPlayedSeconds"))),
    ],
  );
  user.raw = Some(v.clone());
  user
}
