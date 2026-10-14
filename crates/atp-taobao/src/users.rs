//! Users: the logged-in account, and sellers, found by their items (Taobao
//! has no public user pages).

use atp_alibaba::account;
use atp_core::{Error, Result, User};

use crate::api::Api;
use crate::{parse, refs};

/// The account the cookies name, once a chat access token proves they work.
pub async fn whoami(api: &Api) -> Result<User> {
  api.verify().await?;
  let id = api.me()?;
  Ok(User {
    name: account::nick(&api.ctx).unwrap_or_else(|| id.clone()),
    id,
    ..User::default()
  })
}

/// The seller of an item.
pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let id = refs::item(api, arg)
    .await
    .map_err(|_| Error::input("Taobao finds sellers by their items: pass an item id or link"))?;
  let html = api.item_page(&id).await?;
  let seller =
    parse::seller(&html).ok_or_else(|| Error::upstream("the item page names no seller"))?;
  Ok(seller.user)
}
