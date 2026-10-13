//! The logged-in account: from the backend's home page (nickname, avatar,
//! original id, and the upload ticket other modules need), or, with only
//! API credentials, the AppID whose token the API grants.

use std::sync::LazyLock;

use regex::Regex;
use social_core::{Error, Result, User, Value};

use crate::client::Client;
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

pub async fn home(c: &Client) -> Result<Home> {
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
  let html = c
    .web_page("/cgi-bin/home", vec![("t", "home/index".into())])
    .await?;
  let user_name = field(&html, &USER)
    .or_else(|| c.ctx.http.cookie("slave_user"))
    .ok_or_else(|| Error::upstream("the backend home page shows no account"))?;
  Ok(Home {
    nickname: field(&html, &NICK).unwrap_or_else(|| user_name.clone()),
    avatar: field(&html, &HEAD),
    user_name,
    ticket: field(&html, &TICKET),
    time: field(&html, &TIME),
  })
}

pub async fn whoami(c: &Client) -> Result<User> {
  c.require_any()?;
  let api = if c.has_api() {
    // Fetching a token proves the AppID / AppSecret and the IP whitelist.
    c.api_get("/cgi-bin/get_current_selfmenu_info", &[])
      .await
      .map(|_| c.credentials().map(|cr| cr.appid).unwrap_or_default())
  } else {
    Err(Error::auth("no API credentials"))
  };
  if c.has_web() {
    let h = home(c).await?;
    let mut user = User {
      id: h.user_name,
      name: h.nickname,
      avatar: h.avatar,
      ..User::default()
    };
    user.extra.insert("route".into(), "backend".into());
    if let Some(uin) = c
      .ctx
      .http
      .cookie("bizuin")
      .or_else(|| c.ctx.http.cookie("data_bizuin"))
    {
      user.extra.insert("bizuin".into(), uin.into());
    }
    match api {
      Ok(appid) => {
        user.extra.insert("appid".into(), Value::from(appid));
      }
      Err(e) if c.has_api() => {
        user
          .extra
          .insert("api_error".into(), Value::from(e.message));
      }
      Err(_) => {}
    }
    return Ok(user);
  }
  let appid = api?;
  let mut user = User {
    id: appid.clone(),
    name: appid,
    ..User::default()
  };
  user.extra.insert("route".into(), "api".into());
  Ok(user)
}
