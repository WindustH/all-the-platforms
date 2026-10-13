//! The bot and server members.

use atp_core::{Error, Result, User};

use crate::api::Api;
use crate::parse;

pub async fn whoami(api: &Api) -> Result<User> {
  Ok(parse::user(&api.get("/users/@me", &[]).await?))
}

/// `GUILD/USER`: a member of a server (bots look users up by server).
pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let (guild, user) = arg
    .trim()
    .trim_start_matches("guild:")
    .split_once('/')
    .ok_or_else(|| Error::input("give SERVER/USER (bots look users up within a server)"))?;
  let v = api
    .get(&format!("/guilds/{guild}/members/{user}"), &[])
    .await?;
  Ok(parse::member(&v))
}
