//! The bot and other users.

use atp_core::{Result, User};

use crate::api::Api;
use crate::{parse, refs};

pub async fn whoami(api: &Api) -> Result<User> {
  Ok(parse::user(&api.get("/user/me", &[]).await?))
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let id = refs::user(arg)?;
  Ok(parse::user(
    &api.get("/user/view", &[("user_id", id)]).await?,
  ))
}
