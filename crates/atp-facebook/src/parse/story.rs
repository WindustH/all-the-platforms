//! Stories: feed units and permalink nodes, with their counters, media,
//! links and the post they share.

use std::collections::BTreeMap;

use atp_core::text::from_secs;
use atp_core::{Media, Metrics, Post, User, Value, ValueExt};

use super::{text, user, video};
use crate::refs::{canonical, unshim};
use crate::relay::{edges, find_all, find_key};

/// Reaction ids → names (the comment feedback carries ids only).
const REACTIONS: &[(&str, &str)] = &[
  ("1635855486666999", "like"),
  ("1678524932434102", "love"),
  ("115940658764963", "haha"),
  ("478547315650144", "wow"),
  ("908563459236466", "sad"),
  ("444813342392137", "angry"),
  ("613557422527858", "care"),
];

const MESSAGE_PATHS: [&str; 4] = [
  "comet_sections.content.story.message",
  "comet_sections.content.story.comet_sections.message.story.message",
  "comet_sections.content.story.comet_sections.message_container.story.message",
  "message",
];
const ACTOR_PATHS: [&str; 3] = [
  "actors",
  "comet_sections.context_layout.story.actors",
  "comet_sections.content.story.actors",
];

/// The operations whose documents carry a permalink's story.
const PERMALINK_OPS: [&str; 3] = [
  "CometSinglePostDialogContentQuery",
  "CometSinglePostContentQuery",
  "CometPermalinkQuery",
];

/// The story of a permalink page's documents (posts, photos, videos,
/// reels); on other routes the first story-shaped node anywhere.
pub fn permalink(docs: &BTreeMap<String, Value>) -> Option<Post> {
  for op in PERMALINK_OPS {
    let Some(doc) = docs.get(op) else {
      continue;
    };
    let node = ["node_v2", "node", "story"]
      .iter()
      .map(|k| doc.at(k))
      .find(|n| n.is_object())
      .or_else(|| find_key(doc, "node_v2").map(|m| m.at("node_v2")));
    if let Some(node) = node {
      let post = story(node);
      if !post.id.is_empty() {
        return Some(post);
      }
    }
  }
  docs
    .values()
    .find_map(|doc| stories(doc).0.into_iter().next())
}

/// The timeline units a profile page or replay carries, and the cursor.
pub fn timeline(data: &Value) -> (Vec<Post>, Option<String>) {
  let units = match data.at("user.timeline_list_feed_units") {
    v if v.is_object() => v,
    _ => match find_key(data, "timeline_list_feed_units") {
      Some(m) => m.at("timeline_list_feed_units"),
      None => return (Vec::new(), None),
    },
  };
  connection(units)
}

/// Stories of a connection and its next cursor (when it has a next page).
pub fn connection(conn: &Value) -> (Vec<Post>, Option<String>) {
  let posts = edges(conn)
    .into_iter()
    .map(story)
    .filter(|p| !p.id.is_empty())
    .collect();
  let next = (conn.bool("page_info.has_next_page") != Some(false))
    .then(|| conn.str("page_info.end_cursor"))
    .flatten();
  (posts, next)
}

/// Every distinct story in a document, and the first cursor it carries.
pub fn stories(d: &Value) -> (Vec<Post>, Option<String>) {
  let mut found = Vec::new();
  find_all(d, &is_story, &mut found);
  let mut seen = Vec::new();
  let posts = found
    .into_iter()
    .map(story)
    .filter(|p| {
      !p.id.is_empty() && !seen.contains(&p.id) && {
        seen.push(p.id.clone());
        true
      }
    })
    .collect();
  let cursor = find_key(d, "end_cursor").and_then(|m| m.str("end_cursor"));
  (posts, cursor)
}

pub fn is_story(v: &Value) -> bool {
  v.get("post_id").is_some_and(|p| !p.is_null()) && v.get("comet_sections").is_some()
}

/// One story: a feed unit or a permalink's node.
pub fn story(n: &Value) -> Post {
  let mut p = Post {
    id: n
      .first_str(&[
        "post_id",
        "legacy_story_hideable_id",
        "comet_sections.content.story.post_id",
      ])
      .unwrap_or_default(),
    kind: "post".into(),
    url: n
      .first_str(&["permalink_url", "url"])
      .map(|u| canonical(&u)),
    created_at: n
      .i64("creation_time")
      .or_else(|| find_key(n, "creation_time").and_then(|m| m.i64("creation_time")))
      .and_then(from_secs),
    text: MESSAGE_PATHS.iter().find_map(|path| text(n.at(path))),
    author: actor(n),
    metrics: counts(n),
    raw: Some(n.clone()),
    ..Post::default()
  };
  if let Some(story_id) = n.str("id") {
    p.extra.insert("story_id".into(), story_id.into());
  }
  if p.url.is_none() {
    p.url = metadata_url(n);
  }
  for a in attachments(n) {
    attachment(a, &mut p);
  }
  if let Some(shared) = shared(n) {
    p.kind = "share".into();
    p.quoted = Some(Box::new(shared));
  }
  let group = n.at("feedback.associated_group");
  if group.is_object() {
    let name = group.str("name").unwrap_or_default();
    p.extra.insert("group".into(), name.into());
  }
  if p.id.is_empty()
    && let Some(id) = p.extra.get("story_id").and_then(Value::as_str)
  {
    p.id = id.to_owned();
  }
  p
}

