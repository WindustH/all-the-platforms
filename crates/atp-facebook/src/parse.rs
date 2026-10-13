//! The Comet tree into the shared model. Paths after tamnd/facebook-cli:
//! the same story arrives under different nestings per route, so lookups try
//! several paths and fall back to searching the subtree by a stable key.

use atp_core::text::from_secs;
use atp_core::{Comment, Media, MediaKind, Metrics, Post, User, Value, ValueExt};

use crate::refs::handle_of;
use crate::relay::{edges, find_key};
use crate::web::{canonical, unshim};

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

/// A `TextWithEntities`' text.
pub fn text(v: &Value) -> Option<String> {
  v.str("text").filter(|t| !t.trim().is_empty())
}

pub fn user(m: &Value) -> User {
  let url = m
    .first_str(&["url", "profile_url", "permalink_url"])
    .map(|u| canonical(&u));
  User {
    id: m.first_str(&["id", "user_id"]).unwrap_or_default(),
    name: m
      .first_str(&["name", "short_name", "title"])
      .unwrap_or_default(),
    handle: url.as_deref().and_then(handle_of),
    url,
    avatar: m.first_str(&["profile_picture.uri", "profilePicture.uri"]),
    verified: m.bool("is_verified").unwrap_or(false)
      || m.bool("show_verified_badge_on_profile").unwrap_or(false),
    ..User::default()
  }
}

