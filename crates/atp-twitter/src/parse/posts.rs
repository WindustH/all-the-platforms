//! Tweets: retweets unwrapped, quotes, long notes, articles, polls and
//! community notes.

use atp_core::text::parse_time;
use atp_core::{Media, Metrics, Post, Value, ValueExt, json};

use super::media::media;
use super::user;
use crate::refs::tweet_url;

/// `TweetWithVisibilityResults` wraps the tweet of limited-visibility posts.
fn unwrap(v: &Value) -> &Value {
  match v.at("tweet") {
    t @ Value::Object(_)
      if v.str("__typename").as_deref() == Some("TweetWithVisibilityResults") =>
    {
      t
    }
    _ => v,
  }
}

fn is_tweet(v: &Value) -> bool {
  v.at("legacy").is_object() && v.at("core").is_object()
}

/// A tweet result (`tweet_results.result`); retweets become the original post
/// with `extra.retweeted_by`. `None` for tombstones and unavailable tweets.
pub fn post(v: &Value) -> Option<Post> {
  post_at(v, 0)
}

fn post_at(result: &Value, depth: usize) -> Option<Post> {
  let outer = unwrap(result);
  if depth > 2 || !is_tweet(outer) {
    return None;
  }
  let retweeted = unwrap(outer.at("legacy.retweeted_status_result.result"));
  let (t, retweeter) = if is_tweet(retweeted) {
    (retweeted, user(outer.at("core.user_results.result")))
  } else {
    (outer, None)
  };
  let l = t.at("legacy");
  let author = user(t.at("core.user_results.result"));
  let id = t.str("rest_id").or_else(|| l.str("id_str"))?;
  let handle = author.as_ref().and_then(|a| a.handle.clone());

  let mut post = Post {
    kind: "tweet".into(),
    text: Some(text(t)),
    url: Some(tweet_url(handle.as_deref().unwrap_or("i"), &id)),
    created_at: l.str("created_at").and_then(|s| parse_time(&s)),
    metrics: metrics(t),
    media: media(l),
    tags: l
      .list("entities.hashtags")
      .iter()
      .filter_map(|h| h.str("text"))
      .collect(),
    quoted: post_at(t.at("quoted_status_result.result"), depth + 1).map(Box::new),
    raw: Some(result.clone()),
    id,
    author,
    ..Post::default()
  };
  article(t, &mut post);
  extras(t, &mut post);
  if let Some(r) = retweeter {
    if let Some(handle) = r.handle {
      post.extra.insert("retweeted_by".into(), handle.into());
    }
    if let Some(id) = outer.str("rest_id") {
      post.extra.insert("retweet_id".into(), id.into());
    }
  }
  Some(post)
}

/// Language, reply context, links, poll, community note and community, edit
/// count, and the viewer's like / retweet / bookmark flags.
fn extras(t: &Value, post: &mut Post) {
  let l = t.at("legacy");
  let extra = &mut post.extra;
  let mut put = |key: &str, value: Option<Value>| {
    if let Some(v) = value {
      extra.insert(key.into(), v);
    }
  };
  put("lang", l.str("lang").map(Value::from));
  put(
    "conversation_id",
    l.str("conversation_id_str").map(Value::from),
  );
  put(
    "in_reply_to",
    l.str("in_reply_to_status_id_str").map(Value::from),
  );
  put(
    "in_reply_to_user",
    l.str("in_reply_to_screen_name").map(Value::from),
  );
  let urls: Vec<Value> = links(l.at("entities"))
    .into_iter()
    .map(|(_, expanded)| expanded.into())
    .collect();
  if !urls.is_empty() {
    put("urls", Some(urls.into()));
  }
  put("poll", poll(t.at("card.legacy")));
  put("community_note", community_note(t));
  if let Some(community) = t.str("community_results.result.rest_id") {
    put("community", Some(community.into()));
  }
  // Later versions keep the history under `edit_control_initial`.
  let ids = [
    "edit_control.edit_tweet_ids",
    "edit_control.edit_control_initial.edit_tweet_ids",
  ]
  .iter()
  .map(|p| t.list(p))
  .find(|l| !l.is_empty())
  .unwrap_or_default();
  if ids.len() > 1 {
    put("edits", Some(Value::from(ids.len() - 1)));
  }
  for (key, flag) in [
    ("liked", "favorited"),
    ("retweeted", "retweeted"),
    ("bookmarked", "bookmarked"),
  ] {
    if l.bool(flag) == Some(true) {
      put(key, Some(true.into()));
    }
  }
}

