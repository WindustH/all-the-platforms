//! Tieba JSON → shared models.
//!
//! Threads (帖子) become posts, floors (楼) comments and their 楼中楼 replies
//! nested comments; forums (吧) and followed forums are collections. Rich
//! content is a list of typed elements (text, link, emoji, image, @user,
//! video, voice) flattened into text plus media. Listings send authors
//! separately in `user_list`, keyed by `author_id`.

use std::collections::HashMap;

use jiff::Timestamp;
use social_core::text::{from_secs, parse_count};
use social_core::{
  Collection, Comment, Media, MediaKind, Metrics, Notification, Post, User, UserStats, Value,
  ValueExt, json,
};

use crate::refs::{avatar_url, clean_portrait, floor_url, forum_url, thread_url, user_url};

/// Authors of a listing by id.
pub type Users = HashMap<String, Value>;

pub fn users(list: &[Value]) -> Users {
  list
    .iter()
    .filter_map(|u| Some((u.str("id")?, u.clone())))
    .collect()
}

fn secs(v: &Value, keys: &[&str]) -> Option<Timestamp> {
  keys
    .iter()
    .find_map(|k| v.i64(k).filter(|t| *t > 0))
    .and_then(from_secs)
}

fn count(v: &Value, keys: &[&str]) -> Option<u64> {
  keys.iter().find_map(|k| match v.at(k) {
    Value::String(s) => parse_count(&s.replace(['W', 'w'], "万")),
    other => other.as_u64(),
  })
}

// ── rich content ────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct Body {
  pub text: String,
  pub media: Vec<Media>,
  /// Name in a leading `回复 <name> :` (replies inside a floor).
  pub reply_to: Option<String>,
}

fn image(url: String, bsize: Option<String>, size: Option<u64>) -> Media {
  let mut m = Media::image(url);
  if let Some((w, h)) = bsize.as_deref().and_then(|s| s.split_once(',')) {
    m.width = w.trim().parse().ok().filter(|w| *w > 0);
    m.height = h.trim().parse().ok().filter(|h| *h > 0);
  }
  m.size = size.filter(|s| *s > 0);
  m
}

/// Flatten content elements into text and media.
pub fn body(items: &[Value]) -> Body {
  let mut b = Body::default();
  for e in items {
    let text = e.str("text").unwrap_or_default();
    match e.i64("type").unwrap_or(0) {
      1 => {
        b.text += &text;
        if let Some(link) = e.str("link").filter(|l| *l != text) {
          b.text += &format!(" ({link})");
        }
      }
      2 | 11 => b.text += &format!("[{}]", e.first_str(&["c", "text"]).unwrap_or_default()),
      3 => {
        if let Some(url) = e.first_str(&["origin_src", "big_cdn_src", "cdn_src", "src"]) {
          b.media
            .push(image(url, e.str("bsize"), e.u64("origin_size")));
        }
      }
      5 => match (e.str("link"), e.str("src")) {
        (Some(link), Some(cover)) => {
          let mut m = Media::video(link);
          m.alt = Some(cover);
          b.media.push(m);
        }
        _ => b.text += &text,
      },
      10 => {
        if let Some(md5) = e.str("voice_md5") {
          let mut m = Media::new(
            MediaKind::Audio,
            format!("https://tiebac.baidu.com/c/p/voice?voice_md5={md5}&play_from=pb_voice_play"),
          );
          m.duration = e.f64("during_time").map(|ms| ms / 1000.0);
          b.media.push(m);
        }
      }
      20 => {
        if let Some(src) = e.str("src") {
          b.media.push(image(src, e.str("bsize"), None));
        }
      }
      _ if e.str("uid").is_some() && b.text.trim() == "回复" && b.reply_to.is_none() => {
        b.reply_to = Some(text.trim_start_matches('@').to_owned());
        b.text.clear();
      }
      _ => b.text += &text,
    }
  }
  if b.reply_to.is_some() {
    b.text = b
      .text
      .trim_start()
      .trim_start_matches([':', '：'])
      .to_owned();
  }
  b.text = b.text.trim().to_owned();
  b
}

/// Text and media of whichever content field a listing uses.
fn thread_body(t: &Value) -> Body {
  for key in ["first_post_content", "abstract", "rich_abstract", "content"] {
    match t.at(key) {
      Value::Array(items) if !items.is_empty() => {
        // `content` of user posts is a list of {post_content: [...]}.
        if items[0].at("post_content").is_array() {
          return body(items[0].list("post_content"));
        }
        return body(items);
      }
      Value::String(s) if !s.is_empty() => {
        return Body {
          text: s.trim().to_owned(),
          ..Body::default()
        };
      }
      _ => {}
    }
  }
  Body::default()
}

// ── people and forums ───────────────────────────────────────────────────

