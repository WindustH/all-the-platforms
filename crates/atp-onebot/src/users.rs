//! The logged-in account and other QQ users.

use atp_core::{Result, User, json};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, num};

pub async fn whoami(api: &Api) -> Result<User> {
  let v = api.call("get_login_info", json!({})).await?;
  Ok(parse::user(&v))
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let id = refs::user(arg)?;
  let v = api
    .call("get_stranger_info", json!({"user_id": num(&id)}))
    .await?;
  Ok(parse::user(&v))
}
