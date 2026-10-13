//! Accounts: the logged-in one, profiles, searches, and the name cache for
//! authors and members (`POST /users/ids` in batches).

use atp_core::{Error, Page, PageReq, Result, User, Value, ValueExt, json};

use crate::api::Api;

/// What posts need of a user.
#[derive(Debug, Clone, Default)]
pub struct Brief {
  pub name: String,
  pub handle: String,
  pub bot: bool,
}

impl Brief {
  pub fn user(&self, id: &str) -> User {
    let mut u = User {
      id: id.to_owned(),
      name: self.name.clone(),
      handle: Some(self.handle.clone()).filter(|h| !h.is_empty()),
      ..User::default()
    };
    if self.bot {
      u.extra.insert("bot".into(), json!(true));
    }
    u
  }
}

fn display(v: &Value) -> String {
  let full = format!(
    "{} {}",
    v.str("first_name").unwrap_or_default(),
    v.str("last_name").unwrap_or_default()
  );
  v.str("nickname")
    .or_else(|| Some(full.trim().to_owned()).filter(|f| !f.is_empty()))
    .or_else(|| v.str("username"))
    .unwrap_or_default()
}

fn brief(v: &Value) -> Brief {
  Brief {
    name: display(v),
    handle: v.str("username").unwrap_or_default(),
    bot: v.bool("is_bot") == Some(true),
  }
}

pub fn profile(v: &Value, server: &str) -> User {
  let id = v.str("id").unwrap_or_default();
  let mut u = User {
    name: display(v),
    handle: v.str("username"),
    url: v.str("username").map(|n| format!("{server}/messages/@{n}")),
    bio: v.str("position").filter(|p| !p.is_empty()),
    created_at: v.i64("create_at").and_then(atp_core::text::from_millis),
    raw: Some(v.clone()),
    ..User::default()
  };
  for (key, path) in [
    ("email", "email"),
    ("roles", "roles"),
    ("locale", "locale"),
    ("timezone", "timezone.automaticTimezone"),
  ] {
    if let Some(s) = v.str(path).filter(|s| !s.is_empty()) {
      u.extra.insert(key.into(), json!(s));
    }
  }
  if v.bool("is_bot") == Some(true) {
    u.extra.insert("bot".into(), json!(true));
  }
  if v.i64("delete_at").is_some_and(|d| d > 0) {
    u.extra.insert("deactivated".into(), json!(true));
  }
  u.id = id;
  u
}

/// Learn the names of `ids` not seen yet.
pub async fn resolve(api: &Api, ids: impl IntoIterator<Item = String>) {
  let missing: Vec<String> = {
    let known = api.users.borrow();
    let mut m: Vec<String> = ids
      .into_iter()
      .filter(|id| !id.is_empty() && !known.contains_key(id))
      .collect();
    m.sort();
    m.dedup();
    m
  };
  for chunk in missing.chunks(200) {
    if let Ok(v) = api.post_read("/users/ids", json!(chunk)).await {
      let mut known = api.users.borrow_mut();
      for u in v.as_array().into_iter().flatten() {
        if let Some(id) = u.str("id") {
          known.insert(id, brief(u));
        }
      }
    }
  }
}

pub async fn whoami(api: &Api) -> Result<User> {
  let creds = api.require()?;
  let v = api.get("/users/me", &[]).await?;
  let mut u = profile(&v, &creds.server);
  u.extra.insert("server".into(), json!(creds.server));
  Ok(u)
}

/// A user id from an id, `@name` / `name`, an email or `me`.
pub async fn user_id(api: &Api, arg: &str) -> Result<String> {
  lookup(api, arg)
    .await?
    .str("id")
    .ok_or_else(|| Error::not_found(format!("no user {arg}")))
}

async fn lookup(api: &Api, arg: &str) -> Result<Value> {
  let arg = arg.trim();
  if arg == "me" {
    return api.get("/users/me", &[]).await;
  }
  if crate::refs::is_id(arg) {
    return api.get(&format!("/users/{arg}"), &[]).await;
  }
  if arg.contains('@') && !arg.starts_with('@') {
    return api.get(&format!("/users/email/{arg}"), &[]).await;
  }
  let name = arg
    .trim_start_matches('@')
    .rsplit('/')
    .next()
    .unwrap_or(arg)
    .trim_start_matches('@');
  api.get(&format!("/users/username/{name}"), &[]).await
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let server = api.server()?;
  Ok(profile(&lookup(api, arg).await?, &server))
}

pub async fn search(api: &Api, term: &str, page: &PageReq) -> Result<Page<User>> {
  let server = api.server()?;
  let v = api
    .post_read(
      "/users/search",
      json!({"term": term.trim_start_matches('@'), "limit": page.size_within(100)}),
    )
    .await?;
  Ok(Page::last(
    v.as_array()
      .into_iter()
      .flatten()
      .map(|u| profile(u, &server))
      .collect(),
  ))
}

/// Users by id, in order.
pub async fn many(api: &Api, ids: &[String]) -> Vec<User> {
  resolve(api, ids.iter().cloned()).await;
  let known = api.users.borrow();
  ids
    .iter()
    .map(|id| {
      known.get(id).map(|b| b.user(id)).unwrap_or(User {
        id: id.clone(),
        name: id.clone(),
        ..User::default()
      })
    })
    .collect()
}
