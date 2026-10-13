//! Accounts: the logged-in one, profiles, their posts and relations, and the
//! logged-in account's favorites (with their folders) and likes.
//!
//! Lists that weibo.com shows only to logged-in accounts (a user's posts,
//! followers, following) fall back to m.weibo.cn's containers otherwise.

use social_core::{Collection, Error, Page, PageReq, Post, Result, User, Value, ValueExt};

use crate::api::Client;
use crate::parse;
use crate::refs::{self, UserRef};

/// The logged-in account's id (saved after the first lookup).
pub async fn own_uid(c: &Client) -> Result<String> {
  c.require_login()?;
  if let Some(uid) = c.own_uid() {
    return Ok(uid);
  }
  let v = c.get("/ajax/config/get_config", &[]).await?;
  let uid = v
    .first_str(&["data.uid", "data.user.idstr", "data.user.id"])
    .filter(|u| u != "0")
    .ok_or_else(|| {
      Error::auth("Weibo did not accept this session").with_hint(c.ctx.login_hint())
    })?;
  c.set_own_uid(&uid);
  Ok(uid)
}

pub async fn whoami(c: &Client) -> Result<User> {
  let uid = own_uid(c).await?;
  user(c, &uid).await
}

/// `profile/info` for any kind of user reference.
async fn info(c: &Client, arg: &str) -> Result<Value> {
  let (key, value) = match refs::user(arg)? {
    UserRef::Uid(uid) => ("uid", uid),
    UserRef::Name(name) => ("screen_name", name),
    UserRef::Domain(domain) => ("custom", domain),
  };
  let v = c.get("/ajax/profile/info", &[(key, &value)]).await?;
  if !v.at("data.user").is_object() {
    return Err(Error::not_found(format!("no Weibo user {arg}")));
  }
  Ok(v)
}

/// The numeric id of a user reference (looked up for names and domains).
pub async fn uid(c: &Client, arg: &str) -> Result<String> {
  match refs::user(arg)? {
    UserRef::Uid(uid) => Ok(uid),
    _ => info(c, arg)
      .await?
      .str("data.user.idstr")
      .ok_or_else(|| Error::not_found(format!("no Weibo user {arg}"))),
  }
}

pub async fn user(c: &Client, arg: &str) -> Result<User> {
  let v = info(c, arg).await?;
  let mut u =
    parse::user(v.at("data.user")).ok_or_else(|| Error::upstream("a profile without an id"))?;
  // Birthday, sign-up date, IP location, credit ... from the detail card.
  if let Ok(d) = c.get("/ajax/profile/detail", &[("uid", &u.id)]).await {
    let d = d.at("data");
    if u.created_at.is_none() {
      u.created_at = d
        .str("created_at")
        .and_then(|t| jiff::civil::DateTime::strptime("%Y-%m-%d %H:%M:%S", &t).ok())
        .and_then(|t| t.in_tz("Asia/Shanghai").ok())
        .map(|z| z.timestamp());
    }
    for (key, path) in [
      ("birthday", "birthday"),
      ("ip_location", "ip_location"),
      ("credit", "sunshine_credit.level"),
      ("school", "education.school"),
      ("company", "career.company"),
      ("gender", "gender"),
    ] {
      if let Some(s) = d.str(path).filter(|s| !s.is_empty()) {
        u.extra.insert(key.into(), s.into());
      }
    }
    let labels: Vec<String> = d
      .list("label_desc")
      .iter()
      .filter_map(|l| l.str("name"))
      .collect();
    if !labels.is_empty() {
      u.extra.insert("labels".into(), labels.into());
    }
    if let Some(n) = d.u64("interaction.read_count") {
      u.stats.other.insert("reads_yesterday".into(), n);
    }
  }
  Ok(u)
}

// ── posts of a user ───────────────────────────────────────────────────

/// Cursor `page` or `page:since_id` (weibo.com pages by both).
pub async fn user_posts(c: &Client, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let uid = uid(c, arg).await?;
  let cursor = page.cursor.clone().unwrap_or_default();
  let (no, since) = cursor.split_once(':').unwrap_or((cursor.as_str(), ""));
  let no: u64 = no.parse().unwrap_or(1);
  if !c.logged_in() {
    let container = format!("107603{uid}");
    let v = c
      .get_mobile(
        "/api/container/getIndex",
        &[
          ("type", "uid"),
          ("value", &uid),
          ("containerid", &container),
          ("page", &no.to_string()),
        ],
      )
      .await?;
    let items = parse::card_posts(v.list("data.cards"));
    let next = (!items.is_empty()).then(|| (no + 1).to_string());
    return Ok(Page::new(items, next));
  }
  let mut query = vec![
    ("uid", uid.clone()),
    ("page", no.to_string()),
    ("feature", "0".into()),
  ];
  if !since.is_empty() {
    query.push(("since_id", since.to_owned()));
  }
  let q: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
  let v = c.get("/ajax/statuses/mymblog", &q).await?;
  let items: Vec<Post> = v.list("data.list").iter().filter_map(parse::post).collect();
  let next_since = v.str("data.since_id").unwrap_or_default();
  let next = (!items.is_empty()).then(|| format!("{}:{next_since}", no + 1));
  Ok(Page::new(items, next))
}

