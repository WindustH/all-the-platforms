//! Tieba JSON → shared models.
//!
//! Threads (帖子) become posts, floors (楼) comments and their 楼中楼 replies
//! nested comments; forums (吧) and followed forums are collections. Rich
//! content is a list of typed elements (text, link, emoji, image, @user,
//! video, voice) flattened into text plus media. Listings send authors
//! separately in `user_list`, keyed by `author_id`.

mod forums;
mod notifications;
mod threads;
mod users;

use crate::refs::{avatar_url, clean_portrait, floor_url, forum_url, thread_url, user_url};
use atp_core::text::{from_secs, parse_count};
use atp_core::{
  Collection, Comment, Media, MediaKind, Metrics, Notification, Post, User, UserStats, Value,
  ValueExt, json,
};
use jiff::Timestamp;
use std::collections::HashMap;

pub use self::forums::forum;
pub use self::notifications::notification;
pub use self::threads::{floor, thread, topic};
use self::users::author;
pub use self::users::user;

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

#[derive(Debug, Default)]
pub struct Body {
  pub text: String,
  pub media: Vec<Media>,
  /// Name in a leading `回复 <name> :` (replies inside a floor).
  pub reply_to: Option<String>,
}

fn image(url: String, bsize: Option<String>, size: Option<u64>) -> Media {
  let mut media = Media::image(url);
  if let Some((w, h)) = bsize.as_deref().and_then(|s| s.split_once(',')) {
    media.width = w.trim().parse().ok().filter(|w| *w > 0);
    media.height = h.trim().parse().ok().filter(|h| *h > 0);
  }
  media.size = size.filter(|s| *s > 0);
  media
}

/// Flatten content elements into text and media.
pub fn body(items: &[Value]) -> Body {
  let mut body = Body::default();
  for e in items {
    let text = e.str("text").unwrap_or_default();
    match e.i64("type").unwrap_or(0) {
      1 => {
        body.text += &text;
        if let Some(link) = e.str("link").filter(|l| *l != text) {
          body.text += &format!(" ({link})");
        }
      }
      2 | 11 => body.text += &format!("[{}]", e.first_str(&["c", "text"]).unwrap_or_default()),
      3 => {
        if let Some(url) = e.first_str(&["origin_src", "big_cdn_src", "cdn_src", "src"]) {
          body
            .media
            .push(image(url, e.str("bsize"), e.u64("origin_size")));
        }
      }
      5 => match (e.str("link"), e.str("src")) {
        (Some(link), Some(cover)) => {
          let mut media = Media::video(link);
          media.alt = Some(cover);
          body.media.push(media);
        }
        _ => body.text += &text,
      },
      10 => {
        if let Some(md5) = e.str("voice_md5") {
          let mut media = Media::new(
            MediaKind::Audio,
            format!("https://tiebac.baidu.com/c/p/voice?voice_md5={md5}&play_from=pb_voice_play"),
          );
          media.duration = e.f64("during_time").map(|ms| ms / 1000.0);
          body.media.push(media);
        }
      }
      20 => {
        if let Some(src) = e.str("src") {
          body.media.push(image(src, e.str("bsize"), None));
        }
      }
      _ if e.str("uid").is_some() && body.text.trim() == "回复" && body.reply_to.is_none() => {
        body.reply_to = Some(text.trim_start_matches('@').to_owned());
        body.text.clear();
      }
      _ => body.text += &text,
    }
  }
  if body.reply_to.is_some() {
    body.text = body
      .text
      .trim_start()
      .trim_start_matches([':', '：'])
      .to_owned();
  }
  body.text = body.text.trim().to_owned();
  body
}

/// Text and media of whichever content field a listing uses.
fn thread_body(thread: &Value) -> Body {
  for key in ["first_post_content", "abstract", "rich_abstract", "content"] {
    match thread.at(key) {
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
