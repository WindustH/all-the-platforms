//! Reddit JSON → core models. Things arrive as `{kind, data}`: `t1`
//! comments, `t2` accounts, `t3` posts, `t4` messages, `t5` subreddits.

mod collections;
mod comment;
mod media;
mod messages;

use social_core::text::{from_secs, html_to_text};
use social_core::{Metrics, Notification, Post, User, UserStats, Value, ValueExt};

use crate::api::WWW;
use crate::refs::{post_url, user_url};

pub use collections::{multireddit, subreddit};
pub use comment::{comment, comment_node};
pub use media::reddit_video;
pub use messages::thread;

/// The `data` of a thing of this kind.
pub fn data<'a>(thing: &'a Value, kind: &str) -> Option<&'a Value> {
  (thing.str("kind").as_deref() == Some(kind)).then(|| thing.at("data"))
}

fn time(d: &Value, key: &str) -> Option<jiff::Timestamp> {
  d.i64(key).and_then(from_secs)
}

/// Rendered text of a post (`selftext`) or comment (`body`).
fn body(d: &Value, key: &str) -> Option<String> {
  d.str(&format!("{key}_html"))
    .map(|html| html_to_text(&html))
    .or_else(|| d.str(key))
    .filter(|t| !t.is_empty())
}

fn put(extra: &mut social_core::Extra, key: &str, value: Option<Value>) {
  if let Some(v) = value.filter(|v| !v.is_null()) {
    extra.insert(key.into(), v);
  }
}

/// Only `true` flags are worth showing.
fn flag(d: &Value, key: &str) -> Option<Value> {
  (d.bool(key) == Some(true)).then_some(Value::Bool(true))
}

/// The author of a post or comment (`None` for deleted accounts).
fn author(d: &Value) -> Option<User> {
  let name = d.str("author").filter(|n| n != "[deleted]")?;
  Some(named(&name, d.str("author_fullname")))
}

/// An account known by its name only (and its id, when given).
pub fn named(name: &str, id: Option<String>) -> User {
  User {
    id: id.unwrap_or_else(|| name.to_owned()),
    url: Some(user_url(name)),
    handle: Some(name.to_owned()),
    name: name.to_owned(),
    ..User::default()
  }
}

/// `likes` is the logged-in account's vote: true, false or null.
fn vote(d: &Value) -> Option<Value> {
  d.bool("likes")
    .map(|up| Value::from(if up { 1 } else { -1 }))
}

/// A `t3` post.
pub fn post(d: &Value) -> Option<Post> {
  post_at(d, 0)
}

fn post_at(d: &Value, depth: usize) -> Option<Post> {
  let id = d.str("id")?;
  let sub = d.str("subreddit").unwrap_or_default();
  let quoted = match depth {
    0 => d.list("crosspost_parent_list").first(),
    _ => None,
  };
  let mut post = Post {
    kind: "post".into(),
    title: d.str("title"),
    text: body(d, "selftext"),
    url: Some(post_url(&sub, &id)),
    author: author(d),
    created_at: time(d, "created_utc"),
    // `edited` is `false` or the time of the last edit.
    updated_at: time(d, "edited"),
    metrics: metrics(d),
    media: media::of(d),
    quoted: quoted.and_then(|q| post_at(q, depth + 1)).map(Box::new),
    raw: Some(d.clone()),
    id,
    ..Post::default()
  };
  let x = &mut post.extra;
  put(x, "subreddit", d.str("subreddit").map(Value::from));
  put(x, "flair", d.str("link_flair_text").map(Value::from));
  put(x, "score", d.i64("score").map(Value::from));
  put(x, "upvote_ratio", d.f64("upvote_ratio").map(Value::from));
  put(
    x,
    "subreddit_subscribers",
    d.u64("subreddit_subscribers").map(Value::from),
  );
  put(x, "nsfw", Some(d.bool("over_18").unwrap_or(false).into()));
  // `deleted` (by the author), `moderator`, `reddit`, `automod_filtered` ...
  put(x, "removed", d.str("removed_by_category").map(Value::from));
  put(x, "link", link(d).map(Value::from));
  put(x, "vote", vote(d));
  for key in [
    "spoiler", "stickied", "locked", "archived", "saved", "hidden",
  ] {
    put(x, key, flag(d, key));
  }
  put(x, "poll", poll(d));
  Some(post)
}

