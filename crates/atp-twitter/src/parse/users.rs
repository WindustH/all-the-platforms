//! Users: the GraphQL user result and the v1.1 user objects of direct
//! messages.

use atp_core::text::parse_time;
use atp_core::{User, UserStats, Value, ValueExt};

use crate::refs::user_url;

/// A user result (`user_results.result`); `None` for unavailable users.
/// Reads both the current shape (`core`, `relationship_counts`, `profile_bio` ...)
/// and the older one where everything sits in `legacy`.
pub fn user(v: &Value) -> Option<User> {
  if v.str("__typename").as_deref() == Some("UserUnavailable") {
    return None;
  }
  let id = v.str("rest_id")?;
  let handle = v.first_str(&["core.screen_name", "legacy.screen_name"]);
  let mut user = User {
    name: v
      .first_str(&["core.name", "legacy.name"])
      .or_else(|| handle.clone())
      .unwrap_or_default(),
    url: handle.as_deref().map(user_url),
    avatar: v.first_str(&["avatar.image_url", "legacy.profile_image_url_https"]),
    bio: v.first_str(&["profile_bio.description", "legacy.description"]),
    verified: v.bool("is_blue_verified").unwrap_or(false)
      || v.bool("verification.verified").unwrap_or(false)
      || v.bool("legacy.verified").unwrap_or(false),
    location: v.first_str(&["location.location", "legacy.location"]),
    stats: stats(v),
    followed: v
      .bool("relationship_perspectives.following")
      .or_else(|| v.bool("legacy.following")),
    created_at: v
      .first_str(&["core.created_at", "legacy.created_at"])
      .and_then(|t| parse_time(&t)),
    raw: Some(v.clone()),
    id,
    handle,
    ..User::default()
  };
  extras(v, &mut user);
  Some(user)
}

/// Followers, following, tweets and likes; media, listed and subscriptions
/// as `other`.
fn stats(v: &Value) -> UserStats {
  let mut stats = UserStats {
    followers: v.first_count(&["relationship_counts.followers", "legacy.followers_count"]),
    following: v.first_count(&["relationship_counts.following", "legacy.friends_count"]),
    posts: v.first_count(&["tweet_counts.tweets", "legacy.statuses_count"]),
    likes: v.first_count(&["action_counts.favorites_count", "legacy.favourites_count"]),
    ..UserStats::default()
  };
  let other = [
    (
      "media",
      v.first_count(&["tweet_counts.media_tweets", "legacy.media_count"]),
    ),
    ("listed", v.count("legacy.listed_count")),
    ("subscriptions", v.count("creator_subscriptions_count")),
  ];
  for (key, n) in other {
    if let Some(n) = n {
      stats.other.insert(key.into(), n);
    }
  }
  stats
}

/// Website, banner, pinned tweet and the protected flag.
fn extras(v: &Value, user: &mut User) {
  let extra = &mut user.extra;
  let mut put = |key: &str, value: Option<String>| {
    if let Some(value) = value {
      extra.insert(key.into(), value.into());
    }
  };
  put(
    "website",
    v.first_str(&[
      "profile_bio.entities.url.urls.0.expanded_url",
      "legacy.entities.url.urls.0.expanded_url",
    ]),
  );
  put(
    "banner",
    v.first_str(&["banner.image_url", "legacy.profile_banner_url"]),
  );
  put(
    "pinned_tweet",
    v.first_str(&[
      "pinned_items.tweet_ids_str.0",
      "legacy.pinned_tweet_ids_str.0",
    ]),
  );
  let protected = v
    .bool("privacy.protected")
    .or_else(|| v.bool("legacy.protected"));
  if protected == Some(true) {
    user.extra.insert("protected".into(), true.into());
  }
}
