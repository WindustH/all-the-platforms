//! Accounts and members. `whoami` asks `accountapi.dianping.com` (see
//! [`atp_mtsec::account`]); `user MEMBER` reads the member's www page,
//! which Dianping shows after a login.

use atp_core::{Error, Result, User};
use atp_mtsec::account;
use atp_mtsec::guard::Call;

use crate::api::{Api, WWW};
use crate::parse;
use crate::refs::{self, member_url};

pub async fn whoami(api: &Api) -> Result<User> {
  api.require_login()?;
  account::dianping(&api.ctx).await
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let id = refs::member(arg)?;
  api.require_login()?;
  let html = api
    .html(Call::page(member_url(&id), format!("{WWW}/")))
    .await?;
  parse::member(&html, &id).ok_or_else(|| Error::not_found(format!("no Dianping member {id}")))
}
