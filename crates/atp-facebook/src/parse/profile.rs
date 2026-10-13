//! A profile or Page from its page's operations and meta head.

use std::collections::BTreeMap;

use atp_core::{User, Value, ValueExt};

use super::{approx, count_before, text, user};
use crate::refs::{canonical, handle_of};
use crate::relay::{edges, find_key};

pub fn profile(docs: &BTreeMap<String, Value>, head: &BTreeMap<String, String>) -> User {
  let mut u = docs
    .get("ProfileCometHeaderQuery")
    .and_then(header)
    .unwrap_or_default();
  if let Some(d) = docs.get("ProfilePlusCometLoggedOutRootQuery") {
    for section in edges(d.at("user.profile_tile_sections")) {
      for view in section.list("profile_tile_views.nodes") {
        tile(view.at("view_style_renderer"), &mut u);
      }
    }
  }
  from_head(head, &mut u);
  u
}

/// The header: name, picture, cover and the follower / like counts.
fn header(d: &Value) -> Option<User> {
  let node = d
    .at("user.profile_header_renderer.user")
    .as_object()
    .map(|_| d.at("user.profile_header_renderer.user"))
    .or_else(|| {
      find_key(d, "profile_header_renderer").map(|r| r.at("profile_header_renderer.user"))
    })
    .filter(|n| n.is_object())?;
  let mut u = user(node);
  u.avatar = node
    .first_str(&["profile_picture_for_sticky_bar.uri", "profile_picture.uri"])
    .or(u.avatar);
  if let Some(cover) = node.str("cover_photo.photo.image.uri") {
    u.extra.insert("cover".into(), cover.into());
  }
  for item in node.list("profile_social_context.content") {
    let line = item.str("text.text").unwrap_or_default();
    let number = line.split_whitespace().next().and_then(approx);
    if line.contains("follower") {
      u.stats.followers = number;
    } else if line.contains("following") {
      u.stats.following = number;
    } else if line.contains("like") {
      u.stats.likes = number;
    }
  }
  Some(u)
}

/// One tile of the logged-out profile: the bio, or the context list
/// (category, website, other lines into `about`).
fn tile(renderer: &Value, u: &mut User) {
  let items = renderer.list("view.profile_tile_items.nodes");
  match renderer.str("__typename").as_deref() {
    Some("ProfileTileViewIntroBioRenderer") => {
      for item in items {
        if let Some(bio) = text(item.at("node.profile_status_text")) {
          u.bio = Some(bio);
        }
      }
    }
    Some("ProfileTileViewContextListRenderer") => {
      for item in items {
        context_item(item.at("node.timeline_context_item.renderer"), u);
      }
    }
    _ => {}
  }
}

fn context_item(renderer: &Value, u: &mut User) {
  let title = renderer.str("context_item.title.text").unwrap_or_default();
  match renderer.str("__typename").as_deref() {
    Some("InfluencerCategoryContextItemRenderer" | "PageCategoryContextItemRenderer") => {
      u.extra.insert("category".into(), title.into());
    }
    Some("WebsiteContextItemRenderer") => {
      u.extra.insert("website".into(), title.into());
    }
    _ if !title.is_empty() => {
      let about = u
        .extra
        .entry("about".to_owned())
        .or_insert_with(|| Value::Array(Vec::new()));
      if let Value::Array(lines) = about {
        lines.push(title.into());
      }
    }
    _ => {}
  }
}

/// What the `<meta>` head adds: name, id, link, likes and the description.
fn from_head(head: &BTreeMap<String, String>, u: &mut User) {
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
}
