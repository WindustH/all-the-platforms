//! `login --token`: an app's `CORPID:SECRET[:AGENTID]` or an AI bot's
//! `bot:BOTID:SECRET`, kept as given (each kind beside the other).

use atp_core::Result;

use crate::api::{Api, Login};

pub fn login_token(api: &Api, token: &str) -> Result<()> {
  api.save(&Login::parse(token)?);
  Ok(())
}