/// The first actor with a name (or the first one at all).
fn actor(n: &Value) -> Option<User> {
  let mut fallback = None;
  for path in ACTOR_PATHS {
    if let Some(a) = n.list(path).first() {
      let u = user(a);
      if !u.name.is_empty() {
        return Some(u);
      }
      fallback.get_or_insert(u);
    }
  }
  fallback.or_else(|| {
    find_key(n, "actors")
      .and_then(|m| m.list("actors").first())
      .map(user)
  })
}

fn metadata_url(n: &Value) -> Option<String> {
  n.list("comet_sections.context_layout.story.comet_sections.metadata")
    .first()?
    .str("story.url")
    .map(|u| canonical(&u))
}

fn attachments(n: &Value) -> &[Value] {
  let own = n.list("attachments");
  if own.is_empty() {
    n.list("comet_sections.content.story.attachments")
  } else {
    own
  }
}

/// The post a share shares. Several copies of it are nested; the full one
/// carries text or media.
fn shared(n: &Value) -> Option<Post> {
  [
    "comet_sections.content.story.attached_story",
    "attached_story",
  ]
  .iter()
  .map(|k| n.at(k))
  .filter(|s| s.is_object())
  .map(story)
  .find(|q| q.text.is_some() || !q.media.is_empty())
}

/// Reactions (total and by type), comments, shares and views of a story.
fn counts(n: &Value) -> Metrics {
  let mut m = Metrics::default();
  let mut sources: Vec<&Value> = Vec::new();
  if let Some(r) = find_key(n, "comet_ufi_summary_and_actions_renderer") {
    sources.push(r.at("comet_ufi_summary_and_actions_renderer.feedback"));
  }
  sources.push(n.at("feedback"));
  for fb in sources.into_iter().filter(|f| f.is_object()) {
    feedback(fb, &mut m);
  }
  if m.comments.is_none()
    && let Some(cr) = find_key(n, "comment_rendering_instance")
  {
    m.comments = cr.u64("comment_rendering_instance.comments.total_count");
  }
  m
}

/// Counters of one `feedback` object, filling what is still missing.
pub(super) fn feedback(fb: &Value, m: &mut Metrics) {
  if m.likes.is_none() {
    m.likes = fb.first_count(&[
      "reaction_count.count",
      "unified_reactors.count",
      "reactors.count",
    ]);
  }
  if m.shares.is_none() {
    m.shares = fb.count("share_count.count");
  }
  if m.views.is_none() {
    m.views = fb.count("video_view_count");
  }
  if m.comments.is_none() {
    m.comments = fb.first_count(&[
      "total_comment_count",
      "comment_rendering_instance.comments.total_count",
      "comment_rendering_instance_for_feed_location.comments.total_count",
      "comment_count.total_count",
      "comments.total_count",
    ]);
  }
  for edge in fb.list("top_reactions.edges") {
    let Some(count) = edge.count("reaction_count").filter(|n| *n > 0) else {
      continue;
    };
    let id = edge.str("node.id").unwrap_or_default();
    let name = REACTIONS
      .iter()
      .find(|(known, _)| *known == id)
      .map(|(_, name)| (*name).to_owned())
      .or_else(|| edge.str("node.localized_name").map(|s| s.to_lowercase()))
      .unwrap_or(id);
    m.other.entry(format!("reaction_{name}")).or_insert(count);
  }
}

/// Photos and videos of an attachment (with album subattachments) as media;
/// links and shared posts as extras.
fn attachment(a: &Value, p: &mut Post) {
  // Newer stories wrap each attachment in a style renderer.
  let a = if a.at("styles.attachment").is_object() {
    a.at("styles.attachment")
  } else {
    a
  };
  let media = if a.at("media").is_object() {
    a.at("media")
  } else {
    a.at("target")
  };
  let is_video = media.str("__typename").as_deref() == Some("Video")
    || find_key(media, "browser_native_hd_url").is_some();
  if is_video {
    p.media.extend(video(media));
  } else if let Some(image) = image(media) {
    p.media.push(image);
  } else if let Some(link) = a.str("web_link.url").or_else(|| a.str("url")) {
    link_extra(a, &unshim(&link), p);
  }
  for sub in a
    .list("all_subattachments.nodes")
    .iter()
    .chain(a.list("subattachments"))
  {
    attachment(sub, p);
  }
}

fn image(media: &Value) -> Option<Media> {
  let uri = media.first_str(&[
    "image.uri",
    "photo_image.uri",
    "viewer_image.uri",
    "preferred_thumbnail.image.uri",
  ])?;
  let mut image = Media::image(uri);
  image.width = media.u64("image.width").map(|w| w as u32);
  image.height = media.u64("image.height").map(|h| h as u32);
  image.alt = media.str("accessibility_caption");
  Some(image)
}

/// A link to somewhere other than Facebook, and its title.
fn link_extra(a: &Value, link: &str, p: &mut Post) {
  if link.contains("facebook.com") {
    return;
  }
  p.extra.insert("link".into(), link.into());
  if let Some(title) = a.str("title_with_entities.text") {
    p.extra.insert("link_title".into(), title.into());
  }
}
