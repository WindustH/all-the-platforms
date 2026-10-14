//! The community: notes (笔记, once 旅拍), users and their posts, note
//! comments, and the archived travel notes (游记).

use atp_core::{Comment, Media, Page, Post, User, Value, ValueExt, json};

use super::{absolute, copy, images, put, time};
use crate::refs;

/// A community author (`author`, `userInfo`: `clientAuth`, `nickName`).
pub fn author(v: &Value) -> Option<User> {
  let id = v.first_str(&["clientAuth", "uid"])?;
  Some(User {
    url: Some(refs::user_url(&id)),
    name: v.first_str(&["nickName", "userNick"]).unwrap_or_default(),
    avatar: v
      .first_str(&[
        "avatar.url",
        "coverImage.dynamicUrl",
        "headPhoto",
        "userImage",
      ])
      .map(|a| absolute(&a)),
    id,
    ..User::default()
  })
}

/// A note (`16189/json/articleDetail`, `article`).
pub fn note(a: &Value) -> Option<Post> {
  let id = a.str("id")?;
  let mut media = images(a, "images", &["url", "originalUrl"]);
  media.extend(a.list("videos").iter().filter_map(video));
  let mut post = Post {
    kind: "note".into(),
    title: a.str("title"),
    text: a.first_str(&["content", "summary.content"]),
    url: Some(refs::note_url(&id)),
    author: author(a.at("author")),
    created_at: time(a, "summary.publishTime"),
    media,
    tags: a
      .list("topics")
      .iter()
      .filter_map(|t| t.first_str(&["topicName", "name", "title"]))
      .collect(),
    raw: Some(a.clone()),
    id,
    ..Post::default()
  };
  let stats = a.at("stats");
  post.metrics.views = stats.count("readCount");
  post.metrics.likes = stats.count("likeCount");
  post.metrics.comments = stats.count("commentCount");
  post.metrics.favorites = stats.count("collectCount");
  post.metrics.shares = stats.count("shareCount");
  let extra = &mut post.extra;
  put(extra, "place", a.str("summary.location").map(Value::from));
  put(extra, "pois", Some(pois(a)));
  Some(post)
}

fn video(v: &Value) -> Option<Media> {
  let url = v.first_str(&["videoUrl", "url", "playUrl"])?;
  let mut media = Media::video(absolute(&url));
  media.duration = v.f64("duration").filter(|d| *d > 0.0);
  Some(media)
}

/// The places a note or review is tagged with.
fn pois(v: &Value) -> Value {
  let list: Vec<Value> = v
    .list("pois")
    .iter()
    .filter_map(|p| {
      Some(json!({
        "name": p.first_str(&["name", "poiName"])?,
        "poi_id": p.str("poiId"),
        "district": p.str("districtName"),
        "url": p.first_str(&["urls.h5Url", "poiExt.h5Url"]).map(|u| u.trim().to_owned()),
      }))
    })
    .collect();
  list.into()
}

/// A page of mixed entries (at `key`) with a `pageKey` cursor: a user's
/// posts, a feed.
pub fn mix_page(v: &Value, key: &str) -> Page<Post> {
  let items: Vec<Post> = v.list(key).iter().filter_map(mix_item).collect();
  let next = v
    .str("pageKey")
    .filter(|_| v.bool("hasMore") == Some(true) && !items.is_empty());
  Page::new(items, next)
}

/// An entry of a user's posts or of a feed: a note (`article`), a review
/// (`comment`) or a question (`ask`).
pub fn mix_item(v: &Value) -> Option<Post> {
  if let Some(a) = v.get("article").filter(|a| a.is_object()) {
    return mix_note(a);
  }
  if let Some(c) = v.get("comment").filter(|c| c.is_object()) {
    return mix_review(c);
  }
  v.get("ask").filter(|a| a.is_object()).and_then(mix_ask)
}

fn mix_note(a: &Value) -> Option<Post> {
  let id = a.str("articleId")?;
  let mut post = Post {
    kind: "note".into(),
    title: a.str("articleTitle").map(|t| t.trim().to_owned()),
    text: a.str("content"),
    url: Some(refs::note_url(&id)),
    author: author(a.at("author")),
    created_at: time(a, "publishTime"),
    media: images(a, "images", &["originalUrl", "dynamicUrl"]),
    raw: Some(a.clone()),
    id,
    ..Post::default()
  };
  post.metrics.views = a.count("readCount");
  post.metrics.likes = a.count("likeCount");
  post.metrics.comments = a.count("commentCount");
  post.metrics.favorites = a.count("collectCount");
  put(&mut post.extra, "pois", Some(pois(a)));
  Some(post)
}

fn mix_review(c: &Value) -> Option<Post> {
  let id = c.str("commentId")?;
  let mut post = Post {
    kind: "review".into(),
    text: c.str("content"),
    url: c.str("urls.0.h5Url"),
    author: author(c.at("author")),
    created_at: time(c, "publishTime"),
    media: images(c, "imageList", &["originalUrl", "dynamicUrl"]),
    raw: Some(c.clone()),
    id,
    ..Post::default()
  };
  post.title = c
    .list("pois")
    .iter()
    .rev()
    .find_map(|p| p.str("poiName"))
    .map(|p| format!("点评 {p}"));
  put(&mut post.extra, "score", c.get("score").cloned());
  put(&mut post.extra, "pois", Some(pois(c)));
  Some(post)
}

