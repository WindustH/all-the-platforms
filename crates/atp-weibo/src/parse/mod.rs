//! Upstream JSON → core models. Posts come in two shapes: weibo.com's
//! (`text_raw`, `pic_infos`, `mix_media_info`) and m.weibo.cn's (`text` as
//! HTML, `pics`); both carry `page_info` for videos and `retweeted_status`
//! for reposts.

mod cards;
mod comments;
mod hot;
mod media;
mod notifications;

use atp_core::text::parse_time;
use atp_core::{
  Collection, Comment, Extra, Media, MediaKind, Metrics, Notification, Post, User, UserStats,
  Value, ValueExt,
};

pub use self::cards::{card_posts, card_topics, card_users};
pub use self::comments::comment;
pub use self::hot::{topic, trend};
use self::media::media;
pub use self::notifications::notification;
use crate::refs::{mid_to_bid, post_url, user_url};

/// A user object of either API; `None` without an id.
pub fn user(v: &Value) -> Option<User> {
  let id = v.first_str(&["idstr", "id"])?;
  let name = v.first_str(&["screen_name", "name"]).unwrap_or_default();
  let mut stats = UserStats {
    followers: v.first_count(&["followers_count", "followers_count_str"]),
    following: v.first_count(&["friends_count", "follow_count"]),
    posts: v.count("statuses_count"),
    ..UserStats::default()
  };
  if let Some(n) = v.count("status_total_counter.like_cnt") {
    stats.likes = Some(n);
  }
  for (key, path) in [
    ("reposts_received", "status_total_counter.repost_cnt"),
    ("comments_received", "status_total_counter.comment_cnt"),
    ("interactions", "status_total_counter.total_cnt"),
  ] {
    if let Some(n) = v.count(path) {
      stats.other.insert(key.into(), n);
    }
  }
  let mut user = User {
    url: Some(user_url(&id)),
    handle: v.str("domain").filter(|d| !d.is_empty()),
    avatar: v.first_str(&["avatar_hd", "avatar_large", "profile_image_url"]),
    bio: v.str("description").filter(|d| !d.is_empty()),
    verified: v.bool("verified").unwrap_or(false),
    location: v.str("location").filter(|l| !l.is_empty()),
    stats,
    followed: v.bool("following"),
    created_at: v.str("created_at").and_then(|t| parse_time(&t)),
    raw: Some(v.clone()),
    id,
    name,
    ..User::default()
  };
  let extra = &mut user.extra;
  for (key, path) in [
    ("verified_reason", "verified_reason"),
    ("gender", "gender"),
    ("ip_location", "ip_location"),
    ("follows_you", "follow_me"),
    ("verified_type", "verified_type"),
    ("member_rank", "mbrank"),
  ] {
    let value = v.at(path);
    if !value.is_null() && value != &Value::from("") {
      extra.insert(key.into(), value.clone());
    }
  }
  Some(user)
}

/// A post of either API.
pub fn post(v: &Value) -> Option<Post> {
  let mid = v.first_str(&["idstr", "mid", "id"])?;
  let author = user(v.at("user"));
  let bid = v
    .first_str(&["mblogid", "bid"])
    .or_else(|| mid_to_bid(&mid));
  let uid = author.as_ref().map(|a| a.id.clone());
  let mut post = Post {
    kind: if v.at("retweeted_status").is_object() {
      "repost".into()
    } else {
      "post".into()
    },
    text: Some(text(v)),
    url: Some(post_url(uid.as_deref(), &mid, bid.as_deref())),
    created_at: v.str("created_at").and_then(|t| parse_time(&t)),
    updated_at: v.str("edit_at").and_then(|t| parse_time(&t)),
    metrics: metrics(v),
    media: media(v),
    tags: topics(v),
    quoted: v
      .at("retweeted_status")
      .is_object()
      .then(|| post(v.at("retweeted_status")).map(Box::new))
      .flatten(),
    author,
    raw: Some(v.clone()),
    id: mid,
    ..Post::default()
  };
  post.extra = post_extra(v, bid);
  Some(post)
}

