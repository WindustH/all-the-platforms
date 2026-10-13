//! Users: profiles, their threads and replies, fans and follows, and the
//! forums an account follows.

use social_core::{Collection, Error, Page, PageReq, Post, Result, User, Value, ValueExt, json};

use crate::api::{Api, LEGACY, Params};
use crate::parse;
use crate::refs::{self, UserRef};

/// The portrait of an exact user-name match in user search.
async fn portrait_by_name(api: &Api, name: &str) -> Result<String> {
  let v = api
    .web("/mo/q/search/user", &[("word", name.to_owned())])
    .await?;
  let exact = v.at("data.exactMatch");
  let same = [
    exact.str("name"),
    exact.str("show_nickname"),
    exact.str("user_nickname"),
  ]
  .iter()
  .flatten()
  .any(|n| n == name);
  match exact.first_str(&["encry_uid", "portrait"]) {
    Some(p) if same => Ok(refs::clean_portrait(&p)),
    _ => Err(Error::not_found(format!("no Tieba user named {name}")).with_hint(
      "pass the uid, the portrait (tb.1.…) or a home/main?id= link; `social tieba search -t user NAME` finds them",
    )),
  }
}

/// The client profile of a user (uid, portrait or name).
async fn profile(api: &Api, arg: &str) -> Result<Value> {
  let mut params: Params = match refs::user(arg)? {
    UserRef::Uid(uid) => vec![("uid", uid)],
    UserRef::Portrait(p) => vec![("friend_uid_portrait", p)],
    UserRef::Name(n) => vec![("friend_uid_portrait", portrait_by_name(api, &n).await?)],
  };
  params.push(("need_post_count", "1".into()));
  params.push(("pn", "1".into()));
  let v = api.call("/c/u/user/profile", params).await?;
  let Some(uid) = v.str("user.id").filter(|u| u != "0") else {
    return Err(Error::not_found(format!("no Tieba user {arg}")));
  };
  api.cache_uid(arg, &uid);
  Ok(v)
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let v = profile(api, arg).await?;
  parse::user(v.at("user")).ok_or_else(|| Error::upstream("empty Tieba profile"))
}

pub async fn whoami(api: &Api) -> Result<User> {
  let me = api.me().await?;
  let mut user = user(api, &me.uid).await?;
  if user.name.is_empty() {
    user.name = me.name;
  }
  Ok(user)
}

/// The uid of a user argument (numeric ids are taken as they are).
pub async fn uid(api: &Api, arg: &str) -> Result<String> {
  if let UserRef::Uid(uid) = refs::user(arg)? {
    return Ok(uid);
  }
  if let Some(uid) = api.cached_uid(arg) {
    return Ok(uid);
  }
  let v = profile(api, arg).await?;
  v.str("user.id")
    .ok_or_else(|| Error::not_found(format!("no Tieba user {arg}")))
}

/// The portrait of a user argument (follow and unfollow take it).
pub async fn portrait(api: &Api, arg: &str) -> Result<String> {
  if let UserRef::Portrait(p) = refs::user(arg)? {
    return Ok(p);
  }
  let v = profile(api, arg).await?;
  v.str("user.portrait")
    .map(|p| refs::clean_portrait(&p))
    .ok_or_else(|| Error::not_found(format!("no portrait for Tieba user {arg}")))
}

/// One page of a user's threads, or of their replies.
pub async fn posts(api: &Api, arg: &str, threads: bool, req: &PageReq) -> Result<Page<Post>> {
  let uid = uid(api, arg).await?;
  let pn = req.number_or(1);
  let rn = 20;
  let params: Params = vec![
    ("uid", uid),
    ("pn", pn.to_string()),
    ("rn", rn.to_string()),
    ("is_thread", if threads { "1" } else { "0" }.into()),
    ("need_content", "1".into()),
  ];
  let v = api.call_as("/c/u/feed/userpost", params, LEGACY).await?;
  let none = parse::Users::new();
  let list = v.list("post_list");
  let mut items = Vec::new();
  for item in list {
    if threads {
      items.push(parse::thread(item, &none));
      continue;
    }
    // A reply listing groups the user's replies by thread.
    for reply in item.list("content") {
      items.push(reply_post(item, reply));
    }
  }
  let more = list.len() >= rn;
  Ok(Page::new(items, more.then(|| (pn + 1).to_string())))
}

fn reply_post(thread: &Value, reply: &Value) -> Post {
  let tid = thread.str("thread_id").unwrap_or_default();
  let pid = reply.str("post_id").unwrap_or_default();
  let body = parse::body(reply.list("post_content"));
  let mut post = Post {
    kind: "reply".into(),
    text: Some(body.text).filter(|t| !t.is_empty()),
    url: Some(refs::floor_url(&tid, &pid)),
    created_at: reply
      .i64("create_time")
      .and_then(social_core::text::from_secs),
    media: body.media,
    raw: Some(reply.clone()),
    id: pid,
    ..Post::default()
  };
  if let Some(f) = thread.str("forum_name") {
    post.extra.insert("forum".into(), json!(f));
  }
  post
    .extra
    .insert("thread".into(), json!(refs::thread_url(&tid)));
  if let Some(title) = thread.str("title") {
    let title = title.strip_prefix("回复：").unwrap_or(&title).to_owned();
    post.extra.insert("thread_title".into(), json!(title));
  }
  post
}

/// Fans or follows of a user.
pub async fn relations(api: &Api, arg: &str, fans: bool, req: &PageReq) -> Result<Page<User>> {
  let uid = uid(api, arg).await?;
  let pn = req.number_or(1);
  let params: Params = vec![
    ("uid", uid),
    ("pn", pn.to_string()),
    ("rn", req.size_within(50).to_string()),
  ];
  let path = if fans {
    "/c/u/fans/page"
  } else {
    "/c/u/follow/page"
  };
  // The follow list comes with "not logged in" for visitors, and the list anyway.
  let v = api.call_raw(path, params, LEGACY).await?;
  let list = ["user_list", "follow_list"]
    .iter()
    .map(|k| v.list(k))
    .find(|l| !l.is_empty())
    .unwrap_or_default();
  if list.is_empty() {
    crate::api::check(v.clone())?;
  }
  let items: Vec<User> = list.iter().filter_map(parse::user).collect();
  let more = v.str("page.has_more").as_deref() == Some("1") && !items.is_empty();
  Ok(Page::new(items, more.then(|| (pn + 1).to_string())))
}

/// Forums a user follows (yours when `user` is `None`); needs a login.
pub async fn forums(api: &Api, user: Option<&str>, req: &PageReq) -> Result<Page<Collection>> {
  let me = api.me().await?;
  let target = match user {
    Some(u) => uid(api, u).await?,
    None => me.uid.clone(),
  };
  let pn = req.number_or(1);
  let mut params: Params = vec![
    ("page_no", pn.to_string()),
    ("page_size", "50".into()),
    ("uid", me.uid.clone()),
  ];
  if target != me.uid {
    params.push(("friend_uid", target));
    params.push(("is_guest", "1".into()));
  }
  let v = api.call_as("/c/f/forum/like", params, LEGACY).await?;
  let items: Vec<Collection> = [
    "forum_list.non-gconforum",
    "forum_list.gconforum",
    "common_forum_list.non-gconforum",
  ]
  .iter()
  .flat_map(|k| v.list(k))
  .filter_map(parse::forum)
  .collect();
  let more = v.str("has_more").as_deref() == Some("1") && !items.is_empty();
  Ok(Page::new(items, more.then(|| (pn + 1).to_string())))
}