fn mix_ask(a: &Value) -> Option<Post> {
  let id = a.str("askId")?;
  let mut post = Post {
    kind: "answer".into(),
    title: a.str("title").map(|t| t.trim().to_owned()),
    text: a.str("content"),
    url: a.str("url.h5Url"),
    raw: Some(a.clone()),
    id,
    ..Post::default()
  };
  post.metrics.comments = a.count("replyCount");
  put(&mut post.extra, "pois", Some(pois(a)));
  Some(post)
}

/// A comment on a note (`20725/json/ruleSortCommentList`), with its replies.
pub fn note_comment(v: &Value) -> Option<Comment> {
  let id = v.str("commentId")?;
  let replies: Vec<Comment> = v
    .list("childComments")
    .iter()
    .filter_map(note_comment)
    .collect();
  let mut comment = Comment {
    id,
    author: author(v.at("author")),
    text: v.str("content").unwrap_or_default(),
    created_at: time(v, "publishTime"),
    likes: v.count("likeCount"),
    reply_count: v.count("childTotalCount").filter(|n| *n > 0),
    location: v.str("ipLocatedName"),
    replies,
    raw: Some(v.clone()),
    ..Comment::default()
  };
  name_replies(&mut comment);
  Some(comment)
}

/// Name whom each reply answers (`replyCommentId`) within the thread.
fn name_replies(comment: &mut Comment) {
  let name = |c: &Comment| c.author.as_ref().map(|a| a.name.clone());
  let names: Vec<(String, Option<String>)> = std::iter::once(&*comment)
    .chain(&comment.replies)
    .map(|c| (c.id.clone(), name(c)))
    .collect();
  for reply in &mut comment.replies {
    let to = reply.raw.as_ref().and_then(|r| r.str("replyCommentId"));
    reply.reply_to = to
      .and_then(|to| names.iter().find(|(id, _)| *id == to))
      .and_then(|(_, name)| name.clone());
  }
}

/// A community profile (`16486/json/getUserPage`).
pub fn user(v: &Value) -> Option<User> {
  let base = v.at("userBase");
  let mut user = author(base)?;
  user.avatar = base
    .first_str(&["headPhotoOriginal", "headPhoto"])
    .map(|a| absolute(&a));
  user.bio = base.str("memberBrief");
  user.location = base.str("ipLocatedName");
  user.verified = base.bool("isStarAccount") == Some(true);
  // The profile names its counts by what they open: 粉丝 lists `follow`,
  // 关注 lists `friend`.
  user.stats.followers = v.count("userFriend.followCount");
  user.stats.following = v.count("userFriend.friendCount");
  user.stats.likes = v.count("userUGC.countTextMap.tripshootliked.count");
  let ugc = v.at("userUGC");
  for (key, name) in [
    ("tripShootCount", "notes"),
    ("travelCount", "travels"),
    ("commentCount", "reviews"),
    ("askCount", "questions"),
  ] {
    if let Some(n) = ugc.count(key).filter(|n| *n > 0) {
      user.stats.other.insert(name.into(), n);
    }
  }
  let extra = &mut user.extra;
  put(extra, "level", base.str("levelValueText").map(Value::from));
  put(
    extra,
    "identity",
    base.str("identityTypeName").map(Value::from),
  );
  user.raw = Some(v.clone());
  Some(user)
}

/// An archived travel note (游记) page's `data`.
pub fn travel(d: &Value, url: &str) -> Option<Post> {
  let id = d.str("Id")?;
  let nodes = d.list("Nodes");
  let text: Vec<String> = nodes.iter().filter_map(|n| n.str("Text")).collect();
  let media: Vec<Media> = nodes
    .iter()
    .filter_map(|n| n.first_str(&["Picture.Picture.BaseUrl", "Picture.Picture.Url"]))
    .map(|u| Media::image(super::original(&u)))
    .collect();
  let mut post = Post {
    kind: "travel".into(),
    title: d.str("Title"),
    text: Some(text.join("\n\n")).filter(|t| !t.is_empty()),
    url: Some(url.to_owned()),
    author: d.str("Nickname").map(|name| User {
      name,
      url: d.str("UserUrl").map(|u| absolute(&u)),
      avatar: d.str("UserHeadPhoto"),
      ..User::default()
    }),
    created_at: time(d, "PublishTime"),
    media,
    raw: Some(d.clone()),
    id,
    ..Post::default()
  };
  post.metrics.views = d.count("VisitCount");
  post.metrics.likes = d.count("LikeCount");
  post.metrics.comments = d.count("ReplyCount");
  post.metrics.shares = d.count("ShareCount");
  copy(&mut post.extra, d, &[("district", "DistrictName")]);
  let fields = [("days", "TravelDays"), ("spend", "Consume")];
  super::ids(&mut post.extra, d, &fields);
  Some(post)
}
