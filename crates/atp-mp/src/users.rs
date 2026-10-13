//! The logged-in account and its followers.
//!
//! The account comes from the backend's home page (nickname, avatar,
//! original id, and the upload ticket other modules need), or, with only
//! API credentials, the AppID whose token the API grants.
//!
//! Followers, tags and the blacklist come from the official API (verified
//! accounts): `user/get` hands out up to 10000 openids per call; profiles
//! come from `user/info/batchget`, 100 at a time, and carry no nickname or
//! avatar since 2021 (follow time, scene, remark and tags remain). A page
//! cursor is `START_OPENID:OFFSET` into one `user/get` batch.

use std::sync::LazyLock;

use atp_core::text::from_secs;
use atp_core::{Collection, Error, Page, PageReq, Result, User, Value, ValueExt, json};
use regex::Regex;

use crate::api::Api;
use crate::js::entities;

/// What the home page tells about the session.
pub struct Home {
  pub nickname: String,
  pub avatar: Option<String>,
  /// Original id (`gh_...`).
  pub user_name: String,
  /// Upload ticket and server time for `filetransfer`.
  pub ticket: Option<String>,
  pub time: Option<String>,
}

fn field(html: &str, re: &Regex) -> Option<String> {
  re.captures(html)
    .map(|m| entities(&m[1]))
    .filter(|s| !s.is_empty())
}

pub async fn home(api: &Api) -> Result<Home> {
  static NICK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?:nick_name\s*[:=]\s*"([^"]+)")"#).expect("valid"));
  static HEAD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"head_img\s*[:=]\s*"([^"]+)""#).expect("valid"));
  static USER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"user_name\s*[:=]\s*"(gh_[^"]+)""#).expect("valid"));
  static TICKET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\bticket\s*:\s*"([^"]+)""#).expect("valid"));
  static TIME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\btime\s*:\s*"(\d+)""#).expect("valid"));
  let html = api
    .backend()
    .page("/cgi-bin/home", vec![("t", "home/index".into())])
    .await?;
  let user_name = field(&html, &USER)
    .or_else(|| api.ctx.http.cookie("slave_user"))
    .ok_or_else(|| Error::upstream("the backend home page shows no account"))?;
  Ok(Home {
    nickname: field(&html, &NICK).unwrap_or_else(|| user_name.clone()),
    avatar: field(&html, &HEAD),
    user_name,
    ticket: field(&html, &TICKET),
    time: field(&html, &TIME),
  })
}

pub async fn whoami(api: &Api) -> Result<User> {
  api.require_any()?;
  let appid = if api.official().has() {
    // Fetching a token proves the AppID / AppSecret and the IP whitelist.
    api
      .official()
      .get("/cgi-bin/get_current_selfmenu_info", &[])
      .await
      .map(|_| {
        api
          .official()
          .credentials()
          .map(|cr| cr.appid)
          .unwrap_or_default()
      })
  } else {
    Err(Error::auth("no API credentials"))
  };
  if api.backend().has() {
    let h = home(api).await?;
    let mut user = User {
      id: h.user_name,
      name: h.nickname,
      avatar: h.avatar,
      ..User::default()
    };
    user.extra.insert("route".into(), "backend".into());
    if let Some(uin) = api
      .ctx
      .http
      .cookie("bizuin")
      .or_else(|| api.ctx.http.cookie("data_bizuin"))
    {
      user.extra.insert("bizuin".into(), uin.into());
    }
    match appid {
      Ok(appid) => {
        user.extra.insert("appid".into(), Value::from(appid));
      }
      Err(e) if api.official().has() => {
        user
          .extra
          .insert("api_error".into(), Value::from(e.message));
      }
      Err(_) => {}
    }
    return Ok(user);
  }
  let appid = appid?;
  let mut user = User {
    id: appid.clone(),
    name: appid,
    ..User::default()
  };
  user.extra.insert("route".into(), "api".into());
  Ok(user)
}

pub async fn followers(api: &Api, req: &PageReq) -> Result<Page<User>> {
  let (start, offset) = match req.cursor.as_deref().and_then(|c| c.rsplit_once(':')) {
    Some((s, o)) => (s.to_owned(), o.parse::<usize>().unwrap_or(0)),
    None => (String::new(), 0),
  };
  let v = api
    .official()
    .get("/cgi-bin/user/get", &[("next_openid", start.clone())])
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
    let info = api
      .official()
      .post("/cgi-bin/user/info/batchget", &json!({ "user_list": list }))
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

pub async fn tags(api: &Api) -> Result<Page<Collection>> {
  let v = api.official().get("/cgi-bin/tags/get", &[]).await?;
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
pub async fn tagged(api: &Api, tag: &str, req: &PageReq) -> Result<Page<User>> {
  let v = api
    .official()
    .post(
      "/cgi-bin/user/tag/get",
      &json!({
        "tagid": tag.parse::<u64>().unwrap_or(0),
        "next_openid": req.cursor.clone().unwrap_or_default(),
      }),
    )
    .await?;
  Ok(openids(&v))
}

pub async fn blacklist(api: &Api, req: &PageReq) -> Result<Page<User>> {
  let v = api
    .official()
    .post(
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
