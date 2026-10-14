//! Users: the Firebase profile (karma, about, how much they submitted).

use atp_core::{Error, Result, User};

use crate::api::Api;
use crate::{parse, refs};

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let name = refs::user(arg)?;
  let v = api.firebase(&format!("user/{name}")).await?;
  if v.is_null() {
    return Err(Error::not_found(format!("no Hacker News user {name}")));
  }
  Ok(parse::user(&v))
}
