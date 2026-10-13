//! Accounts: the logged-in one (or the bot key), profiles, and the server's
//! users, loaded once per run for authors and member lists.

use atp_core::{Result, User, ValueExt, json};

use crate::api::{Api, Mode};
use crate::parse::{self, Users};
use crate::refs;

/// Every user of the server (user mode; a bot key cannot list them).
pub async fn all(api: &Api) -> Result<Users> {
  if let Some(users) = api.users.borrow().clone() {
    return Ok(users);
  }
  let server = api.server().unwrap_or_default();
  let users: Users = if api.mode() == Some(Mode::User) {
    api
      .get("/user")
      .await?
      .as_array()
      .cloned()
      .unwrap_or_default()
      .iter()
      .filter_map(|v| Some((v.i64("uid")?, parse::user(v, &server))))
      .collect()
  } else {
    Users::new()
  };
  *api.users.borrow_mut() = Some(users.clone());
  Ok(users)
}

/// The account's user id; `None` for a bot key.
pub async fn me(api: &Api) -> Result<Option<i64>> {
  if api.mode() != Some(Mode::User) {
    return Ok(None);
  }
  if let Some(id) = *api.me.borrow() {
    return Ok(Some(id));
  }
  let id = api.get("/user/me").await?.i64("uid");
  *api.me.borrow_mut() = id;
  Ok(id)
}

pub async fn whoami(api: &Api) -> Result<User> {
  let (server, mode) = api.require()?;
  if mode == Mode::Bot {
    let groups = api.get("/bot").await?;
    let mut user = User {
      id: "bot".into(),
      name: format!("bot key on {server}"),
      ..User::default()
    };
    user.extra.insert(
      "channels".into(),
      json!(groups.as_array().map(Vec::len).unwrap_or(0)),
    );
    return Ok(user);
  }
  Ok(parse::user(&api.get("/user/me").await?, &server))
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let server = api.server().unwrap_or_default();
  let Some(uid) = refs::user(arg)? else {
    return whoami(api).await;
  };
  let path = if api.is_bot() {
    format!("/bot/user/{uid}")
  } else {
    format!("/user/{uid}")
  };
  Ok(parse::user(&api.get(&path).await?, &server))
}
