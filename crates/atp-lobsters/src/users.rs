//! Users: the profile and the stories they submitted.

use atp_core::{Page, PageReq, Post, Result, User};

use crate::api::Api;
use crate::{parse, refs, stories};

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let name = refs::user(arg)?;
  Ok(parse::user(&api.json(&format!("~{name}.json")).await?))
}

/// The user's stories, newest first.
pub async fn posts(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let name = refs::user(arg)?;
  stories::listing(api, &format!("~{name}/stories"), page).await
}