pub fn user(v: &Value) -> Option<User> {
  let portrait = v
    .first_str(&["portrait", "encry_uid", "portraith"])
    .map(|p| clean_portrait(&p))
    .filter(|p| !p.is_empty());
  let id = v.first_str(&["id", "user_id", "uid"]).filter(|i| i != "0");
  if id.is_none() && portrait.is_none() {
    return None;
  }
  let shown = v.first_str(&[
    "name_show",
    "show_nickname",
    "user_nickname",
    "nickname",
    "name",
    "user_name",
  ]);
  // Hidden accounts show `贴吧用户_xxx` with the bare `贴吧用户` as name.
  let handle = v
    .first_str(&["name", "user_name"])
    .filter(|n| shown.as_ref().is_none_or(|s| !s.starts_with(n.as_str())));
  let mut stats = UserStats {
    followers: count(v, &["fans_num"]),
    following: count(v, &["concern_num"]),
    posts: count(v, &["post_num"]),
    likes: count(v, &["total_agree_num"]),
    ..UserStats::default()
  };
  if let Some(n) = count(v, &["thread_num"]).filter(|n| *n > 0) {
    stats.other.insert("threads".into(), n);
  }
  if let Some(n) = count(v, &["my_like_num", "like_forum_num"]).filter(|n| *n > 0) {
    stats.other.insert("forums".into(), n);
  }
  let mut extra = social_core::Extra::new();
  for (key, path) in [
    ("level", "level_id"),
    ("level_name", "level_name"),
    ("tieba_age", "tb_age"),
    ("tieba_uid", "tieba_uid"),
    ("bawu", "bawu_type"),
  ] {
    if let Some(s) = v.str(path).filter(|s| s != "0") {
      extra.insert(key.into(), json!(s));
    }
  }
  Some(User {
    id: id.unwrap_or_default(),
    name: shown.clone().or_else(|| handle.clone()).unwrap_or_default(),
    handle,
    url: portrait.as_deref().map(user_url),
    avatar: portrait.as_deref().map(avatar_url),
    bio: v.str("intro"),
    location: v.str("ip_address"),
    stats,
    followed: v
      .first_str(&["has_concerned", "is_followed", "is_like"])
      .map(|s| s == "1" || s == "true"),
    extra,
    raw: Some(v.clone()),
    ..User::default()
  })
}

/// The author of an item: its own object, or the listing's user by id.
fn author(item: &Value, users: &Users) -> Option<User> {
  ["author", "user", "replyer"]
    .iter()
    .find_map(|k| user(item.at(k)))
    .or_else(|| {
      let id = item.first_str(&["author_id", "user_id"])?;
      user(users.get(&id)?)
    })
}

