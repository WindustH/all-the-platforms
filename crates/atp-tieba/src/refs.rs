//! What users pass in, and the canonical links we hand out.
//!
//! - Threads: the id (`10083467821`), `tieba.baidu.com/p/<id>` links (with
//!   `?pid=<floor>` or `#<floor>` for one floor), `?kz=` / `?tid=` links, or
//!   `<thread>/<floor>`.
//! - Users: the numeric uid, the portrait (`tb.1.xxxx`), `home/main?id=` /
//!   `?un=` links, or a user name (`@name` or plain).
//! - Forums (吧): the name (`rust`), or a `/f?kw=` link.
//! - Hot topics: the id, or a `hottopic?topic_id=…&topic_name=…` link.

use atp_core::{Error, Result};
use url::Url;

use crate::api::WEB;

pub fn thread_url(tid: &str) -> String {
  format!("{WEB}/p/{tid}")
}

pub fn floor_url(tid: &str, pid: &str) -> String {
  format!("{WEB}/p/{tid}?pid={pid}#{pid}")
}

pub fn forum_url(name: &str) -> String {
  let mut url = Url::parse(&format!("{WEB}/f")).expect("static url");
  url.query_pairs_mut().append_pair("kw", name);
  url.into()
}

pub fn user_url(portrait: &str) -> String {
  format!("{WEB}/home/main?id={portrait}")
}

pub fn avatar_url(portrait: &str) -> String {
  format!("https://gss0.bdstatic.com/6LZ1dD3d1sgCo2Kml5_Y_D3/sys/portrait/item/{portrait}")
}

/// `tb.1.xxx` out of a bare portrait, a `?t=` stamped one or an avatar URL.
pub fn clean_portrait(text: &str) -> String {
  let last = text.rsplit('/').next().unwrap_or(text);
  let bare = last.split(['?', '&', '#']).next().unwrap_or(last);
  bare.trim_end_matches(".jpg").to_owned()
}

fn is_id(text: &str) -> bool {
  (4..=20).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit())
}

/// A Baidu URL, also without its scheme.
fn parse_url(arg: &str) -> Option<Url> {
  let url = if arg.contains("://") {
    Url::parse(arg).ok()?
  } else if arg.contains('.') && arg.contains('/') {
    Url::parse(&format!("https://{arg}")).ok()?
  } else {
    return None;
  };
  let host = url.host_str()?;
  ["baidu.com", "bdimg.com"]
    .iter()
    .any(|d| host == *d || host.ends_with(&format!(".{d}")))
    .then_some(url)
}

fn query(url: &Url, keys: &[&str]) -> Option<String> {
  url
    .query_pairs()
    .find(|(k, v)| keys.contains(&k.as_ref()) && !v.is_empty())
    .map(|(_, v)| v.into_owned())
}

/// A thread, and the floor (post id) a link points at.
#[derive(Debug, Clone)]
pub struct PostRef {
  pub tid: String,
  pub pid: Option<String>,
}

pub fn post(arg: &str) -> Result<PostRef> {
  let arg = arg.trim();
  let bad = || Error::input(format!("not a Tieba thread id or link: {arg}"));
  if is_id(arg) {
    return Ok(PostRef {
      tid: arg.to_owned(),
      pid: None,
    });
  }
  if let Some((t, p)) = arg.split_once('/')
    && is_id(t)
    && is_id(p)
  {
    return Ok(PostRef {
      tid: t.to_owned(),
      pid: Some(p.to_owned()),
    });
  }
  let url = parse_url(arg).ok_or_else(bad)?;
  let segs: Vec<&str> = url.path_segments().map(|s| s.collect()).unwrap_or_default();
  let from_path = segs
    .windows(2)
    .find(|w| w[0] == "p" && is_id(w[1]))
    .map(|w| w[1].to_owned());
  let tid = from_path
    .or_else(|| query(&url, &["kz", "tid", "thread_id", "z"]))
    .filter(|t| is_id(t))
    .ok_or_else(bad)?;
  let pid = query(&url, &["pid", "post_id"])
    .or_else(|| url.fragment().map(str::to_owned))
    .filter(|p| is_id(p) && *p != tid);
  Ok(PostRef { tid, pid })
}

/// Who a user argument names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserRef {
  Uid(String),
  Portrait(String),
  Name(String),
}

pub fn user(arg: &str) -> Result<UserRef> {
  let arg = arg.trim();
  if arg.is_empty() {
    return Err(Error::input("empty user"));
  }
  if let Some(url) = parse_url(arg) {
    if let Some(id) = query(&url, &["id", "portrait"]) {
      return Ok(classify(&id));
    }
    if let Some(name) = query(&url, &["un", "uname"]) {
      return Ok(UserRef::Name(name));
    }
    return Err(Error::input(format!("not a Tieba user link: {arg}")));
  }
  Ok(match arg.strip_prefix('@') {
    Some(name) => UserRef::Name(name.to_owned()),
    None => classify(arg),
  })
}

fn classify(text: &str) -> UserRef {
  if text.bytes().all(|b| b.is_ascii_digit()) {
    UserRef::Uid(text.to_owned())
  } else if text.starts_with("tb.") {
    UserRef::Portrait(clean_portrait(text))
  } else {
    UserRef::Name(text.to_owned())
  }
}

/// A forum (吧) name.
pub fn forum(arg: &str) -> Result<String> {
  let arg = arg.trim();
  let name = match parse_url(arg) {
    Some(url) => query(&url, &["kw", "word", "fname"])
      .ok_or_else(|| Error::input(format!("not a Tieba forum link: {arg}")))?,
    None => arg.to_owned(),
  };
  if name.is_empty() {
    return Err(Error::input("empty forum name"));
  }
  Ok(name)
}

/// A hot topic: id, and its name when the link has one.
pub fn topic(arg: &str) -> Result<(String, String)> {
  let arg = arg.trim().replace("&amp;", "&");
  if is_id(&arg) {
    return Ok((arg, String::new()));
  }
  let url =
    parse_url(&arg).ok_or_else(|| Error::input(format!("not a Tieba topic id or link: {arg}")))?;
  let id =
    query(&url, &["topic_id"]).ok_or_else(|| Error::input(format!("no topic_id in {arg}")))?;
  Ok((id, query(&url, &["topic_name"]).unwrap_or_default()))
}
