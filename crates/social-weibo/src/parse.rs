//! Upstream JSON → core models. Posts come in two shapes: weibo.com's
//! (`text_raw`, `pic_infos`, `mix_media_info`) and m.weibo.cn's (`text` as
//! HTML, `pics`); both carry `page_info` for videos and `retweeted_status`
//! for reposts.

use social_core::text::parse_time;
use social_core::{
  Collection, Comment, Media, MediaKind, Metrics, Notification, Post, User, UserStats, Value,
  ValueExt,
};

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
  let mut u = User {
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
  let extra = &mut u.extra;
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
  Some(u)
}

/// A post of either API.
pub fn post(v: &Value) -> Option<Post> {
  let mid = v.first_str(&["idstr", "mid", "id"])?;
  let author = user(v.at("user"));
  let bid = v
    .first_str(&["mblogid", "bid"])
    .or_else(|| mid_to_bid(&mid));
  let uid = author.as_ref().map(|a| a.id.clone());
  let mut p = Post {
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
  if let Some(title) = v.str("title.text").filter(|t| !t.is_empty()) {
    p.extra.insert("label".into(), title.into());
  }
  let extra = &mut p.extra;
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
  Some(p)
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
  let i = tag.find(&format!("{name}="))? + name.len() + 1;
  let v = &tag[i..];
  let (quote, v) = match v.chars().next()? {
    q @ ('"' | '\'') => (q, &v[1..]),
    _ => (' ', v),
  };
  Some(v[..v.find(quote).unwrap_or(v.len())].to_owned())
}

fn unescape(s: &str) -> String {
  s.replace("&lt;", "<")
    .replace("&gt;", ">")
    .replace("&quot;", "\"")
    .replace("&#39;", "'")
    .replace("&nbsp;", " ")
    .replace("&amp;", "&")
}

fn metrics(v: &Value) -> Metrics {
  let mut m = Metrics {
    likes: v.count("attitudes_count"),
    comments: v.count("comments_count"),
    shares: v.count("reposts_count"),
    views: v
      .first_count(&["reads_count", "page_info.play_count"])
      .or_else(|| v.count("page_info.media_info.online_users_number")),
    ..Metrics::default()
  };
  if let Some(n) = v.count("favorites_count").filter(|n| *n > 0) {
    m.favorites = Some(n);
  }
  m
}

fn topics(v: &Value) -> Vec<String> {
  v.list("topic_struct")
    .iter()
    .filter_map(|t| t.str("topic_title"))
    .collect()
}

// ── media ─────────────────────────────────────────────────────────────

fn media(v: &Value) -> Vec<Media> {
  let mut out = Vec::new();
  let mix = v.list("mix_media_info.items");
  if !mix.is_empty() {
    for item in mix {
      match item.str("type").as_deref() {
        Some("video") => out.extend(video(item.at("data"))),
        _ => out.extend(picture(item.at("data"))),
      }
    }
    return out;
  }
  let infos = v.at("pic_infos");
  if infos.is_object() {
    let ids: Vec<String> = v
      .list("pic_ids")
      .iter()
      .filter_map(|p| p.as_str().map(str::to_owned))
      .collect();
    let order: Vec<String> = if ids.is_empty() {
      infos
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
    } else {
      ids
    };
    for id in order {
      out.extend(picture(infos.at(&id)));
    }
  } else {
    for p in v.list("pics") {
      out.extend(mobile_picture(p));
    }
  }
  if v.str("page_info.object_type").as_deref() == Some("video")
    || v.at("page_info.media_info").is_object()
  {
    out.extend(video(v.at("page_info")));
  }
  out
}

/// A web picture (`pic_infos` entry): the largest rendition; live photos and
/// GIFs also bring their motion part.
fn picture(p: &Value) -> Vec<Media> {
  let Some(url) = p.first_str(&["largest.url", "mw2000.url", "original.url", "large.url"]) else {
    return Vec::new();
  };
  let kind = p.str("type").unwrap_or_default();
  let mut image = Media::image(url);
  image.width = p
    .first_str(&["largest.width", "original.width"])
    .and_then(|w| w.parse().ok());
  image.height = p
    .first_str(&["largest.height", "original.height"])
    .and_then(|h| h.parse().ok());
  let mut out = vec![image];
  if let Some(motion) = p.str("video").filter(|u| u.starts_with("http")) {
    let kind = if kind == "gif" {
      MediaKind::Gif
    } else {
      MediaKind::Video
    };
    out.push(Media::new(kind, motion));
  }
  out
}

fn mobile_picture(p: &Value) -> Vec<Media> {
  let Some(url) = p.first_str(&["large.url", "url"]) else {
    return Vec::new();
  };
  let mut out = vec![Media::image(url)];
  if let Some(motion) = p.str("videoSrc").filter(|u| u.starts_with("http")) {
    out.push(Media::video(motion));
  }
  out
}

/// A video `page_info`: the best stream of `playback_list`, else the best
/// named rendition.
fn video(info: &Value) -> Vec<Media> {
  let mi = info.at("media_info");
  let best = mi
    .list("playback_list")
    .iter()
    .filter(|e| e.str("play_info.url").is_some())
    .max_by_key(|e| {
      (
        e.i64("meta.quality_index").unwrap_or(0),
        e.i64("play_info.bitrate").unwrap_or(0),
      )
    });
  let mut m = match best {
    Some(e) => {
      let mut m = Media::video(e.str("play_info.url").unwrap_or_default());
      m.width = e.u64("play_info.width").map(|w| w as u32);
      m.height = e.u64("play_info.height").map(|h| h as u32);
      m.size = e.u64("play_info.size");
      m
    }
    None => {
      let Some(url) = mi
        .first_str(&[
          "mp4_1080p_mp4",
          "mp4_720p_mp4",
          "mp4_hd_url",
          "stream_url_hd",
          "mp4_sd_url",
          "stream_url",
        ])
        .or_else(|| {
          info.first_str(&[
            "urls.mp4_1080p_mp4",
            "urls.mp4_720p_mp4",
            "urls.mp4_hd_mp4",
            "urls.mp4_ld_mp4",
          ])
        })
      else {
        return Vec::new();
      };
      Media::video(url)
    }
  };
  m.duration = mi.f64("duration");
  m.alt = info.first_str(&["title", "page_title", "content1"]);
  vec![m]
}

// ── comments ──────────────────────────────────────────────────────────

pub fn comment(v: &Value) -> Option<Comment> {
  let id = v.first_str(&["idstr", "id"])?;
  let text = v
    .str("text_raw")
    .unwrap_or_else(|| strip_html(&v.str("text").unwrap_or_default()));
  let mut c = Comment {
    author: user(v.at("user")),
    text,
    created_at: v.str("created_at").and_then(|t| parse_time(&t)),
    likes: v.first_count(&["like_counts", "like_count"]),
    reply_count: v.count("total_number").filter(|n| *n > 0),
    reply_to: v.str("reply_comment.user.screen_name"),
    location: v
      .str("source")
      .map(|s| s.trim_start_matches("来自").trim().to_owned())
      .filter(|s| !s.is_empty()),
    replies: v.list("comments").iter().filter_map(comment).collect(),
    raw: Some(v.clone()),
    id,
    ..Comment::default()
  };
  if let Some(pic) = v.str("pic.large.url").or_else(|| v.str("pic.url")) {
    c.extra.insert("image".into(), pic.into());
  }
  if v.bool("isLikedByMblogAuthor") == Some(true) {
    c.extra.insert("liked_by_author".into(), true.into());
  }
  if let Some(floor) = v.i64("floor_number") {
    c.extra.insert("floor".into(), floor.into());
  }
  Some(c)
}

// ── hot lists ─────────────────────────────────────────────────────────

/// A hot-list entry (热搜) as a post of kind `trend`; `None` for promotions.
pub fn trend(v: &Value, rank: usize) -> Option<Post> {
  let word = v.first_str(&["word", "note"])?;
  if v.i64("is_ad") == Some(1) || v.str("promotion").is_some() {
    return None;
  }
  let query = v
    .str("word_scheme")
    .filter(|w| w.starts_with('#'))
    .unwrap_or_else(|| word.clone());
  let url = format!(
    "https://s.weibo.com/weibo?q={}",
    url::form_urlencoded::byte_serialize(query.as_bytes()).collect::<String>()
  );
  let mut p = Post {
    id: v
      .first_str(&["mid", "word"])
      .unwrap_or_else(|| word.clone()),
    kind: "trend".into(),
    title: Some(word.clone()),
    text: v.str("note").filter(|n| n != &word),
    url: Some(url),
    created_at: v.i64("onboard_time").and_then(social_core::text::from_secs),
    metrics: Metrics {
      views: v.first_count(&["num", "raw_hot"]),
      ..Metrics::default()
    },
    tags: v.str("category").into_iter().collect(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  let extra = &mut p.extra;
  extra.insert(
    "rank".into(),
    v.i64("realpos").unwrap_or(rank as i64 + 1).into(),
  );
  if let Some(label) = v
    .first_str(&["label_name", "icon_desc"])
    .filter(|l| !l.is_empty())
  {
    extra.insert("label".into(), label.into());
  }
  Some(p)
}

/// A topic (话题) search result card.
pub fn topic(v: &Value) -> Option<Collection> {
  let title = v.str("title_sub")?;
  let name = title.trim_matches('#').to_owned();
  let stats = v.str("desc2").unwrap_or_default();
  let mut c = Collection {
    id: name.clone(),
    kind: "topic".into(),
    description: v.str("desc1").filter(|d| !d.is_empty()),
    url: Some(format!(
      "https://s.weibo.com/weibo?q={}",
      url::form_urlencoded::byte_serialize(format!("#{name}#").as_bytes()).collect::<String>()
    )),
    raw: Some(v.clone()),
    name,
    ..Collection::default()
  };
  // "324.5万讨论 48.2亿阅读"
  for part in stats.split_whitespace() {
    let (num, label) = part.split_at(
      part
        .char_indices()
        .find(|(_, ch)| !(ch.is_ascii_digit() || *ch == '.' || *ch == '万' || *ch == '亿'))
        .map_or(part.len(), |(i, _)| i),
    );
    let n = social_core::text::parse_count(num);
    match label {
      "讨论" => c.items = n,
      "阅读" => c.views = n,
      _ => {}
    }
  }
  Some(c)
}

// ── inbox ─────────────────────────────────────────────────────────────

/// A message-box entry: a post or comment that mentions you, a comment you
/// received or sent, or a like.
pub fn notification(v: &Value, kind: &str) -> Option<Notification> {
  let id = v.first_str(&["idstr", "id"])?;
  let actor = user(v.at("user"));
  let (text, target, url) = match kind {
    "mention" => {
      let p = post(v)?;
      let target = p.quoted.as_ref().and_then(|q| q.text.clone());
      (p.text.clone().unwrap_or_default(), target, p.url.clone())
    }
    "like" => {
      let status = v.at("status");
      let target = post(status).and_then(|p| p.text);
      let url = post(status).and_then(|p| p.url);
      (
        v.str("attitude_type_text")
          .unwrap_or_else(|| "liked".into()),
        target,
        url,
      )
    }
    _ => {
      let status = v.at("status");
      let target = v
        .str("reply_comment.text_raw")
        .or_else(|| post(status).and_then(|p| p.text));
      (
        v.str("text_raw")
          .unwrap_or_else(|| strip_html(&v.str("text").unwrap_or_default())),
        target,
        post(status).and_then(|p| p.url),
      )
    }
  };
  Some(Notification {
    id,
    kind: kind.into(),
    text,
    actor,
    target: target.map(|t| social_core::text::truncate(&t, 120)),
    url,
    created_at: v.str("created_at").and_then(|t| parse_time(&t)),
    unread: None,
    raw: Some(v.clone()),
  })
}

/// All posts inside m.weibo.cn cards, including those nested in card groups.
pub fn card_posts(cards: &[Value]) -> Vec<Post> {
  let mut out = Vec::new();
  for card in cards {
    if card.i64("card_type") == Some(9) {
      out.extend(post(card.at("mblog")));
    }
    out.extend(card_posts(card.list("card_group")));
  }
  out
}

/// All users inside m.weibo.cn cards.
pub fn card_users(cards: &[Value]) -> Vec<User> {
  let mut out = Vec::new();
  for card in cards {
    if card.at("user").is_object() && card.i64("card_type") != Some(9) {
      out.extend(user(card.at("user")));
    }
    out.extend(card_users(card.list("card_group")));
  }
  out
}

pub fn card_topics(cards: &[Value]) -> Vec<Collection> {
  let mut out = Vec::new();
  for card in cards {
    if card.i64("card_type") == Some(8) {
      out.extend(topic(card));
    }
    out.extend(card_topics(card.list("card_group")));
  }
  out
}