/// A forum. Its name is the id: Tieba addresses forums by name (`/f?kw=`)
/// and has no lookup by number, so `#N` refs must carry the name; the
/// number is `extra.fid`.
pub fn forum(v: &Value) -> Option<Collection> {
  let name = v.first_str(&["name", "forum_name", "fname", "forum_name_show"])?;
  let mut c = Collection {
    id: name.clone(),
    kind: "forum".into(),
    url: Some(forum_url(&name)),
    name,
    description: v.first_str(&["slogan", "intro"]),
    items: count(v, &["thread_num"]),
    followers: count(
      v,
      &[
        "member_num",
        "concern_num_ori",
        "concern_num",
        "member_count",
      ],
    ),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if let Some(fid) = v.first_str(&["id", "forum_id", "fid"]) {
    c.extra.insert("fid".into(), json!(fid));
  }
  if let Some(n) = count(v, &["post_num_ori", "post_num"]) {
    c.extra.insert("posts".into(), json!(n));
  }
  for (key, path) in [
    ("avatar", "avatar"),
    ("category", "first_class"),
    ("subcategory", "second_class"),
    ("level", "level_id"),
    ("level_name", "level_name"),
    ("score", "cur_score"),
  ] {
    if let Some(s) = v.str(path).filter(|s| s != "0") {
      c.extra.insert(key.into(), json!(s));
    }
  }
  if let Some(liked) = v.first_str(&["is_like", "has_concerned"]) {
    c.extra.insert("followed".into(), json!(liked == "1"));
  }
  Some(c)
}

// ── threads and floors ──────────────────────────────────────────────────

fn thread_media(t: &Value, mut media: Vec<Media>) -> Vec<Media> {
  for m in t.list("media") {
    if let Some(url) = m.first_str(&["origin_pic", "big_pic", "src_pic", "water_pic", "small_pic"])
      && !media.iter().any(|x| x.url == url)
    {
      let mut img = Media::image(url);
      img.width = m.u64("width").map(|w| w as u32).filter(|w| *w > 0);
      img.height = m.u64("height").map(|h| h as u32).filter(|h| *h > 0);
      media.push(img);
    }
  }
  let video = t.at("video_info");
  if let Some(url) = video.str("video_url") {
    let mut m = Media::video(url);
    m.duration = video.f64("video_duration");
    m.width = video.u64("video_width").map(|w| w as u32);
    m.height = video.u64("video_height").map(|h| h as u32);
    m.alt = video.str("thumbnail_url");
    media.push(m);
  }
  media
}

/// A thread of a forum page, search, topic or user listing.
pub fn thread(t: &Value, users: &Users) -> Post {
  let tid = t.first_str(&["tid", "thread_id", "id"]).unwrap_or_default();
  let b = thread_body(t);
  let mut extra = social_core::Extra::new();
  let forum = t
    .first_str(&["forum_name", "fname", "forum_info.forum_name", "forum.name"])
    .or_else(|| t.str("forum_info.name"));
  if let Some(f) = forum {
    extra.insert("forum".into(), json!(f));
  }
  if let Some(fid) = t.first_str(&["fid", "forum_id"]).filter(|f| f != "0") {
    extra.insert("forum_id".into(), json!(fid));
  }
  for key in ["is_top", "is_good"] {
    if t.str(key).is_some_and(|s| s != "0") {
      extra.insert(key.trim_start_matches("is_").into(), json!(true));
    }
  }
  if let Some(pid) = t.first_str(&["first_post_id", "post_id"]) {
    extra.insert("first_post_id".into(), json!(pid));
  }
  Post {
    kind: "thread".into(),
    title: t.str("title"),
    text: Some(b.text).filter(|s| !s.is_empty()),
    url: Some(thread_url(&tid)),
    author: author(t, users),
    created_at: secs(t, &["create_time", "time"]),
    updated_at: secs(t, &["last_time_int", "modified_time"]),
    metrics: Metrics {
      views: count(t, &["view_num"]),
      likes: count(t, &["agree_num", "agree.agree_num", "like_num"]),
      comments: count(t, &["reply_num", "post_num"]),
      shares: count(t, &["share_num"]),
      ..Metrics::default()
    },
    media: thread_media(t, b.media),
    extra,
    raw: Some(t.clone()),
    id: tid,
    ..Post::default()
  }
}

/// A floor (楼) of a thread, with the replies inlined under it.
pub fn floor(p: &Value, users: &Users, tid: &str) -> Comment {
  let pid = p.str("id").unwrap_or_default();
  let b = body(p.list("content"));
  let mut extra = social_core::Extra::new();
  if let Some(n) = p.u64("floor") {
    extra.insert("floor".into(), json!(n));
  }
  extra.insert("url".into(), json!(floor_url(tid, &pid)));
  if !b.media.is_empty() {
    extra.insert(
      "images".into(),
      json!(b.media.iter().map(|m| m.url.clone()).collect::<Vec<_>>()),
    );
  }
  let author = author(p, users);
  Comment {
    location: author.as_ref().and_then(|a| a.location.clone()),
    author,
    text: b.text,
    created_at: secs(p, &["time"]),
    likes: count(p, &["agree.agree_num", "agree_num"]),
    reply_count: count(p, &["sub_post_number"]),
    reply_to: b.reply_to,
    replies: p
      .list("sub_post_list.sub_post_list")
      .iter()
      .map(|s| floor(s, users, tid))
      .collect(),
    extra,
    raw: Some(p.clone()),
    id: pid,
  }
}

/// A hot topic (热议) as a post; its threads come from `topic`.
pub fn topic(t: &Value) -> Option<Post> {
  let id = t.str("topic_id")?;
  let mut media = Vec::new();
  if let Some(pic) = t.str("topic_pic") {
    media.push(Media::image(pic));
  }
  let mut extra = social_core::Extra::new();
  if let Some(rank) = t.u64("idx_num") {
    extra.insert("rank".into(), json!(rank));
  }
  Some(Post {
    kind: "topic".into(),
    title: t.str("topic_name"),
    text: t.first_str(&["topic_desc", "abstract"]),
    url: t.str("topic_url").map(|u| u.replace("&amp;", "&")),
    created_at: secs(t, &["create_time"]),
    metrics: Metrics {
      comments: count(t, &["discuss_num"]),
      ..Metrics::default()
    },
    media,
    extra,
    raw: Some(t.clone()),
    id,
    ..Post::default()
  })
}

/// A reply / @ / agree message of the inbox.
pub fn notification(m: &Value, kind: &str) -> Notification {
  let tid = m.str("thread_id").unwrap_or_default();
  let pid = m.str("post_id").unwrap_or_default();
  Notification {
    id: if pid.is_empty() {
      tid.clone()
    } else {
      pid.clone()
    },
    kind: kind.to_owned(),
    text: m.first_str(&["content", "title"]).unwrap_or_default(),
    actor: ["replyer", "user", "agree_user"]
      .iter()
      .find_map(|k| user(m.at(k))),
    target: m.first_str(&["quote_content", "title", "fname"]),
    url: (!tid.is_empty()).then(|| {
      if pid.is_empty() {
        thread_url(&tid)
      } else {
        floor_url(&tid, &pid)
      }
    }),
    created_at: secs(m, &["time"]),
    unread: m.str("unread").map(|u| u == "1"),
    raw: Some(m.clone()),
  }
}
