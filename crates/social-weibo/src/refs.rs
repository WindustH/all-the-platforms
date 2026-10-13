//! Post and user references: weibo.com / m.weibo.cn / weibo.cn links, numeric
//! post ids (`mid`), their base62 form (`mblogid` / `bid`, as in
//! `weibo.com/<uid>/<bid>`), user ids, `@names` and personal domains.

use social_core::{Error, Result};
use url::Url;

const BASE62: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

const HOSTS: &[&str] = &[
  "weibo.com",
  "m.weibo.cn",
  "weibo.cn",
  "m.weibo.com",
  "share.api.weibo.cn",
  "service.weibo.com",
];

/// First path segments of weibo.com that are not users or posts.
const RESERVED: &[&str] = &[
  "ajax", "detail", "status", "u", "n", "p", "hot", "search", "newlogin", "login", "set",
  "messages", "at", "comment", "like", "fav", "mygroups", "tv", "upload", "profile",
];

/// Numeric post id → base62 `bid` (groups of 7 digits from the right, each
/// written as 4 base62 characters except the first).
pub fn mid_to_bid(mid: &str) -> Option<String> {
  if mid.is_empty() || !mid.bytes().all(|b| b.is_ascii_digit()) {
    return None;
  }
  let digits = mid.as_bytes();
  let mut groups = Vec::new();
  let mut end = digits.len();
  while end > 0 {
    let start = end.saturating_sub(7);
    let n: u64 = std::str::from_utf8(&digits[start..end])
      .ok()?
      .parse()
      .ok()?;
    let mut out = to_base62(n);
    if start > 0 {
      while out.len() < 4 {
        out.insert(0, '0');
      }
    }
    groups.push(out);
    end = start;
  }
  groups.reverse();
  Some(groups.concat())
}

/// Base62 `bid` → numeric post id.
pub fn bid_to_mid(bid: &str) -> Option<String> {
  if bid.is_empty() || !bid.bytes().all(|b| b.is_ascii_alphanumeric()) {
    return None;
  }
  let chars = bid.as_bytes();
  let mut groups = Vec::new();
  let mut end = chars.len();
  while end > 0 {
    let start = end.saturating_sub(4);
    let mut n: u64 = 0;
    for c in &chars[start..end] {
      n = n * 62 + BASE62.iter().position(|b| b == c)? as u64;
    }
    let mut out = n.to_string();
    if start > 0 {
      while out.len() < 7 {
        out.insert(0, '0');
      }
    }
    groups.push(out);
    end = start;
  }
  groups.reverse();
  let mid = groups.concat().trim_start_matches('0').to_owned();
  (!mid.is_empty()).then_some(mid)
}

fn to_base62(mut number: u64) -> String {
  if number == 0 {
    return "0".into();
  }
  let mut out = Vec::new();
  while number > 0 {
    out.push(BASE62[(number % 62) as usize]);
    number /= 62;
  }
  out.reverse();
  String::from_utf8(out).unwrap_or_default()
}

fn is_mid(text: &str) -> bool {
  text.len() >= 12 && text.len() <= 20 && text.bytes().all(|b| b.is_ascii_digit())
}

fn is_bid(text: &str) -> bool {
  (8..=10).contains(&text.len())
    && text.bytes().all(|b| b.is_ascii_alphanumeric())
    && text.bytes().any(|b| b.is_ascii_alphabetic())
}

fn is_uid(text: &str) -> bool {
  (5..=12).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit())
}

fn parse_url(arg: &str) -> Option<Url> {
  let url = if arg.contains("://") {
    Url::parse(arg).ok()?
  } else {
    Url::parse(&format!("https://{arg}")).ok()?
  };
  let host = url.host_str()?.trim_start_matches("www.");
  HOSTS.contains(&host).then_some(url)
}

fn segments(url: &Url) -> Vec<String> {
  url
    .path_segments()
    .map(|s| s.filter(|p| !p.is_empty()).map(str::to_owned).collect())
    .unwrap_or_default()
}

/// The numeric id of a post, from any of its forms.
pub fn mid(arg: &str) -> Result<String> {
  let arg = arg.trim();
  let found = match parse_url(arg) {
    Some(url) => post_in_url(&url),
    None => post_token(arg),
  };
  found.ok_or_else(|| {
    Error::input(format!(
      "not a Weibo post: {arg} (use a weibo.com/<uid>/<id> or m.weibo.cn link, or the post id)"
    ))
  })
}

/// A numeric id or a bid, as a numeric id.
fn post_token(text: &str) -> Option<String> {
  if is_mid(text) {
    Some(text.to_owned())
  } else if is_bid(text) {
    bid_to_mid(text)
  } else {
    None
  }
}

fn post_in_url(url: &Url) -> Option<String> {
  for key in ["weibo_id", "mblogid", "id", "mid"] {
    if let Some((_, v)) = url.query_pairs().find(|(k, _)| k == key)
      && let Some(m) = post_token(&v)
    {
      return Some(m);
    }
  }
  let segs = segments(url);
  // `/<uid>/<bid>`, `/detail/<mid>`, `/status/<bid>`, `/comment/<bid>` ...: the
  // last segment that looks like a post.
  segs.iter().rev().find_map(|s| post_token(s))
}

/// Canonical link of a post: `weibo.com/<uid>/<bid>` when the author is known.
pub fn post_url(uid: Option<&str>, mid: &str, bid: Option<&str>) -> String {
  let bid = bid.map(str::to_owned).or_else(|| mid_to_bid(mid));
  match (uid, bid) {
    (Some(uid), Some(bid)) => format!("https://weibo.com/{uid}/{bid}"),
    _ => format!("https://m.weibo.cn/detail/{mid}"),
  }
}

pub fn user_url(uid: &str) -> String {
  format!("https://weibo.com/u/{uid}")
}

/// A user as given: a numeric id, a screen name, or a personal domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserRef {
  Uid(String),
  Name(String),
  Domain(String),
}

pub fn user(arg: &str) -> Result<UserRef> {
  let arg = arg.trim();
  if let Some(url) = parse_url(arg) {
    let segs = segments(&url);
    let first = segs.first().map(String::as_str);
    let second = segs.get(1).cloned();
    let found = match (first, second) {
      (Some("u" | "profile"), Some(id)) if is_uid(&id) => Some(UserRef::Uid(id)),
      (Some("n"), Some(name)) => Some(UserRef::Name(name)),
      (Some(id), _) if is_uid(id) => Some(UserRef::Uid(id.to_owned())),
      (Some(domain), _) if !RESERVED.contains(&domain) => Some(UserRef::Domain(domain.to_owned())),
      _ => url
        .query_pairs()
        .find(|(k, _)| k == "uid")
        .map(|(_, v)| UserRef::Uid(v.into_owned())),
    };
    return found.ok_or_else(|| Error::input(format!("not a Weibo user link: {arg}")));
  }
  if let Some(name) = arg.strip_prefix('@') {
    return Ok(UserRef::Name(name.to_owned()));
  }
  if is_uid(arg) {
    return Ok(UserRef::Uid(arg.to_owned()));
  }
  if arg.is_empty() {
    return Err(Error::input("empty user"));
  }
  Ok(UserRef::Name(arg.to_owned()))
}