/// A profile or Page from its page's operations and meta head.
pub fn profile(
  docs: &std::collections::BTreeMap<String, Value>,
  head: &std::collections::BTreeMap<String, String>,
) -> User {
  let mut u = User::default();
  if let Some(d) = docs.get("ProfileCometHeaderQuery") {
    let node = d
      .at("user.profile_header_renderer.user")
      .as_object()
      .map(|_| d.at("user.profile_header_renderer.user"))
      .or_else(|| {
        find_key(d, "profile_header_renderer").map(|r| r.at("profile_header_renderer.user"))
      });
    if let Some(n) = node.filter(|n| n.is_object()) {
      u = user(n);
      u.avatar = n
        .first_str(&["profile_picture_for_sticky_bar.uri", "profile_picture.uri"])
        .or(u.avatar);
      if let Some(c) = n.str("cover_photo.photo.image.uri") {
        u.extra.insert("cover".into(), c.into());
      }
      for item in n.list("profile_social_context.content") {
        let t = item.str("text.text").unwrap_or_default();
        let number = t.split_whitespace().next().and_then(approx);
        if t.contains("follower") {
          u.stats.followers = number;
        } else if t.contains("following") {
          u.stats.following = number;
        } else if t.contains("like") {
          u.stats.likes = number;
        }
      }
    }
  }
  if let Some(d) = docs.get("ProfilePlusCometLoggedOutRootQuery") {
    for sec in edges(d.at("user.profile_tile_sections")) {
      for view in sec.list("profile_tile_views.nodes") {
        let r = view.at("view_style_renderer");
        match r.str("__typename").as_deref() {
          Some("ProfileTileViewIntroBioRenderer") => {
            for item in r.list("view.profile_tile_items.nodes") {
              if let Some(t) = text(item.at("node.profile_status_text")) {
                u.bio = Some(t);
              }
            }
          }
          Some("ProfileTileViewContextListRenderer") => {
            for item in r.list("view.profile_tile_items.nodes") {
              let rend = item.at("node.timeline_context_item.renderer");
              let title = rend.str("context_item.title.text").unwrap_or_default();
              match rend.str("__typename").as_deref() {
                Some(
                  "InfluencerCategoryContextItemRenderer" | "PageCategoryContextItemRenderer",
                ) => {
                  u.extra.insert("category".into(), title.into());
                }
                Some("WebsiteContextItemRenderer") => {
                  u.extra.insert("website".into(), title.into());
                }
                _ if !title.is_empty() => {
                  let list = u
                    .extra
                    .entry("about".to_owned())
                    .or_insert_with(|| Value::Array(Vec::new()));
                  if let Value::Array(a) = list {
                    a.push(title.into());
                  }
                }
                _ => {}
              }
            }
          }
          _ => {}
        }
      }
    }
  }
  if u.name.is_empty() {
    u.name = head.get("og:title").cloned().unwrap_or_default();
  }
  if u.id.is_empty()
    && let Some(id) = head
      .get("al:ios:url")
      .or_else(|| head.get("al:android:url"))
      .and_then(|a| a.rsplit(['/', '=']).next().map(str::to_owned))
      .filter(|id| id.bytes().all(|b| b.is_ascii_digit()) && !id.is_empty())
  {
    u.id = id;
  }
  if u.url.is_none() {
    u.url = head.get("og:url").map(|x| canonical(x));
    u.handle = u.url.as_deref().and_then(handle_of);
  }
  if let Some(desc) = head.get("og:description") {
    if u.stats.likes.is_none() {
      u.stats.likes = count_before(desc, " likes");
    }
    if let Some(n) = count_before(desc, " talking about this") {
      u.stats.other.insert("talking_about".into(), n);
    }
    if u.bio.is_none() && !desc.is_empty() {
      u.bio = Some(desc.clone());
    }
  }
  u
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
    raw: Some(n.clone()),
    ..Post::default()
  };
  if let Some(sid) = n.str("id") {
    p.extra.insert("story_id".into(), sid.into());
  }
  for path in [
    "comet_sections.content.story.message",
    "comet_sections.content.story.comet_sections.message.story.message",
    "comet_sections.content.story.comet_sections.message_container.story.message",
    "message",
  ] {
    if let Some(t) = text(n.at(path)) {
      p.text = Some(t);
      break;
    }
  }
  for path in [
    "actors",
    "comet_sections.context_layout.story.actors",
    "comet_sections.content.story.actors",
  ] {
    if let Some(a) = n.list(path).first() {
      let u = user(a);
      if !u.name.is_empty() {
        p.author = Some(u);
        break;
      }
      p.author.get_or_insert(u);
    }
  }
  if p.author.is_none()
    && let Some(a) = find_key(n, "actors").and_then(|m| m.list("actors").first())
  {
    p.author = Some(user(a));
  }
  if p.url.is_none()
    && let Some(md) = n
      .list("comet_sections.context_layout.story.comet_sections.metadata")
      .first()
  {
    p.url = md.str("story.url").map(|u| canonical(&u));
  }
  p.metrics = counts(n);
  let mut atts = n.list("attachments").to_vec();
  if atts.is_empty() {
    atts = n.list("comet_sections.content.story.attachments").to_vec();
  }
  for a in &atts {
    attachment(a, &mut p);
  }
  // A share: the post it shares.
  // Several copies of it are nested; the full one carries text or media.
  let shared = [
    "comet_sections.content.story.attached_story",
    "attached_story",
  ]
  .iter()
  .map(|k| n.at(k))
  .filter(|s| s.is_object())
  .map(story)
  .find(|q| q.text.is_some() || !q.media.is_empty());
  if let Some(q) = shared {
    p.kind = "share".into();
    p.quoted = Some(Box::new(q));
  }
  if let Some(g) = n
    .at("feedback.associated_group")
    .as_object()
    .map(|_| n.at("feedback.associated_group"))
  {
    p.extra
      .insert("group".into(), g.str("name").unwrap_or_default().into());
  }
  if p.id.is_empty()
    && let Some(id) = p.extra.get("story_id").and_then(Value::as_str)
  {
    p.id = id.to_owned();
  }
  p
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

