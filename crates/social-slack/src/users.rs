//! Accounts: the token's own, profiles, token checks, and the name cache
//! that turns `U…` ids (authors, `<@U…>` mentions) into names.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use social_core::{Error, Page, PageReq, Result, User, Value, ValueExt, json};

use crate::api::{self, Api, Identity, Kind};

/// Names of user ids, kept a day in the cache.
const CACHE_TTL: Duration = Duration::from_secs(24 * 3600);

/// What messages need of a user: names and avatar.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Brief {
  pub name: String,
  pub handle: String,
  pub avatar: Option<String>,
  pub bot: bool,
}

impl Brief {
  pub fn user(&self, id: &str) -> User {
    let mut u = User {
      id: id.to_owned(),
      name: self.name.clone(),
      handle: Some(self.handle.clone()).filter(|h| !h.is_empty()),
      avatar: self.avatar.clone(),
      ..User::default()
    };
    if self.bot {
      u.extra.insert("bot".into(), json!(true));
    }
    u
  }
}

/// Check a token (and an optional app-level token) and keep them.
pub async fn login(api: &Api, raw: &str) -> Result<()> {
  let mut token = None;
  let mut app = None;
  for part in raw.split(|c: char| c == '|' || c.is_whitespace()) {
    let part = part.trim();
    if part.starts_with("xapp-") {
      app = Some(part.to_owned());
    } else if part.starts_with("xox") {
      token = Some(part.to_owned());
    } else if !part.is_empty() {
      return Err(
        Error::input("that is not a Slack token")
          .with_hint("expected xoxb-… (bot) or xoxp-… (user), optionally with an xapp-… app token"),
      );
    }
  }
  let Some(token) = token else {
    return Err(Error::input("no xoxb-… or xoxp-… token given").with_hint(
      "an app-level xapp-… token alone cannot read anything; pass it together with the bot or user token",
    ));
  };
  if token.starts_with("xoxc-") {
    return Err(
      Error::unsupported("xoxc- browser tokens")
        .with_hint("create a Slack app (api.slack.com/apps) and use its bot or user OAuth token"),
    );
  }
  let id: Identity = api::identity(&api.call_with(&token, "auth.test", Vec::new()).await?)?;
  api.save(&token, app.as_deref(), &id);
  Ok(())
}

pub async fn whoami(api: &Api) -> Result<User> {
  let token = api.require()?;
  let v = api.call("auth.test", Vec::new()).await?;
  let id = api::identity(&v)?;
  let mut u = match api
    .call("users.info", vec![("user", id.user_id.clone())])
    .await
  {
    Ok(info) => profile(info.at("user")),
    Err(_) => User {
      id: id.user_id.clone(),
      name: v.str("user").unwrap_or_default(),
      ..User::default()
    },
  };
  u.extra
    .insert("token".into(), json!(Kind::of(&token).name()));
  u.extra.insert("team".into(), json!(id.team));
  u.extra.insert("team_url".into(), json!(id.team_url));
  if let Some(app) = api.app_token() {
    u.extra.insert("socket_mode".into(), json!(!app.is_empty()));
  }
  Ok(u)
}

/// A full profile.
pub fn profile(v: &Value) -> User {
  let p = v.at("profile");
  let mut u = User {
    id: v.str("id").unwrap_or_default(),
    name: display_name(v),
    handle: v.str("name"),
    avatar: p.first_str(&["image_512", "image_192", "image_72"]),
    bio: p.str("title").filter(|t| !t.is_empty()),
    location: v.str("tz_label"),
    raw: Some(v.clone()),
    ..User::default()
  };
  for (key, path) in [
    ("email", "profile.email"),
    ("phone", "profile.phone"),
    ("status", "profile.status_text"),
    ("timezone", "tz"),
  ] {
    if let Some(s) = v.str(path).filter(|s| !s.is_empty()) {
      u.extra.insert(key.into(), json!(s));
    }
  }
  for flag in ["is_admin", "is_owner", "is_bot", "deleted", "is_restricted"] {
    if v.bool(flag) == Some(true) {
      u.extra
        .insert(flag.trim_start_matches("is_").into(), json!(true));
    }
  }
  u
}

fn display_name(v: &Value) -> String {
  v.first_str(&[
    "profile.display_name",
    "profile.real_name",
    "real_name",
    "name",
  ])
  .unwrap_or_default()
}

fn brief(v: &Value) -> Brief {
  Brief {
    name: display_name(v),
    handle: v.str("name").unwrap_or_default(),
    avatar: v.first_str(&["profile.image_72", "profile.image_48"]),
    bot: v.bool("is_bot") == Some(true),
  }
}

