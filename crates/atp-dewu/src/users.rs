//! `whoami` and `login --token`. Dewu has no web login atp could use; the
//! only credentials are an Open Platform app's, which `whoami` checks with
//! one catalogue lookup.

use atp_core::{Result, User};

use crate::api::Api;
use crate::market;
use crate::open::Open;

pub async fn whoami(api: &Api) -> Result<User> {
  let creds = api.open.require(&api.ctx)?;
  market::check(api).await?;
  Ok(User {
    id: mask(&creds.key),
    name: "Dewu Open Platform app".into(),
    url: Some("https://open.poizon.com".into()),
    ..User::default()
  })
}

/// Keep `APP_KEY:APP_SECRET`; `whoami` checks them afterwards.
pub fn login_token(api: &Api, token: &str) -> Result<()> {
  Open::save(&api.ctx, token)
}

/// The first and last four characters of a key.
fn mask(key: &str) -> String {
  let chars: Vec<char> = key.chars().collect();
  if chars.len() <= 8 {
    return "*".repeat(chars.len());
  }
  let head: String = chars[..4].iter().collect();
  let tail: String = chars[chars.len() - 4..].iter().collect();
  format!("{head}…{tail}")
}
