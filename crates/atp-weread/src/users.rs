//! Users: you, and other readers by their number.

use atp_core::{Error, Result, User};

use crate::api::Api;
use crate::parse;
use crate::refs;

pub async fn whoami(api: &Api) -> Result<User> {
  api.require_login()?;
  let vid = api
    .vid()
    .ok_or_else(|| Error::auth("the session has no wr_vid cookie"))?;
  user(api, &vid).await
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let vid = refs::user(arg)?;
  api.require_login()?;
  let v = api.get("/web/user", &[("userVid", vid.clone())]).await?;
  let mut user = parse::user(&v);
  if user.id.is_empty() {
    user.id = vid;
  }
  Ok(user)
}
