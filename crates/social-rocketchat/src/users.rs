//! Accounts: the logged-in one and profiles.

use social_core::{Result, User, ValueExt, json};

use crate::api::Api;
use crate::parse;

pub async fn whoami(api: &Api) -> Result<User> {
  let server = api.require()?.server;
  let v = api.get("me", &[]).await?;
  let mut u = parse::user(&v, &server);
  u.extra.insert("server".into(), json!(server));
  Ok(u)
}

/// A user by id, `@name` / `name`, or `me`.
pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let arg = arg.trim();
  if arg == "me" {
    return whoami(api).await;
  }
  let server = api.require()?.server;
  let key = if crate::refs::is_id(arg) {
    "userId"
  } else {
    "username"
  };
  let name = arg
    .rsplit('/')
    .next()
    .unwrap_or(arg)
    .trim_start_matches('@');
  let v = api.get("users.info", &[(key, name.to_owned())]).await?;
  Ok(parse::user(v.at("user"), &server))
}
