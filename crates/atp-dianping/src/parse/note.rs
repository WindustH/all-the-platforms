//! A discovery note from its mobile page, whose `__xhrCache__` holds the
//! note as `discovery/api/getFeedDetail` answered it: title, HTML text,
//! author, times, counts and pictures.

use atp_core::text::{from_beijing, html_to_text};
use atp_core::{Media, Post, User, Value, ValueExt};

use super::cached;
use crate::api::absolute;
use crate::refs::{member_url, note_url};

pub fn note(cache: &Value, id: &str) -> Option<Post> {
  let v = cached(cache, "getFeedDetail")?.at("data");
  let title = v.str("title");
  let text = v.str("content").map(|c| html_to_text(&c));
  if title.is_none() && text.is_none() {
    return None;
  }
  let mut post = Post {
    id: v.str("contentId").unwrap_or_else(|| id.to_owned()),
    kind: "note".into(),
    title,
    text,
    url: Some(note_url(id)),
    author: author(v),
    created_at: v.str("publishTime").and_then(|t| from_beijing(&t)),
    media: media(v),
    tags: v
      .list("tags")
      .iter()
      .filter_map(|t| t.as_str().map(str::to_owned).or_else(|| t.str("name")))
      .collect(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  post.metrics.likes = v.count("likeTimes");
  post.metrics.comments = v.count("commentTimes");
  post.metrics.views = v.count("scanTimes").filter(|n| *n > 0);
  if let Some(shop) = v.str("shopId") {
    post.extra.insert("shop_id".into(), shop.into());
  }
  Some(post)
}

fn author(v: &Value) -> Option<User> {
  let name = v.first_str(&["userNickName", "userInfoDto.userNickName"])?;
  let id = v.str("userId").unwrap_or_else(|| name.clone());
  Some(User {
    url: v.str("userId").map(|u| member_url(&u)),
    avatar: v
      .first_str(&["userInfoDto.userPic", "userPic"])
      .map(|a| absolute(&a)),
    id,
    name,
    ..User::default()
  })
}

/// Pictures at their large size, and the video when there is one.
fn media(v: &Value) -> Vec<Media> {
  let mut media: Vec<Media> = Vec::new();
  for pic in v.list("contentPicList").iter().chain(v.list("picList")) {
    let Some(url) = pic.first_str(&["bigPicUrl", "smallPicUrl"]) else {
      continue;
    };
    let url = absolute(&url);
    if !media.iter().any(|m| m.url == url) {
      media.push(Media::image(url));
    }
  }
  if let Some(play) = v.str("contentVideo.playUrl") {
    media.push(Media::video(absolute(&play)));
  }
  media
}
