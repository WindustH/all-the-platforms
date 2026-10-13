//! Instagram objects into the shared model. Two shapes arrive: the v1 API's
//! (`pk`, `code`, `image_versions2`, `video_versions`, `carousel_media`) and
//! the GraphQL / web-info one (`shortcode`, `display_url`, `edge_…`).

use atp_core::text::from_secs;
use atp_core::{Comment, Media, MediaKind, Metrics, Post, User, Value, ValueExt};

use crate::refs;

pub fn user(v: &Value) -> User {
  let username = v.str("username");
  let mut u = User {
    id: v.first_str(&["pk", "pk_id", "id"]).unwrap_or_default(),
    name: v
      .str("full_name")
      .filter(|n| !n.is_empty())
      .or_else(|| username.clone())
      .unwrap_or_default(),
    url: username
      .as_ref()
      .map(|n| format!("https://www.instagram.com/{n}/")),
    handle: username,
    avatar: v.first_str(&[
      "hd_profile_pic_url_info.url",
      "profile_pic_url_hd",
      "profile_pic_url",
    ]),
    bio: v.str("biography").filter(|b| !b.is_empty()),
    verified: v.bool("is_verified").unwrap_or(false),
    raw: Some(v.clone()),
    ..User::default()
  };
  u.stats.followers = v.first_count(&["follower_count", "edge_followed_by.count"]);
  u.stats.following = v.first_count(&["following_count", "edge_follow.count"]);
  u.stats.posts = v.first_count(&["media_count", "edge_owner_to_timeline_media.count"]);
  for (key, path) in [
    ("private", "is_private"),
    ("business", "is_business_account"),
    ("category", "category_name"),
    ("external_url", "external_url"),
  ] {
    if let Some(x) = v.get(path).filter(|x| !x.is_null() && *x != "") {
      u.extra.insert(key.into(), x.clone());
    }
  }
  if let Some(f) = v
    .bool("followed_by_viewer")
    .or_else(|| v.bool("friendship_status.following"))
  {
    u.followed = Some(f);
  }
  u
}

/// A post (feed photo, carousel, reel, IGTV) in either shape.
pub fn media(v: &Value) -> Post {
  let code = v.first_str(&["code", "shortcode"]);
  let id = v
    .first_str(&["pk", "id"])
    .map(|i| i.split('_').next().unwrap_or(&i).to_owned())
    .or_else(|| code.as_deref().and_then(refs::decode))
    .unwrap_or_default();
  let code = code.or_else(|| refs::encode(&id));
  let product = v.str("product_type").unwrap_or_default();
  let video = v.i64("media_type") == Some(2) || v.bool("is_video") == Some(true);
  let kind = match (product.as_str(), v.i64("media_type")) {
    ("clips", _) => "reel",
    ("igtv", _) => "igtv",
    (_, Some(8)) => "carousel",
    _ if v.at("edge_sidecar_to_children").is_object() => "carousel",
    _ if video => "video",
    _ => "photo",
  };
  let owner = if v.at("user").is_object() {
    v.at("user")
  } else {
    v.at("owner")
  };
  let text = v
    .str("caption.text")
    .or_else(|| v.str("edge_media_to_caption.edges.0.node.text"));
  let mut post = Post {
    id,
    kind: kind.into(),
    text,
    url: code.as_deref().map(|c| match kind {
      "reel" => format!("https://www.instagram.com/reel/{c}/"),
      _ => refs::post_url(c),
    }),
    author: owner.is_object().then(|| user(owner)),
    created_at: v
      .first_str(&["taken_at", "taken_at_timestamp"])
      .and_then(|t| t.parse().ok())
      .and_then(from_secs),
    metrics: Metrics {
      likes: v.first_count(&[
        "like_count",
        "edge_media_preview_like.count",
        "edge_liked_by.count",
      ]),
      comments: v.first_count(&[
        "comment_count",
        "edge_media_to_parent_comment.count",
        "edge_media_to_comment.count",
      ]),
      views: v.first_count(&[
        "play_count",
        "ig_play_count",
        "view_count",
        "video_play_count",
        "video_view_count",
      ]),
      shares: v.count("reshare_count"),
      ..Metrics::default()
    },
    media: media_files(v),
    raw: Some(v.clone()),
    ..Post::default()
  };
  if let Some(c) = &code {
    post.extra.insert("shortcode".into(), c.clone().into());
  }
  if let Some(loc) = v.str("location.name") {
    post.extra.insert("location".into(), loc.into());
  }
  post
}

/// Every image / video of a post at its largest size (carousels flattened).
fn media_files(v: &Value) -> Vec<Media> {
  let children: Vec<&Value> = if !v.list("carousel_media").is_empty() {
    v.list("carousel_media").iter().collect()
  } else if v.at("edge_sidecar_to_children").is_object() {
    v.list("edge_sidecar_to_children.edges")
      .iter()
      .map(|e| e.at("node"))
      .collect()
  } else {
    vec![v]
  };
  children.into_iter().filter_map(file).collect()
}

fn file(v: &Value) -> Option<Media> {
  let video = v
    .list("video_versions")
    .iter()
    .max_by_key(|x| x.u64("width").unwrap_or(0) * x.u64("height").unwrap_or(0))
    .cloned()
    .or_else(|| {
      v.str("video_url")
        .map(|u| serde_json::json!({"url": u, "width": v.at("dimensions.width"), "height": v.at("dimensions.height")}))
    });
  if let Some(vid) = video.filter(|x| x.str("url").is_some()) {
    let mut m = Media::new(MediaKind::Video, vid.str("url")?);
    m.width = vid.u64("width").map(|w| w as u32);
    m.height = vid.u64("height").map(|h| h as u32);
    m.duration = v.f64("video_duration");
    return Some(m);
  }
  let best = v
    .list("image_versions2.candidates")
    .iter()
    .max_by_key(|x| x.u64("width").unwrap_or(0))
    .cloned();
  let (url, w, h) = match best {
    Some(b) => (b.str("url")?, b.u64("width"), b.u64("height")),
    None => (
      v.str("display_url")?,
      v.u64("dimensions.width"),
      v.u64("dimensions.height"),
    ),
  };
  let mut m = Media::image(url);
  m.width = w.map(|x| x as u32);
  m.height = h.map(|x| x as u32);
  m.alt = v.str("accessibility_caption");
  Some(m)
}

pub fn comment(v: &Value) -> Comment {
  let owner = if v.at("user").is_object() {
    v.at("user")
  } else {
    v.at("owner")
  };
  Comment {
    id: v.first_str(&["pk", "id"]).unwrap_or_default(),
    author: owner.is_object().then(|| user(owner)),
    text: v.str("text").unwrap_or_default(),
    created_at: v
      .first_str(&["created_at_utc", "created_at"])
      .and_then(|t| t.parse().ok())
      .and_then(from_secs),
    likes: v.first_count(&["comment_like_count", "edge_liked_by.count"]),
    reply_count: v.first_count(&["child_comment_count", "edge_threaded_comments.count"]),
    replies: v
      .list("preview_child_comments")
      .iter()
      .chain(
        v.list("edge_threaded_comments.edges")
          .iter()
          .map(|e| e.at("node")),
      )
      .map(comment)
      .collect(),
    raw: Some(v.clone()),
    ..Comment::default()
  }
}