/// What a post carries beyond the shared fields: label, base62 id, client,
/// region, pinned / truncated / edited flags and your like and favorite.
fn post_extra(v: &Value, bid: Option<String>) -> Extra {
  let mut extra = Extra::new();
  if let Some(title) = v.str("title.text").filter(|t| !t.is_empty()) {
    extra.insert("label".into(), title.into());
  }
  if let Some(bid) = bid {
    extra.insert("bid".into(), bid.into());
  }
  if let Some(src) = v
    .str("source")
    .map(|s| strip_html(&s))
    .filter(|s| !s.is_empty())
  {
    extra.insert("source".into(), src.into());
  }
  if let Some(region) = v.str("region_name") {
    let region = region.trim_start_matches("发布于").trim().to_owned();
    if !region.is_empty() {
      extra.insert("region".into(), region.into());
    }
  }
  if v.i64("isTop") == Some(1) || v.str("title.text").as_deref() == Some("置顶") {
    extra.insert("pinned".into(), true.into());
  }
  if v.bool("isLongText") == Some(true) && !has_long_text(v) {
    extra.insert("truncated".into(), true.into());
  }
  if let Some(n) = v.i64("edit_count").filter(|n| *n > 0) {
    extra.insert("edits".into(), n.into());
  }
  if let Some(liked) = v.i64("attitudes_status") {
    extra.insert("liked".into(), (liked == 1).into());
  }
  if let Some(fav) = v.bool("favorited") {
    extra.insert("favorited".into(), fav.into());
  }
  extra
}

fn has_long_text(v: &Value) -> bool {
  v.str("longText.longTextContent").is_some()
}

/// Full text: the long text when present, the plain web text, or the mobile HTML.
fn text(v: &Value) -> String {
  if let Some(long) = v.str("longText.longTextContent") {
    return long.trim().to_owned();
  }
  if let Some(raw) = v.str("text_raw") {
    return raw.trim().trim_end_matches('\u{200b}').trim().to_owned();
  }
  let text = strip_html(&v.str("text").unwrap_or_default());
  // Truncated listings end with a "全文" (full text) link.
  match text.strip_suffix("全文") {
    Some(cut) if v.bool("isLongText") == Some(true) => cut.trim_end().to_owned(),
    _ => text,
  }
}

/// m.weibo.cn's HTML text as Weibo shows it: links as their text, emoji
/// images as their `[name]`, `<br />` as line breaks.
pub fn strip_html(html: &str) -> String {
  let mut out = String::with_capacity(html.len());
  let mut rest = html;
  while let Some(i) = rest.find('<') {
    out.push_str(&rest[..i]);
    let Some(j) = rest[i..].find('>') else {
      out.push_str(&rest[i..]);
      rest = "";
      break;
    };
    let tag = &rest[i + 1..i + j];
    let name = tag
      .trim_start_matches('/')
      .split(|c: char| c.is_whitespace() || c == '/')
      .next()
      .unwrap_or_default()
      .to_ascii_lowercase();
    match name.as_str() {
      "br" => out.push('\n'),
      "img" => {
        if let Some(alt) = attr(tag, "alt") {
          out.push_str(&alt);
        }
      }
      _ => {}
    }
    rest = &rest[i + j + 1..];
  }
  out.push_str(rest);
  unescape(&out).trim().to_owned()
}

fn attr(tag: &str, name: &str) -> Option<String> {
  let at = tag.find(&format!("{name}="))? + name.len() + 1;
  let v = &tag[at..];
  let (quote, v) = match v.chars().next()? {
    q @ ('"' | '\'') => (q, &v[1..]),
    _ => (' ', v),
  };
  Some(v[..v.find(quote).unwrap_or(v.len())].to_owned())
}

fn unescape(text: &str) -> String {
  text
    .replace("&lt;", "<")
    .replace("&gt;", ">")
    .replace("&quot;", "\"")
    .replace("&#39;", "'")
    .replace("&nbsp;", " ")
    .replace("&amp;", "&")
}

fn metrics(v: &Value) -> Metrics {
  let mut metrics = Metrics {
    likes: v.count("attitudes_count"),
    comments: v.count("comments_count"),
    shares: v.count("reposts_count"),
    views: v
      .first_count(&["reads_count", "page_info.play_count"])
      .or_else(|| v.count("page_info.media_info.online_users_number")),
    ..Metrics::default()
  };
  if let Some(n) = v.count("favorites_count").filter(|n| *n > 0) {
    metrics.favorites = Some(n);
  }
  metrics
}

fn topics(v: &Value) -> Vec<String> {
  v.list("topic_struct")
    .iter()
    .filter_map(|t| t.str("topic_title"))
    .collect()
}
