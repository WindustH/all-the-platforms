//! `login --token APPID:SECRET[:sandbox]`: the credentials must buy an
//! access token before they are kept.

use atp_core::Result;

use crate::api::{Api, Credentials};

pub async fn login_token(api: &Api, token: &str) -> Result<()> {
  let creds = Credentials::parse(token)?;
  api.access_token(&creds).await?;
  api.save(&creds);
  Ok(())
}