/// A poll post's options with their votes (counts show once voting ended or
/// to voters), the total, the end time and the account's choice.
fn poll(d: &Value) -> Option<Value> {
  let p = d.at("poll_data");
  if p.is_null() {
    return None;
  }
  let options: Vec<Value> = p
    .list("options")
    .iter()
    .map(|o| {
      serde_json::json!({
        "id": o.str("id"),
        "text": o.str("text"),
        "votes": o.u64("vote_count"),
      })
    })
    .collect();
  Some(serde_json::json!({
    "options": options,
    "total_votes": p.u64("total_vote_count"),
    "ends_at": p.i64("voting_end_timestamp").and_then(|ms| from_secs(ms / 1000)).map(|t| t.to_string()),
    "chosen": p.str("user_selection"),
  }))
}

/// Every counter of a post. `likes` is the score (upvotes minus downvotes;
/// `ups` carries the same number and `downs` is always 0); the author's own
/// views and shares are in `insights`.
fn metrics(d: &Value) -> Metrics {
  let mut m = Metrics {
    views: d.count("view_count"),
    likes: d.u64("score"),
    comments: d.count("num_comments"),
    ..Metrics::default()
  };
  // `gilded` (legacy gold) and `num_reports` (moderators only) are mostly empty.
  let counters = [
    ("crossposts", "num_crossposts", false),
    ("awards", "total_awards_received", false),
    ("gilded", "gilded", true),
    ("reports", "num_reports", true),
  ];
  for (name, key, skip_zero) in counters {
    if let Some(n) = d.u64(key).filter(|n| *n > 0 || !skip_zero) {
      m.other.insert(name.into(), n);
    }
  }
  m
}

/// The linked page of a link post (not Reddit's own media or the post itself).
fn link(d: &Value) -> Option<String> {
  if d.bool("is_self") == Some(true) {
    return None;
  }
  let url = d.first_str(&["url_overridden_by_dest", "url"])?;
  let host = url::Url::parse(&url).ok()?.host_str()?.to_owned();
  let own = ["i.redd.it", "v.redd.it", "www.reddit.com", "reddit.com"];
  (!own.contains(&host.as_str())).then_some(url)
}

/// A `t2` account (`/user/<name>/about`, `/api/me`, user search).
pub fn user(d: &Value) -> Option<User> {
  let name = d.str("name")?;
  let profile = d.at("subreddit");
  let mut stats = UserStats {
    followers: profile.count("subscribers"),
    likes: d.count("total_karma"),
    ..UserStats::default()
  };
  for key in [
    "link_karma",
    "comment_karma",
    "awarder_karma",
    "awardee_karma",
  ] {
    if let Some(n) = d.count(key) {
      stats.other.insert(key.into(), n);
    }
  }
  let mut user = User {
    id: d
      .str("id")
      .map(|id| format!("t2_{id}"))
      .unwrap_or_else(|| name.clone()),
    name: profile.str("title").unwrap_or_else(|| name.clone()),
    url: Some(user_url(&name)),
    avatar: d
      .first_str(&["icon_img", "snoovatar_img"])
      .or_else(|| profile.str("icon_img")),
    bio: profile.str("public_description"),
    stats,
    followed: profile.bool("user_is_subscriber"),
    created_at: time(d, "created_utc"),
    raw: Some(d.clone()),
    handle: Some(name),
    ..User::default()
  };
  let x = &mut user.extra;
  put(x, "banner", profile.str("banner_img").map(Value::from));
  for key in ["is_mod", "is_gold", "is_employee", "is_suspended"] {
    put(x, key, flag(d, key));
  }
  put(x, "nsfw", flag(profile, "over_18"));
  put(x, "inbox_count", d.u64("inbox_count").map(Value::from));
  Some(user)
}

/// An inbox item: `t1` (comment reply, post reply, mention) or `t4` message.
pub fn notification(thing: &Value) -> Option<Notification> {
  let d = thing.at("data");
  let kind = match (thing.str("kind")?.as_str(), d.str("type").as_deref()) {
    ("t4", _) => "message",
    (_, Some("comment_reply")) => "reply",
    (_, Some("post_reply")) => "comment",
    (_, Some("username_mention")) => "mention",
    _ => "other",
  };
  Some(Notification {
    id: d.str("name")?,
    kind: kind.into(),
    text: body(d, "body").unwrap_or_default(),
    actor: author(d),
    target: d.first_str(&["link_title", "subject"]),
    url: d
      .str("context")
      .filter(|c| !c.is_empty())
      .map(|c| format!("{WWW}{c}")),
    created_at: time(d, "created_utc"),
    unread: d.bool("new"),
    raw: Some(d.clone()),
  })
}