/// A poll card (`pollNchoice…`): choices with their votes, end time, whether final.
fn poll(card: &Value) -> Option<Value> {
  let name = card.str("name")?;
  if !name.starts_with("poll") {
    return None;
  }
  let binding = |key: &str| -> Option<&Value> {
    card
      .list("binding_values")
      .iter()
      .find(|b| b.str("key").as_deref() == Some(key))
      .map(|b| b.at("value"))
  };
  let text = |key: &str| binding(key).and_then(|v| v.str("string_value"));
  let mut choices = Vec::new();
  for n in 1..=4 {
    let Some(label) = text(&format!("choice{n}_label")) else {
      break;
    };
    let votes = text(&format!("choice{n}_count")).and_then(|c| c.parse::<u64>().ok());
    choices.push(json!({ "label": label, "votes": votes }));
  }
  if choices.is_empty() {
    return None;
  }
  let finished = binding("counts_are_final").and_then(|v| v.bool("boolean_value"));
  Some(json!({
    "choices": choices,
    "ends_at": text("end_datetime_utc"),
    "final": finished,
  }))
}

/// The community note shown under a tweet, if any.
fn community_note(t: &Value) -> Option<Value> {
  let pivot = t.at("birdwatch_pivot");
  let text = pivot
    .str("subtitle.text")
    .or_else(|| pivot.str("note.summary.text"))?;
  Some(json!({
    "text": text,
    "id": pivot.str("note.rest_id"),
    "url": pivot.str("destinationUrl").or_else(|| pivot.str("destination_url")),
  }))
}

fn metrics(t: &Value) -> Metrics {
  let l = t.at("legacy");
  let mut m = Metrics {
    views: t.count("views.count"),
    likes: l.count("favorite_count"),
    comments: l.count("reply_count"),
    shares: l.count("retweet_count"),
    favorites: l.count("bookmark_count"),
    ..Metrics::default()
  };
  if let Some(q) = l.count("quote_count") {
    m.other.insert("quotes".into(), q);
  }
  m
}

/// `(t.co, expanded)` pairs of an entities object.
fn links(entities: &Value) -> Vec<(String, String)> {
  entities
    .list("urls")
    .iter()
    .filter_map(|u| Some((u.str("url")?, u.str("expanded_url")?)))
    .collect()
}

/// Full text: the long note when present, t.co links expanded, media links dropped.
fn text(t: &Value) -> String {
  let (raw, entities) = match t.str("note_tweet.note_tweet_results.result.text") {
    Some(note) => (
      note,
      t.at("note_tweet.note_tweet_results.result.entity_set"),
    ),
    None => (
      t.str("legacy.full_text").unwrap_or_default(),
      t.at("legacy.entities"),
    ),
  };
  let mut text = raw;
  for (short, long) in links(entities) {
    text = text.replace(&short, &long);
  }
  for m in t.list("legacy.entities.media") {
    if let Some(short) = m.str("url") {
      text = text.replace(&short, "");
    }
  }
  text
    .replace("&lt;", "<")
    .replace("&gt;", ">")
    .replace("&amp;", "&")
    .trim()
    .to_owned()
}

/// Long-form article attached to the tweet: title, plain text and cover.
fn article(t: &Value, post: &mut Post) {
  let a = t.at("article.article_results.result");
  if !a.is_object() {
    return;
  }
  post.kind = "article".into();
  post.title = a.str("title");
  if let Some(body) = a.first_str(&["plain_text", "preview_text"]) {
    post.text = Some(body);
  }
  if let Some(cover) = a.str("cover_media.media_info.original_img_url") {
    post.media.insert(0, Media::image(cover));
  }
  if let Some(id) = a.str("rest_id") {
    post.extra.insert("article_id".into(), id.into());
  }
}
