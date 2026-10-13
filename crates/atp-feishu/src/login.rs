//! `login --token APP_ID:APP_SECRET` (`lark:` for larksuite.com), kept as
//! given.

use atp_core::Result;

use crate::api::{Api, Creds};

pub fn login_token(api: &Api, token: &str) -> Result<()> {
  api.save(&Creds::parse(token)?);
  Ok(())
}
