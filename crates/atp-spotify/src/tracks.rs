//! Tracks: one by id, with its album, artists, duration and ISRC.

use atp_core::{Error, Post, Result};

use crate::api::Api;
use crate::parse;

pub async fn read(api: &Api, id: &str) -> Result<Post> {
  let v = api.get(&format!("/tracks/{id}"), &[]).await?;
  parse::track(&v).ok_or_else(|| Error::not_found(format!("no track {id}")))
}
