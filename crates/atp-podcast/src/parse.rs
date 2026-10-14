//! Apple, Podcast Index and feed payloads → posts: shows are posts of
//! kind `show` (their link the Apple page or the feed), episodes posts of
//! kind `episode` whose link is `FEED_URL#GUID` and whose media is the
//! audio (or video) file the publisher put in the feed.

use atp_core::feed::{self, Feed};
use atp_core::text::html_to_text;
use atp_core::{Media, MediaKind, Post, User, Value, ValueExt, json};
use jiff::Timestamp;

use crate::refs::{apple_show_url, clean_apple_url};

fn by(name: Option<String>) -> Option<User> {
  name.map(|name| User {
    id: name.clone(),
    name,
    ..User::default()
  })
}

fn time(v: &Value, path: &str) -> Option<Timestamp> {
  v.str(path).and_then(|t| t.parse().ok())
}

fn put(post: &mut Post, pairs: Vec<(&str, Value)>) {
  for (key, value) in pairs {
    if !value.is_null() {
      post.extra.insert(key.into(), value);
    }
  }
}

/// A show from iTunes Search / Lookup.
pub fn apple_show(v: &Value) -> Post {
  let id = v.str("collectionId").unwrap_or_default();
  let mut post = Post {
    url: v
      .str("collectionViewUrl")
      .map(|url| clean_apple_url(&url))
      .or_else(|| id.parse().ok().map(apple_show_url)),
    kind: "show".into(),
    title: v.str("collectionName"),
    author: by(v.str("artistName")),
    updated_at: time(v, "releaseDate"),
    tags: v
      .list("genres")
      .iter()
      .filter_map(Value::as_str)
      .filter(|genre| *genre != "Podcasts")
      .map(str::to_owned)
      .collect(),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  put(
    &mut post,
    vec![
      ("feed", json!(v.str("feedUrl"))),
      ("episodes", json!(v.u64("trackCount"))),
      (
        "image",
        json!(v.first_str(&["artworkUrl600", "artworkUrl100"])),
      ),
      (
        "explicit",
        json!(v.str("collectionExplicitness").map(|e| e == "explicit")),
      ),
      ("country", json!(v.str("country"))),
    ],
  );
  post
}

/// An episode from iTunes Search / Lookup.
pub fn apple_episode(v: &Value) -> Post {
  let apple_url = v.str("trackViewUrl").map(|url| clean_apple_url(&url));
  let feed_url = v.str("feedUrl");
  let guid = v.str("episodeGuid");
  let url = match (&feed_url, &guid) {
    (Some(feed_url), Some(guid)) => Some(feed::entry_ref(feed_url, guid)),
    _ => apple_url.clone(),
  };
  let mut post = Post {
    id: guid
      .clone()
      .or_else(|| v.str("trackId"))
      .unwrap_or_default(),
    kind: "episode".into(),
    title: v.str("trackName"),
    text: v
      .first_str(&["description", "shortDescription"])
      .map(|text| html_to_text(&text)),
    url,
    author: by(v.str("collectionName")),
    created_at: time(v, "releaseDate"),
    raw: Some(v.clone()),
    ..Post::default()
  };
  let duration = v.f64("trackTimeMillis").map(|ms| ms / 1000.0);
  if let Some(audio) = v.str("episodeUrl") {
    let kind = match v.str("episodeContentType").as_deref() {
      Some("video") => MediaKind::Video,
      _ => MediaKind::Audio,
    };
    let mut media = Media::new(kind, audio);
    media.duration = duration;
    post.media.push(media);
  }
  put(
    &mut post,
    vec![
      ("apple_url", json!(apple_url)),
      ("show_id", json!(v.str("collectionId"))),
      ("feed", json!(feed_url)),
      ("duration", json!(duration)),
      (
        "image",
        json!(v.first_str(&["artworkUrl600", "artworkUrl160"])),
      ),
    ],
  );
  post
}

/// A row of Apple's charts (a show, or an episode with its `?i=` link).
pub fn chart_row(v: &Value, episodes: bool) -> Post {
  let url = v.str("url").map(|url| clean_apple_url(&url));
  let mut post = Post {
    id: v.str("id").unwrap_or_default(),
    kind: if episodes { "episode" } else { "show" }.into(),
    title: v.str("name"),
    author: by(v.str("artistName")),
    created_at: time(v, "releaseDate"),
    url,
    tags: v
      .list("genres")
      .iter()
      .filter_map(|g| g.str("name"))
      .collect(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  put(&mut post, vec![("image", json!(v.str("artworkUrl100")))]);
  post
}

/// A show from Podcast Index (search, trending).
pub fn index_show(v: &Value) -> Post {
  let feed_url = v.first_str(&["url", "originalUrl"]);
  let apple = v.u64("itunesId").filter(|id| *id > 0);
  let mut post = Post {
    id: v.str("id").unwrap_or_default(),
    kind: "show".into(),
    title: v.str("title"),
    text: v.str("description").map(|text| html_to_text(&text)),
    url: apple.map(apple_show_url).or_else(|| feed_url.clone()),
    author: by(v.first_str(&["author", "ownerName"])),
    updated_at: v
      .i64("newestItemPublishTime")
      .or_else(|| v.i64("lastUpdateTime"))
      .and_then(atp_core::text::from_secs),
    tags: v
      .at("categories")
      .as_object()
      .map(|map| {
        map
          .values()
          .filter_map(Value::as_str)
          .map(str::to_owned)
          .collect()
      })
      .unwrap_or_default(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  put(
    &mut post,
    vec![
      ("feed", json!(feed_url)),
      ("index_id", json!(v.u64("id"))),
      ("apple_id", json!(apple)),
      ("episodes", json!(v.u64("episodeCount"))),
      ("trend_score", json!(v.u64("trendScore"))),
      ("language", json!(v.str("language"))),
      ("website", json!(v.str("link"))),
      ("image", json!(v.first_str(&["artwork", "image"]))),
    ],
  );
  post
}

/// A show as its feed describes it; `url` is where it is referred to.
pub fn feed_show(feed: &Feed, url: String) -> Post {
  let mut post = Post {
    id: feed.url.clone(),
    kind: "show".into(),
    title: feed.title.clone(),
    text: feed.description.clone(),
    url: Some(url),
    author: by(feed.author.clone()),
    updated_at: feed
      .updated
      .or_else(|| feed.entries.iter().filter_map(|e| e.created_at).max()),
    tags: feed.categories.clone(),
    ..Post::default()
  };
  put(
    &mut post,
    vec![
      ("feed", json!(feed.url)),
      ("website", json!(feed.link)),
      ("episodes", json!(feed.entries.len())),
      ("language", json!(feed.language)),
      ("image", json!(feed.image)),
    ],
  );
  post
}

/// An entry of a show's feed as an episode: link `FEED_URL#GUID`, the
/// episode page in `extra.link`, the show in `extra.show`.
pub fn feed_episode(feed: &Feed, entry: &Post) -> Post {
  let mut post = entry.clone();
  post.kind = "episode".into();
  let link = post.url.replace(feed::entry_ref(&feed.url, &entry.id));
  if post.author.is_none() {
    post.author = by(feed.title.clone());
  }
  put(
    &mut post,
    vec![
      ("link", json!(link)),
      ("show", json!(feed.title)),
      ("feed", json!(feed.url)),
    ],
  );
  post
}
