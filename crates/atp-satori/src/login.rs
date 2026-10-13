//! `login --token 'URL[|TOKEN][|PLATFORM:USER_ID]'`: the server, and the
//! account to act as — the one named, else the first the server offers.

use atp_core::output::note;
use atp_core::{Error, Result, ValueExt, json};

use crate::api::{Api, Server};

pub async fn login(api: &Api, token: &str) -> Result<()> {
  let mut server = Server::parse(token)?;
  if server.platform.is_none() {
    pick_account(api, &mut server).await?;
  }
  api.call_at(&server, "login.get", json!({})).await?;
  api.save(&server);
  Ok(())
}

/// The first account the server offers; a note says when there are more.
async fn pick_account(api: &Api, server: &mut Server) -> Result<()> {
  let logins = api.logins(server).await?;
  let first = logins
    .first()
    .ok_or_else(|| Error::upstream("the Satori server offers no logged-in account"))?;
  server.platform = first.str("platform");
  server.user_id = first.str("user.id");
  if logins.len() > 1 {
    note(&format!(
      "the server offers {} accounts; using {}:{} (pick another with `--token 'URL|TOKEN|PLATFORM:USER_ID'`, see `atp satori logins`)",
      logins.len(),
      server.platform.as_deref().unwrap_or_default(),
      server.user_id.as_deref().unwrap_or_default()
    ));
  }
  Ok(())
}