// ── relations ─────────────────────────────────────────────────────────

pub async fn followers(c: &Client, arg: &str, page: &PageReq) -> Result<Page<User>> {
  relations(c, arg, true, page).await
}

pub async fn following(c: &Client, arg: &str, page: &PageReq) -> Result<Page<User>> {
  relations(c, arg, false, page).await
}

async fn relations(c: &Client, arg: &str, fans: bool, page: &PageReq) -> Result<Page<User>> {
  let uid = uid(c, arg).await?;
  if !c.logged_in() {
    let container = format!(
      "231051_-_{}_-_{uid}",
      if fans { "fans" } else { "followers" }
    );
    let mut query = vec![("containerid", container.as_str())];
    let since = page.cursor.clone().unwrap_or_default();
    if !since.is_empty() {
      query.push(("since_id", since.as_str()));
    }
    let v = c.get_mobile("/api/container/getIndex", &query).await?;
    let items = parse::card_users(v.list("data.cards"));
    let next = v
      .str("data.cardlistInfo.since_id")
      .filter(|s| s != "0" && !items.is_empty());
    return Ok(Page::new(items, next));
  }
  let no = page.number_or(1).to_string();
  let mut query = vec![("uid", uid.as_str()), ("page", no.as_str())];
  if fans {
    query.extend([
      ("relate", "fans"),
      ("type", "all"),
      ("newFollowerCount", "0"),
    ]);
  }
  let v = c.get("/ajax/friendships/friends", &query).await?;
  let items: Vec<User> = v.list("users").iter().filter_map(parse::user).collect();
  let more = !items.is_empty() && v.i64("next_cursor").is_none_or(|n| n != 0);
  Ok(Page::new(
    items,
    more.then(|| (page.number_or(1) + 1).to_string()),
  ))
}

// ── favorites and likes ───────────────────────────────────────────────

/// Favorite folders (收藏 tags) of the logged-in account.
pub async fn folders(c: &Client) -> Result<Page<Collection>> {
  c.require_login()?;
  let v = c.get("/ajax/favorites/tags", &[("page", "1")]).await?;
  let tags = if v.list("data.tags").is_empty() {
    v.list("tags")
  } else {
    v.list("data.tags")
  };
  let items = tags
    .iter()
    .filter_map(|t| {
      Some(Collection {
        id: t.first_str(&["id", "tag_id"])?,
        kind: "folder".into(),
        name: t.first_str(&["tag", "name"]).unwrap_or_default(),
        items: t.count("count"),
        raw: Some(t.clone()),
        ..Collection::default()
      })
    })
    .filter(|f| f.id != "0")
    .collect();
  Ok(Page::last(items))
}

pub async fn favorites(c: &Client, folder: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let uid = own_uid(c).await?;
  let no = page.number_or(1).to_string();
  let mut query = vec![
    ("uid", uid.as_str()),
    ("page", no.as_str()),
    ("with_total", "true"),
  ];
  if let Some(tid) = folder {
    query.push(("tid", tid));
  }
  let v = c.get("/ajax/favorites/all_fav", &query).await?;
  Ok(numbered_posts(&v, page.number_or(1)))
}

/// Posts the logged-in account liked.
pub async fn likes(c: &Client, arg: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let uid = match arg {
    Some(a) => uid(c, a).await?,
    None => own_uid(c).await?,
  };
  let no = page.number_or(1).to_string();
  let v = c
    .get(
      "/ajax/statuses/likelist",
      &[("uid", &uid), ("page", &no), ("with_total", "true")],
    )
    .await?;
  Ok(numbered_posts(&v, page.number_or(1)))
}

/// A numbered page of posts in `data`, `data.list` or `data.statuses`;
/// entries may wrap the post in `status`.
fn numbered_posts(v: &Value, no: u64) -> Page<Post> {
  let rows = ["data.list", "data.statuses", "data.data", "data"]
    .iter()
    .map(|k| v.list(k))
    .find(|l| !l.is_empty())
    .unwrap_or_default();
  let items: Vec<Post> = rows
    .iter()
    .filter_map(|r| {
      if r.at("status").is_object() {
        parse::post(r.at("status"))
      } else {
        parse::post(r)
      }
    })
    .collect();
  let next = (!items.is_empty()).then(|| (no + 1).to_string());
  Page::new(items, next)
}
