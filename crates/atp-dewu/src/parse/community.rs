//! Community posts: the PC community page's hot list
//! (`community-home-hot-content`) and the share page's server-rendered
//! post (`metaOGInfo`).

use atp_core::{Media, Post, User, Value, ValueExt};

use super::{insert, media};
use crate::api::{media_url, share_url};

/// `contentType` 1 is a video; the rest are image posts.
fn kind(v: &Value) -> String {
  match v.i64("contentType") {
    Some(1) => "video".into(),
    _ => "post".into(),
  }
}

fn author(name: Option<String>, avatar: Option<String>) -> Option<User> {
  Some(User {
    name: name?,
    avatar: avatar.map(|a| media_url(&a)),
    ..User::default()
  })
}

/// A post of the hot list (`data.contentList[]`).
pub fn hot_content(v: &Value) -> Option<Post> {
  let id = v.str("contentId")?;
  let mut post = Post {
    url: Some(share_url(&id)),
    id,
    kind: kind(v),
    title: v.str("dynamicTitle"),
    author: author(v.str("authorName"), v.str("authorAvatar")),
    media: v
      .str("coverUrl")
      .map(|u| media(Media::image(media_url(&u))))
      .into_iter()
      .collect(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  post.metrics.likes = v.count("likeNum");
  post.metrics.favorites = v.count("collectNum");
  Some(post)
}

/// The post of a share page (`props.pageProps.metaOGInfo.data[0]`): its
/// clean video (`dw264_720p`, no watermark) or its images.
pub fn shared(info: &Value, trend: &str) -> Option<Post> {
  let item = info.at("data.0");
  let content = item.at("content");
  if !content.is_object() {
    return None;
  }
  let user = item.at("userInfo");
  let mut post = Post {
    id: content.str("contentId").unwrap_or_else(|| trend.to_owned()),
    kind: kind(content),
    title: content.str("title").map(|t| t.trim().to_owned()),
    text: content.str("content").map(|t| t.trim().to_owned()),
    url: Some(share_url(trend)),
    author: author(user.str("userName"), user.str("icon")),
    media: files(content),
    raw: Some(item.clone()),
    ..Post::default()
  };
  post.tags = hashtags(post.text.as_deref().unwrap_or_default());
  insert(
    &mut post.extra,
    "cover",
    content.str("cover.url").map(|u| media_url(&u).into()),
  );
  Some(post)
}

/// Videos when there are any, else images; the watermarked share video
/// only when nothing else is there.
fn files(content: &Value) -> Vec<Media> {
  let list = content.list("media.list");
  let of = |kind: &str| -> Vec<String> {
    list
      .iter()
      .filter(|m| m.str("mediaType").as_deref() == Some(kind))
      .filter_map(|m| m.str("url"))
      .map(|u| media_url(&u))
      .collect()
  };
  let videos = of("video");
  if !videos.is_empty() {
    return videos.into_iter().map(|u| media(Media::video(u))).collect();
  }
  let images = of("img");
  if !images.is_empty() {
    return images.into_iter().map(|u| media(Media::image(u))).collect();
  }
  content
    .str("videoShareUrl")
    .map(|u| {
      let mut video = media(Media::video(media_url(&u)));
      video.alt = Some("watermarked".into());
      video
    })
    .into_iter()
    .collect()
}

/// `#topic` words of a post's text.
fn hashtags(text: &str) -> Vec<String> {
  let mut tags: Vec<String> = Vec::new();
  for word in text.split('#').skip(1) {
    let tag = word.split_whitespace().next().unwrap_or_default();
    if !tag.is_empty() && !tags.iter().any(|t| t == tag) {
      tags.push(tag.to_owned());
    }
  }
  tags
}