fn feedback(fb: &Value, m: &mut Metrics) {
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
  for e in fb.list("top_reactions.edges") {
    let Some(n) = e.count("reaction_count").filter(|n| *n > 0) else {
      continue;
    };
    let id = e.str("node.id").unwrap_or_default();
    let name = REACTIONS
      .iter()
      .find(|(i, _)| *i == id)
      .map(|(_, n)| (*n).to_owned())
      .or_else(|| e.str("node.localized_name").map(|s| s.to_lowercase()))
      .unwrap_or(id);
    m.other.entry(format!("reaction_{name}")).or_insert(n);
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
  let typename = media.str("__typename").unwrap_or_default();
  if typename == "Video" || find_key(media, "browser_native_hd_url").is_some() {
    if let Some(v) = video(media) {
      p.media.push(v);
    }
  } else if let Some(uri) = media.first_str(&[
    "image.uri",
    "photo_image.uri",
    "viewer_image.uri",
    "preferred_thumbnail.image.uri",
  ]) {
    let mut img = Media::image(uri);
    img.width = media.u64("image.width").map(|w| w as u32);
    img.height = media.u64("image.height").map(|h| h as u32);
    img.alt = media.str("accessibility_caption");
    p.media.push(img);
  } else if let Some(link) = a.str("web_link.url").or_else(|| a.str("url")) {
    let link = unshim(&link);
    if !link.contains("facebook.com") {
      p.extra.insert("link".into(), link.into());
      if let Some(t) = a.str("title_with_entities.text") {
        p.extra.insert("link_title".into(), t.into());
      }
    }
  }
  for sub in a
    .list("all_subattachments.nodes")
    .iter()
    .chain(a.list("subattachments"))
  {
    attachment(sub, p);
  }
}

/// The best stream of a video node.
pub fn video(v: &Value) -> Option<Media> {
  let node = find_key(v, "browser_native_hd_url")
    .or_else(|| find_key(v, "playable_url"))
    .unwrap_or(v);
  let url = node.first_str(&[
    "browser_native_hd_url",
    "playable_url_quality_hd",
    "browser_native_sd_url",
    "playable_url",
  ])?;
  let mut m = Media::new(MediaKind::Video, url);
  m.width = node.u64("width").map(|w| w as u32);
  m.height = node.u64("height").map(|h| h as u32);
  m.duration = node
    .f64("playable_duration_in_ms")
    .map(|ms| ms / 1000.0)
    .or_else(|| node.f64("length_in_second"));
  Some(m)
}

/// The comments a story shipped (`comment_rendering_instance…`) and their cursor.
pub fn comments(n: &Value, author: Option<&str>) -> (Vec<Comment>, Option<String>) {
  let conn = [
    "comment_rendering_instance_for_feed_location",
    "comment_rendering_instance",
  ]
  .iter()
  .find_map(|k| {
    find_key(n, k)
      .map(|m| m.at(&format!("{k}.comments")))
      .filter(|c| c.is_object())
  });
  let Some(conn) = conn else {
    return (Vec::new(), None);
  };
  let list = edges(conn)
    .into_iter()
    .map(|c| comment(c, author))
    .filter(|c| !c.id.is_empty())
    .collect();
  (list, conn.str("page_info.end_cursor"))
}

pub fn comment(n: &Value, author: Option<&str>) -> Comment {
  let mut c = Comment {
    id: n
      .str("legacy_fbid")
      .or_else(|| n.str("id"))
      .unwrap_or_default(),
    author: n.at("author").is_object().then(|| user(n.at("author"))),
    text: text(n.at("body"))
      .or_else(|| find_key(n, "body").and_then(|b| text(b.at("body"))))
      .unwrap_or_default(),
    created_at: n.i64("created_time").and_then(from_secs),
    likes: n.at("feedback").is_object().then(|| {
      let mut m = Metrics::default();
      feedback(n.at("feedback"), &mut m);
      m.likes.unwrap_or(0)
    }),
    reply_count: n.count("feedback.replies_fields.total_count"),
    raw: Some(n.clone()),
    ..Comment::default()
  };
  if let Some(url) = n.str("feedback.url").or_else(|| n.str("url")) {
    c.extra.insert("url".into(), canonical(&url).into());
  }
  if let (Some(a), Some(me)) = (&c.author, author)
    && a.id == me
  {
    c.extra.insert("by_author".into(), true.into());
  }
  c
}

/// `28M`, `2.3K`, `1,234` as a number.
pub fn approx(s: &str) -> Option<u64> {
  let s = s.trim().replace(',', "");
  let (num, mult) = match s.chars().last()? {
    'K' => (&s[..s.len() - 1], 1e3),
    'M' => (&s[..s.len() - 1], 1e6),
    'B' => (&s[..s.len() - 1], 1e9),
    _ => (s.as_str(), 1.0),
  };
  num.parse::<f64>().ok().map(|n| (n * mult).round() as u64)
}

/// The number written just before `suffix` in a sentence.
fn count_before(text: &str, suffix: &str) -> Option<u64> {
  let at = text.find(suffix)?;
  approx(text[..at].rsplit(' ').next()?)
}
