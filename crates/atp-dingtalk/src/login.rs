//! `login --token APPKEY:APPSECRET[:ROBOTCODE]`, kept as given.

use atp_core::Result;

use crate::api::{Api, Creds};

pub fn login_token(api: &Api, token: &str) -> Result<()> {
  api.save(&Creds::parse(token)?);
  Ok(())
}
