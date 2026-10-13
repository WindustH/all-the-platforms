//! Followers, tags and the blacklist (official API, verified accounts).
//!
//! `user/get` hands out up to 10000 openids per call; profiles come from
//! `user/info/batchget`, 100 at a time, and carry no nickname or avatar
//! since 2021 (follow time, scene, remark and tags remain). A page cursor is
//! `START_OPENID:OFFSET` into one `user/get` batch.

use social_core::text::from_secs;
use social_core::{Collection, Page, PageReq, Result, User, Value, ValueExt, json};

use crate::client::Client;

pub async fn followers(c: &Client, req: &PageReq) -> Result<Page<User>> {
  let (start, offset) = match req.cursor.as_deref().and_then(|c| c.rsplit_once(':')) {
    Some((s, o)) => (s.to_owned(), o.parse::<usize>().unwrap_or(0)),
    None => (String::new(), 0),
  };
  let v = c
    .api_get("/cgi-bin/user/get", &[("next_openid", start.clone())])
    .await?;
  let ids: Vec<String> = v
    .list("data.openid")
    .iter()
    .filter_map(|o| o.as_str().map(str::to_owned))
    .collect();
  let take = req.size_within(100);
  let slice: Vec<&String> = ids.iter().skip(offset).take(take).collect();
  let users = if slice.is_empty() {
    Vec::new()
  } else {
    let list: Vec<Value> = slice
      .iter()
      .map(|id| json!({ "openid": id, "lang": "zh_CN" }))
      .collect();
    let info = c
      .api_post("/cgi-bin/user/info/batchget", &json!({ "user_list": list }))
      .await?;
    info.list("user_info_list").iter().map(follower).collect()
  };
  let next = if offset + take < ids.len() {
    Some(format!("{start}:{}", offset + take))
  } else {
    v.str("next_openid")
      .filter(|n| !n.is_empty() && ids.len() >= 10000)
      .map(|n| format!("{n}:0"))
  };
  Ok(Page::new(users, next))
}

fn follower(u: &Value) -> User {
  let id = u.str("openid").unwrap_or_default();
  let mut user = User {
    name: u.str("remark").unwrap_or_else(|| id.clone()),
    id,
    followed: Some(u.i64("subscribe") == Some(1)),
    raw: Some(u.clone()),
    ..User::default()
  };
  if let Some(t) = u.i64("subscribe_time").and_then(from_secs) {
    user
      .extra
      .insert("followed_at".into(), t.to_string().into());
  }
  for (key, path) in [
    ("scene", "subscribe_scene"),
    ("unionid", "unionid"),
    ("qr_scene", "qr_scene_str"),
  ] {
    if let Some(v) = u.str(path) {
      user.extra.insert(key.into(), v.into());
    }
  }
  if !u.list("tagid_list").is_empty() {
    user.extra.insert("tags".into(), u.at("tagid_list").clone());
  }
  user
}

pub async fn tags(c: &Client) -> Result<Page<Collection>> {
  let v = c.api_get("/cgi-bin/tags/get", &[]).await?;
  let tags = v
    .list("tags")
    .iter()
    .map(|t| Collection {
      id: t.str("id").unwrap_or_default(),
      kind: "tag".into(),
      name: t.str("name").unwrap_or_default(),
      items: t.u64("count"),
      ..Collection::default()
    })
    .collect();
  Ok(Page::last(tags))
}

/// Followers carrying one tag (openids only).
pub async fn tagged(c: &Client, tag: &str, req: &PageReq) -> Result<Page<User>> {
  let v = c
    .api_post(
      "/cgi-bin/user/tag/get",
      &json!({
        "tagid": tag.parse::<u64>().unwrap_or(0),
        "next_openid": req.cursor.clone().unwrap_or_default(),
      }),
    )
    .await?;
  Ok(openids(&v))
}

pub async fn blacklist(c: &Client, req: &PageReq) -> Result<Page<User>> {
  let v = c
    .api_post(
      "/cgi-bin/tags/members/getblacklist",
      &json!({ "begin_openid": req.cursor.clone().unwrap_or_default() }),
    )
    .await?;
  Ok(openids(&v))
}

fn openids(v: &Value) -> Page<User> {
  let users: Vec<User> = v
    .list("data.openid")
    .iter()
    .filter_map(|o| o.as_str())
    .map(|id| User {
      id: id.into(),
      name: id.into(),
      ..User::default()
    })
    .collect();
  let next = v
    .str("next_openid")
    .filter(|n| !n.is_empty() && !users.is_empty());
  Page::new(users, next)
}