/// Learn the names of `ids` (users.info each unknown one), with a day-long
/// cache shared between runs.
pub async fn resolve(api: &Api, ids: impl IntoIterator<Item = String>) {
  let key = cache_key(api);
  if api.users.borrow().is_empty()
    && let Some(cached) = api
      .ctx
      .store
      .cache_get::<BTreeMap<String, Brief>>(&key, CACHE_TTL)
  {
    api.users.borrow_mut().extend(cached);
  }
  let mut learned = false;
  for id in ids {
    if id.is_empty() || api.users.borrow().contains_key(&id) {
      continue;
    }
    if let Ok(v) = api.call("users.info", vec![("user", id.clone())]).await {
      api.users.borrow_mut().insert(id, brief(v.at("user")));
      learned = true;
    }
  }
  if learned {
    let all: BTreeMap<String, Brief> = api
      .users
      .borrow()
      .iter()
      .map(|(k, v)| (k.clone(), v.clone()))
      .collect();
    api.ctx.store.cache_put(&key, &all);
  }
}

fn cache_key(api: &Api) -> String {
  let team: String = api
    .ctx
    .extra("team")
    .unwrap_or_default()
    .chars()
    .filter(|c| c.is_ascii_alphanumeric())
    .collect();
  format!("users-{team}")
}

/// A user id from `U…` / `W…`, `<@U…>`, `@name`, `name`, or an email.
pub async fn user_id(api: &Api, arg: &str) -> Result<String> {
  let arg = arg.trim();
  if arg == "me" {
    return api.me().await;
  }
  let inner = arg
    .strip_prefix("<@")
    .and_then(|s| s.strip_suffix('>'))
    .map(|s| s.split('|').next().unwrap_or(s));
  if let Some(id) = inner {
    return Ok(id.to_owned());
  }
  if is_user_id(arg) {
    return Ok(arg.to_owned());
  }
  if arg.contains('@') && !arg.starts_with('@') {
    let v = api
      .call("users.lookupByEmail", vec![("email", arg.to_owned())])
      .await?;
    return v
      .str("user.id")
      .ok_or_else(|| Error::not_found(format!("no user with email {arg}")));
  }
  let name = arg.trim_start_matches('@').to_lowercase();
  let found = scan(api, |u| {
    u.str("name").map(|n| n.to_lowercase()) == Some(name.clone())
      || display_name(u).to_lowercase() == name
  })
  .await?;
  found
    .into_iter()
    .next()
    .and_then(|u| u.str("id"))
    .ok_or_else(|| Error::not_found(format!("no user named {arg} in this workspace")))
}

pub fn is_user_id(s: &str) -> bool {
  (s.starts_with('U') || s.starts_with('W'))
    && s.len() >= 9
    && s
      .bytes()
      .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let id = user_id(api, arg).await?;
  let v = api.call("users.info", vec![("user", id)]).await?;
  Ok(profile(v.at("user")))
}

/// Members of the workspace whose names contain `query`.
pub async fn search(api: &Api, query: &str, page: &PageReq) -> Result<Page<User>> {
  let q = query.trim().trim_start_matches('@').to_lowercase();
  let found = scan(api, |u| {
    u.bool("deleted") != Some(true)
      && [
        u.str("name"),
        u.str("real_name"),
        u.str("profile.display_name"),
        u.str("profile.email"),
      ]
      .into_iter()
      .flatten()
      .any(|n| n.to_lowercase().contains(&q))
  })
  .await?;
  let mut users: Vec<User> = found.iter().map(profile).collect();
  users.truncate(page.size.max(1));
  Ok(Page::last(users))
}

/// Walk `users.list` and keep the users `keep` accepts.
async fn scan(api: &Api, keep: impl Fn(&Value) -> bool) -> Result<Vec<Value>> {
  let mut out = Vec::new();
  let mut cursor = String::new();
  for _ in 0..50 {
    let mut params = vec![("limit", "200".to_owned())];
    if !cursor.is_empty() {
      params.push(("cursor", cursor.clone()));
    }
    let v = api.call("users.list", params).await?;
    out.extend(v.list("members").iter().filter(|u| keep(u)).cloned());
    cursor = v.str("response_metadata.next_cursor").unwrap_or_default();
    if cursor.is_empty() {
      break;
    }
  }
  Ok(out)
}

/// Users by id, for member lists.
pub async fn many(api: &Api, ids: &[String]) -> Vec<User> {
  resolve(api, ids.iter().cloned()).await;
  let known = api.users.borrow();
  ids
    .iter()
    .map(|id| match known.get(id) {
      Some(b) => b.user(id),
      None => User {
        id: id.clone(),
        name: id.clone(),
        ..User::default()
      },
    })
    .collect()
}
